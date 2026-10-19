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
    let resp = client
        .head(url)
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
            .header("Range", "bytes=0-0")
            .send()
            .await
            .context("fallback GET probe failed")?
    } else {
        resp
    };

    let final_url = resp.url().to_string();
    let headers = resp.headers().clone();

    let accepts_ranges = headers
        .get("accept-ranges")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("bytes"))
        .unwrap_or(false);

    let content_length = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());

    let filename = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_content_disposition)
        .or_else(|| filename_from_url(&final_url));

    Ok(ProbeResult {
        content_length,
        accepts_ranges,
        filename,
        final_url,
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract a filename from a `Content-Disposition` header value.
///
/// Handles both `filename="foo.zip"` and `filename*=UTF-8''foo%20bar.zip`.
fn filename_from_content_disposition(cd: &str) -> Option<String> {
    for part in cd.split(';') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();

        if lower.starts_with("filename*=") {
            // RFC 5987 extended value: filename*=UTF-8''encoded%20name.zip
            let val = part.splitn(2, '=').nth(1)?;
            let decoded = percent_decode(val.trim().trim_matches('\''));
            if !decoded.is_empty() {
                return Some(decoded);
            }
        } else if lower.starts_with("filename=") {
            let val = part.splitn(2, '=').nth(1)?;
            let clean = val.trim().trim_matches('"').trim_matches('\'');
            if !clean.is_empty() {
                return Some(clean.to_owned());
            }
        }
    }
    None
}

/// Extract the last path segment from a URL as a filename.
fn filename_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    let last = path.rsplit('/').next()?;
    if last.contains('.') && !last.is_empty() {
        Some(last.to_owned())
    } else {
        None
    }
}

/// Minimal percent-decoder for `Content-Disposition` filenames.
fn percent_decode(s: &str) -> String {
    // Strip the charset prefix "UTF-8''" if present.
    let s = if let Some(pos) = s.find("''") {
        &s[pos + 2..]
    } else {
        s
    };
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hi = chars.next().unwrap_or('0');
            let lo = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{hi}{lo}"), 16) {
                out.push(b as char);
                continue;
            }
        }
        out.push(c);
    }
    out
}
