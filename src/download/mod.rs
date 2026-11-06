//! Download subsystem.
//!
//! Modules:
//! - [`probe`]   – Validated GET metadata and range probe
//! - [`segment`] – Single range-request worker
//! - [`engine`]  – Orchestrator: splits file, spawns workers, tracks progress
//! - [`types`]   – Shared data types and state

pub mod bandwidth;
pub mod connections;
pub mod engine;
pub mod gate;
pub mod history;
pub mod media;
pub mod music;
pub mod probe;
pub mod request;
pub mod segment;
pub mod types;
pub mod writer;

/// The desktop and command-line benchmark use the same production HTTP policy.
pub fn http_client_builder() -> reqwest::ClientBuilder {
    // Concurrent ranges use independent, reusable HTTP/1 connections. A shared
    // HTTP/2 connection showed long tail delays in the same-machine trials.
    http_transport_builder().http1_only()
}

/// Common transport settings; automatic protocol negotiation is also available
/// to the diagnostic CLI so protocol experiments do not alter desktop settings.
pub fn http_transport_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
            "AppleWebKit/537.36 (KHTML, like Gecko) ",
            "Chrome/124.0.0.0 Safari/537.36"
        ))
        // Negotiate HTTP/2 over TLS while retaining HTTP/1.1 fallback. Adaptive
        // windows avoid limiting high-latency transfers to a small fixed window.
        .http2_adaptive_window(true)
        .pool_max_idle_per_host(connections::MAX_CONNECTIONS)
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(15))
}

pub fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(http_client_builder().build()?)
}

/// Wake network operations promptly when the UI cancels a download.
pub(crate) async fn cancelled(flag: &std::sync::atomic::AtomicBool) {
    while !flag.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
