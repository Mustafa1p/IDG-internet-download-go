//! Thin CLI around the exact download modules used by the desktop.
//! cargo run --release --example idg-get -- URL OUTPUT_FILE [CONNECTIONS]
#![allow(dead_code)]

#[path = "../src/download/mod.rs"]
mod download;

use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("Usage: idg-get URL OUTPUT_FILE [CONNECTIONS]"))?;
    let path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("OUTPUT_FILE is required"))?,
    );
    let connections = args
        .next()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(8);
    anyhow::ensure!(
        connections <= 32 && args.next().is_none(),
        "CONNECTIONS must be 0 (automatic) or 1..32"
    );
    let item = download::types::DownloadItem::new(url, path, connections);
    let id = item.id.clone();
    let downloads = Arc::new(std::sync::Mutex::new(vec![item]));
    download::engine::start_download(Arc::new(download::http_client()?), downloads.clone(), id)
        .await?;
    let item = downloads.lock().unwrap()[0].clone();
    anyhow::ensure!(
        item.status == download::types::Status::Completed,
        "Download did not complete"
    );
    println!(
        "{}",
        serde_json::json!({"path":item.save_path,"bytes":item.downloaded_bytes,"connections":item.num_segments})
    );
    Ok(())
}
