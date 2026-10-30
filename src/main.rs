//! IDG – Internet Download GO
//!
//! Entry point: initialises the Tokio runtime, builds the shared HTTP client,
//! starts the local API server (port 17843), checks for updates, then hands
//! control to eframe for the GUI event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod desktop;
mod download;
mod integration;
mod server;
mod updater;

use eframe::egui;
use std::sync::Arc;
use std::time::Duration;

/// Current application version — kept in sync with Cargo.toml.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Isolated UI review, available only in development builds.
pub fn settings_preview() -> bool {
    cfg!(debug_assertions)
        && std::env::args()
            .any(|argument| argument == "--settings-preview" || argument == "--ui-preview")
}

pub fn native_dpi_preview() -> bool {
    settings_preview() && std::env::args().any(|arg| arg == "--preview-native-dpi")
}

fn settings_preview_size() -> [f32; 2] {
    std::env::args()
        .find_map(|argument| {
            let (width, height) = argument
                .strip_prefix("--settings-preview-size=")?
                .split_once('x')?;
            Some([
                width.parse::<f32>().ok()?.clamp(800.0, 3840.0),
                height.parse::<f32>().ok()?.clamp(500.0, 2160.0),
            ])
        })
        .unwrap_or([1440.0, 1000.0])
}

fn main() -> eframe::Result {
    #[cfg(windows)]
    {
        // Select per-monitor V2 before any library can create a native window.
        // The thread context also prevents an inherited compatibility context
        // from leaving the UI on the startup monitor's scale.
        unsafe {
            use windows_sys::Win32::UI::HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
                SetThreadDpiAwarenessContext,
            };
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let app_id: Vec<u16> = "IDG.InternetDownloadGO\0".encode_utf16().collect();
        // SAFETY: a null-terminated UTF-16 constant, valid for this call.
        unsafe {
            windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr());
        }
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("idg-worker")
        .build()
        .expect("failed to build Tokio runtime");

    let background = std::env::args().any(|argument| argument == "--background");
    let _instance_lock = if settings_preview() {
        None
    } else {
        match desktop::acquire_instance()
            .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?
        {
            Some(lock) => Some(lock),
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
        }
    };

    let http_client = Arc::new(download::http_client().expect("failed to build HTTP client"));

    let rt_handle = runtime.handle().clone();

    // Keep the runtime alive on a background thread.
    std::thread::Builder::new()
        .name("idg-runtime".to_string())
        .spawn(move || runtime.block_on(std::future::pending::<()>()))
        .expect("failed to spawn runtime thread");

    // Load application icon from the embedded PNG bytes.
    let icon = load_icon();

    let options = eframe::NativeOptions {
        window_builder: Some(Box::new(desktop::repair_window_builder)),
        persist_window: !settings_preview(),
        persistence_path: settings_preview().then(|| {
            std::env::temp_dir().join(format!("idg-settings-preview-{}.ron", std::process::id()))
        }),
        viewport: egui::ViewportBuilder::default()
            .with_title(if settings_preview() {
                "IDG Settings Preview"
            } else {
                "IDG – Internet Download GO"
            })
            .with_inner_size(if settings_preview() {
                settings_preview_size()
            } else {
                desktop::DEFAULT_WINDOW_SIZE.into()
            })
            .with_min_inner_size(desktop::MIN_WINDOW_SIZE)
            .with_decorations(false)
            .with_clamp_size_to_monitor_size(!settings_preview())
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        if settings_preview() {
            "IDG Settings Preview"
        } else {
            "IDG"
        },
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
