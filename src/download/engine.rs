//! Download orchestrator – coordinates probe, pre-allocation, and segments.
//!
//! # Algorithm
//!
//! ```text
//! 1. HEAD probe  →  Content-Length + Accept-Ranges
//! 2. Pre-allocate the output file (avoids filesystem fragmentation)
//! 3. Divide the file into N equal byte ranges
//! 4. Spawn N async tasks, each issuing Range: bytes=X-Y
//! 5. All tasks write into the same file at their own offsets (no copy needed)
//! 6. Wait for all tasks; mark Completed or Failed
//! ```
//!
//! # Evidence from IDMan.exe
//! - `"Range: bytes=%I64d-%I64d"` – multi-segment range format  
//! - `"ConnectionsNumber"` – configurable parallel connection count  
//! - `"Connection is restarted..."` – retry / resume behaviour

use std::sync::Arc;

use anyhow::{Context, Result};
use futures::future::join_all;
use tokio::{fs::OpenOptions, sync::Mutex};

use crate::download::{
    connections::choose_count,
    probe::probe,
    segment::{SegmentArgs, run_segment},
    types::{Downloads, Status, safe_filename, with_item},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum per-segment retry attempts before giving up.
const MAX_RETRIES: u32 = 5;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Start downloading the item identified by `id` from the shared `downloads` list.
///
/// This function is designed to be spawned as a Tokio task.  It updates the
/// shared [`DownloadItem`](crate::download::types::DownloadItem) in-place as
/// the download progresses so the GUI can reflect live state.
///
/// # Errors
/// Returns an error if the probe or file pre-allocation fails.  Segment errors
/// that exceed `MAX_RETRIES` are also propagated.
pub async fn start_download(
    client: Arc<reqwest::Client>,
    downloads: Downloads,
    id: String,
) -> Result<()> {
    // ── Read the configuration from shared state ─────────────────────────────
    let (url, save_path, num_segments, cancel, pause, headers) = {
        let guard = downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let item = guard
            .iter()
            .find(|i| i.id == id)
            .context("download item not found in queue")?;
        (
            item.url.clone(),
            item.save_path.clone(),
            item.num_segments,
            item.cancel_flag.clone(),
            item.pause_flag.clone(),
            item.headers.clone(),
        )
    };

    if super::media::video_url(&url).is_some() {
        return super::media::download(downloads, id).await;
    }

    // ── Step 1: Probe ─────────────────────────────────────────────────────────
    let client = if headers.is_empty() {
        client
    } else {
        Arc::new(
            reqwest::Client::builder()
                .default_headers(headers)
                .connect_timeout(std::time::Duration::from_secs(15))
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
        )
    };
    with_item(&downloads, &id, |i| i.status = Status::Probing);

    let probe_result = probe(&client, &url).await.context("URL probe failed")?;

    // Persist probe results; update the save path if the server suggested a
    // different filename via Content-Disposition.
    with_item(&downloads, &id, |i| {
        i.total_bytes = probe_result.content_length;
        if let Some(ref server_name) = probe_result.filename
            && i.filename == "download"
        {
            let dir = i.save_path.parent().unwrap_or(std::path::Path::new("."));
            i.save_path = dir.join(safe_filename(server_name));
            i.filename = safe_filename(server_name);
        }
        i.status = if pause.load(std::sync::atomic::Ordering::Relaxed) {
            Status::Paused
        } else {
            Status::Downloading
        };
    });

    // Re-read the (possibly updated) save path.
    let save_path = {
        let guard = downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        guard
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.save_path.clone())
            .unwrap_or(save_path)
    };

    // ── Step 2: Pre-allocate the output file ──────────────────────────────────
    if let Some(dir) = save_path.parent() {
        tokio::fs::create_dir_all(dir).await.ok();
    }

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&save_path)
        .await
        .with_context(|| format!("cannot open output file: {}", save_path.display()))?;

    // Reserving space up-front prevents fragmentation and ensures the disk
    // has room before we start writing data.
    if let Some(size) = probe_result.content_length {
        file.set_len(size)
            .await
            .context("failed to pre-allocate file")?;
    }

    let file = Arc::new(Mutex::new(file));

    // ── Step 3 & 4: Divide and spawn segments ─────────────────────────────────
    let seg_count = choose_count(
        num_segments,
        probe_result.content_length,
        probe_result.supports_multipart(),
    );
    with_item(&downloads, &id, |item| item.num_segments = seg_count);
    let segments = build_segments(seg_count, probe_result.content_length);
    let final_url = probe_result.final_url.clone();

    eprintln!(
        "[IDG] starting download id={} segments={} size={:?}",
        id, seg_count, probe_result.content_length
    );

    let tasks: Vec<_> = segments
        .into_iter()
        .enumerate()
        .map(|(seg_id, (start, end))| {
            let client = client.clone();
            let url = final_url.clone();
            let file = file.clone();
            let cancel = cancel.clone();
            let pause = pause.clone();
            let downloads = downloads.clone();
            let id = id.clone();

            let args = SegmentArgs {
                client: client.clone(),
                id: seg_id,
                url,
                start,
                end: if probe_result.content_length.is_some() && probe_result.accepts_ranges {
                    Some(end)
                } else {
                    None
                },
                file,
                cancel,
                pause,
                on_progress: Box::new(move |bytes| {
                    with_item(&downloads, &id, |item| item.record_progress(bytes));
                }),
            };

            tokio::spawn(async move { run_segment(args, MAX_RETRIES).await })
        })
        .collect();

    // ── Step 5 & 6: Wait and finalise ─────────────────────────────────────────
    let results = join_all(tasks).await;

    for result in results {
        let outcome = result.context("segment task panicked")?;
        if let Err(e) = outcome {
            // If the user cancelled, we exit cleanly without an error.
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            return Err(e);
        }
    }

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        with_item(&downloads, &id, |i| i.status = Status::Cancelled);
    } else {
        eprintln!(
            "[IDG] download complete id={} path={}",
            id,
            save_path.display()
        );
        with_item(&downloads, &id, |i| {
            i.status = Status::Completed;
            i.speed_bps = 0.0;
            i.completed_at = Some(chrono::Utc::now());
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Divide `total` bytes into `count` equal non-overlapping ranges.
///
/// The last segment absorbs any remainder bytes so the ranges are contiguous.
/// Returns a `Vec` of `(start, end)` pairs (both inclusive).
fn build_segments(count: usize, total: Option<u64>) -> Vec<(u64, u64)> {
    match total {
        None => vec![(0, u64::MAX / 2)],
        Some(0) => vec![],
        Some(size) => {
            let chunk = size / count as u64;
            (0..count)
                .map(|i| {
                    let start = i as u64 * chunk;
                    let end = if i == count - 1 {
                        size - 1
                    } else {
                        start + chunk - 1
                    };
                    (start, end)
                })
                .collect()
        }
    }
}
