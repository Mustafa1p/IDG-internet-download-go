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

use anyhow::{bail, Context, Result};
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
pub async fn run_segment(args: SegmentArgs, max_retries: u32) -> Result<()> {
    // Each segment builds its own lightweight client so the connection pool
    // is not shared with other segments (mirrors IDM's per-connection model).
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(60))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client for segment")?;

    let mut attempts = 0u32;
    // `resume_at` advances after each successful chunk write, enabling
    // within-segment resume across retries.
    let mut resume_at = args.start;

    loop {
        let result = download_range(
            &client,
            &args.url,
            resume_at,
            args.end,
            &args.file,
            &args.cancel,
            &args.pause,
            &args.on_progress,
            &mut resume_at,
        )
        .await;

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                if args.cancel.load(Ordering::Relaxed) {
                    bail!("segment {} cancelled", args.id);
                }
                attempts += 1;
                if attempts > max_retries {
                    bail!(
                        "segment {} failed after {} retries: {}",
                        args.id, attempts, e
                    );
                }
                // Exponential back-off: 300ms, 600ms, 900ms, …
                let backoff = Duration::from_millis(300 * u64::from(attempts));
                eprintln!(
                    "[IDG] segment {} retry {}/{} after {}ms: {}",
                    args.id, attempts, max_retries,
                    backoff.as_millis(), e
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
    client: &Client,
    url: &str,
    start: u64,
    end: Option<u64>,
    file: &Arc<Mutex<File>>,
    cancel: &AtomicBool,
    pause: &AtomicBool,
    on_progress: &(dyn Fn(u64) + Send + Sync),
    cursor: &mut u64,
) -> Result<()> {
    // Build the Range header value.
    let range = match end {
        Some(e) => format!("bytes={start}-{e}"),
        None    => format!("bytes={start}-"),
    };

    let resp = client
        .get(url)
        .header("Range", &range)
        .send()
        .await
        .context("GET request failed")?;

    if !resp.status().is_success() {
        bail!("server returned HTTP {}", resp.status());
    }

    let mut stream = resp.bytes_stream();

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
        let chunk_len = chunk.len() as u64;

        // Seek to the current write position and flush the chunk.
        {
            let mut f = file.lock().await;
            f.seek(std::io::SeekFrom::Start(*cursor))
                .await
                .context("seek failed")?;
            f.write_all(&chunk).await.context("write failed")?;
        }

        *cursor += chunk_len;
        on_progress(chunk_len);

        // Stop consuming once we have written past the segment boundary.
        if let Some(e) = end {
            if *cursor > e {
                break;
            }
        }
    }

    Ok(())
}
