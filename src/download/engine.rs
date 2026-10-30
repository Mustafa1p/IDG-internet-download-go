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
use tokio::fs::OpenOptions;

use crate::download::{
    connections::choose_count,
    probe::probe,
    segment::{SegmentArgs, run_segment},
    types::{Downloads, Status, safe_filename, with_item},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Consecutive failures without acknowledged progress before giving up.
const MAX_RETRIES: u32 = 10;

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

    // Reserve atomically: two downloads with the same name must never truncate
    // each other or replace a user's existing file.
    let (file, save_path) = reserve_output(&save_path).await?;
    with_item(&downloads, &id, |item| {
        item.save_path = save_path.clone();
        item.filename = save_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
    });

    // Sparse setup avoids Windows zero-filling unwritten holes when parallel
    // ranges arrive out of order. Disk-full errors still propagate on write.
    if probe_result.supports_multipart() {
        super::writer::prepare_sparse(&file);
    }
    if let Some(size) = probe_result.content_length {
        file.set_len(size)
            .await
            .context("failed to pre-allocate file")?;
    }

    let file = Arc::new(super::writer::DownloadWriter::new(file.into_std().await)?);

    // ── Step 3 & 4: Divide and spawn segments ─────────────────────────────────
    let seg_count = choose_count(
        num_segments,
        probe_result.content_length,
        probe_result.supports_multipart(),
    );
    with_item(&downloads, &id, |item| item.num_segments = seg_count);
    let segments = build_segments(seg_count, probe_result.content_length);
    let gate = Arc::new(super::gate::ConnectionGate::new(seg_count));
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
                gate: gate.clone(),
                if_range: probe_result.if_range.clone(),
                total_size: probe_result.content_length,
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
        // The writer barrier syncs every acknowledged range before completion.
        file.finish().await.context("failed to sync output file")?;
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

async fn reserve_output(path: &std::path::Path) -> Result<(tokio::fs::File, std::path::PathBuf)> {
    for number in 0..10_000 {
        let candidate = if number == 0 {
            path.to_owned()
        } else {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let extension = path
                .extension()
                .map(|v| format!(".{}", v.to_string_lossy()))
                .unwrap_or_default();
            path.with_file_name(format!("{stem} ({number}){extension}"))
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
            .await
        {
            Ok(file) => return Ok((file, candidate)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Cannot create {}", candidate.display()));
            }
        }
    }
    anyhow::bail!("Too many files with this name; choose another filename")
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

    #[tokio::test]
    async fn connection_refusals_queue_remaining_ranges_without_losing_bytes() {
        let slots = Arc::new(tokio::sync::Semaphore::new(4));
        let rejected = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = rejected.clone();
        let router = Router::new().route(
            "/file",
            get(move |method: Method, headers: HeaderMap| {
                let slots = slots.clone();
                let count = count.clone();
                async move {
                    if method == Method::HEAD {
                        return range_file(method, headers).await;
                    }
                    let Ok(permit) = slots.try_acquire_owned() else {
                        count.fetch_add(1, Ordering::Relaxed);
                        return (StatusCode::SERVICE_UNAVAILABLE, "connection limit")
                            .into_response();
                    };
                    range_file(method, headers).await.map(|body| {
                        Body::from_stream(futures::stream::unfold(
                            (body.into_data_stream(), permit),
                            |(mut stream, permit)| async move {
                                use futures::StreamExt;
                                stream.next().await.map(|chunk| (chunk, (stream, permit)))
                            },
                        ))
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capped.bin");
        let item = crate::download::types::DownloadItem::new(url, path.clone(), 16);
        let id = item.id.clone();
        let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
        tokio::time::timeout(
            Duration::from_secs(10),
            start_download(Arc::new(reqwest::Client::new()), downloads.clone(), id),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(rejected.load(Ordering::Relaxed) > 0);
        assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
        assert_eq!(
            std::fs::read(path).unwrap(),
            (0..SIZE).map(|n| (n % 251) as u8).collect::<Vec<_>>()
        );
        server.abort();
    }

    #[tokio::test]
    async fn productive_reconnects_exceed_retry_budget_without_discarding_short_buffers() {
        let router = Router::new().route(
            "/file",
            get(|method: Method, headers: HeaderMap| async move {
                let response = range_file(method.clone(), headers).await;
                if method == Method::HEAD {
                    return response;
                }
                response.map(|body| {
                    use futures::StreamExt;
                    let data = body
                        .into_data_stream()
                        .map(|chunk| chunk.map_err(std::io::Error::other))
                        .take(2);
                    let broken = futures::stream::once(async {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        Err::<bytes::Bytes, _>(std::io::Error::new(
                            std::io::ErrorKind::ConnectionReset,
                            "injected drop",
                        ))
                    });
                    Body::from_stream(data.chain(broken))
                })
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reconnected.bin");
        let item = crate::download::types::DownloadItem::new(url, path.clone(), 1);
        let id = item.id.clone();
        let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
        tokio::time::timeout(
            Duration::from_secs(10),
            start_download(Arc::new(reqwest::Client::new()), downloads.clone(), id),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(downloads.lock().unwrap()[0].downloaded_bytes, SIZE as u64);
        assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
        assert_eq!(
            std::fs::read(path).unwrap(),
            (0..SIZE).map(|n| (n % 251) as u8).collect::<Vec<_>>()
        );
        server.abort();
    }

    #[tokio::test]
    async fn duplicate_destinations_are_reserved_atomically_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.zip");
        tokio::fs::write(&path, b"existing user file")
            .await
            .unwrap();
        let (a, b) = tokio::join!(reserve_output(&path), reserve_output(&path));
        let (_, a) = a.unwrap();
        let (_, b) = b.unwrap();
        assert_ne!(a, b);
        assert_ne!(a, path);
        assert_ne!(b, path);
        assert_eq!(tokio::fs::read(path).await.unwrap(), b"existing user file");
    }

    #[tokio::test]
    async fn changed_source_and_wrong_ranges_never_report_completion() {
        for changed in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/file.zip", listener.local_addr().unwrap());
            let router = Router::new().route(
                "/file.zip",
                get(move |method: Method, headers: HeaderMap| async move {
                    let mut response = range_file(method.clone(), headers.clone()).await;
                    response
                        .headers_mut()
                        .insert("etag", "\"version-one\"".parse().unwrap());
                    if method != Method::HEAD
                        && headers.get("range").is_some_and(|v| v != "bytes=0-0")
                    {
                        assert_eq!(headers.get("if-range").unwrap(), "\"version-one\"");
                        if changed {
                            response
                                .headers_mut()
                                .insert("etag", "\"version-two\"".parse().unwrap());
                        } else {
                            response.headers_mut().insert(
                                "content-range",
                                format!("bytes 1-2/{SIZE}").parse().unwrap(),
                            );
                        }
                    }
                    response
                }),
            );
            let source = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let directory = tempfile::tempdir().unwrap();
            let item = crate::download::types::DownloadItem::new(
                url,
                directory.path().join("file.zip"),
                1,
            );
            let id = item.id.clone();
            let downloads: Downloads = Arc::new(std::sync::Mutex::new(vec![item]));
            let error = start_download(Arc::new(reqwest::Client::new()), downloads.clone(), id)
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains(if changed {
                    "file changed"
                } else {
                    "Content-Range"
                }),
                "{error:#}"
            );
            assert_ne!(downloads.lock().unwrap()[0].status, Status::Completed);
            assert_eq!(downloads.lock().unwrap()[0].downloaded_bytes, 0);
            source.abort();
        }
    }

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
