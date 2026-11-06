//! URL probe – discovers file metadata before downloading.
//!
//! Uses a one-byte `GET Range` request to determine:
//! - `Content-Range` (or a whole-response `Content-Length`) – total file size
//! - Verified byte-range support – whether parallel requests are safe
//! - Suggested filename from `Content-Disposition` or the URL path
//!
//! Reuses a full response when ranges are ignored, or falls back to a plain GET.
//!

use super::gate::{HostBudget, LeasedResponse};
use super::request::{Control, InvalidRedirect, RequestContext, Spec};
use anyhow::{Context, Result};
use reqwest::StatusCode;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of probing a URL before starting a download.
#[derive(Debug)]
pub struct ProbeResult {
    /// Negotiated HTTP protocol, retained for transfer diagnostics.
    pub http_version: reqwest::Version,
    /// Strong server validator used to keep reconnects on the same representation.
    pub if_range: Option<String>,
    /// Reuse a streaming GET when the endpoint cannot be probed safely.
    pub response: Option<LeasedResponse>,
    /// Inclusive end of a validated initial partial response retained as data.
    pub initial_range_end: Option<u64>,
    /// Retain learned capacity while this download schedules its range workers.
    pub host_budget: std::sync::Arc<HostBudget>,
    /// File size in bytes, if the server reported `Content-Length`.
    pub content_length: Option<u64>,
    /// A complete one-byte response, or retained partial headers whose data
    /// must still pass the segment worker's byte/validator checks.
    pub accepts_ranges: bool,
    /// Resolved filename (from `Content-Disposition` or URL path).
    pub filename: Option<String>,
    /// Final URL after redirects – used for all segment requests.
    pub final_url: String,
}

impl ProbeResult {
    /// Returns `true` when the file can be downloaded with parallel segments.
    ///
    /// Both conditions must hold: the server must support range requests **and**
    /// the total size must be known (so we can divide it into chunks).
    pub fn supports_multipart(&self) -> bool {
        self.accepts_ranges && self.content_length.is_some()
    }
}

/// A browser challenge is never a file. Allow intentional HTML downloads, but
/// do not silently save an HTML login/error response under a binary filename.
pub fn validate_file_response(headers: &reqwest::header::HeaderMap, url: &str) -> Result<()> {
    let challenge = headers
        .get("cf-mitigated")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("challenge"));
    anyhow::ensure!(
        !challenge,
        "This site requires a browser check. Complete it in your browser and download again."
    );
    let html = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            matches!(
                v.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "text/html" | "application/xhtml+xml"
            )
        });
    let name = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_content_disposition)
        .unwrap_or_else(|| filename_from_url(url));
    let intentional_html = std::path::Path::new(&name)
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "html" | "htm" | "xhtml"));
    anyhow::ensure!(
        !html || intentional_html,
        "The site returned a web page instead of a file. Sign in or complete the browser check, then download again."
    );
    Ok(())
}

#[cfg(test)]
mod response_guard_tests {
    use super::*;

    #[test]
    fn refuses_browser_pages_but_keeps_intentional_html_and_json_downloads() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        assert!(validate_file_response(&headers, "https://example.com/driver.exe").is_err());
        assert!(validate_file_response(&headers, "https://example.com/page.html").is_ok());
        headers.insert("cf-mitigated", "challenge".parse().unwrap());
        assert!(validate_file_response(&headers, "https://example.com/page.html").is_err());
        headers.remove("cf-mitigated");
        headers.insert("content-type", "application/json".parse().unwrap());
        assert!(validate_file_response(&headers, "https://example.com/data.json").is_ok());
    }
}

// ---------------------------------------------------------------------------
// Public function
// ---------------------------------------------------------------------------

/// Probe `url` and return metadata needed to plan the download.
///
/// # Errors
/// Returns an error if the network request fails or the server returns a
/// non-2xx status on the range probe or fallback full stream.
pub async fn probe(client: &RequestContext, url: &str) -> Result<ProbeResult> {
    probe_impl(client, url, false).await
}

/// Keep a length-proven bounded initial range as file data; validate ambiguous bodies through the original probe.
pub async fn probe_with_initial_range(client: &RequestContext, url: &str) -> Result<ProbeResult> {
    probe_impl(client, url, true).await
}

const INITIAL_RANGE_END: u64 = super::connections::MIN_SEGMENT_BYTES - 1;

async fn probe_impl(client: &RequestContext, url: &str, mut reuse: bool) -> Result<ProbeResult> {
    anyhow::ensure!(
        !is_token_download(url),
        "This download requires a fresh website token. Open the source page and click its download button with the updated IDG browser extension enabled; the download.php URL alone cannot download the file."
    );
    if is_mail_export(url) {
        return streaming_probe(client, url).await;
    }
    // One request establishes metadata and verifies real range semantics. A
    // separate HEAD adds an avoidable round trip and can disagree with GET.
    let initial = if reuse {
        initial_probe_impl(client, url, std::time::Duration::from_secs(5), true).await
    } else {
        initial_probe(client, url, std::time::Duration::from_secs(5)).await
    };
    let (leased, accepts_ranges) = match initial {
        Ok((leased, accepts_ranges))
            if reuse
                && ((leased.response.status() == StatusCode::PARTIAL_CONTENT
                    && (!accepts_ranges || leased.response.content_length().is_none()))
                    || prefix_refused(leased.response.status())) =>
        {
            // Decoding can hide the encoded length, and larger ranges can be
            // refused. Restore the original body proof without losing learned
            // host capacity or valid short/chunked range support.
            let _budget = HostBudget::for_url(leased.response.url().as_str());
            drop(leased);
            reuse = false;
            initial_probe(client, url, std::time::Duration::from_secs(5)).await?
        }
        Err(error) if reuse && !error.is::<InvalidRedirect>() => {
            reuse = false;
            initial_probe(client, url, std::time::Duration::from_secs(5)).await?
        }
        result => result?,
    };
    let LeasedResponse {
        response: resp,
        permit,
    } = leased;
    let reuse_response = !accepts_ranges || reuse;
    let resp = resp
        .error_for_status()
        .context("Server rejected download probe")?;
    anyhow::ensure!(
        resp.status().is_success(),
        "Server returned an unexpected probe status: {}",
        resp.status()
    );
    let http_version = resp.version();
    let final_url = resp.url().to_string();
    let host_budget = HostBudget::for_url(&final_url);
    let headers = resp.headers().clone();
    validate_file_response(&headers, &final_url)?;
    let if_range = headers
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim_start().starts_with("W/"))
        .or_else(|| headers.get("last-modified").and_then(|v| v.to_str().ok()))
        .map(str::to_owned);

    let partial = reuse.then(|| initial_range(&headers)).flatten();
    let content_length = if accepts_ranges {
        if reuse {
            partial.map(|(total, _)| total)
        } else {
            range_probe_size(&headers)
        }
    } else {
        resp.content_length()
    };

    let filename = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_content_disposition)
        .or_else(|| Some(filename_from_url(&final_url)));

    Ok(ProbeResult {
        http_version,
        if_range,
        initial_range_end: partial.filter(|_| accepts_ranges).map(|(_, end)| end),
        response: reuse_response.then_some(LeasedResponse {
            response: resp,
            permit,
        }),
        host_budget,
        content_length,
        accepts_ranges,
        filename,
        final_url,
    })
}

/// Retry inconclusive network/overload failures without a speculative HEAD.
/// A valid one-byte response is drained so its connection returns to the pool.
/// The five-second deadline covers only the probe, never a reused full stream.
async fn initial_probe(
    client: &RequestContext,
    url: &str,
    deadline: std::time::Duration,
) -> Result<(LeasedResponse, bool)> {
    initial_probe_impl(client, url, deadline, false).await
}

async fn initial_probe_impl(
    client: &RequestContext,
    url: &str,
    deadline: std::time::Duration,
    reuse: bool,
) -> Result<(LeasedResponse, bool)> {
    for attempt in 0..3 {
        let probing = async {
            let received = client
                .send(
                    (if reuse {
                        Spec::get_once(url)
                    } else {
                        Spec::get(url)
                    })
                    .header(
                        "range",
                        if reuse {
                            std::borrow::Cow::Owned(format!("bytes=0-{INITIAL_RANGE_END}"))
                        } else {
                            std::borrow::Cow::Borrowed("bytes=0-0")
                        },
                    )?,
                    Control {
                        network_timeout: Some(deadline),
                        ..Control::default()
                    },
                )
                .await?;
            let LeasedResponse {
                mut response,
                permit,
            } = received.leased;
            if reuse && response.status() == StatusCode::PARTIAL_CONTENT {
                let valid = initial_range(response.headers()).is_some_and(|(_, end)| {
                    response
                        .content_length()
                        .is_none_or(|length| length == end + 1)
                });
                return Ok::<_, anyhow::Error>((LeasedResponse { response, permit }, valid));
            }
            let mut verified = false;
            if response.status() == StatusCode::PARTIAL_CONTENT
                && range_probe_size(response.headers()).is_some()
            {
                let reading = async {
                    let mut count = 0;
                    while let Some(bytes) =
                        response.chunk().await.context("Range probe interrupted")?
                    {
                        count += bytes.len();
                        if count > 1 {
                            break;
                        }
                    }
                    anyhow::ensure!(count > 0, "Range probe returned no bytes");
                    Ok::<_, anyhow::Error>(count == 1)
                };
                verified = tokio::time::timeout(
                    received.remaining_network_time.unwrap_or(deadline),
                    reading,
                )
                .await
                .map_err(|_| anyhow::anyhow!("Range probe timed out"))??;
            }
            Ok::<_, anyhow::Error>((LeasedResponse { response, permit }, verified))
        };
        match probing.await {
            result if reuse => return result,
            Ok((response, _))
                if matches!(
                    response.response.status().as_u16(),
                    429 | 500 | 502 | 503 | 504
                ) =>
            {
                if attempt == 2 {
                    break;
                }
            }
            Ok((response, true)) => return Ok((response, true)),
            Ok((response, false)) => {
                if !matches!(
                    response.response.status().as_u16(),
                    206 | 400 | 403 | 405 | 416 | 501
                ) {
                    return Ok((response, false));
                }
                break;
            }
            Err(error) if error.is::<InvalidRedirect>() => return Err(error),
            Err(error) if attempt == 2 => {
                eprintln!("[IDG] range probe inconclusive; trying a full stream: {error:#}");
                break;
            }
            Err(_) => {}
        }
        eprintln!(
            "[IDG] retrying inconclusive range probe ({}/3)",
            attempt + 1
        );
        tokio::time::sleep(std::time::Duration::from_millis(50 * (attempt + 1))).await;
    }
    let leased = client
        .send(Spec::get(url), Control::default())
        .await?
        .leased;
    anyhow::ensure!(
        leased.response.status() != StatusCode::PARTIAL_CONTENT,
        "Server returned partial data for a full download request"
    );
    Ok((leased, false))
}

fn range_probe_size(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("content-range")?
        .to_str()
        .ok()?
        .trim()
        .strip_prefix("bytes 0-0/")?
        .parse::<u64>()
        .ok()
        .filter(|size| *size > 0)
}

fn initial_range(headers: &reqwest::header::HeaderMap) -> Option<(u64, u64)> {
    let value = headers.get("content-range")?.to_str().ok()?.trim();
    let (end, total) = value.strip_prefix("bytes 0-")?.split_once('/')?;
    let end = end.parse::<u64>().ok()?;
    let total = total.parse::<u64>().ok()?.checked_sub(1)?;
    (end == total.min(INITIAL_RANGE_END)).then_some((total + 1, end))
}

fn prefix_refused(status: StatusCode) -> bool {
    !status.is_success()
}

/// Token-only endpoints must be submitted through the original browser form.
pub fn is_token_download(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|url| {
        url.host_str()
            .and_then(|host| host.strip_suffix(".safefilehosting.com"))
            .is_some_and(|subdomain| {
                !subdomain.is_empty()
                    && subdomain
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
            && url.path() == "/files/download.php"
    })
}

/// Mail exports are generated streams, not static files. Do not issue HEAD,
/// speculative ranges, or a second GET that regenerates the archive.
pub fn is_mail_export(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|url| {
        url.host_str() == Some("mail.hostinger.com") && is_mail_export_path(url.path())
    })
}

fn is_mail_export_path(path: &str) -> bool {
    path.strip_prefix("/bridge/api/v3/mailboxes/")
        .and_then(|path| path.split_once('/'))
        .is_some_and(|(mailbox, rest)| !mailbox.is_empty() && rest == "messages/sources/export")
}

/// Send a token-bearing form exactly once; reuse the response as the file stream.
/// HEAD/range probes and parallel POST replays can consume one-use tokens.
pub async fn post_probe(client: &RequestContext, url: &str, body: String) -> Result<ProbeResult> {
    let LeasedResponse { response, permit } = client
        .send(Spec::form(url, body), Control::default())
        .await
        .context("Download form request failed")?
        .leased;
    let response = response
        .error_for_status()
        .context("Server rejected download form; return to the website for a fresh link")?;
    anyhow::ensure!(
        response.status().is_success(),
        "Server returned an unexpected form status"
    );
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    anyhow::ensure!(
        !matches!(
            content_type.as_str(),
            "text/html" | "application/json" | "application/problem+json"
        ),
        "The website returned an error or waiting page; return to the browser for a fresh download token"
    );
    let filename = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_content_disposition);
    Ok(ProbeResult {
        http_version: response.version(),
        if_range: None,
        initial_range_end: None,
        content_length: response.content_length(),
        accepts_ranges: false,
        filename,
        final_url: response.url().to_string(),
        host_budget: HostBudget::for_url(response.url().as_str()),
        response: Some(LeasedResponse { response, permit }),
    })
}

async fn streaming_probe(client: &RequestContext, url: &str) -> Result<ProbeResult> {
    let LeasedResponse { response, permit } = client
        .send(Spec::get(url).single_use(), Control::default())
        .await
        .context("Email export request failed")?
        .leased;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        anyhow::bail!(
            "Email session expired or missing. Sign in to Hostinger Mail and export again using the IDG browser extension."
        );
    }
    let response = response
        .error_for_status()
        .context("Server rejected email export")?;
    anyhow::ensure!(
        response.status().is_success(),
        "Server returned an unexpected export status"
    );
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if matches!(
        content_type,
        "text/html" | "application/json" | "application/problem+json"
    ) {
        anyhow::bail!(
            "Hostinger returned a sign-in page or API error instead of an email export. Sign in and export again using the IDG browser extension."
        );
    }
    let final_url = response.url().to_string();
    let filename = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_content_disposition)
        .unwrap_or_else(|| filename_from_url(url));
    Ok(ProbeResult {
        http_version: response.version(),
        if_range: None,
        initial_range_end: None,
        content_length: response.content_length(),
        accepts_ranges: false,
        filename: Some(filename),
        final_url,
        host_budget: HostBudget::for_url(response.url().as_str()),
        response: Some(LeasedResponse { response, permit }),
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Decode HTTP filename parameters, preferring RFC 5987 over the legacy fallback.
fn filename_from_content_disposition(cd: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in cd.chars() {
        if escaped {
            part.push(c);
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            parts.push(std::mem::take(&mut part));
        } else {
            part.push(c);
        }
    }
    parts.push(part);
    let mut fallback = None;
    let mut extended = None;
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if key.trim().eq_ignore_ascii_case("filename*") {
            let mut fields = value.splitn(3, '\'');
            if let (Some(charset), Some(_language), Some(encoded)) =
                (fields.next(), fields.next(), fields.next())
                && charset.eq_ignore_ascii_case("UTF-8")
            {
                extended = percent_decode(encoded);
            }
        } else if key.trim().eq_ignore_ascii_case("filename") {
            fallback = Some(value.to_owned());
        }
    }
    extended.filter(|s| !s.is_empty()).or(fallback)
}

/// Shared URL suggestion for the desktop dialog and browser API. Server headers
/// can replace this suggestion after redirects; explicit user edits are retained.
pub fn filename_from_url(url: &str) -> String {
    if is_mail_export(url) {
        let count = reqwest::Url::parse(url)
            .unwrap()
            .query_pairs()
            .filter(|(key, _)| key == "m")
            .count();
        return if count == 1 {
            "message.eml"
        } else {
            "messages.zip"
        }
        .into();
    }
    if let Some(name) = super::media::default_filename(url) {
        return name;
    }
    let Ok(url) = reqwest::Url::parse(url) else {
        return "download".into();
    };
    for (key, value) in url.query_pairs() {
        if matches!(
            key.to_ascii_lowercase().as_str(),
            "filename" | "file_name" | "download"
        ) && value.contains('.')
        {
            return super::types::safe_filename(&value);
        }
    }
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .and_then(percent_decode)
        .map(|name| super::types::safe_filename(&name))
        .unwrap_or_else(|| "download".into())
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            decoded.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generated_get_export_is_not_replayed_after_a_capacity_close() {
        for redirect in [None, Some(301), Some(302), Some(303)] {
            single_use_export_case(redirect, false).await;
        }
        single_use_export_case(None, true).await;
    }

    async fn single_use_export_case(redirect: Option<u16>, reuse: bool) {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = if reuse {
            format!("http://mail.hostinger.com:{}", address.port())
        } else {
            format!("http://{address}")
        };
        let exports = Arc::new(AtomicUsize::new(0));
        let seen = exports.clone();
        let submissions = Arc::new(AtomicUsize::new(0));
        let submitted = submissions.clone();
        let (release, released) = tokio::sync::watch::channel(false);
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut released = released.clone();
                let seen = seen.clone();
                let submitted = submitted.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                        let count = socket.read(&mut buffer).await.unwrap();
                        assert!(count > 0 && request.len() < 8192);
                        request.extend_from_slice(&buffer[..count]);
                    }
                    if request.starts_with(b"GET /healthy ") {
                        socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n")
                            .await
                            .unwrap();
                        released.wait_for(|value| *value).await.unwrap();
                        socket.write_all(b"data").await.unwrap();
                    } else if request.starts_with(b"POST /submit ") {
                        let header_end =
                            request.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
                        let length = std::str::from_utf8(&request[..header_end])
                            .unwrap()
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        while request.len() < header_end + length {
                            let count = socket.read(&mut buffer).await.unwrap();
                            assert!(count > 0);
                            request.extend_from_slice(&buffer[..count]);
                        }
                        assert_eq!(&request[header_end..header_end + length], b"token=once");
                        submitted.fetch_add(1, Ordering::Relaxed);
                        socket.write_all(format!("HTTP/1.1 {} Redirect\r\nLocation: /export\r\nContent-Length: 0\r\n\r\n", redirect.unwrap()).as_bytes()).await.unwrap();
                    } else if request.starts_with(b"GET /export ")
                        || request.starts_with(
                            b"GET /bridge/api/v3/mailboxes/test/messages/sources/export ",
                        )
                    {
                        assert!(
                            !String::from_utf8_lossy(&request)
                                .lines()
                                .any(|line| line.starts_with("range:"))
                        );
                        seen.fetch_add(1, Ordering::Relaxed);
                    } else {
                        assert!(request.starts_with(b"GET /prime "));
                    }
                });
            }
        });
        let context = RequestContext::new(
            Arc::new(
                super::super::http_client_builder()
                    .no_proxy()
                    .resolve("mail.hostinger.com", address)
                    .build()
                    .unwrap(),
            ),
            &base,
            reqwest::header::HeaderMap::new(),
        )
        .unwrap();
        let LeasedResponse { response, permit } = context
            .send(Spec::get(&format!("{base}/healthy")), Control::default())
            .await
            .unwrap()
            .leased;
        assert!(
            context
                .send(Spec::get(&format!("{base}/prime")), Control::default())
                .await
                .is_err()
        );
        let export_url = if reuse {
            format!("{base}/bridge/api/v3/mailboxes/test/messages/sources/export")
        } else {
            format!("{base}/export")
        };
        let submit_url = format!("{base}/submit");
        let result = tokio::time::timeout(std::time::Duration::from_millis(250), async {
            if redirect.is_some() {
                post_probe(&context, &submit_url, "token=once".into()).await
            } else if reuse {
                probe_with_initial_range(&context, &export_url).await
            } else {
                streaming_probe(&context, &export_url).await
            }
        })
        .await;
        let attempts = exports.load(Ordering::Relaxed);
        release.send(true).unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"data");
        drop(permit);
        server.abort();
        assert!(
            result.is_ok(),
            "A generated GET export must return its first failure without capacity retries; attempts={attempts}"
        );
        assert!(result.unwrap().is_err());
        assert_eq!(attempts, 1);
        assert_eq!(
            submissions.load(Ordering::Relaxed),
            usize::from(redirect.is_some())
        );
    }
    fn context(client: reqwest::Client, url: &str) -> RequestContext {
        RequestContext::new(
            std::sync::Arc::new(client),
            url,
            reqwest::header::HeaderMap::new(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn one_get_probe_validates_exact_bytes_and_reuses_fallback_streams() {
        use axum::{Router, body::Body, http::Method, response::IntoResponse, routing::get};
        use std::sync::{Arc, Mutex};
        for mode in 0..5 {
            let requests = Arc::new(Mutex::new(Vec::new()));
            let seen = requests.clone();
            let router = Router::new().route(
                "/file.bin",
                get(move |method: Method, headers: reqwest::header::HeaderMap| {
                    let seen = seen.clone();
                    async move {
                        let range = headers
                            .get("range")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_owned);
                        let count = {
                            let mut seen = seen.lock().unwrap();
                            seen.push((method.clone(), range.clone()));
                            seen.len()
                        };
                        if mode == 4 {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        if method == Method::HEAD {
                            return (
                                [("content-length", "8192"), ("accept-ranges", "bytes")],
                                Body::empty(),
                            )
                                .into_response();
                        }
                        if range.is_some() {
                            let bytes = match mode {
                                1 => "bad",
                                2 => "xx",
                                3 if count == 1 => "",
                                _ => "x",
                            };
                            return (
                                StatusCode::PARTIAL_CONTENT,
                                [
                                    (
                                        "content-range",
                                        if mode == 1 {
                                            "bytes 0-2/8192"
                                        } else {
                                            "bytes 0-0/8192"
                                        },
                                    ),
                                    ("etag", "\"fixed\""),
                                    ("content-type", "application/octet-stream"),
                                ],
                                bytes,
                            )
                                .into_response();
                        }
                        (
                            [("content-type", "application/octet-stream")],
                            vec![b'x'; 8192],
                        )
                            .into_response()
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let result = probe(&context(super::super::http_client().unwrap(), &url), &url).await;
            if mode == 4 {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("unexpected probe status")
                );
                server.abort();
                continue;
            }
            let result = result.unwrap();
            assert_eq!(result.content_length, Some(8192));
            let ranged = (Method::GET, Some("bytes=0-0".to_owned()));
            if mode == 0 || mode == 3 {
                assert!(result.supports_multipart());
                assert!(result.response.is_none());
                assert_eq!(result.if_range.as_deref(), Some("\"fixed\""));
                assert_eq!(
                    *requests.lock().unwrap(),
                    vec![ranged; if mode == 3 { 2 } else { 1 }]
                );
            } else {
                assert!(!result.supports_multipart());
                assert_eq!(
                    result
                        .response
                        .unwrap()
                        .response
                        .bytes()
                        .await
                        .unwrap()
                        .as_ref(),
                    vec![b'x'; 8192]
                );
                assert_eq!(*requests.lock().unwrap(), vec![ranged, (Method::GET, None)]);
            }
            server.abort();
        }
    }

    #[tokio::test]
    async fn ambiguous_initial_lengths_preserve_decoded_files_and_chunked_ranges() {
        use axum::{Router, body::Body, response::IntoResponse, routing::get};
        use std::sync::{Arc, Mutex};
        // gzip.compress(payload, mtime=0), kept inline to require no external tool.
        const ENCODED: &[u8] = &[
            31, 139, 8, 0, 0, 0, 0, 0, 2, 10, 237, 199, 193, 9, 128, 48, 12, 64, 209, 187, 83, 100,
            1, 215, 16, 143, 30, 186, 64, 104, 3, 6, 52, 45, 53, 224, 250, 238, 33, 255, 221, 222,
            230, 151, 201, 235, 121, 138, 134, 88, 212, 222, 172, 201, 94, 202, 33, 211, 198, 180,
            199, 34, 53, 189, 199, 226, 247, 232, 51, 53, 114, 109, 154, 74, 41, 165, 148, 82, 74,
            41, 165, 148, 254, 179, 31, 230, 128, 204, 67, 41, 15, 0, 0,
        ];
        let mut payload = b"File with an encoded HTTP representation\n".to_vec();
        for _ in 0..256 {
            payload.extend_from_slice(b"important-data\n");
        }
        for compressed in [false, true] {
            let requests = Arc::new(Mutex::new(Vec::new()));
            let observed = requests.clone();
            let router = Router::new().route(
                "/file.bin",
                get(move |request: reqwest::header::HeaderMap| {
                    let observed = observed.clone();
                    async move {
                        let body: &[u8] = if compressed {
                            ENCODED
                        } else {
                            b"chunked static file"
                        };
                        let range = request.get("range").map(|v| v.to_str().unwrap().to_owned());
                        observed.lock().unwrap().push(range.clone());
                        let mut headers = reqwest::header::HeaderMap::new();
                        let data = if let Some(value) = &range {
                            let (start, end) = value
                                .strip_prefix("bytes=")
                                .unwrap()
                                .split_once('-')
                                .unwrap();
                            let start: usize = start.parse().unwrap();
                            let end = end.parse::<usize>().unwrap().min(body.len() - 1);
                            headers.insert(
                                "content-range",
                                format!("bytes {start}-{end}/{}", body.len())
                                    .parse()
                                    .unwrap(),
                            );
                            body[start..=end].to_vec()
                        } else {
                            body.to_vec()
                        };
                        if compressed {
                            headers.insert("content-encoding", "gzip".parse().unwrap());
                            headers
                                .insert("content-length", data.len().to_string().parse().unwrap());
                        }
                        let stream = futures::stream::once(async move {
                            Ok::<_, std::io::Error>(bytes::Bytes::from(data))
                        });
                        (
                            if range.is_some() {
                                StatusCode::PARTIAL_CONTENT
                            } else {
                                StatusCode::OK
                            },
                            headers,
                            Body::from_stream(stream),
                        )
                            .into_response()
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let result = probe_with_initial_range(
                &context(super::super::http_client().unwrap(), &url),
                &url,
            )
            .await
            .unwrap();
            assert!(
                result.initial_range_end.is_none(),
                "Ambiguous decoded lengths cannot be treated as retained partial data"
            );
            if compressed {
                assert!(!result.accepts_ranges);
                assert_eq!(result.content_length, None);
                assert_eq!(
                    result
                        .response
                        .unwrap()
                        .response
                        .bytes()
                        .await
                        .unwrap()
                        .as_ref(),
                    payload.as_slice()
                );
                assert_eq!(requests.lock().unwrap().last(), Some(&None));
            } else {
                assert!(
                    result.accepts_ranges,
                    "A body-validated chunked source keeps parallel range support"
                );
                assert_eq!(
                    result.content_length,
                    Some(b"chunked static file".len() as u64)
                );
                assert!(result.response.is_none());
                assert_eq!(
                    *requests.lock().unwrap(),
                    vec![Some("bytes=0-2097151".into()), Some("bytes=0-0".into())]
                );
            }
            server.abort();
        }
    }

    #[tokio::test]
    async fn inconclusive_prefix_preserves_servers_with_short_range_support() {
        use axum::{Router, response::IntoResponse, routing::get};
        use std::sync::{Arc, Mutex};
        for status in [
            206, 304, 400, 401, 403, 404, 405, 408, 413, 416, 429, 500, 501, 502, 503, 504,
        ] {
            let requests = Arc::new(Mutex::new(Vec::new()));
            let observed = requests.clone();
            let router = Router::new().route(
                "/file.bin",
                get(move |headers: reqwest::header::HeaderMap| {
                    let observed = observed.clone();
                    async move {
                        let range = headers.get("range").unwrap().to_str().unwrap();
                        observed.lock().unwrap().push(range.to_owned());
                        if range == "bytes=0-2097151" {
                            if status == 206 {
                                return (
                                    StatusCode::PARTIAL_CONTENT,
                                    [("content-range", "bytes 0-524287/4194304")],
                                    vec![b'x'; 512 * 1024],
                                )
                                    .into_response();
                            }
                            return StatusCode::from_u16(status).unwrap().into_response();
                        }
                        assert_eq!(range, "bytes=0-0");
                        (
                            StatusCode::PARTIAL_CONTENT,
                            [("content-range", "bytes 0-0/4194304")],
                            "x",
                        )
                            .into_response()
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let result = probe_with_initial_range(
                &context(super::super::http_client().unwrap(), &url),
                &url,
            )
            .await
            .unwrap();
            assert!(
                result.supports_multipart(),
                "A rejected/inconclusive larger prefix must preserve proven short ranges: {status}"
            );
            assert_eq!(result.content_length, Some(4 * 1024 * 1024));
            assert!(result.initial_range_end.is_none());
            assert!(result.response.is_none());
            assert_eq!(*requests.lock().unwrap(), ["bytes=0-2097151", "bytes=0-0"]);
            server.abort();
        }
    }

    #[tokio::test]
    async fn queued_server_capacity_does_not_consume_the_probe_deadline() {
        use axum::{Router, routing::get};
        let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = requests.clone();
        let router = Router::new().route(
            "/file.bin",
            get(move |headers: reqwest::header::HeaderMap| {
                seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async move {
                    if headers.get("range").is_some_and(|v| v == "bytes=0-0") {
                        (
                            StatusCode::PARTIAL_CONTENT,
                            [("content-range", "bytes 0-0/8192")],
                            "x",
                        )
                    } else {
                        (
                            StatusCode::OK,
                            [("content-type", "application/octet-stream")],
                            "unexpected full fallback",
                        )
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let budget = HostBudget::for_url(&url);
        let held = budget.acquire().await;
        held.refused(&url);
        let client = context(super::super::http_client().unwrap(), &url);
        let probing = initial_probe(&client, &url, std::time::Duration::from_millis(50));
        tokio::pin!(probing);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(400), &mut probing)
                .await
                .is_err()
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 0);
        drop(held);
        let (_, verified) = tokio::time::timeout(std::time::Duration::from_secs(1), probing)
            .await
            .unwrap()
            .unwrap();
        assert!(
            verified,
            "Queued time must not exhaust the verified-range retry budget"
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 1);
        server.abort();
    }

    #[tokio::test]
    async fn retained_partial_headers_are_strict_and_do_not_time_the_file_body() {
        use axum::{Router, body::Body, response::IntoResponse, routing::get};
        use std::sync::{Arc, Mutex};
        for mode in 0..8 {
            let requests = Arc::new(Mutex::new(Vec::new()));
            let observed = requests.clone();
            let router = Router::new().route(
                "/file.bin",
                get(move |headers: reqwest::header::HeaderMap| {
                    let observed = observed.clone();
                    async move {
                        let range = headers.get("range").map(|v| v.to_str().unwrap().to_owned());
                        observed.lock().unwrap().push(range.clone());
                        if range.is_none() || mode == 3 {
                            return (StatusCode::OK, "complete").into_response();
                        }
                        assert!(matches!(
                            range.as_deref(),
                            Some("bytes=0-2097151" | "bytes=0-0")
                        ));
                        let mut headers = reqwest::header::HeaderMap::new();
                        if mode != 6 {
                            headers.insert(
                                "content-range",
                                match mode {
                                    1 => "bytes 1-7/8",
                                    5 => "bytes 0-0/0",
                                    7 => "bytes 0-8/8",
                                    _ => "bytes 0-7/8",
                                }
                                .parse()
                                .unwrap(),
                            );
                        }
                        headers.insert(
                            "content-length",
                            if mode == 2 { "9" } else { "8" }.parse().unwrap(),
                        );
                        let stream = futures::stream::once(async move {
                            if mode == 4 {
                                tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                            } else if mode == 2 {
                                // Make the invalid declared length visible in headers
                                // before Hyper detects the deliberately short body.
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                            Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"complete"))
                        });
                        (
                            StatusCode::PARTIAL_CONTENT,
                            headers,
                            Body::from_stream(stream),
                        )
                            .into_response()
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let client = context(super::super::http_client().unwrap(), &url);
            let invalid = !matches!(mode, 0 | 3 | 4);
            let (response, verified) = if invalid {
                let result = probe_with_initial_range(&client, &url).await.unwrap();
                assert!(result.initial_range_end.is_none());
                (result.response.unwrap(), result.accepts_ranges)
            } else {
                initial_probe_impl(&client, &url, std::time::Duration::from_millis(250), true)
                    .await
                    .unwrap()
            };
            assert_eq!(verified, mode == 0 || mode == 4);
            assert_eq!(
                response.response.bytes().await.unwrap().as_ref(),
                b"complete"
            );
            let actual = requests.lock().unwrap();
            assert_eq!(actual.len(), if invalid { 3 } else { 1 });
            if invalid {
                assert_eq!(
                    actual[1],
                    Some("bytes=0-0".into()),
                    "An inconclusive prefix must preserve the original body proof"
                );
                assert_eq!(
                    actual[2], None,
                    "Invalid legacy metadata must fall back to an un-ranged full stream"
                );
            }
            server.abort();
        }
    }

    #[tokio::test]
    async fn reused_full_stream_outlives_the_probe_deadline_without_replay() {
        use axum::{Router, body::Body, routing::get};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let router = Router::new().route(
            "/stream.bin",
            get(move || {
                seen.fetch_add(1, Ordering::Relaxed);
                async {
                    let stream = futures::stream::once(async {
                        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"complete"))
                    });
                    (
                        [
                            ("content-type", "application/octet-stream"),
                            ("content-length", "8"),
                        ],
                        Body::from_stream(stream),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/stream.bin", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let (response, accepts_ranges) = initial_probe(
            &context(super::super::http_client().unwrap(), &url),
            &url,
            std::time::Duration::from_millis(250),
        )
        .await
        .unwrap();
        assert!(!accepts_ranges);
        assert_eq!(
            response.response.bytes().await.unwrap().as_ref(),
            b"complete"
        );
        assert_eq!(requests.load(Ordering::Relaxed), 1);
        server.abort();
    }
    #[tokio::test]
    async fn token_only_urls_fail_before_sending_a_probe() {
        let error = probe(
            &context(
                super::super::http_client().unwrap(),
                "https://server53.safefilehosting.com/files/download.php",
            ),
            "https://server53.safefilehosting.com/files/download.php",
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("fresh website token"));
        assert!(!is_token_download(
            "https://server53.safefilehosting.com.evil.test/files/download.php"
        ));
        assert!(!is_token_download("https://example.com/files/download.php"));
    }

    #[test]
    fn header_names_and_url_encoding() {
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename=Anti-Youtube-Ads-Extension-main.zip"
            ),
            Some("Anti-Youtube-Ads-Extension-main.zip".into())
        );
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename=backup.zip; filename*=UTF-8'en'%D9%85%D9%84%D9%81.zip"
            ),
            Some("ملف.zip".into())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"a;b.zip\""),
            Some("a;b.zip".into())
        );
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename*=UTF-8''%FF; filename=backup.zip"
            ),
            Some("backup.zip".into())
        );
        assert_eq!(
            filename_from_url("https://example.com/my%20file.zip#section"),
            "my file.zip"
        );
        assert_eq!(
            filename_from_url("https://example.com/get?filename=report%20final.pdf"),
            "report final.pdf"
        );
        assert_eq!(percent_decode("bad%QZ.zip").unwrap(), "bad%QZ.zip");
    }
}
