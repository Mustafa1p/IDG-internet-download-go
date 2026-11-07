//! Experimental full stream with optional validated tail helpers.
//! No production policy changes. Known length and strong ETag required for helpers.
//! Usage: idg-hedged URL OUTPUT_FILE (--helpers | --auto-helpers) 0..31
#![allow(dead_code)]
#[path = "support/adaptive.rs"]
mod adaptive;
#[path = "support/coverage.rs"]
mod coverage;
#[path = "../src/download/mod.rs"]
mod download;

use anyhow::{Context, Result};
use coverage::{CHUNK, Coverage, DiskFailure};
use download::{
    gate::LeasedResponse,
    request::{Control, RequestContext, Spec},
};
use futures::StreamExt;
use std::{
    collections::{BinaryHeap, HashSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const GROUP_CHUNKS: usize = 4;

#[derive(Debug)]
struct NetworkStall;
impl std::fmt::Display for NetworkStall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Response body stalled")
    }
}
impl std::error::Error for NetworkStall {}

#[derive(Debug)]
struct CapacityRefusal(reqwest::StatusCode);
impl std::fmt::Display for CapacityRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Server temporarily refused capacity (HTTP {})", self.0)
    }
}
impl std::error::Error for CapacityRefusal {}

async fn pump(
    response: reqwest::Response,
    start: u64,
    end: u64,
    coverage: &Coverage,
) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut offset = start;
    let mut buffered = Vec::with_capacity(CHUNK as usize);
    let mut last_flush = Instant::now();
    loop {
        if coverage.covers(start, end) {
            return Ok(());
        }
        let reading = tokio::time::timeout(Duration::from_secs(15), stream.next());
        tokio::pin!(reading);
        let result = loop {
            tokio::select! {
                biased;
                result = &mut reading => break result,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if coverage.covers(start, end) { return Ok(()); }
                }
            }
        };
        let next = match result {
            Ok(Some(Ok(bytes))) => bytes,
            Ok(None) => break,
            result => {
                coverage.write(offset, buffered).await?;
                return Err(match result {
                    Ok(Some(Err(error))) => anyhow::Error::new(error),
                    Err(_) => NetworkStall.into(),
                    _ => unreachable!(),
                });
            }
        };
        anyhow::ensure!(
            offset + buffered.len() as u64 + next.len() as u64 <= end,
            "Response exceeded its validated extent"
        );
        let mut source = next.as_ref();
        while !source.is_empty() {
            let available = coverage.chunk_end(offset) - offset - buffered.len() as u64;
            let take = source.len().min(available as usize);
            buffered.extend_from_slice(&source[..take]);
            source = &source[take..];
            if buffered.len() as u64 + offset == coverage.chunk_end(offset)
                || last_flush.elapsed() >= Duration::from_millis(50)
            {
                let count = buffered.len() as u64;
                buffered = coverage.write(offset, buffered).await?;
                offset += count;
                last_flush = Instant::now();
            }
        }
    }
    let count = buffered.len() as u64;
    coverage.write(offset, buffered).await?;
    anyhow::ensure!(
        offset + count == end,
        "Response ended before its validated extent"
    );
    Ok(())
}

struct Helpers {
    coverage: Arc<Coverage>,
    request: Arc<RequestContext>,
    url: String,
    etag: reqwest::header::HeaderValue,
    next_group: AtomicUsize,
    returned_groups: Mutex<BinaryHeap<(usize, usize, u64)>>,
    primary_finished: AtomicBool,
    failures: AtomicUsize,
    requests: AtomicUsize,
    reconnects: AtomicUsize,
    capacity_retries: AtomicUsize,
}

/// Cancellation returns a partially covered original group before fresh work.
/// Permanent refusals disarm this lease rather than retrying blindly elsewhere.
struct GroupLease<'a> {
    helpers: &'a Helpers,
    group: Option<(usize, usize, u64)>,
}
impl Drop for GroupLease<'_> {
    fn drop(&mut self) {
        if let Some((first, stop, end)) = self.group
            && self.helpers.coverage.first_missing(first, stop).is_some()
        {
            self.helpers
                .returned_groups
                .lock()
                .unwrap()
                .push((first, stop, end));
        }
    }
}

impl Helpers {
    async fn run(&self) -> Result<()> {
        while !self.coverage.complete() {
            // Claim each tail group once. Retry that group's acknowledged
            // remainder locally, avoiding repeated scans of completed groups.
            let returned = self.returned_groups.lock().unwrap().pop();
            let (first, stop, end, _steal) = if let Some((first, stop, end)) = returned {
                (first, stop, end, None)
            } else {
                let group = self.next_group.fetch_add(1, Ordering::Relaxed);
                if group < self.coverage.chunk_count().div_ceil(GROUP_CHUNKS) {
                    let stop = self.coverage.chunk_count() - group * GROUP_CHUNKS;
                    (
                        stop.saturating_sub(GROUP_CHUNKS),
                        stop,
                        (stop as u64 * CHUNK).min(self.coverage.size),
                        None,
                    )
                } else {
                    let Some(claim) = self.coverage.claim_missing() else {
                        break;
                    };
                    (claim.index, claim.index + 1, claim.end, Some(claim))
                }
            };
            let mut lease = GroupLease {
                helpers: self,
                group: _steal.is_none().then_some((first, stop, end)),
            };
            let mut consecutive = 0u32;
            while let Some(start) = self.coverage.first_missing(first, stop) {
                if self.coverage.complete() {
                    return Ok(());
                }
                self.requests.fetch_add(1, Ordering::Relaxed);
                match self.range(start, end).await {
                    Ok(()) => consecutive = 0,
                    Err(error) => {
                        if error.is::<DiskFailure>() {
                            lease.group = None;
                            return Err(error);
                        }
                        if error.is::<reqwest::Error>()
                            || error.is::<NetworkStall>()
                            || error.is::<CapacityRefusal>()
                        {
                            let next = self.coverage.first_missing(first, stop).unwrap_or(end);
                            let progressed = next > start;
                            consecutive = if progressed { 0 } else { consecutive + 1 };
                            if consecutive <= 10 {
                                if error.is::<CapacityRefusal>() {
                                    self.capacity_retries.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    self.reconnects.fetch_add(1, Ordering::Relaxed);
                                }
                                eprintln!(
                                    "[IDG diagnostic] helper retry after acknowledged byte {next}, consecutive={consecutive}"
                                );
                                if !progressed {
                                    tokio::time::sleep(Duration::from_millis(
                                        (50_u64 << consecutive.saturating_sub(1).min(5)).min(1000),
                                    ))
                                    .await;
                                }
                                continue;
                            }
                        }
                        lease.group = None;
                        self.failures.fetch_add(1, Ordering::Relaxed);
                        eprintln!("[IDG diagnostic] optional helper stopped: {error}");
                        // Refused/malformed representations never cause blind
                        // retries. Keep the original response available.
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    async fn range(&self, start: u64, end: u64) -> Result<()> {
        let admitted = Arc::new(AtomicBool::new(false));
        let sending = self.request.send(
            Spec::get(&self.url)
                .header("range", format!("bytes={start}-{}", end - 1))?
                .header("if-range", self.etag.to_str()?)?
                .observe_admission(admitted.clone()),
            Control {
                network_timeout: Some(Duration::from_secs(20)),
                ..Default::default()
            },
        );
        tokio::pin!(sending);
        let LeasedResponse {
            response,
            permit: _permit,
        } = loop {
            tokio::select! {
                biased;
                result = &mut sending => break result?.leased,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    // Another stream can fill a queued helper's prefix. Drop
                    // the superseded request and rebuild its exact remainder.
                    let first = (start / CHUNK) as usize;
                    let stop = end.div_ceil(CHUNK) as usize;
                    let next = self.coverage.first_missing(first, stop).unwrap_or(end);
                    if next == end || (next > start && !admitted.load(Ordering::Acquire)) {
                        return Ok(());
                    }
                }
            }
        };
        if matches!(
            response.status(),
            reqwest::StatusCode::TOO_MANY_REQUESTS | reqwest::StatusCode::SERVICE_UNAVAILABLE
        ) {
            return Err(CapacityRefusal(response.status()).into());
        }
        anyhow::ensure!(
            response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
            "Optional range returned HTTP {}",
            response.status()
        );
        let expected = format!(
            "bytes {start}-{}/{total}",
            end - 1,
            total = self.coverage.size
        );
        anyhow::ensure!(
            response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                == Some(expected.as_str()),
            "Optional range boundaries changed"
        );
        anyhow::ensure!(
            response.headers().get("etag") == Some(&self.etag),
            "Optional range validator changed"
        );
        anyhow::ensure!(
            response.content_length() == Some(end - start),
            "Optional range length changed or was decoded"
        );
        download::probe::validate_file_response(response.headers(), response.url().as_str())?;
        pump(response, start, end, &self.coverage).await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .context("Usage: idg-hedged URL OUTPUT_FILE --helpers 0..31")?;
    let path = PathBuf::from(args.next().context("OUTPUT_FILE required")?);
    let mode = args
        .next()
        .context("Explicit --helpers or --auto-helpers required")?;
    let automatic = mode == "--auto-helpers";
    anyhow::ensure!(
        automatic || mode == "--helpers",
        "Use --helpers or --auto-helpers 0..31"
    );
    let requested: usize = args.next().context("Helper count required")?.parse()?;
    anyhow::ensure!(
        requested <= 31 && args.next().is_none(),
        "Use 0..31 helpers"
    );
    anyhow::ensure!(
        !download::probe::is_mail_export(&url),
        "Diagnostic does not support generated exports"
    );
    let request = Arc::new(RequestContext::new(
        Arc::new(download::http_client()?),
        &url,
        Default::default(),
    )?);
    let started = Instant::now();
    let LeasedResponse { response, permit } = request
        .send(
            Spec::get_once(&url),
            Control {
                network_timeout: Some(Duration::from_secs(20)),
                ..Default::default()
            },
        )
        .await?
        .leased;
    let headers_ms = started.elapsed().as_millis();
    anyhow::ensure!(
        response.status() == reqwest::StatusCode::OK,
        "Full request returned HTTP {}",
        response.status()
    );
    download::probe::validate_file_response(response.headers(), response.url().as_str())?;
    let size = response
        .content_length()
        .context("Diagnostic requires known decoded length")?;
    let etag = response
        .headers()
        .get("etag")
        .filter(|value| {
            value
                .to_str()
                .is_ok_and(|v| v.starts_with('"') && v.ends_with('"'))
        })
        .cloned();
    let url = response.url().to_string();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent).await?;
    }
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await?;
    download::writer::prepare_sparse(&file);
    file.set_len(size).await?;
    let writer = Arc::new(download::writer::DownloadWriter::new(
        file.into_std().await,
    )?);
    let coverage = Arc::new(Coverage::new(size, writer.clone())?);
    let effective = if etag.is_some() && size > 0 {
        requested
    } else {
        0
    };
    let helpers = etag.map(|etag| {
        Arc::new(Helpers {
            coverage: coverage.clone(),
            request,
            url,
            etag,
            next_group: AtomicUsize::new(0),
            returned_groups: Mutex::new(BinaryHeap::new()),
            primary_finished: AtomicBool::new(false),
            failures: AtomicUsize::new(0),
            requests: AtomicUsize::new(0),
            reconnects: AtomicUsize::new(0),
            capacity_retries: AtomicUsize::new(0),
        })
    });
    let mut adaptive = automatic.then(|| adaptive::Adaptive::new(effective, headers_ms));
    let mut helper_handles: Vec<(usize, tokio::task::AbortHandle)> = Vec::new();
    let mut expected_cancellations = HashSet::new();
    let mut peak_helpers = 0;
    let mut trial_cancellations = 0;
    let mut tasks = tokio::task::JoinSet::new();
    let primary_coverage = coverage.clone();
    let primary_helpers = helpers.clone();
    tasks.spawn(async move {
        let _permit = permit;
        let outcome = pump(response, 0, size, &primary_coverage).await;
        if let Some(helpers) = primary_helpers {
            helpers.primary_finished.store(true, Ordering::Release);
        }
        (true, outcome)
    });
    for _ in 0..if automatic { 0 } else { effective } {
        let helpers = helpers.as_ref().unwrap().clone();
        tasks.spawn(async move { (false, helpers.run().await) });
    }
    let mut failures = Vec::new();
    let mut primary_replacements = 0;
    while !coverage.complete() {
        let result = if let Some(controller) = adaptive.as_mut() {
            tokio::select! {
                result = tasks.join_next_with_id() => result,
                _ = tokio::time::sleep_until(controller.deadline()) => {
                    if let Some(target) = controller.observe(coverage.bytes(), size) {
                        // Cancel optional trial workers only. The original
                        // stream and its valid response are never sacrificed.
                        helper_handles.retain(|(index, handle)| {
                            if *index < target { return true; }
                            if !handle.is_finished() {
                                expected_cancellations.insert(handle.id());
                                trial_cancellations += 1;
                                handle.abort();
                            }
                            false
                        });
                        for index in 0..target {
                            if helper_handles.iter().any(|(slot, _)| *slot == index) { continue; }
                            let helpers = helpers.as_ref().unwrap().clone();
                            let handle = tasks.spawn(async move { (false, helpers.run().await) });
                            helper_handles.push((index, handle));
                        }
                        peak_helpers = peak_helpers.max(target);
                    }
                    continue;
                }
            }
        } else {
            tasks.join_next_with_id().await
        };
        let Some(result) = result else {
            break;
        };
        let result = match result {
            Ok((_, result)) => Ok(result),
            Err(error) if error.is_cancelled() && expected_cancellations.remove(&error.id()) => {
                continue;
            }
            Err(error) => Err(error),
        };
        match result {
            Ok((_, Ok(()))) => {}
            Ok((_, Err(error))) if error.is::<DiskFailure>() => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return Err(error);
            }
            Ok((primary, Err(error))) => {
                failures.push(error.to_string());
                if primary && let Some(helpers) = helpers.as_ref() {
                    // Replace the failed original with one range worker. Its
                    // response/lease has closed, so total concurrency stays N+1.
                    primary_replacements += 1;
                    let helpers = helpers.clone();
                    tasks.spawn(async move { (false, helpers.run().await) });
                }
            }
            Err(error) => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return Err(error.into());
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    anyhow::ensure!(
        coverage.complete(),
        "Incomplete acknowledged coverage: {}/{}; {:?}",
        coverage.bytes(),
        size,
        failures
    );
    writer.finish().await?;
    println!(
        "{}",
        serde_json::json!({"path":path,"bytes":coverage.bytes(),"requested_helpers":requested,
        "effective_helpers":if automatic { peak_helpers } else { effective },"automatic":automatic,
        "trial_cancellations":trial_cancellations,"automatic_decisions":adaptive.as_ref().map(|controller| &controller.decisions),"helper_requests":helpers.as_ref().map_or(0,|h|h.requests.load(Ordering::Relaxed)),
        "optional_failures":helpers.as_ref().map_or(0,|h|h.failures.load(Ordering::Relaxed)),
        "helper_reconnects":helpers.as_ref().map_or(0,|h|h.reconnects.load(Ordering::Relaxed)),
        "capacity_retries":helpers.as_ref().map_or(0,|h|h.capacity_retries.load(Ordering::Relaxed)),
        "primary_replacements":primary_replacements,"primary_or_task_failures":failures.len(),"headers_ms":headers_ms,"policy":if automatic {"diagnostic_hedged_adaptive_helpers"} else {"diagnostic_hedged_fixed_helpers"}})
    );
    Ok(())
}
