//! Download subsystem.
//!
//! Modules:
//! - [`probe`]   – HTTP HEAD probe (Content-Length, Accept-Ranges)
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
pub mod segment;
pub mod types;
pub mod writer;

/// The desktop and command-line benchmark use the same production HTTP policy.
pub fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
            "AppleWebKit/537.36 (KHTML, like Gecko) ",
            "Chrome/124.0.0.0 Safari/537.36"
        ))
        .pool_max_idle_per_host(16)
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()?)
}
