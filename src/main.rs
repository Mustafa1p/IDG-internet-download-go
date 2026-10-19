//! IDG – Internet Download GO
//!
//! Entry point: initialises the Tokio runtime, builds the shared HTTP client,
//! starts the local API server (port 17843), checks for updates, then hands
//! control to eframe for the GUI event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod desktop;
mod download;
mod server;
mod updater;

use eframe::egui;
use std::sync::Arc;
use std::time::Duration;

/// Current application version — kept in sync with Cargo.toml.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("idg-worker")
        .build()
        .expect("failed to build Tokio runtime");

    let background = std::env::args().any(|argument| argument == "--background");
    let _instance_lock = match desktop::acquire_instance()
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?
    {
        Some(lock) => lock,
        None => {
            if !background {
                runtime.block_on(async {
                    let client = reqwest::Client::builder()
                        .no_proxy()
                        .timeout(Duration::from_millis(500))
                        .build()
                        .unwrap();
                    for _ in 0..25 {
                        if client
                            .post("http://127.0.0.1:17843/show")
                            .send()
                            .await
                            .is_ok_and(|response| response.status().is_success())
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                });
            }
            return Ok(());
        }
    };

    let http_client = Arc::new(runtime.block_on(async {
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
    }));

    let rt_handle = runtime.handle().clone();

    // Keep the runtime alive on a background thread.
    std::thread::Builder::new()
        .name("idg-runtime".to_string())
        .spawn(move || runtime.block_on(std::future::pending::<()>()))
        .expect("failed to spawn runtime thread");

    // Load application icon from the embedded PNG bytes.
    let icon = load_icon();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IDG – Internet Download GO")
            .with_inner_size([1080.0, 640.0])
            .with_min_inner_size([960.0, 480.0])
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "IDG",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::IDGApp::new(
                cc,
                rt_handle,
                http_client,
                background,
            )))
        }),
    )
}

/// Load the embedded icon PNG and convert to egui `IconData`.
fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../extension/images/icon128.png");
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(_) => egui::IconData {
            rgba: vec![],
            width: 0,
            height: 0,
        },
    }
}
