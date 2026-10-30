//! Local HTTP API server — listens on `127.0.0.1:17843`.
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
    connections::SharedConnectionSettings,
    engine::start_download,
    types::{DownloadItem, Downloads, Status, safe_filename},
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
    pub egui_ctx: egui::Context,
    pub connection_settings: SharedConnectionSettings,
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddDownloadRequest {
    pub url: String,
    #[serde(default)]
    pub show_dialog: bool,
    pub filename: Option<String>,
    pub save_dir: Option<String>,
    pub segments: Option<usize>,
    pub referrer: Option<String>,
    pub cookies: Option<String>,
    pub media_cookies: Option<String>,
    pub media_options: Option<crate::download::media::MediaOptions>,
    pub user_agent: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub post_data: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddDownloadResponse {
    pub id: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    build: &'static str,
    capabilities: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        build: env!("IDG_BUILD_ID"),
        capabilities: serde_json::json!({"youtube": crate::download::media::tools_available(), "x": crate::download::media::tools_available(), "social_media": crate::download::media::tools_available(), "music": crate::download::music::ready(), "post_downloads": true}),
    })
}

async fn add_download(
    State(state): State<ServerState>,
    Json(req): Json<AddDownloadRequest>,
) -> impl IntoResponse {
    if let Some(reason) = crate::download::media::unsupported_reason(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": reason})),
        );
    }
    if !reqwest::Url::parse(&req.url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Use an HTTP or HTTPS download URL"})),
        );
    }
    if crate::download::media::video_url(&req.url).is_some()
        && !crate::download::media::tools_available()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                serde_json::json!({"error": "Video tools are missing. Install the complete IDG package."}),
            ),
        );
    }
    if req
        .media_cookies
        .as_ref()
        .is_some_and(|cookies| cookies.len() > 128 * 1024)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Cookie data is too large"})),
        );
    }
    let save_dir = req.save_dir.unwrap_or(state.default_dir.clone());
    if req
        .post_data
        .as_ref()
        .is_some_and(|body| body.len() > 65536)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Download form is too large"})),
        );
    }
    let preserve_filename = req.filename.as_ref().is_some_and(|name| {
        let name = safe_filename(name);
        name.contains('.') && name != filename_from_url(&req.url)
    });
    let filename = req
        .filename
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| filename_from_url(&req.url));

    let save_path = PathBuf::from(&save_dir).join(safe_filename(&filename));
    let segments = req
        .segments
        .unwrap_or_else(|| state.connection_settings.lock().unwrap().requested_count())
        .min(32);

    let mut item = DownloadItem::new(req.url, save_path, segments);
    item.bandwidth = Arc::new(crate::download::bandwidth::BandwidthLimiter::new(
        state
            .connection_settings
            .lock()
            .unwrap()
            .speed_limit_bps
            .unwrap_or(0),
    ));
    item.preserve_filename = preserve_filename;
    let id = item.id.clone();

    for (name, value) in [
        ("referer", req.referrer),
        ("cookie", req.cookies),
        ("user-agent", req.user_agent),
    ] {
        if let Some(value) = value {
            let Ok(value) = reqwest::header::HeaderValue::from_str(&value) else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Invalid request header"})),
                );
            };
            item.headers
                .insert(reqwest::header::HeaderName::from_static(name), value);
        }
    }
    // Only replay captured credentials to the original, recognized mail endpoint.
    if crate::download::probe::is_mail_export(&item.url) {
        for (name, value) in req.headers.unwrap_or_default() {
            let name = name.to_ascii_lowercase();
            if !matches!(
                name.as_str(),
                "authorization"
                    | "cookie"
                    | "referer"
                    | "origin"
                    | "user-agent"
                    | "x-csrf-token"
                    | "x-xsrf-token"
                    | "x-requested-with"
            ) {
                continue;
            }
            let Ok(value) = reqwest::header::HeaderValue::from_str(&value) else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Invalid request header"})),
                );
            };
            item.headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value,
            );
        }
    }
    item.awaiting_confirmation = req.show_dialog;
    item.media_cookies = req.media_cookies;
    item.post_data = req.post_data;
    item.media_options = req.media_options.unwrap_or_default();

    {
        let mut downloads = state.downloads.lock().unwrap();
        if state
            .egui_ctx
            .data(|data| data.get_temp::<bool>(egui::Id::new("idg-installing")))
            .unwrap_or(false)
        {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    serde_json::json!({"error":"IDG is installing an update. Please retry after it restarts."}),
                ),
            );
        }
        downloads.push(item);
    }

    let dl = state.downloads.clone();
    let client = state.client.clone();
    let id2 = id.clone();

    if !req.show_dialog {
        state.rt_handle.spawn(async move {
            if let Err(e) = start_download(client, dl.clone(), id2.clone()).await {
                crate::download::types::with_item(&dl, &id2, |i| {
                    i.status = Status::Failed;
                    i.error = Some(e.to_string());
                });
            }
        });
    }
    state
        .egui_ctx
        .data_mut(|data| data.insert_temp(egui::Id::new("idg-reveal-download"), id.clone()));
    crate::desktop::show_window(&state.egui_ctx);
    state.egui_ctx.request_repaint();
    (
        StatusCode::CREATED,
        Json(serde_json::json!(AddDownloadResponse { id })),
    )
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
                "added_at": i.added_at.to_rfc3339(),
                "error": i.error,
            })
        })
        .collect();
    Json(list)
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Build the Axum router.
async fn show_window(State(state): State<ServerState>) -> StatusCode {
    crate::desktop::show_window(&state.egui_ctx);
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct ExtensionHeartbeat {
    version: String,
    install_type: String,
}
async fn extension_heartbeat(
    State(state): State<ServerState>,
    Json(report): Json<ExtensionHeartbeat>,
) -> impl IntoResponse {
    if report.version.len() > 32 || report.install_type.len() > 32 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Invalid extension report"})),
        );
    }
    let automatic = state
        .egui_ctx
        .data(|data| data.get_temp::<bool>(egui::Id::new("extension-auto-update")))
        .unwrap_or(true);
    let error = if automatic && report.install_type == "development" {
        tokio::task::spawn_blocking(crate::integration::synchronize)
            .await
            .ok()
            .and_then(Result::err)
            .map(|e| e.to_string())
    } else {
        None
    };
    let reload = automatic
        && report.install_type == "development"
        && report.version != crate::APP_VERSION
        && error.is_none();
    state.egui_ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("extension-connection"),
            crate::integration::Connection {
                version: report.version,
                seen: std::time::Instant::now(),
                install_type: report.install_type,
                error,
            },
        )
    });
    state.egui_ctx.request_repaint();
    (
        StatusCode::OK,
        Json(serde_json::json!({"reload":reload,"version":crate::APP_VERSION})),
    )
}

pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/extension/heartbeat", post(extension_heartbeat))
        .route("/show", post(show_window))
        .route("/downloads", post(add_download).get(list_downloads))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
                    origin.as_bytes().starts_with(b"chrome-extension://")
                }))
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        )
        .with_state(state)
}

/// Start the API server in the background.  Binds to `127.0.0.1:17843`.
pub async fn run_server(state: ServerState) {
    let ctx = state.egui_ctx.clone();
    let app = build_router(state);
    match tokio::net::TcpListener::bind("127.0.0.1:17843").await {
        Ok(listener) => {
            eprintln!("[IDG] API server listening on http://127.0.0.1:17843");
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("[IDG] API server error: {e}");
            }
        }
        Err(e) => {
            let error = format!("Browser integration could not start on port 17843: {e}");
            eprintln!("[IDG] {error}");
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("idg-api-error"), error));
            ctx.request_repaint();
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn filename_from_url(url: &str) -> String {
    crate::download::probe::filename_from_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::connections::ConnectionSettings;
    use axum::http::{HeaderMap, Method};
    use std::sync::Mutex;
    use std::time::Duration;

    #[tokio::test]
    async fn hostinger_exports_use_one_authenticated_get_and_keep_eml_and_zip_bytes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        const EML: &[u8] =
            b"From: sender@example.com\r\nSubject: Export test\r\n\r\nTest message.\r\n";
        const ZIP: &[u8] = include_bytes!("../tests/fixtures/email-export.zip");
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let (source, task) = serve(Router::new().route(
            "/bridge/api/v3/mailboxes/test/messages/sources/export",
            get(
                move |method: Method, uri: axum::http::Uri, headers: HeaderMap| {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(method, Method::GET, "Exports must not be probed with HEAD");
                        assert!(!headers.contains_key("range"));
                        assert_eq!(headers["cookie"], "session=abc");
                        assert_eq!(headers["authorization"], "Bearer export-test");
                        assert_eq!(headers["x-csrf-token"], "csrf-test");
                        let query = uri.query().unwrap();
                        let (bytes, name, mime) = if query.contains("&m=") {
                            assert!(query.starts_with("m=534-INBOX.Sent&m=533-INBOX.Sent"));
                            (ZIP, "sent-emails.zip", "application/zip")
                        } else {
                            assert!(query.starts_with("m=2464-INBOX"));
                            (EML, "message.eml", "message/rfc822")
                        };
                        let mut result = HeaderMap::new();
                        result.insert("content-type", mime.parse().unwrap());
                        // Even if an export advertises ranges, it must remain a stream.
                        result.insert("accept-ranges", "bytes".parse().unwrap());
                        if query.contains("named") {
                            result.insert(
                                "content-disposition",
                                format!("attachment; filename={name}").parse().unwrap(),
                            );
                            (result, bytes.to_vec()).into_response()
                        } else {
                            let chunks: Vec<_> = bytes
                                .chunks(13)
                                .map(|chunk| {
                                    Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk))
                                })
                                .collect();
                            (
                                result,
                                axum::body::Body::from_stream(futures::stream::iter(chunks)),
                            )
                                .into_response()
                        }
                    }
                },
            ),
        ))
        .await;
        let socket: std::net::SocketAddr = source.strip_prefix("http://").unwrap().parse().unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("cookie", "session=abc".parse().unwrap());
        headers.insert("authorization", "Bearer export-test".parse().unwrap());
        headers.insert("x-csrf-token", "csrf-test".parse().unwrap());
        let client = Arc::new(
            reqwest::Client::builder()
                .no_proxy()
                .resolve("mail.hostinger.com", socket)
                .default_headers(headers)
                .build()
                .unwrap(),
        );
        let directory = tempfile::tempdir().unwrap();
        let downloads = Arc::new(Mutex::new(Vec::new()));
        for (query, expected_name, expected_bytes) in [
            ("m=2464-INBOX", "message.eml", EML),
            ("m=534-INBOX.Sent&m=533-INBOX.Sent", "messages.zip", ZIP),
            (
                "m=534-INBOX.Sent&m=533-INBOX.Sent&named=1",
                "sent-emails.zip",
                ZIP,
            ),
        ] {
            let before = calls.load(Ordering::SeqCst);
            let mut item = DownloadItem::new(
                format!(
                    "http://mail.hostinger.com:{}/bridge/api/v3/mailboxes/test/messages/sources/export?{query}",
                    socket.port()
                ),
                directory.path().join("export"),
                32,
            );
            item.preserve_filename = false;
            let id = item.id.clone();
            downloads.lock().unwrap().push(item);
            start_download(client.clone(), downloads.clone(), id)
                .await
                .unwrap();
            let item = downloads.lock().unwrap().last().unwrap().clone();
            assert_eq!(item.status, Status::Completed);
            assert_eq!(item.filename, expected_name);
            assert_eq!(item.num_segments, 1);
            assert_eq!(item.downloaded_bytes, expected_bytes.len() as u64);
            assert_eq!(std::fs::read(&item.save_path).unwrap(), expected_bytes);
            assert_eq!(calls.load(Ordering::SeqCst), before + 1);
        }
        task.abort();
    }

    #[tokio::test]
    async fn hostinger_login_and_api_errors_are_reported_before_creating_files() {
        let (source, task) = serve(Router::new().route(
            "/bridge/api/v3/mailboxes/test/messages/sources/export",
            get(|uri: axum::http::Uri| async move {
                match uri.query().unwrap() {
                    "unauthorized" => StatusCode::UNAUTHORIZED.into_response(),
                    "forbidden" => StatusCode::FORBIDDEN.into_response(),
                    "html" => axum::response::Html("Sign in").into_response(),
                    _ => Json(serde_json::json!({"error":"Session expired"})).into_response(),
                }
            }),
        ))
        .await;
        let socket: std::net::SocketAddr = source.strip_prefix("http://").unwrap().parse().unwrap();
        let client = Arc::new(
            reqwest::Client::builder()
                .no_proxy()
                .resolve("mail.hostinger.com", socket)
                .build()
                .unwrap(),
        );
        let directory = tempfile::tempdir().unwrap();
        for query in ["unauthorized", "forbidden", "html", "json"] {
            let path = directory.path().join("messages.zip");
            let item = DownloadItem::new(
                format!(
                    "http://mail.hostinger.com:{}/bridge/api/v3/mailboxes/test/messages/sources/export?{query}",
                    socket.port()
                ),
                path.clone(),
                0,
            );
            let id = item.id.clone();
            let downloads = Arc::new(Mutex::new(vec![item]));
            let error = start_download(client.clone(), downloads, id)
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("IDG browser extension"),
                "{error}"
            );
            assert!(!path.exists());
        }
        task.abort();
    }

    async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (address, task)
    }

    fn state(directory: &std::path::Path) -> ServerState {
        ServerState {
            downloads: Arc::new(Mutex::new(Vec::new())),
            client: Arc::new(reqwest::Client::new()),
            rt_handle: tokio::runtime::Handle::current(),
            default_dir: directory.to_string_lossy().into_owned(),
            egui_ctx: egui::Context::default(),
            connection_settings: Arc::new(Mutex::new(ConnectionSettings::default())),
        }
    }

    #[tokio::test]
    async fn spotify_requests_require_the_optional_backend_before_queueing() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let downloads = state.downloads.clone();
        let (address, server) = serve(build_router(state)).await;
        let response = reqwest::Client::new().post(format!("{address}/downloads"))
            .json(&serde_json::json!({"url":"https://open.spotify.com/track/3uCdelPvIjIA0q0V6KRWFY?si=test", "show_dialog":true}))
            .send().await.unwrap();
        if crate::download::music::ready() {
            assert_eq!(response.status(), StatusCode::CREATED);
            assert_eq!(downloads.lock().unwrap().len(), 1);
            assert!(downloads.lock().unwrap()[0].awaiting_confirmation);
        } else {
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(
                response.json::<serde_json::Value>().await.unwrap()["error"]
                    .as_str()
                    .unwrap()
                    .contains("Settings > Music")
            );
            assert!(downloads.lock().unwrap().is_empty());
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        server.abort();
    }

    #[tokio::test]
    async fn browser_request_waits_for_confirmation_and_reports_popup_schema() {
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let state = state(&directory);
        state.connection_settings.lock().unwrap().speed_limit_bps = Some(65536);
        let downloads = state.downloads.clone();
        let context = state.egui_ctx.clone();
        let (address, server) = serve(build_router(state)).await;
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{address}/downloads"))
            .json(&serde_json::json!({
                "url": "https://example.com/file.zip", "filename": "C:\\Downloads\\file.zip",
                "show_dialog": true, "cookies": "session=abc", "segments": 100
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        {
            let items = downloads.lock().unwrap();
            assert_eq!(
                context.data(|data| data.get_temp::<String>(egui::Id::new("idg-reveal-download"))),
                Some(items[0].id.clone())
            );
            assert!(items[0].awaiting_confirmation);
            assert_eq!(items[0].status, Status::Queued);
            assert_eq!(items[0].save_path, directory.join("file.zip"));
            assert_eq!(items[0].headers["cookie"], "session=abc");
            assert_eq!(items[0].num_segments, 32);
            assert_eq!(items[0].bandwidth.rate(), 65536);
            assert!(!directory.exists());
        }
        let list: serde_json::Value = client
            .get(format!("{address}/downloads"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(list[0]["downloaded_bytes"], 0);
        assert_eq!(list[0]["status"], "Queued");
        assert!(list[0]["added_at"].is_string());
        let bad = client
            .post(format!("{address}/downloads"))
            .json(&serde_json::json!({"url": "file:///secret"}))
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        server.abort();
    }

    #[tokio::test]
    async fn captured_export_headers_are_allowed_only_for_hostinger_exports() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let downloads = state.downloads.clone();
        let (address, task) = serve(build_router(state)).await;
        for (host, captured) in [("mail.hostinger.com", true), ("example.com", false)] {
            let response = reqwest::Client::new().post(format!("{address}/downloads"))
                .json(&serde_json::json!({
                    "url": format!("https://{host}/bridge/api/v3/mailboxes/test/messages/sources/export?m=1-INBOX"),
                    "show_dialog": true,
                    "cookies": "fallback=abc",
                    "headers": {"Authorization": "Bearer test", "Cookie": "captured=abc", "X-CSRF-Token": "csrf", "Host": "evil.example", "Range": "bytes=0-0"}
                })).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let item = downloads.lock().unwrap().last().unwrap().clone();
            assert_eq!(item.headers.contains_key("authorization"), captured);
            assert_eq!(item.headers.contains_key("x-csrf-token"), captured);
            assert_eq!(
                item.headers["cookie"],
                if captured {
                    "captured=abc"
                } else {
                    "fallback=abc"
                }
            );
            assert!(!item.headers.contains_key("host"));
            assert!(!item.headers.contains_key("range"));
        }
        task.abort();
    }

    #[tokio::test]
    async fn redirected_archives_resolve_server_names_and_preserve_user_edits() {
        let (source, source_task) = serve(
            Router::new()
                .route(
                    "/archive/refs/heads/main.zip",
                    get(|| async { axum::response::Redirect::temporary("/zip/refs/heads/main") }),
                )
                .route(
                    "/zip/refs/heads/main",
                    get(|| async {
                        (
                            [(
                                "content-disposition",
                                "attachment; filename=Anti-Youtube-Ads-Extension-main.zip",
                            )],
                            b"archive bytes".to_vec(),
                        )
                    }),
                ),
        )
        .await;
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let downloads = state.downloads.clone();
        let (api, api_task) = serve(build_router(state)).await;
        let client = reqwest::Client::new();
        for (number, supplied) in [None, Some("main"), Some("main.zip"), Some("my-archive.zip")]
            .into_iter()
            .enumerate()
        {
            let response = client.post(format!("{api}/downloads"))
                .json(&serde_json::json!({"url": format!("{source}/archive/refs/heads/main.zip"), "filename": supplied}))
                .send().await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let id = response.json::<serde_json::Value>().await.unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned();
            let item = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let item = downloads
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|i| i.id == id)
                        .unwrap()
                        .clone();
                    if matches!(item.status, Status::Completed | Status::Failed) {
                        break item;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(item.status, Status::Completed, "{:?}", item.error);
            let expected = if supplied == Some("my-archive.zip") {
                "my-archive.zip".to_owned()
            } else if number == 0 {
                "Anti-Youtube-Ads-Extension-main.zip".to_owned()
            } else {
                format!("Anti-Youtube-Ads-Extension-main ({number}).zip")
            };
            assert_eq!(item.filename, expected);
            assert_eq!(item.save_path, directory.path().join(&expected));
            assert_eq!(std::fs::read(item.save_path).unwrap(), b"archive bytes");
        }
        source_task.abort();
        api_task.abort();
    }

    #[tokio::test]
    #[ignore = "Live GitHub archive/network test"]
    async fn supplied_github_archive_uses_complete_filename_and_zip_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let mut item = DownloadItem::new(
            "https://github.com/Mustafa1p/Anti-Youtube-Ads-Extension-/archive/refs/heads/main.zip"
                .into(),
            directory.path().join("main.zip"),
            0,
        );
        item.preserve_filename = false;
        let id = item.id.clone();
        let downloads = Arc::new(Mutex::new(vec![item]));
        start_download(Arc::new(reqwest::Client::new()), downloads.clone(), id)
            .await
            .unwrap();
        let items = downloads.lock().unwrap();
        let item = &items[0];
        assert_eq!(item.status, Status::Completed);
        assert_eq!(item.filename, "Anti-Youtube-Ads-Extension-main.zip");
        let bytes = std::fs::read(&item.save_path).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert_eq!(item.downloaded_bytes, bytes.len() as u64);
        println!("Downloaded {} ({} bytes)", item.filename, bytes.len());
    }

    #[tokio::test]
    async fn installation_defers_browser_downloads_and_extension_sync_can_be_disabled() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = state.egui_ctx.clone();
        let downloads = state.downloads.clone();
        context.data_mut(|data| {
            data.insert_temp(egui::Id::new("idg-installing"), true);
            data.insert_temp(egui::Id::new("extension-auto-update"), false);
        });
        let (address, task) = serve(build_router(state)).await;
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{address}/downloads"))
            .json(&serde_json::json!({"url":"https://example.com/file.zip"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(downloads.lock().unwrap().is_empty());
        let response: serde_json::Value = client
            .post(format!("{address}/extension/heartbeat"))
            .json(&serde_json::json!({"version":"1.2.1","install_type":"development"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["reload"], false);
        let connection = context
            .data(|data| {
                data.get_temp::<crate::integration::Connection>(egui::Id::new(
                    "extension-connection",
                ))
            })
            .unwrap();
        assert_eq!(connection.version, "1.2.1");
        task.abort();
    }

    async fn authenticated_file(method: Method, headers: HeaderMap) -> impl IntoResponse {
        if headers.get("cookie").and_then(|v| v.to_str().ok()) != Some("session=abc")
            || headers.get("referer").and_then(|v| v.to_str().ok())
                != Some("https://example.com/source")
            || headers.get("user-agent").and_then(|v| v.to_str().ok()) != Some("IDG-test")
        {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let data: Vec<u8> = (0..10_003).map(|n| (n % 251) as u8).collect();
        let mut response_headers = HeaderMap::new();
        response_headers.insert("accept-ranges", "bytes".parse().unwrap());
        response_headers.insert("content-length", data.len().to_string().parse().unwrap());
        if method == Method::HEAD {
            return (response_headers, Vec::<u8>::new()).into_response();
        }
        if let Some(range) = headers.get("range") {
            let (start, end) = range
                .to_str()
                .unwrap()
                .strip_prefix("bytes=")
                .unwrap()
                .split_once('-')
                .unwrap();
            let start: usize = start.parse().unwrap();
            let end: usize = end.parse().unwrap();
            let bytes = data[start..=end].to_vec();
            response_headers.insert("content-length", bytes.len().to_string().parse().unwrap());
            response_headers.insert(
                "content-range",
                format!("bytes {start}-{end}/{}", data.len())
                    .parse()
                    .unwrap(),
            );
            return (StatusCode::PARTIAL_CONTENT, response_headers, bytes).into_response();
        }
        (response_headers, data).into_response()
    }

    #[tokio::test]
    async fn authenticated_browser_download_preserves_headers_and_exact_file_bytes() {
        let (source, source_task) =
            serve(Router::new().route("/file", get(authenticated_file))).await;
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let state = state(&directory);
        let downloads = state.downloads.clone();
        let (address, api_task) = serve(build_router(state)).await;
        let response = reqwest::Client::new().post(format!("{address}/downloads")).json(&serde_json::json!({
            "url": format!("{source}/file"), "filename": "chosen.zip", "segments": 8,
            "cookies": "session=abc", "referrer": "https://example.com/source", "user_agent": "IDG-test"
        })).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let item = downloads.lock().unwrap()[0].clone();
                assert_ne!(item.status, Status::Failed, "{:?}", item.error);
                if item.status == Status::Completed {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let expected: Vec<u8> = (0..10_003).map(|n| (n % 251) as u8).collect();
        assert_eq!(
            tokio::fs::read(directory.join("chosen.zip")).await.unwrap(),
            expected
        );
        assert_eq!(
            downloads.lock().unwrap()[0].downloaded_bytes,
            expected.len() as u64
        );
        api_task.abort();
        source_task.abort();
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn streaming_server_without_ranges_or_known_size_downloads_to_eof() {
        async fn stream_file(headers: HeaderMap) -> impl IntoResponse {
            assert!(!headers.contains_key("range"));
            let chunks = futures::stream::iter([
                Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"first chunk\n")),
                Ok(bytes::Bytes::from_static(b"last chunk\n")),
            ]);
            axum::body::Body::from_stream(chunks)
        }
        let (source, source_task) = serve(Router::new().route("/stream", get(stream_file))).await;
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let state = state(&directory);
        let item = DownloadItem::new(format!("{source}/stream"), directory.join("stream.txt"), 8);
        let id = item.id.clone();
        state.downloads.lock().unwrap().push(item);
        start_download(state.client, state.downloads.clone(), id)
            .await
            .unwrap();
        assert_eq!(state.downloads.lock().unwrap()[0].status, Status::Completed);
        assert_eq!(
            tokio::fs::read(directory.join("stream.txt")).await.unwrap(),
            b"first chunk\nlast chunk\n"
        );
        source_task.abort();
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn browser_downloads_use_the_saved_connection_policy() {
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let state = state(&directory);
        let settings = state.connection_settings.clone();
        let downloads = state.downloads.clone();
        settings.lock().unwrap().parallel_segments = Some(12);
        let (address, server) = serve(build_router(state)).await;
        let client = reqwest::Client::new();
        for expected in [12, 0] {
            let response = client.post(format!("{address}/downloads"))
                .json(&serde_json::json!({"url": "https://example.com/file.zip", "show_dialog": true}))
                .send().await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            assert_eq!(
                downloads.lock().unwrap().last().unwrap().num_segments,
                expected
            );
            settings.lock().unwrap().parallel_segments = None;
        }
        server.abort();
    }

    #[tokio::test]
    async fn automatic_download_uses_32_ranges_and_falls_back_when_ranges_are_ignored() {
        const SIZE: u64 = 64 * 1024 * 1024;
        async fn file(
            uri: axum::http::Uri,
            method: Method,
            headers: HeaderMap,
        ) -> impl IntoResponse {
            let mut result = HeaderMap::new();
            result.insert("accept-ranges", "bytes".parse().unwrap());
            result.insert("content-length", SIZE.to_string().parse().unwrap());
            if method == Method::HEAD {
                return (result, Vec::<u8>::new()).into_response();
            }
            if uri.path() != "/ignores-ranges"
                && let Some(range) = headers.get("range")
            {
                let (start, end) = range
                    .to_str()
                    .unwrap()
                    .strip_prefix("bytes=")
                    .unwrap()
                    .split_once('-')
                    .unwrap();
                let (start, end): (u64, u64) = (start.parse().unwrap(), end.parse().unwrap());
                let data: Vec<u8> = (start..=end).map(|offset| (offset % 251) as u8).collect();
                result.insert("content-length", data.len().to_string().parse().unwrap());
                result.insert(
                    "content-range",
                    format!("bytes {start}-{end}/{SIZE}").parse().unwrap(),
                );
                return (StatusCode::PARTIAL_CONTENT, result, data).into_response();
            }
            let data: Vec<u8> = (0..SIZE).map(|offset| (offset % 251) as u8).collect();
            (result, data).into_response()
        }
        let (source, task) = serve(
            Router::new()
                .route("/parallel", get(file))
                .route("/ignores-ranges", get(file)),
        )
        .await;
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let state = state(&directory);
        for (route, expected) in [("parallel", 32), ("ignores-ranges", 1)] {
            let path = directory.join(route);
            let item = DownloadItem::new(format!("{source}/{route}"), path.clone(), 0);
            let id = item.id.clone();
            state.downloads.lock().unwrap().push(item);
            start_download(state.client.clone(), state.downloads.clone(), id)
                .await
                .unwrap();
            let item = state.downloads.lock().unwrap().last().unwrap().clone();
            assert_eq!(item.status, Status::Completed);
            assert_eq!(item.num_segments, expected);
            assert_eq!(item.downloaded_bytes, SIZE);
            let bytes = tokio::fs::read(&path).await.unwrap();
            assert_eq!(bytes.len() as u64, SIZE);
            assert!(
                bytes
                    .iter()
                    .enumerate()
                    .all(|(offset, byte)| *byte == (offset % 251) as u8)
            );
            tokio::fs::remove_file(path).await.unwrap();
        }
        task.abort();
        tokio::fs::remove_dir(directory).await.unwrap();
    }
    #[tokio::test]
    async fn token_form_download_posts_once_without_probes_and_preserves_file_bytes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0));
        let requests = count.clone();
        let expected: Vec<u8> = (0..250_003).map(|n| (n % 251) as u8).collect();
        let bytes = expected.clone();
        let (source, source_task) = serve(Router::new().route(
            "/download.php",
            axum::routing::any(move |method: Method, headers: HeaderMap, body: String| {
                let bytes = bytes.clone();
                let requests = requests.clone();
                async move {
                    assert_eq!(method, Method::POST);
                    assert_eq!(
                        requests.fetch_add(1, Ordering::SeqCst),
                        0,
                        "one-use token was replayed"
                    );
                    assert_eq!(body, "token=fixture%2Btoken&filename=fixture.zip");
                    assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
                    assert_eq!(
                        headers["referer"],
                        "https://adsume.com/your-file-is-loading-please-wait/"
                    );
                    assert_eq!(headers["cookie"], "session=fixture");
                    assert!(!headers.contains_key("range"));
                    (
                        [
                            ("content-type", "application/octet-stream"),
                            ("content-disposition", "attachment; filename=fixture.zip"),
                            ("accept-ranges", "bytes"),
                        ],
                        bytes,
                    )
                }
            }),
        ))
        .await;
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let downloads = state.downloads.clone();
        let (address, api_task) = serve(build_router(state)).await;
        let response = reqwest::Client::new().post(format!("{address}/downloads")).json(&serde_json::json!({
            "url": format!("{source}/download.php"), "post_data": "token=fixture%2Btoken&filename=fixture.zip", "segments": 32,
            "cookies": "session=fixture", "referrer": "https://adsume.com/your-file-is-loading-please-wait/"
        })).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let item = downloads.lock().unwrap()[0].clone();
                assert_ne!(item.status, Status::Failed, "{:?}", item.error);
                if item.status == Status::Completed {
                    assert_eq!(item.num_segments, 1);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(directory.path().join("fixture.zip")).unwrap(),
            expected
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        api_task.abort();
        source_task.abort();
    }

    #[tokio::test]
    async fn token_form_errors_do_not_create_files_or_replay_tokens() {
        for content_type in ["text/html; charset=utf-8", "application/json"] {
            let (source, source_task) = serve(Router::new().route(
                "/download.php",
                post(move || async move { ([("content-type", content_type)], "Expired token") }),
            ))
            .await;
            let directory = tempfile::tempdir().unwrap();
            let downloads: Downloads = Arc::new(Mutex::new(Vec::new()));
            let mut item = DownloadItem::new(
                format!("{source}/download.php"),
                directory.path().join("file.zip"),
                8,
            );
            item.post_data = Some("token=expired".into());
            let id = item.id.clone();
            downloads.lock().unwrap().push(item);
            let error = start_download(Arc::new(reqwest::Client::new()), downloads, id)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("fresh download token"));
            assert!(!directory.path().join("file.zip").exists());
            source_task.abort();
        }
    }
}
