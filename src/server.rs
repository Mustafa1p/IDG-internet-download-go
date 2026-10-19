//! Local HTTP API server — listens on `127.0.0.1:1001`.
//!
//! The browser extension connects here to:
//! - Check IDG is running (`GET /health`)
//! - Send download requests (`POST /downloads`)
//! - Read the current download queue (`GET /downloads`)
//!
//! The server shares the same [`Downloads`] list and HTTP client as the GUI,
//! so downloads started by the extension appear in the UI instantly.

use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::download::{
    engine::start_download,
    types::{DownloadItem, Downloads, Status},
};

// ---------------------------------------------------------------------------
// Shared server state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ServerState {
    pub downloads: Downloads,
    pub client: Arc<reqwest::Client>,
    pub rt_handle: tokio::runtime::Handle,
    pub default_dir: String,
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddDownloadRequest {
    pub url: String,
    pub filename: Option<String>,
    pub save_dir: Option<String>,
    pub segments: Option<usize>,
    pub referrer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddDownloadResponse {
    pub id: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn add_download(
    State(state): State<ServerState>,
    Json(req): Json<AddDownloadRequest>,
) -> impl IntoResponse {
    let save_dir = req.save_dir.unwrap_or(state.default_dir.clone());
    let filename = req
        .filename
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| filename_from_url(&req.url));

    let save_path = PathBuf::from(&save_dir).join(&filename);
    let segments = req.segments.unwrap_or(8).clamp(1, 32);

    let mut item = DownloadItem::new(req.url, save_path, segments);
    let id = item.id.clone();

    // Attach extra headers the extension may have forwarded
    // (stored in the item for the engine to use; extend DownloadItem if needed)
    let _ = (req.referrer, req.cookies, req.user_agent); // reserved for future

    state.downloads.lock().unwrap().push(item);

    let dl = state.downloads.clone();
    let client = state.client.clone();
    let id2 = id.clone();

    state.rt_handle.spawn(async move {
        if let Err(e) = start_download(client, dl.clone(), id2.clone()).await {
            crate::download::types::with_item(&dl, &id2, |i| {
                i.status = Status::Failed;
                i.error = Some(e.to_string());
            });
        }
    });

    (StatusCode::CREATED, Json(AddDownloadResponse { id }))
}

async fn list_downloads(State(state): State<ServerState>) -> impl IntoResponse {
    let guard = state.downloads.lock().unwrap();
    let list: Vec<serde_json::Value> = guard
        .iter()
        .map(|i| {
            serde_json::json!({
                "id":               i.id,
                "url":              i.url,
                "filename":         i.filename,
                "total_bytes":      i.total_bytes,
                "downloaded_bytes": i.downloaded_bytes,
                "speed_bps":        i.speed_bps,
                "status":           format!("{:?}", i.status),
            })
        })
        .collect();
    Json(list)
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Build the Axum router.
pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/downloads", post(add_download).get(list_downloads))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the API server in the background.  Binds to `127.0.0.1:1001`.
pub async fn run_server(state: ServerState) {
    let app = build_router(state);
    match tokio::net::TcpListener::bind("127.0.0.1:1001").await {
        Ok(listener) => {
            eprintln!("[IDG] API server listening on http://127.0.0.1:1001");
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("[IDG] API server error: {e}");
            }
        }
        Err(e) => eprintln!("[IDG] Cannot bind port 1001: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn filename_from_url(url: &str) -> String {
    url.split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("download")
        .to_string()
}
