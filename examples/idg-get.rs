//! Thin CLI around the exact download modules used by the desktop.
//! cargo run --release --example idg-get -- URL OUTPUT_FILE [CONNECTIONS]
//! Optional --http1, --http2, --no-pool, --http2-fixed and --reuse-prefix policies are diagnostics only.
//! With no diagnostic flag, this uses the desktop's production HTTP policy.
#![allow(dead_code)]

#[path = "../src/download/mod.rs"]
mod download;

use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: idg-get URL OUTPUT_FILE [CONNECTIONS] [--http1|--http2|--no-pool|--http2-fixed|--reuse-prefix]"
        )
    })?;
    let path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("OUTPUT_FILE is required"))?,
    );
    let connections = args
        .next()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(8);
    let policy = args.next();
    anyhow::ensure!(
        connections <= 32 && args.next().is_none(),
        "CONNECTIONS must be 0 (automatic) or 1..32"
    );
    let builder = match policy.as_deref() {
        None | Some("--http1") | Some("--reuse-prefix") => download::http_client_builder(),
        Some("--http2") => download::http_transport_builder(),
        Some("--no-pool") => download::http_transport_builder().pool_max_idle_per_host(0),
        Some("--http2-fixed") => download::http_transport_builder()
            .http2_adaptive_window(false)
            .http2_initial_stream_window_size(2 * 1024 * 1024)
            .http2_initial_connection_window_size(32 * 1024 * 1024),
        Some(_) => {
            anyhow::bail!(
                "Unknown diagnostic policy; use --http1, --http2, --no-pool, --http2-fixed or --reuse-prefix"
            )
        }
    };
    let client = Arc::new(builder.build()?);
    let item = download::types::DownloadItem::new(url, path, connections);
    let id = item.id.clone();
    let downloads = Arc::new(std::sync::Mutex::new(vec![item]));
    // Opt-in diagnostics stay out of normal runs and retain the production
    // engine's behavior while separating setup delays from slow body transfer.
    let reporter = std::env::var_os("IDG_TRACE_PROGRESS").map(|_| {
        let downloads = downloads.clone();
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tick.tick().await;
                let items = downloads.lock().unwrap();
                if let Some(item) = items.first() {
                    eprintln!(
                        "IDG_TRACE {}",
                        serde_json::json!({
                            "elapsed_ms": started.elapsed().as_millis(),
                            "bytes": item.downloaded_bytes, "total": item.total_bytes,
                            "segments": item.num_segments, "status": format!("{:?}", item.status),
                        })
                    );
                }
            }
        })
    });
    let result = if policy.as_deref() == Some("--reuse-prefix") {
        download::engine::start_download_with_initial_range(client, downloads.clone(), id).await
    } else {
        download::engine::start_download(client, downloads.clone(), id).await
    };
    if let Some(reporter) = reporter {
        reporter.abort();
    }
    result?;
    let item = downloads.lock().unwrap()[0].clone();
    anyhow::ensure!(
        item.status == download::types::Status::Completed,
        "Download did not complete"
    );
    println!(
        "{}",
        serde_json::json!({"path":item.save_path,"bytes":item.downloaded_bytes,"connections":item.num_segments,"policy":policy.as_deref().unwrap_or("production")})
    );
    Ok(())
}
