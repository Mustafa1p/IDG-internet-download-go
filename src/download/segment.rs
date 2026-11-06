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

#[cfg(test)]
use super::gate::HostBudget;
use super::{
    gate::{ConnectionGate, LeasedResponse},
    request::{Control, InvalidRedirect, PausedRequest as PausedRange, RequestContext, Spec},
    writer::DownloadWriter,
};
use anyhow::{Context, Result, bail};
use bytes::Bytes;
use futures::StreamExt;

/// Retrying cannot repair invalid representations, rejected credentials or a
/// failed destination writer. Keep the underlying error available to callers.
#[derive(Debug)]
struct TerminalFailure(anyhow::Error);

impl std::fmt::Display for TerminalFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for TerminalFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

fn terminal(error: anyhow::Error) -> anyhow::Error {
    TerminalFailure(error).into()
}

/// Cooperative completion: the old stream is closed and every preceding byte
/// is disk-acknowledged. The orchestrator may safely redistribute this remainder.
#[derive(Debug)]
pub(super) struct YieldedRange {
    pub start: u64,
    pub end: u64,
}

impl std::fmt::Display for YieldedRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Range yielded at acknowledged byte {}",
            self.start
        )
    }
}

impl std::error::Error for YieldedRange {}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Arguments passed to a single segment worker.
pub struct SegmentArgs {
    /// Only an original replayable bounded range may yield to idle siblings.
    pub handoff: Option<Arc<AtomicBool>>,
    /// Local capacity already reserved for a retained initial partial response.
    pub initial_permit: Option<tokio::sync::OwnedSemaphorePermit>,
    pub gate: Arc<ConnectionGate>,
    pub if_range: Option<String>,
    pub total_size: Option<u64>,
    pub bandwidth: Arc<super::bandwidth::BandwidthLimiter>,
    /// A GET already opened by the probe; consume it instead of requesting again.
    pub response: Option<LeasedResponse>,
    pub request: Arc<RequestContext>,
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
/// Returns an error after `max_retries` failed attempts, or immediately for
/// cancellation, invalid representations, rejected credentials or disk errors.
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
        while args.pause.load(Ordering::Relaxed) {
            if args.end.is_some() {
                args.response.take();
                args.initial_permit.take();
            }
            anyhow::ensure!(
                !args.cancel.load(Ordering::Relaxed),
                "cancelled during pause"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        anyhow::ensure!(
            !args.cancel.load(Ordering::Relaxed),
            "segment {} cancelled",
            args.id
        );
        if args.end.is_some_and(|end| resume_at > end) {
            return Ok(());
        }
        if args.response.is_none()
            && let Some(remaining) = pending_handoff(&args, resume_at)
        {
            return Err(remaining.into());
        }
        let response = args.response.take();
        let streaming_export =
            super::probe::is_mail_export(&args.url) || (args.end.is_none() && response.is_some());
        let before = resume_at;
        let timeout = if args.end.is_some() && !streaming_export {
            idle_timeout
        } else {
            Duration::from_secs(300)
        };
        let initial_permit = args.initial_permit.take();
        let result = download_range(
            &args,
            resume_at,
            &mut resume_at,
            response,
            initial_permit,
            timeout,
        )
        .await;

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                if e.is::<YieldedRange>() {
                    return Err(e);
                }
                if e.downcast_ref::<TerminalFailure>().is_some() {
                    return Err(e);
                }
                if e.downcast_ref::<PausedRange>().is_some() {
                    // This is intentional suspension, not a failed attempt.
                    continue;
                }
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
    response: Option<LeasedResponse>,
    initial_permit: Option<tokio::sync::OwnedSemaphorePermit>,
    idle_timeout: Duration,
) -> Result<()> {
    let SegmentArgs {
        request: context,
        url,
        end,
        file,
        cancel,
        pause,
        on_progress,
        ..
    } = args;
    let end = *end;
    let release_on_pause = end.is_some();
    // Build the Range header value.
    let range = match end {
        Some(e) => format!("bytes={start}-{e}"),
        None => format!("bytes={start}-"),
    };

    let mut spec = Spec::get(url);
    if end.is_some() || start > 0 {
        spec = spec.header("range", &range)?;
        if let Some(validator) = &args.if_range {
            spec = spec.header("if-range", validator)?;
        }
    }
    let permit = match initial_permit {
        Some(permit) => permit,
        None => args.gate.acquire(cancel).await?,
    };
    let mut handoff_grace = Duration::from_millis(50);
    let LeasedResponse {
        response: resp,
        permit: _host_permit,
    } = match response {
        Some(response) => response,
        None => {
            let sending = context.send(
                spec,
                Control {
                    cancel: Some(cancel),
                    pause: release_on_pause.then_some(pause),
                    network_timeout: Some(idle_timeout),
                    ..Control::default()
                },
            );
            let result = if args.handoff.is_some() && *cursor > args.start {
                tokio::select! {
                    result = sending => result,
                    remaining = wait_for_handoff(args, *cursor) => return Err(remaining.into()),
                }
            } else {
                sending.await
            };
            let result = result.map_err(|error| {
                if error.is::<InvalidRedirect>() {
                    terminal(error)
                } else {
                    error
                }
            })?;
            if let Some(remaining) = result.remaining_network_time {
                // Capacity queues are excluded from this observed request cost.
                // Give a valid stream at least that long to finish naturally.
                handoff_grace = idle_timeout
                    .saturating_sub(remaining)
                    .clamp(handoff_grace, Duration::from_secs(1));
            }
            result.leased
        }
    };
    if release_on_pause && pause.load(Ordering::Relaxed) {
        return Err(PausedRange.into());
    }
    let mut clock = tokio::time::interval(Duration::from_millis(50));
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_network;

    if matches!(
        resp.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS | reqwest::StatusCode::SERVICE_UNAVAILABLE
    ) {
        args.gate.refused(permit);
        bail!("server limited connections (HTTP {})", resp.status());
    }
    let _permit = permit;
    if !resp.status().is_success() {
        let error = anyhow::anyhow!("server returned HTTP {}", resp.status());
        return Err(
            if resp.status().is_client_error()
                && resp.status() != reqwest::StatusCode::REQUEST_TIMEOUT
            {
                terminal(error)
            } else {
                error
            },
        );
    }
    super::probe::validate_file_response(resp.headers(), url).map_err(terminal)?;

    if (end.is_some() || start > 0) && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(terminal(anyhow::anyhow!(
            "The file changed or the server ignored its byte range. Add a fresh download link."
        )));
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
        if !correct {
            return Err(terminal(anyhow::anyhow!(
                "Server returned an incorrect Content-Range; refusing to mix file bytes"
            )));
        }
        if let Some(validator) = &args.if_range {
            let header = if validator.starts_with('"') {
                "etag"
            } else {
                "last-modified"
            };
            if let Some(actual) = resp.headers().get(header).and_then(|v| v.to_str().ok())
                && actual != validator
            {
                return Err(terminal(anyhow::anyhow!(
                    "The file changed during download; add a fresh download link"
                )));
            }
        }
    }
    let mut stream = resp.bytes_stream();
    let mut buffered = Vec::with_capacity(1024 * 1024);
    let mut last_write = std::time::Instant::now();
    let mut handoff_seen = None;
    last_network = tokio::time::Instant::now();

    loop {
        let chunk_result = tokio::select! {
            biased;
            // Consume bytes already available before considering a handoff.
            // This avoids replaying a fast response whose peers finished first.
            chunk = stream.next() => chunk,
            _ = clock.tick() => {
                anyhow::ensure!(!cancel.load(Ordering::Relaxed), "cancelled waiting for response body");
                if release_on_pause && pause.load(Ordering::Relaxed) {
                    write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                    return Err(PausedRange.into());
                }
                if handoff_requested(args) {
                    let seen = handoff_seen.get_or_insert_with(std::time::Instant::now);
                    if seen.elapsed() >= handoff_grace {
                        write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                        // Buffered bytes may leave only a tiny tail. Keep its
                        // valid response instead of opening another request.
                        if let Some(remaining) = pending_handoff(args, *cursor) {
                            return Err(remaining.into());
                        }
                    }
                } else {
                    handoff_seen = None;
                }
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
            if release_on_pause {
                write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                return Err(PausedRange.into());
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
                            if release_on_pause && pause.load(Ordering::Relaxed) {
                                write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                                return Err(PausedRange.into());
                            }
                        }
                    }
                }
            }
            while pause.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    bail!("cancelled during pause");
                }
                if release_on_pause {
                    write_buffer(file, cursor, &mut buffered, on_progress.as_ref()).await?;
                    return Err(PausedRange.into());
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

fn handoff_requested(args: &SegmentArgs) -> bool {
    args.handoff
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
        && args.bandwidth.rate() == 0
}

fn pending_handoff(args: &SegmentArgs, cursor: u64) -> Option<YieldedRange> {
    let end = args.end?;
    // At most 64 KiB offers little work to redistribute. Normal inactivity
    // recovery still applies if this valid response actually stalls.
    // A peer finishing sooner is not evidence that an initial handshake is
    // stalled. Require acknowledged data before replacing an active request.
    (cursor > args.start
        && end.saturating_sub(cursor).saturating_add(1) > 64 * 1024
        && handoff_requested(args))
    .then_some(YieldedRange { start: cursor, end })
}

async fn wait_for_handoff(args: &SegmentArgs, cursor: u64) -> YieldedRange {
    loop {
        if let Some(remaining) = pending_handoff(args, cursor) {
            return remaining;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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
        .context("write failed")
        .map_err(terminal)?;
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

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Fault {
        StalledHeaders,
        StalledBody,
        PausedBody,
        RateLimited,
        PermanentStall,
        ReadOnlyDestination,
        Forbidden,
        RequestTimeout,
    }

    #[tokio::test]
    async fn idle_reconnects_preserve_short_buffers_credentials_and_intentional_waits() {
        const SIZE: u64 = 2048;
        const PREFIX: u64 = 193;
        for http2 in [false, true] {
            for fault in [
                Fault::StalledHeaders,
                Fault::StalledBody,
                Fault::PausedBody,
                Fault::RateLimited,
                Fault::PermanentStall,
                Fault::ReadOnlyDestination,
                Fault::Forbidden,
                Fault::RequestTimeout,
            ] {
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
                            if fault == Fault::Forbidden {
                                return StatusCode::FORBIDDEN.into_response();
                            }
                            if first && fault == Fault::RequestTimeout {
                                return StatusCode::REQUEST_TIMEOUT.into_response();
                            }
                            if (first && fault == Fault::StalledHeaders)
                                || fault == Fault::PermanentStall
                            {
                                std::future::pending::<()>().await;
                            }
                            let content: Vec<u8> = (start..SIZE).map(|n| (n % 251) as u8).collect();
                            let body = if first
                                && matches!(fault, Fault::StalledBody | Fault::PausedBody)
                            {
                                if fault == Fault::PausedBody {
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
                let file = if fault == Fault::ReadOnlyDestination {
                    std::fs::write(&output, b"keep").unwrap();
                    std::fs::File::open(&output).unwrap()
                } else {
                    let file = std::fs::File::create(&output).unwrap();
                    file.set_len(SIZE).unwrap();
                    file
                };
                let writer = Arc::new(DownloadWriter::new(file).unwrap());
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert("cookie", "session=retained".parse().unwrap());
                let builder = super::super::http_client_builder();
                let builder = if http2 {
                    builder.http2_prior_knowledge()
                } else {
                    builder
                };
                let acknowledged = Arc::new(AtomicU64::new(0));
                let progress = acknowledged.clone();
                let args = SegmentArgs {
                    // A handoff request must not disable normal inactivity
                    // recovery for a small, genuinely stalled response.
                    handoff: (fault == Fault::StalledBody).then(|| Arc::new(AtomicBool::new(true))),
                    initial_permit: None,
                    gate: Arc::new(ConnectionGate::new(1)),
                    if_range: Some("\"fixed\"".into()),
                    total_size: Some(SIZE),
                    bandwidth: Arc::new(super::super::bandwidth::BandwidthLimiter::new(
                        if fault == Fault::RateLimited { 1024 } else { 0 },
                    )),
                    response: None,
                    request: Arc::new(
                        RequestContext::new(Arc::new(builder.build().unwrap()), &url, headers)
                            .unwrap(),
                    ),
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
                if fault == Fault::PausedBody {
                    tokio::time::timeout(Duration::from_secs(1), ready.notified())
                        .await
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis(350)).await;
                    assert_eq!(
                        requests.lock().unwrap().len(),
                        1,
                        "A paused range must not open another request"
                    );
                    pause.store(false, Ordering::Relaxed);
                }
                let outcome = tokio::time::timeout(Duration::from_secs(5), work)
                    .await
                    .unwrap()
                    .unwrap();
                if fault == Fault::PermanentStall {
                    let error = outcome.unwrap_err().to_string();
                    assert!(
                        error.contains("failed after 3 retries")
                            && error.contains("Network stalled")
                    );
                    assert_eq!(*requests.lock().unwrap(), vec![0, 0, 0]);
                    assert_eq!(acknowledged.load(Ordering::Relaxed), 0);
                    writer.finish().await.unwrap();
                    server.abort();
                    continue;
                }
                if matches!(fault, Fault::ReadOnlyDestination | Fault::Forbidden) {
                    let error = outcome.unwrap_err().to_string();
                    assert!(error.contains(if fault == Fault::Forbidden {
                        "403"
                    } else {
                        "write failed"
                    }));
                    assert_eq!(
                        *requests.lock().unwrap(),
                        vec![0],
                        "Permanent failures must not repeat network requests"
                    );
                    assert_eq!(acknowledged.load(Ordering::Relaxed), 0);
                    assert_eq!(
                        writer.finish().await.is_err(),
                        fault == Fault::ReadOnlyDestination
                    );
                    if fault == Fault::ReadOnlyDestination {
                        assert_eq!(std::fs::read(output).unwrap(), b"keep");
                    }
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
                    match fault {
                        Fault::StalledHeaders | Fault::RequestTimeout | Fault::PausedBody =>
                            vec![0, 0],
                        Fault::StalledBody => vec![0, PREFIX],
                        _ => vec![0],
                    }
                );
                server.abort();
            }
        }
    }

    #[tokio::test]
    async fn pause_releases_ranges_at_acknowledged_offsets_but_keeps_one_use_streams() {
        const PAUSE_AFTER: u64 = 1024 * 1024;
        const PREFIX: u64 = 2 * PAUSE_AFTER;
        const LENGTH: u64 = 2 * PREFIX;
        for http2 in [false, true] {
            for export in [false, true] {
                let requests = Arc::new(Mutex::new(Vec::new()));
                let seen = requests.clone();
                let (resume, resumed) = tokio::sync::watch::channel(false);
                let router = Router::new().route(
                    "/file",
                    axum::routing::any(
                        move |method: axum::http::Method,
                              headers: HeaderMap,
                              body: bytes::Bytes| {
                            let seen = seen.clone();
                            let mut resumed = resumed.clone();
                            async move {
                                assert_eq!(headers["cookie"], "session=retained");
                                let start = if export {
                                    assert_eq!(method, axum::http::Method::POST);
                                    assert_eq!(body.as_ref(), b"token=once");
                                    assert!(!headers.contains_key("range"));
                                    0
                                } else {
                                    assert_eq!(method, axum::http::Method::GET);
                                    assert_eq!(headers["if-range"], "\"fixed\"");
                                    headers["range"]
                                        .to_str()
                                        .unwrap()
                                        .strip_prefix("bytes=")
                                        .unwrap()
                                        .split('-')
                                        .next()
                                        .unwrap()
                                        .parse::<u64>()
                                        .unwrap()
                                };
                                let first = {
                                    let mut seen = seen.lock().unwrap();
                                    seen.push(start);
                                    seen.len() == 1
                                };
                                let body = if first {
                                    let prefix = futures::stream::once(async {
                                        Ok::<_, std::io::Error>(Bytes::from(
                                            (0..PREFIX)
                                                .map(|n| (n % 251) as u8)
                                                .collect::<Vec<_>>(),
                                        ))
                                    });
                                    if export {
                                        Body::from_stream(prefix.chain(futures::stream::once(
                                            async move {
                                                resumed.wait_for(|resumed| *resumed).await.unwrap();
                                                Ok::<_, std::io::Error>(Bytes::from(
                                                    (PREFIX..LENGTH)
                                                        .map(|n| (n % 251) as u8)
                                                        .collect::<Vec<_>>(),
                                                ))
                                            },
                                        )))
                                    } else {
                                        Body::from_stream(prefix.chain(futures::stream::pending()))
                                    }
                                } else {
                                    Body::from(
                                        (start..LENGTH)
                                            .map(|n| (n % 251) as u8)
                                            .collect::<Vec<_>>(),
                                    )
                                };
                                let mut response = HeaderMap::new();
                                response.insert(
                                    "content-type",
                                    "application/octet-stream".parse().unwrap(),
                                );
                                response.insert(
                                    "content-length",
                                    (LENGTH - start).to_string().parse().unwrap(),
                                );
                                response.insert("etag", "\"fixed\"".parse().unwrap());
                                if !export {
                                    response.insert(
                                        "content-range",
                                        format!("bytes {start}-{}/{LENGTH}", LENGTH - 1)
                                            .parse()
                                            .unwrap(),
                                    );
                                }
                                (
                                    if export {
                                        StatusCode::OK
                                    } else {
                                        StatusCode::PARTIAL_CONTENT
                                    },
                                    response,
                                    body,
                                )
                            }
                        },
                    ),
                );
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let url = format!("http://{}/file", listener.local_addr().unwrap());
                let server =
                    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
                let budget = HostBudget::for_url(&url);
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert("cookie", "session=retained".parse().unwrap());
                let builder = super::super::http_client_builder();
                let client = Arc::new(
                    if http2 {
                        builder.http2_prior_knowledge()
                    } else {
                        builder
                    }
                    .build()
                    .unwrap(),
                );
                let request = Arc::new(RequestContext::new(client, &url, headers).unwrap());
                let response = if export {
                    super::super::probe::post_probe(&request, &url, "token=once".into())
                        .await
                        .unwrap()
                        .response
                } else {
                    None
                };
                let directory = tempfile::tempdir().unwrap();
                let output = directory.path().join("paused.bin");
                let file = std::fs::File::create(&output).unwrap();
                file.set_len(LENGTH).unwrap();
                let writer = Arc::new(DownloadWriter::new(file).unwrap());
                let pause = Arc::new(AtomicBool::new(false));
                let paused = pause.clone();
                let ready = Arc::new(tokio::sync::Notify::new());
                let signal = ready.clone();
                let acknowledged = Arc::new(AtomicU64::new(0));
                let progress = acknowledged.clone();
                let args = SegmentArgs {
                    handoff: None,
                    initial_permit: None,
                    gate: Arc::new(ConnectionGate::new(1)),
                    if_range: (!export).then(|| "\"fixed\"".into()),
                    total_size: Some(LENGTH),
                    bandwidth: Arc::new(super::super::bandwidth::BandwidthLimiter::new(0)),
                    response,
                    request,
                    id: 0,
                    url,
                    start: 0,
                    end: (!export).then_some(LENGTH - 1),
                    file: writer.clone(),
                    cancel: Arc::new(AtomicBool::new(false)),
                    pause: pause.clone(),
                    on_progress: Box::new(move |bytes| {
                        let before = progress.fetch_add(bytes, Ordering::Relaxed);
                        if before < PAUSE_AFTER && before + bytes >= PAUSE_AFTER {
                            paused.store(true, Ordering::Relaxed);
                            signal.notify_one();
                        }
                    }),
                };
                // Zero retries proves an intentional pause does not spend the failure budget.
                let work = tokio::spawn(run_segment(args, 0));
                tokio::time::timeout(Duration::from_secs(2), ready.notified())
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(150)).await;
                let paused_at = acknowledged.load(Ordering::Relaxed);
                assert!((PAUSE_AFTER..=PREFIX).contains(&paused_at));
                assert_eq!(*requests.lock().unwrap(), vec![0]);
                assert_eq!(
                    budget.counts().0,
                    usize::from(export),
                    "Only a one-use stream keeps its accepted connection during pause"
                );
                assert!(!work.is_finished());
                pause.store(false, Ordering::Relaxed);
                resume.send(true).unwrap();
                tokio::time::timeout(Duration::from_secs(3), work)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                writer.finish().await.unwrap();
                assert_eq!(acknowledged.load(Ordering::Relaxed), LENGTH);
                assert_eq!(
                    *requests.lock().unwrap(),
                    if export { vec![0] } else { vec![0, paused_at] }
                );
                assert_eq!(
                    std::fs::read(output).unwrap(),
                    (0..LENGTH).map(|n| (n % 251) as u8).collect::<Vec<_>>()
                );
                server.abort();
            }
        }
    }
}
