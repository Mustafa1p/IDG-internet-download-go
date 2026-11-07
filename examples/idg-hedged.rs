//! Experimental full stream with optional validated tail helpers.
//! No production policy changes. Known length and strong ETag required for helpers.
//! Usage: idg-hedged URL OUTPUT_FILE --helpers 0..31
#![allow(dead_code)]
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
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const GROUP_CHUNKS: usize = 4;

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
        if coverage.complete() {
            return Ok(());
        }
        let next = match tokio::time::timeout(Duration::from_secs(15), stream.next()).await {
            Ok(Some(Ok(bytes))) => bytes,
            Ok(None) => break,
            result => {
                coverage.write(offset, buffered).await?;
                return Err(match result {
                    Ok(Some(Err(error))) => anyhow::Error::new(error),
                    Err(_) => anyhow::anyhow!("Response body stalled"),
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
    claims: Vec<AtomicBool>,
    primary_finished: AtomicBool,
    failures: AtomicUsize,
    requests: AtomicUsize,
}

impl Helpers {
    async fn run(&self) -> Result<()> {
        let threshold = CHUNK.min(self.coverage.size / 8);
        while self.coverage.bytes() < threshold && !self.primary_finished.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        while !self.coverage.complete() {
            let choice = self.claims.iter().enumerate().find_map(|(group, claim)| {
                // Keep a tiny final fragment together with earlier chunks.
                let stop = self.coverage.chunk_count() - group * GROUP_CHUNKS;
                let first = stop.saturating_sub(GROUP_CHUNKS);
                let start = self.coverage.first_missing(first, stop)?;
                claim
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                    .ok()?;
                Some((group, start, (stop as u64 * CHUNK).min(self.coverage.size)))
            });
            let Some((group, start, end)) = choice else {
                break;
            };
            self.requests.fetch_add(1, Ordering::Relaxed);
            let result = self.range(start, end).await;
            self.claims[group].store(false, Ordering::Release);
            if let Err(error) = result {
                if error.is::<DiskFailure>() {
                    return Err(error);
                }
                self.failures.fetch_add(1, Ordering::Relaxed);
                eprintln!("[IDG diagnostic] optional helper stopped: {error}");
                // Original response stays open. No blind size/authentication retries.
                return Ok(());
            }
        }
        Ok(())
    }

    async fn range(&self, start: u64, end: u64) -> Result<()> {
        let LeasedResponse {
            response,
            permit: _permit,
        } = self
            .request
            .send(
                Spec::get(&self.url)
                    .header("range", format!("bytes={start}-{}", end - 1))?
                    .header("if-range", self.etag.to_str()?)?,
                Control {
                    network_timeout: Some(Duration::from_secs(20)),
                    ..Default::default()
                },
            )
            .await?
            .leased;
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
    anyhow::ensure!(
        args.next().as_deref() == Some("--helpers"),
        "Explicit --helpers 0..31 required"
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
            claims: (0..coverage.chunk_count().div_ceil(GROUP_CHUNKS))
                .map(|_| AtomicBool::new(false))
                .collect(),
            primary_finished: AtomicBool::new(false),
            failures: AtomicUsize::new(0),
            requests: AtomicUsize::new(0),
        })
    });
    let mut tasks = tokio::task::JoinSet::new();
    let primary_coverage = coverage.clone();
    let primary_helpers = helpers.clone();
    tasks.spawn(async move {
        let _permit = permit;
        let outcome = pump(response, 0, size, &primary_coverage).await;
        if let Some(helpers) = primary_helpers {
            helpers.primary_finished.store(true, Ordering::Release);
        }
        outcome
    });
    for _ in 0..effective {
        let helpers = helpers.as_ref().unwrap().clone();
        tasks.spawn(async move { helpers.run().await });
    }
    let mut failures = Vec::new();
    while !coverage.complete() {
        let Some(result) = tasks.join_next().await else {
            break;
        };
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) if error.is::<DiskFailure>() => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return Err(error);
            }
            Ok(Err(error)) => failures.push(error.to_string()),
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
        "effective_helpers":effective,"helper_requests":helpers.as_ref().map_or(0,|h|h.requests.load(Ordering::Relaxed)),
        "optional_failures":helpers.as_ref().map_or(0,|h|h.failures.load(Ordering::Relaxed)),
        "primary_or_task_failures":failures.len(),"headers_ms":headers_ms,"policy":"diagnostic_hedged_fixed_helpers"})
    );
    Ok(())
}
