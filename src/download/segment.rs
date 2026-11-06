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

use super::{gate::ConnectionGate, writer::DownloadWriter};
use anyhow::{Context, Result, bail};
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Arguments passed to a single segment worker.
pub struct SegmentArgs {
    pub gate: Arc<ConnectionGate>,
    pub if_range: Option<String>,
    pub total_size: Option<u64>,
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
    pub file: Arc<DownloadWriter>,
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
    run_segment_with_idle_timeout(args, max_retries, Duration::from_secs(15)).await
}

async fn run_segment_with_idle_timeout(
    mut args: SegmentArgs,
    max_retries: u32,
    idle_timeout: Duration,
) -> Result<()> {
    let mut attempts = 0u32;
    // `resume_at` advances after each successful chunk write, enabling
    // within-segment resume across retries.
    let mut resume_at = args.start;

    loop {
        if args.end.is_some_and(|end| resume_at > end) {
            return Ok(());
        }
        let response = args.response.take();
        let streaming_export = super::probe::is_mail_export(&args.url) || response.is_some();
        let before = resume_at;
        let timeout = if args.end.is_some() && !streaming_export {
            idle_timeout
        } else {
            Duration::from_secs(300)
        };
        let result = download_range(&args, resume_at, &mut resume_at, response, timeout).await;

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
                let progressed = resume_at > before;
                attempts = if progressed { 0 } else { attempts + 1 };
                if attempts > max_retries {
                    bail!(
                        "segment {} failed after {} retries: {}",
                        args.id,
                        attempts,
                        e
                    );
                }
                // A productive reconnect is not a consecutive failure. Back
                // off only without progress, bounded at one second per retry.
                let backoff = Duration::from_millis(if progressed {
                    0
                } else {
                    (50_u64 << attempts.saturating_sub(1).min(5)).min(1000)
                });
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
    idle_timeout: Duration,
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
        let request = request.header("Range", &range);
        if let Some(validator) = &args.if_range {
            request.header("If-Range", validator)
        } else {
            request
        }
    } else {
        request
    };
    let permit = args.gate.acquire(cancel).await?;
    // A single clock observes cancellation, pauses and stalled network reads.
    // It is not reset for each packet and excludes intentional local backpressure.
    let mut clock = tokio::time::interval(Duration::from_millis(50));
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_network = tokio::time::Instant::now();
    let resp = match response {
        Some(response) => response,
        None => {
            let sending = request.send();
            tokio::pin!(sending);
            loop {
                tokio::select! {
                    response = &mut sending => break response.context("GET request failed")?,
                    _ = clock.tick() => {
                        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "cancelled waiting for response headers");
                        if network_stalled(&mut last_network, pause, idle_timeout) {
                            bail!("Network stalled waiting for response headers");
                        }
                    }
                }
            }
        }
    };

    if matches!(
        resp.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS | reqwest::StatusCode::SERVICE_UNAVAILABLE
    ) {
        args.gate.refused(permit);
        bail!("server limited connections (HTTP {})", resp.status());
    }
    let _permit = permit;
    if !resp.status().is_success() {
        bail!("server returned HTTP {}", resp.status());
    }
    super::probe::validate_file_response(resp.headers(), url)?;

    if (end.is_some() || start > 0) && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        bail!("The file changed or the server ignored its byte range. Add a fresh download link.");
    }
    if end.is_some() || start > 0 {
        let actual = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok());
        let expected = end
            .zip(args.total_size)
            .map(|(end, total)| format!("bytes {start}-{end}/{total}"));
        let correct = match expected {
            Some(expected) => actual.is_some_and(|v| v.trim() == expected),
            None => actual
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split_once('-'))
                .is_some_and(|(offset, _)| offset.parse::<u64>().ok() == Some(start)),
        };
        anyhow::ensure!(
            correct,
            "Server returned an incorrect Content-Range; refusing to mix file bytes"
        );
        if let Some(validator) = &args.if_range {
            let header = if validator.starts_with('"') {
                "etag"
            } else {
                "last-modified"
            };
            if let Some(actual) = resp.headers().get(header).and_then(|v| v.to_str().ok()) {
                anyhow::ensure!(
                    actual == validator,
                    "The file changed during download; add a fresh download link"
                );
            }
        }
    }
    let mut stream = resp.bytes_stream();
    let mut buffered = Vec::with_capacity(1024 * 1024);
    let mut last_write = std::time::Instant::now();
    last_network = tokio::time::Instant::now();

    loop {
        let chunk_result = tokio::select! {
            chunk = stream.next() => chunk,
            _ = clock.tick() => {
                anyhow::ensure!(!cancel.load(Ordering::Relaxed), "cancelled waiting for response body");
                if network_stalled(&mut last_network, pause, idle_timeout) {
                    // Resume only after every validated short buffer is written.
                    write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                    bail!("Network stalled waiting for response body");
                }
                continue;
            }
        };
        let Some(chunk_result) = chunk_result else {
            break;
        };
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

        let chunk: Bytes = match chunk_result {
            Ok(chunk) => chunk,
            Err(error) => {
                // These bytes already passed range/validator checks. Acknowledge
                // them before reconnecting, rather than discarding a short batch.
                write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                return Err(error).context("stream read error");
            }
        };
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
            if buffered.len() >= 1024 * 1024
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
        // Disk writes, rate limiting and user pauses are not network inactivity.
        last_network = tokio::time::Instant::now();
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

fn network_stalled(
    last_activity: &mut tokio::time::Instant,
    pause: &AtomicBool,
    timeout: Duration,
) -> bool {
    if pause.load(Ordering::Relaxed) {
        *last_activity = tokio::time::Instant::now();
        return false;
    }
    last_activity.elapsed() >= timeout
}

// Coalesce packets and advance the retry cursor only after a disk acknowledgement.
async fn write_buffer(
    file: &Arc<DownloadWriter>,
    cursor: &mut u64,
    buffered: &mut Vec<u8>,
    on_progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<()> {
    if buffered.is_empty() {
        return Ok(());
    }
    let count = buffered.len() as u64;
    *buffered = file
        .write_at(*cursor, std::mem::take(buffered))
        .await
        .context("write failed")?;
    *cursor += count;
    buffered.clear();
    on_progress(count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::get,
    };
    use std::sync::{Mutex, atomic::AtomicU64};

    #[tokio::test]
    async fn idle_reconnects_preserve_short_buffers_credentials_and_intentional_waits() {
        const SIZE: u64 = 2048;
        const PREFIX: u64 = 193;
        for http2 in [false, true] {
            for phase in 0..5 {
                let requests = Arc::new(Mutex::new(Vec::new()));
                let seen = requests.clone();
                let pause = Arc::new(AtomicBool::new(false));
                let paused = pause.clone();
                let ready = Arc::new(tokio::sync::Notify::new());
                let signal = ready.clone();
                let router = Router::new().route(
                    "/file",
                    get(move |headers: HeaderMap| {
                        let seen = seen.clone();
                        let pause = paused.clone();
                        let signal = signal.clone();
                        async move {
                            assert_eq!(headers["cookie"], "session=retained");
                            assert_eq!(headers["if-range"], "\"fixed\"");
                            let start: u64 = headers["range"]
                                .to_str()
                                .unwrap()
                                .strip_prefix("bytes=")
                                .unwrap()
                                .split('-')
                                .next()
                                .unwrap()
                                .parse()
                                .unwrap();
                            let first = {
                                let mut seen = seen.lock().unwrap();
                                seen.push(start);
                                seen.len() == 1
                            };
                            if (first && phase == 0) || phase == 4 {
                                std::future::pending::<()>().await;
                            }
                            let content: Vec<u8> = (start..SIZE).map(|n| (n % 251) as u8).collect();
                            let body = if first && matches!(phase, 1 | 2) {
                                if phase == 2 {
                                    pause.store(true, Ordering::Relaxed);
                                    signal.notify_one();
                                }
                                Body::from_stream(
                                    futures::stream::once(async {
                                        Ok::<_, std::io::Error>(Bytes::from(
                                            (0..PREFIX)
                                                .map(|n| (n % 251) as u8)
                                                .collect::<Vec<_>>(),
                                        ))
                                    })
                                    .chain(futures::stream::pending()),
                                )
                            } else {
                                Body::from(content)
                            };
                            (
                                StatusCode::PARTIAL_CONTENT,
                                [
                                    (
                                        "content-range",
                                        format!("bytes {start}-{}/{SIZE}", SIZE - 1),
                                    ),
                                    ("content-length", (SIZE - start).to_string()),
                                    ("etag", "\"fixed\"".into()),
                                    ("content-type", "application/octet-stream".into()),
                                ],
                                body,
                            )
                                .into_response()
                        }
                    }),
                );
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let url = format!("http://{}/file", listener.local_addr().unwrap());
                let server = tokio::spawn(async move {
                    axum::serve(listener, router).await.unwrap();
                });
                let directory = tempfile::tempdir().unwrap();
                let output = directory.path().join("resumed.bin");
                let file = std::fs::File::create(&output).unwrap();
                file.set_len(SIZE).unwrap();
                let writer = Arc::new(DownloadWriter::new(file).unwrap());
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert("cookie", "session=retained".parse().unwrap());
                let builder = super::super::http_client_builder().default_headers(headers);
                let builder = if http2 {
                    builder.http2_prior_knowledge()
                } else {
                    builder
                };
                let acknowledged = Arc::new(AtomicU64::new(0));
                let progress = acknowledged.clone();
                let args = SegmentArgs {
                    gate: Arc::new(ConnectionGate::new(1)),
                    if_range: Some("\"fixed\"".into()),
                    total_size: Some(SIZE),
                    bandwidth: Arc::new(super::super::bandwidth::BandwidthLimiter::new(
                        if phase == 3 { 1024 } else { 0 },
                    )),
                    response: None,
                    client: Arc::new(builder.build().unwrap()),
                    id: 0,
                    url,
                    start: 0,
                    end: Some(SIZE - 1),
                    file: writer.clone(),
                    cancel: Arc::new(AtomicBool::new(false)),
                    pause: pause.clone(),
                    on_progress: Box::new(move |bytes| {
                        progress.fetch_add(bytes, Ordering::Relaxed);
                    }),
                };
                let work = tokio::spawn(run_segment_with_idle_timeout(
                    args,
                    2,
                    Duration::from_millis(100),
                ));
                if phase == 2 {
                    tokio::time::timeout(Duration::from_secs(1), ready.notified())
                        .await
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis(350)).await;
                    assert_eq!(
                        requests.lock().unwrap().len(),
                        1,
                        "A user pause must not cause reconnects"
                    );
                    pause.store(false, Ordering::Relaxed);
                }
                let outcome = tokio::time::timeout(Duration::from_secs(5), work)
                    .await
                    .unwrap()
                    .unwrap();
                if phase == 4 {
                    let error = outcome.unwrap_err().to_string();
                    assert!(
                        error.contains("failed after 3 retries")
                            && error.contains("Network stalled")
                    );
                    assert_eq!(*requests.lock().unwrap(), vec![0, 0, 0]);
                    assert_eq!(acknowledged.load(Ordering::Relaxed), 0);
                    server.abort();
                    continue;
                }
                outcome.unwrap();
                writer.finish().await.unwrap();
                assert_eq!(acknowledged.load(Ordering::Relaxed), SIZE);
                assert_eq!(
                    std::fs::read(output).unwrap(),
                    (0..SIZE).map(|n| (n % 251) as u8).collect::<Vec<_>>()
                );
                assert_eq!(
                    *requests.lock().unwrap(),
                    match phase {
                        0 => vec![0, 0],
                        1 | 2 => vec![0, PREFIX],
                        _ => vec![0],
                    }
                );
                server.abort();
            }
        }
    }
}
