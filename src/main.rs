//! IDG – Internet Download GO
//!
//! Entry point: initialises the Tokio async runtime, builds the shared HTTP
//! client, then hands control to [`eframe`] for the GUI event loop.
//!
//! The runtime is kept alive on a background thread so that download tasks
//! (`tokio::spawn`) continue running even while the GUI is rendering.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod download;

use std::sync::Arc;
use std::time::Duration;
use eframe::egui;

fn main() -> eframe::Result {
    // Build a multi-threaded Tokio runtime.  We create it manually so we can
    // hold a Handle and keep it alive for the entire application lifetime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("idg-worker")
        .build()
        .expect("failed to build Tokio runtime");

    // Build a shared HTTP client inside the runtime context.
    // Reusing one client across all downloads enables connection pooling.
    let http_client = Arc::new(
        runtime.block_on(async {
            reqwest::Client::builder()
                .user_agent(concat!(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
                    "AppleWebKit/537.36 (KHTML, like Gecko) ",
                    "Chrome/124.0.0.0 Safari/537.36"
                ))
                .pool_max_idle_per_host(16)
                .tcp_keepalive(Duration::from_secs(30))
                .timeout(Duration::from_secs(60))
                .connect_timeout(Duration::from_secs(15))
                .build()
                .expect("failed to build HTTP client")
        }),
    );

    let runtime_handle = runtime.handle().clone();

    // Keep the runtime running on a background thread.
    // `block_on(pending())` parks the thread indefinitely without burning CPU.
    std::thread::Builder::new()
        .name("idg-runtime".to_string())
        .spawn(move || runtime.block_on(std::future::pending::<()>()))
        .expect("failed to spawn runtime thread");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IDG – Internet Download GO")
            .with_inner_size([920.0, 520.0])
            .with_min_inner_size([700.0, 380.0]),
        ..Default::default()
    };

    eframe::run_native(
        "IDG",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::IDGApp::new(cc, runtime_handle, http_client)))
        }),
    )
}
