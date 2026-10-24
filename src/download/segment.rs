//! Segment worker – downloads a single byte range.
//!
//! Each worker issues one `GET` request with a `Range: bytes=X-Y` header,
//! streams the response body, and writes each chunk directly into the
//! pre-allocated output file at the correct byte offset.
//!
//! # Resume support
//! The worker tracks its current write cursor (`resume_at`).  On a retry it
//! resumes from `resume_at` rather than from `start`, so a partially-completed
//! segment does not re-download already-written bytes.
//!
//! # Evidence from IDMan.exe
//! - `"Range: bytes=%I64d-%I64d"` – parallel segment range pattern  
//! - `"Connection is restarted..."` – per-segment retry logic

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt},
    sync::Mutex,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Arguments passed to a single segment worker.
pub struct SegmentArgs {
    pub bandwidth: Arc<super::bandwidth::BandwidthLimiter>,
    /// A GET already opened by the probe; consume it instead of requesting again.
    pub response: Option<reqwest::Response>,
    pub client: Arc<Client>,
    /// Segment index (used in error messages).
    pub id: usize,
    /// URL to request (already resolved after redirects).
    pub url: String,
    /// First byte of this segment (inclusive).
    pub start: u64,
    /// Last byte of this segment (inclusive), or `None` to download to EOF.
    pub end: Option<u64>,
    /// Shared handle to the pre-allocated output file.
    pub file: Arc<Mutex<File>>,
    /// When set to `true` by the UI, the worker exits cleanly.
    pub cancel: Arc<AtomicBool>,
    /// When set to `true` by the UI, the worker spins until cleared.
    pub pause: Arc<AtomicBool>,
    /// Called after every chunk with the number of bytes just written.
    pub on_progress: Box<dyn Fn(u64) + Send + Sync + 'static>,
}

// ---------------------------------------------------------------------------
// Public function
// ---------------------------------------------------------------------------

/// Run a segment worker to completion, retrying up to `max_retries` times.
///
/// # Errors
/// Returns an error after `max_retries` failed attempts, or immediately if
/// the segment is cancelled.
pub async fn run_segment(mut args: SegmentArgs, max_retries: u32) -> Result<()> {
    let mut attempts = 0u32;
    // `resume_at` advances after each successful chunk write, enabling
    // within-segment resume across retries.
    let mut resume_at = args.start;

    loop {
        let response = args.response.take();
        let streaming_export = super::probe::is_mail_export(&args.url) || response.is_some();
        let result = download_range(&args, resume_at, &mut resume_at, response).await;

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Generated exports cannot be resumed by byte offset or replayed
                // reliably. Report the original error instead of appending a new export.
                if streaming_export {
                    return Err(e.context(
                        "Download stream interrupted; request a fresh download from the browser",
                    ));
                }
                if args.cancel.load(Ordering::Relaxed) {
                    bail!("segment {} cancelled", args.id);
                }
                attempts += 1;
                if attempts > max_retries {
                    bail!(
                        "segment {} failed after {} retries: {}",
                        args.id,
                        attempts,
                        e
                    );
                }
                // Exponential back-off: 300ms, 600ms, 900ms, …
                let backoff = Duration::from_millis(300 * u64::from(attempts));
                eprintln!(
                    "[IDG] segment {} retry {}/{} after {}ms: {}",
                    args.id,
                    attempts,
                    max_retries,
                    backoff.as_millis(),
                    e
                );
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

/// Issue a single `GET Range: bytes=X-Y` request and stream the body to disk.
///
/// `cursor` is updated as bytes are written so the caller can resume
/// from the correct offset on the next attempt.
async fn download_range(
    args: &SegmentArgs,
    start: u64,
    cursor: &mut u64,
    response: Option<reqwest::Response>,
) -> Result<()> {
    let SegmentArgs {
        client,
        url,
        end,
        file,
        cancel,
        pause,
        on_progress,
        ..
    } = args;
    let end = *end;
    // Build the Range header value.
    let range = match end {
        Some(e) => format!("bytes={start}-{e}"),
        None => format!("bytes={start}-"),
    };

    let request = client.get(url).header("Accept-Encoding", "identity");
    let request = if end.is_some() || start > 0 {
        request.header("Range", &range)
    } else {
        request
    };
    let resp = match response {
        Some(response) => response,
        None => request.send().await.context("GET request failed")?,
    };

    if !resp.status().is_success() {
        bail!("server returned HTTP {}", resp.status());
    }

    if (end.is_some() || start > 0) && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        bail!("Server ignored the requested byte range");
    }
    let mut stream = resp.bytes_stream();
    let mut buffered = Vec::with_capacity(256 * 1024);
    let mut last_write = std::time::Instant::now();

    while let Some(chunk_result) = stream.next().await {
        // Check cancel / pause before processing each chunk.
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        while pause.load(Ordering::Relaxed) {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled during pause");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let chunk: Bytes = chunk_result.context("stream read error")?;
        let rate = args.bandwidth.rate();
        let quantum = if rate == 0 {
            256 * 1024
        } else {
            (rate / 20).clamp(1, 64 * 1024) as usize
        };
        for part in chunk.chunks(quantum) {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let remaining = end.map_or(part.len(), |end| {
                (end + 1)
                    .saturating_sub(*cursor + buffered.len() as u64)
                    .min(part.len() as u64) as usize
            });
            let part = &part[..remaining];
            if rate > 0 {
                let permit = args.bandwidth.acquire(part.len());
                tokio::pin!(permit);
                loop {
                    tokio::select! {
                        _ = &mut permit => break,
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {
                            if cancel.load(Ordering::Relaxed) { bail!("cancelled while speed limited"); }
                        }
                    }
                }
            }
            while pause.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    bail!("cancelled during pause");
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            buffered.extend_from_slice(part);
            let finished = end.is_some_and(|end| *cursor + buffered.len() as u64 > end);
            if buffered.len() >= 256 * 1024
                || last_write.elapsed() >= Duration::from_millis(50)
                || finished
            {
                write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                last_write = std::time::Instant::now();
            }
            if finished {
                break;
            }
        }
        if end.is_some_and(|end| *cursor > end) {
            break;
        }
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;

    if end.is_some_and(|end| *cursor <= end) {
        bail!("Response ended before the requested range was complete");
    }
    Ok(())
}

// Coalesce small network packets before taking the shared seek/write lock.
async fn write_buffer(
    file: &Arc<Mutex<File>>,
    cursor: &mut u64,
    buffered: &mut Vec<u8>,
    on_progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<()> {
    if buffered.is_empty() {
        return Ok(());
    }
    let count = buffered.len() as u64;
    {
        let mut file = file.lock().await;
        file.seek(std::io::SeekFrom::Start(*cursor))
            .await
            .context("seek failed")?;
        file.write_all(buffered).await.context("write failed")?;
    }
    *cursor += count;
    buffered.clear();
    on_progress(count);
    Ok(())
}
