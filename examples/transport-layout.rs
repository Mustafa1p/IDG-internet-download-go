//! Diagnose inline async state without polling requests or touching files.
//! cargo run --release --example transport-layout
#![allow(dead_code)]

#[path = "../src/download/mod.rs"]
mod download;

use std::{mem::size_of_val, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = "http://layout.invalid/file";
    let client = Arc::new(download::http_client()?);
    let context = download::request::RequestContext::new(
        client.clone(),
        url,
        reqwest::header::HeaderMap::new(),
    )?;
    let budget = download::gate::HostBudget::for_url(url);
    let admission = budget.acquire_request(true);
    let request = context.send(
        download::request::Spec::get(url),
        download::request::Control::default(),
    );
    let item = download::types::DownloadItem::new(
        url.into(),
        std::env::temp_dir().join("IDG-layout-not-polled.bin"),
        8,
    );
    let id = item.id.clone();
    let downloads = Arc::new(std::sync::Mutex::new(vec![item]));
    let engine = download::engine::start_download(client, downloads, id);
    // All futures remain unpolled: no DNS, HTTP request or output file exists.
    println!(
        "{}",
        serde_json::json!({
            "build": env!("IDG_BUILD_ID"),
            "scope": "Inline future state in this optimized binary; excludes heap allocations and working set",
            "admission_bytes": size_of_val(&admission),
            "request_bytes": size_of_val(&request),
            "engine_bytes": size_of_val(&engine),
        })
    );
    Ok(())
}
