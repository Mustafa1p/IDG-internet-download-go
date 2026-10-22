//! URL probe – discovers file metadata before downloading.
//!
//! Sends a `HEAD` request to determine:
//! - `Content-Length` – total file size in bytes
//! - `Accept-Ranges: bytes` – whether the server supports parallel range requests
//! - Suggested filename from `Content-Disposition` or the URL path
//!
//! Falls back to a `GET` with `Range: bytes=0-0` if the server rejects `HEAD`.
//!
//! # Evidence from IDMan.exe
//! The string `"Range: bytes=0-"` in IDMan.exe confirms IDM uses this exact
//! fallback probe pattern.

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of probing a URL before starting a download.
#[derive(Debug)]
pub struct ProbeResult {
    /// Reuse a streaming GET when the endpoint cannot be probed safely.
    pub response: Option<reqwest::Response>,
    /// File size in bytes, if the server reported `Content-Length`.
    pub content_length: Option<u64>,
    /// `true` when the server sent `Accept-Ranges: bytes`.
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

// ---------------------------------------------------------------------------
// Public function
// ---------------------------------------------------------------------------

/// Probe `url` and return metadata needed to plan the download.
///
/// # Errors
/// Returns an error if the network request fails or the server returns a
/// non-2xx status on both `HEAD` and the fallback `GET`.
pub async fn probe(client: &Client, url: &str) -> Result<ProbeResult> {
    if is_mail_export(url) {
        return streaming_probe(client, url).await;
    }
    let resp = client
        .head(url)
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .context("HEAD request failed")?;

    // Some servers (nginx, CDNs) reject HEAD – fall back to a zero-byte GET.
    let resp = if matches!(
        resp.status(),
        StatusCode::METHOD_NOT_ALLOWED
            | StatusCode::NOT_IMPLEMENTED
            | StatusCode::BAD_REQUEST
            | StatusCode::FORBIDDEN
    ) {
        client
            .get(url)
            .header("Accept-Encoding", "identity")
            .header("Range", "bytes=0-0")
            .send()
            .await
            .context("fallback GET probe failed")?
    } else {
        resp
    };

    let resp = resp
        .error_for_status()
        .context("Server rejected download probe")?;
    let final_url = resp.url().to_string();
    let headers = resp.headers().clone();

    let mut accepts_ranges = headers
        .get("accept-ranges")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("bytes"))
        .unwrap_or(false);

    let content_length = headers
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('/').next())
        .and_then(|v| v.parse::<u64>().ok())
        .or_else(|| {
            headers
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
        });

    // Verify range semantics before spawning workers; advertised support alone
    // can be incorrect on CDNs/proxies. Never write a full response at a range offset.
    if let Some(size) = content_length.filter(|size| *size > 0) {
        if resp.status() == StatusCode::PARTIAL_CONTENT {
            accepts_ranges = headers
                .get("content-range")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.trim() == format!("bytes 0-0/{size}"));
        } else if accepts_ranges {
            accepts_ranges = match client
                .get(&final_url)
                .header("Accept-Encoding", "identity")
                .header("Range", "bytes=0-0")
                .send()
                .await
            {
                Ok(response) => {
                    response.status() == StatusCode::PARTIAL_CONTENT
                        && response
                            .headers()
                            .get("content-range")
                            .and_then(|value| value.to_str().ok())
                            .is_some_and(|value| value.trim() == format!("bytes 0-0/{size}"))
                }
                Err(_) => false,
            };
        }
    }

    let filename = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_content_disposition)
        .or_else(|| Some(filename_from_url(&final_url)));

    Ok(ProbeResult {
        response: None,
        content_length,
        accepts_ranges,
        filename,
        final_url,
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

async fn streaming_probe(client: &Client, url: &str) -> Result<ProbeResult> {
    let response = client
        .get(url)
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .context("Email export request failed")?;
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
        content_length: response.content_length(),
        accepts_ranges: false,
        filename: Some(filename),
        final_url,
        response: Some(response),
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
