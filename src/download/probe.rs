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
    /// Retain learned capacity while this download schedules its range workers.
    pub host_budget: std::sync::Arc<HostBudget>,
    /// File size in bytes, if the server reported `Content-Length`.
    pub content_length: Option<u64>,
    /// `true` after validating a complete one-byte partial response.
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
    anyhow::ensure!(
        !is_token_download(url),
        "This download requires a fresh website token. Open the source page and click its download button with the updated IDG browser extension enabled; the download.php URL alone cannot download the file."
    );
    if is_mail_export(url) {
        return streaming_probe(client, url).await;
    }
    // One request establishes metadata and verifies real range semantics. A
    // separate HEAD adds an avoidable round trip and can disagree with GET.
    let (leased, accepts_ranges) =
        initial_probe(client, url, std::time::Duration::from_secs(5)).await?;
    let LeasedResponse {
        response: resp,
        permit,
    } = leased;
    let reuse_response = !accepts_ranges;
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

    let content_length = if accepts_ranges {
        range_probe_size(&headers)
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
    for attempt in 0..3 {
        let probing = async {
            let received = client
                .send(
                    Spec::get(url).header("range", "bytes=0-0")?,
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
        .send(Spec::get(url), Control::default())
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
