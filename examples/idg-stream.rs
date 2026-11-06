//! Diagnostic: one full GET using the production request/segment/writer modules.
//! This isolates orchestration cost; it does not provide the app's range recovery.
//! cargo run --release --example idg-stream -- URL OUTPUT_FILE
#![allow(dead_code)]

#[path = "../src/download/mod.rs"]
mod download;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let url = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("Usage: idg-stream URL OUTPUT_FILE"))?;
    let path = PathBuf::from(
        arguments
            .next()
            .ok_or_else(|| anyhow::anyhow!("OUTPUT_FILE is required"))?,
    );
    anyhow::ensure!(arguments.next().is_none(), "Unexpected argument");
    let client = Arc::new(download::http_client()?);
    let request = Arc::new(download::request::RequestContext::new(
        client,
        &url,
        reqwest::header::HeaderMap::new(),
    )?);
    let started = Instant::now();
    let download::gate::LeasedResponse { response, permit } = request
        .send(
            download::request::Spec::get_once(&url),
            download::request::Control {
                network_timeout: Some(Duration::from_secs(20)),
                ..Default::default()
            },
        )
        .await?
        .leased;
    let header_millis = started.elapsed().as_millis();
    let response = response.error_for_status()?;
    anyhow::ensure!(
        response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT,
        "A full GET must return a complete representation"
    );
    download::probe::validate_file_response(response.headers(), response.url().as_str())?;
    let expected = response.content_length();
    let protocol = format!("{:?}", response.version());
    let final_url = response.url().to_string();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await?;
    if let Some(length) = expected {
        file.set_len(length).await?;
    }
    let file = Arc::new(download::writer::DownloadWriter::new(
        file.into_std().await,
    )?);
    let received = Arc::new(AtomicU64::new(0));
    let progress = received.clone();
    let args = download::segment::SegmentArgs {
        handoff: None,
        initial_permit: None,
        gate: Arc::new(download::gate::ConnectionGate::new(1)),
        if_range: None,
        total_size: expected,
        bandwidth: Arc::new(download::bandwidth::BandwidthLimiter::new(0)),
        response: Some(download::gate::LeasedResponse { response, permit }),
        request,
        id: 0,
        url: final_url,
        start: 0,
        end: None,
        file: file.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        on_progress: Box::new(move |bytes| {
            progress.fetch_add(bytes, Ordering::Relaxed);
        }),
    };
    download::segment::run_segment(args, 0).await?;
    let received = received.load(Ordering::Relaxed);
    if let Some(expected) = expected {
        anyhow::ensure!(received == expected, "Full stream byte count mismatch");
    }
    file.finish().await?;
    println!(
        "{}",
        serde_json::json!({
            "path":path, "bytes":received, "connections":1,
            "policy":"diagnostic_full_get_http1", "protocol":protocol,
            "headers_ms":header_millis
        })
    );
    Ok(())
}
