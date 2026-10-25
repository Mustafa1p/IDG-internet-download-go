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
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};

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
    let (url, save_path, num_segments, cancel, pause, headers, post_data, bandwidth) = {
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
            item.post_data.clone(),
            item.bandwidth.clone(),
        )
    };

    if super::music::source(&url).is_some() {
        return super::music::download(downloads, id).await;
    }
    if let Some(reason) = super::media::unsupported_reason(&url) {
        anyhow::bail!("{reason}");
    }
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
                .read_timeout(std::time::Duration::from_secs(300))
                .build()?,
        )
    };
    with_item(&downloads, &id, |i| i.status = Status::Probing);

    let mut probe_result = match post_data {
        Some(body) => super::probe::post_probe(&client, &url, body).await,
        None => probe(&client, &url).await,
    }
    .map_err(|error| anyhow::anyhow!("URL probe failed: {error:#}"))?;

    // Persist probe results; update the save path if the server suggested a
    // different filename via Content-Disposition.
    with_item(&downloads, &id, |i| {
        i.total_bytes = probe_result.content_length;
        if let Some(ref server_name) = probe_result.filename
            && (!i.preserve_filename || i.filename == "download")
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

    let mut initial_response = probe_result.response.take();
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
                bandwidth: bandwidth.clone(),
                response: initial_response.take(),
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
        // Tokio file writes can still be buffered after the workers finish.
        // Flush before advertising completion or consumers may read zeroes.
        file.lock()
            .await
            .flush()
            .await
            .context("failed to flush output file")?;
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

#[cfg(test)]
mod regression_tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, Method, StatusCode},
        response::{IntoResponse, Response},
        routing::get,
    };
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    const SIZE: usize = 256 * 1024 + 19;

    // A slow, local byte-range source gives the worker a real socket and file
    // while keeping pause/resume assertions independent of internet services.
    async fn range_file(method: Method, headers: HeaderMap) -> Response {
        let mut result = HeaderMap::new();
        result.insert("accept-ranges", "bytes".parse().unwrap());
        result.insert("content-length", SIZE.to_string().parse().unwrap());
        if method == Method::HEAD {
            return (result, Body::empty()).into_response();
        }
        let (start, end) = headers["range"]
            .to_str()
            .unwrap()
            .strip_prefix("bytes=")
            .unwrap()
            .split_once('-')
            .unwrap();
        let (start, end): (usize, usize) = (start.parse().unwrap(), end.parse().unwrap());
        result.insert(
            "content-length",
            (end - start + 1).to_string().parse().unwrap(),
        );
        result.insert(
            "content-range",
            format!("bytes {start}-{end}/{SIZE}").parse().unwrap(),
        );
        let stream = futures::stream::unfold(start, move |offset| async move {
            if offset > end {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
            let next = (offset + 4096).min(end + 1);
            let bytes: Vec<u8> = (offset..next).map(|n| (n % 251) as u8).collect();
            Some((Ok::<_, std::io::Error>(bytes), next))
        });
        (
            StatusCode::PARTIAL_CONTENT,
            result,
            Body::from_stream(stream),
        )
            .into_response()
    }

    async fn source() -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/file", get(range_file)))
                .await
                .unwrap();
        });
        (url, task)
    }

    #[tokio::test]
    async fn range_download_pauses_resumes_and_preserves_exact_bytes() {
        let (url, source) = source().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pause-resume.bin");
        let item = crate::download::types::DownloadItem::new(url, path.clone(), 1);
        let id = item.id.clone();
        let pause = item.pause_flag.clone();
        let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
        let worker = tokio::spawn(start_download(
            Arc::new(reqwest::Client::new()),
            downloads.clone(),
            id,
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if downloads.lock().unwrap()[0].downloaded_bytes > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("download did not begin");
        pause.store(true, Ordering::Relaxed);
        // Let an already-writing chunk settle before measuring paused bytes.
        tokio::time::sleep(Duration::from_millis(120)).await;
        let paused_bytes = downloads.lock().unwrap()[0].downloaded_bytes;
        assert!(paused_bytes > 0 && paused_bytes < SIZE as u64);
        tokio::time::sleep(Duration::from_millis(220)).await;
        assert_eq!(downloads.lock().unwrap()[0].downloaded_bytes, paused_bytes);
        assert!(!worker.is_finished(), "paused worker must remain resumable");
        pause.store(false, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("resumed download did not finish")
            .unwrap()
            .unwrap();
        let item = downloads.lock().unwrap()[0].clone();
        assert_eq!(item.status, Status::Completed);
        assert_eq!(item.downloaded_bytes, SIZE as u64);
        assert!(item.completed_at.is_some());
        let expected: Vec<u8> = (0..SIZE).map(|n| (n % 251) as u8).collect();
        assert_eq!(tokio::fs::read(path).await.unwrap(), expected);
        source.abort();
    }

    #[tokio::test]
    async fn cancel_while_paused_exits_without_reporting_completion_or_deleting_file() {
        let (url, source) = source().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cancelled.bin");
        let item = crate::download::types::DownloadItem::new(url, path.clone(), 1);
        item.pause_flag.store(true, Ordering::Relaxed);
        let cancel = item.cancel_flag.clone();
        let id = item.id.clone();
        let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
        let worker = tokio::spawn(start_download(
            Arc::new(reqwest::Client::new()),
            downloads.clone(),
            id,
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if downloads.lock().unwrap()[0].status == Status::Paused {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("worker did not enter paused state");
        cancel.store(true, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("cancelling a paused worker must not hang")
            .unwrap()
            .unwrap();
        let item = downloads.lock().unwrap()[0].clone();
        assert_eq!(item.status, Status::Cancelled);
        assert_eq!(item.downloaded_bytes, 0);
        assert!(item.completed_at.is_none());
        assert!(path.exists(), "cancelling must not delete downloaded files");
        source.abort();
    }
    #[tokio::test]
    async fn speed_limit_is_shared_across_segments_and_preserves_exact_bytes() {
        let (url, source) = source().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("limited.bin");
        let mut item = crate::download::types::DownloadItem::new(url, path.clone(), 4);
        item.bandwidth = Arc::new(super::super::bandwidth::BandwidthLimiter::new(512 * 1024));
        let id = item.id.clone();
        let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
        let started = tokio::time::Instant::now();
        tokio::time::timeout(
            Duration::from_secs(8),
            start_download(Arc::new(reqwest::Client::new()), downloads.clone(), id),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(480));
        assert_eq!(downloads.lock().unwrap()[0].num_segments, 4);
        let expected: Vec<u8> = (0..SIZE).map(|n| (n % 251) as u8).collect();
        assert_eq!(std::fs::read(path).unwrap(), expected);
        source.abort();
    }

    #[tokio::test]
    async fn cancelling_a_slow_speed_limit_does_not_wait_for_the_bandwidth_budget() {
        let (url, source) = source().await;
        let directory = tempfile::tempdir().unwrap();
        let mut item =
            crate::download::types::DownloadItem::new(url, directory.path().join("cancel.bin"), 1);
        item.bandwidth = Arc::new(super::super::bandwidth::BandwidthLimiter::new(1024));
        let cancel = item.cancel_flag.clone();
        let id = item.id.clone();
        let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
        let worker = tokio::spawn(start_download(
            Arc::new(reqwest::Client::new()),
            downloads.clone(),
            id,
        ));
        tokio::time::sleep(Duration::from_millis(180)).await;
        cancel.store(true, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(downloads.lock().unwrap()[0].status, Status::Cancelled);
        source.abort();
    }
}
