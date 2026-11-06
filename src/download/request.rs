//! Origin-scoped request headers and explicit redirects over a shared transport.
//!
//! The production client disables automatic redirects so every hop receives its
//! own origin admission and only the headers authorized for that destination.
use super::gate::{HostBudget, LeasedResponse};
use anyhow::{Context, Result};
use reqwest::{
    Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Debug)]
pub struct PausedRequest;
impl std::fmt::Display for PausedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Request paused")
    }
}
impl std::error::Error for PausedRequest {}

#[derive(Debug)]
pub struct InvalidRedirect(String);
impl std::fmt::Display for InvalidRedirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for InvalidRedirect {}

#[derive(Clone, Copy, Default)]
pub struct Control<'a> {
    pub cancel: Option<&'a AtomicBool>,
    pub pause: Option<&'a AtomicBool>,
    pub network_timeout: Option<Duration>,
    /// Optional whole-request timeout for updater responses, including bodies.
    pub total_timeout: Option<Duration>,
}
impl Control<'_> {
    fn check(&self) -> Result<()> {
        anyhow::ensure!(
            !self.cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)),
            "Download cancelled"
        );
        if self.pause.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(PausedRequest.into());
        }
        Ok(())
    }
    async fn interrupted(&self) -> anyhow::Error {
        if self.cancel.is_none() && self.pause.is_none() {
            return std::future::pending().await;
        }
        loop {
            if let Err(error) = self.check() {
                return error;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplayPolicy {
    Never,
    Caller,
    Capacity,
}

pub struct Spec {
    method: Method,
    url: String,
    headers: HeaderMap,
    body: Option<String>,
    replay: ReplayPolicy,
}
impl Spec {
    pub fn get(url: &str) -> Self {
        Self {
            method: Method::GET,
            url: url.into(),
            headers: HeaderMap::new(),
            body: None,
            replay: ReplayPolicy::Capacity,
        }
    }
    pub fn form(url: &str, body: String) -> Self {
        Self {
            method: Method::POST,
            url: url.into(),
            headers: HeaderMap::new(),
            body: Some(body),
            replay: ReplayPolicy::Never,
        }
    }
    /// Generated/token-bearing GETs must not be used to test capacity or be
    /// replayed after ambiguous connection loss. Intent survives redirects.
    pub fn single_use(mut self) -> Self {
        self.replay = ReplayPolicy::Never;
        self
    }
    /// One attempt per redirect hop. This safe GET may test recovered capacity,
    /// but the caller owns fallback/retry after a connection failure.
    pub fn get_once(url: &str) -> Self {
        let mut spec = Self::get(url);
        spec.replay = ReplayPolicy::Caller;
        spec
    }
    pub fn header(mut self, name: &str, value: impl AsRef<str>) -> Result<Self> {
        self.headers.insert(
            HeaderName::from_bytes(name.as_bytes()).context("Invalid request header name")?,
            value.as_ref().parse().context("Invalid request header")?,
        );
        Ok(self)
    }
}

pub struct Received {
    pub leased: LeasedResponse,
    pub remaining_network_time: Option<Duration>,
}

pub struct RequestContext {
    client: Arc<Client>,
    origin: String,
    headers: HeaderMap,
}

fn cross_origin_safe(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-language"
            | "accept-encoding"
            | "user-agent"
            | "referer"
            | "origin"
            | "range"
            | "if-range"
            | "cache-control"
            | "pragma"
    )
}

fn automatic_referer(prior: &Url, next: &Url) -> Option<HeaderValue> {
    if prior.scheme() == "https" && next.scheme() == "http" {
        return None;
    }
    let mut referer = prior.clone();
    let _ = referer.set_username("");
    let _ = referer.set_password(None);
    referer.set_fragment(None);
    // Match the browser's strict-origin-when-cross-origin default. In particular,
    // signed source URL query parameters must not become another origin's referrer.
    if prior.origin() != next.origin() {
        referer.set_path("/");
        referer.set_query(None);
    }
    HeaderValue::from_str(referer.as_str()).ok()
}

/// Distinguish a closed/refused connection from invalid HTTP, DNS, certificate and
/// configuration errors. Incomplete HTTP/1 headers can be a clean TCP EOF and
/// need not contain an underlying io::Error.
fn connection_closed(error: &reqwest::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = cause {
        if let Some(error) = error.downcast_ref::<hyper::Error>()
            && (error.is_incomplete_message() || error.is_closed())
        {
            return true;
        }
        if let Some(error) = error.downcast_ref::<std::io::Error>()
            && matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::UnexpectedEof
            )
        {
            return true;
        }
        cause = error.source();
    }
    false
}

impl RequestContext {
    pub fn new(client: Arc<Client>, source: &str, mut headers: HeaderMap) -> Result<Self> {
        let url = Url::parse(source).context("Invalid download URL")?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https"),
            "Download URL must use HTTP or HTTPS"
        );
        // Framing/range headers belong to the engine, never browser input.
        for name in [
            "host",
            "connection",
            "content-length",
            "transfer-encoding",
            "range",
            "if-range",
            "accept-encoding",
            "content-type",
        ] {
            headers.remove(name);
        }
        // Preserve explicit URL authentication, scoped just like browser headers.
        if (!url.username().is_empty() || url.password().is_some())
            && !headers.contains_key("authorization")
            && let Some(value) = client
                .get(url.clone())
                .build()?
                .headers()
                .get("authorization")
        {
            headers.insert("authorization", value.clone());
        }
        for (name, value) in headers.iter_mut() {
            if !cross_origin_safe(name.as_str()) {
                value.set_sensitive(true);
            }
        }
        Ok(Self {
            client,
            origin: url.origin().ascii_serialization(),
            headers,
        })
    }

    pub async fn send(&self, mut spec: Spec, control: Control<'_>) -> Result<Received> {
        let mut url = Url::parse(&spec.url).context("Invalid request URL")?;
        url.set_fragment(None);
        let spec_origin = url.origin().ascii_serialization();
        let mut visited = Vec::new();
        let mut previous: Option<Url> = None;
        let mut remaining = control.network_timeout;
        let mut total_remaining = control.total_timeout;
        for _ in 0..10 {
            control.check()?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(
                    InvalidRedirect("Redirect target must use HTTP or HTTPS".into()).into(),
                );
            }
            if visited.contains(&(spec.method.clone(), url.clone())) {
                return Err(InvalidRedirect("Download redirect loop".into()).into());
            }
            visited.push((spec.method.clone(), url.clone()));
            let budget = HostBudget::for_url(url.as_str());
            let allow_probe = spec.replay != ReplayPolicy::Never
                && matches!(spec.method, Method::GET | Method::HEAD);
            let (response, permit, elapsed) = loop {
                // No network deadline runs while another file owns the capacity.
                let permit = tokio::select! {
                    permit = budget.acquire_request(allow_probe) => permit,
                    error = control.interrupted() => return Err(error),
                };
                control.check()?;
                let origin = url.origin().ascii_serialization();
                let mut headers = HeaderMap::new();
                for (name, value) in &self.headers {
                    if origin == self.origin || cross_origin_safe(name.as_str()) {
                        headers.append(name.clone(), value.clone());
                    }
                }
                for (name, value) in &spec.headers {
                    if origin == spec_origin || cross_origin_safe(name.as_str()) {
                        headers.insert(name.clone(), value.clone());
                    }
                }
                if spec.body.is_none() {
                    headers.remove("content-type");
                    headers.remove("content-length");
                }
                if !headers.contains_key("referer")
                    && let Some(prior) = previous.as_ref()
                    && let Some(value) = automatic_referer(prior, &url)
                {
                    headers.insert("referer", value);
                }
                let mut request = self
                    .client
                    .request(spec.method.clone(), url.clone())
                    .headers(headers)
                    .header("accept-encoding", "identity");
                if let Some(body) = &spec.body {
                    request = request
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(body.clone());
                }
                if let Some(timeout) = total_remaining {
                    request = request.timeout(timeout);
                }
                let started = tokio::time::Instant::now();
                let sending = async {
                    let result = match remaining {
                        Some(timeout) => tokio::time::timeout(timeout, request.send())
                            .await
                            .map_err(|_| {
                                anyhow::anyhow!("Network stalled waiting for response headers")
                            })?,
                        None => request.send().await,
                    };
                    Ok::<_, anyhow::Error>(result)
                };
                let result = tokio::select! {
                    response = sending => response?,
                    error = control.interrupted() => return Err(error),
                };
                match result {
                    Ok(response) => break (response, permit, started.elapsed()),
                    Err(error) => {
                        let pressure = connection_closed(&error) && permit.connection_closed();
                        if pressure && allow_probe && spec.replay == ReplayPolicy::Capacity {
                            // A confirmed capacity refusal is queue time. GET/HEAD
                            // can try another admitted connection without burning
                            // the caller's failure budget; forms are never replayed.
                            drop(permit);
                            continue;
                        }
                        return Err(error).context("HTTP request failed");
                    }
                }
            };
            remaining = remaining.map(|value| value.saturating_sub(elapsed));
            total_remaining = total_remaining.map(|value| value.saturating_sub(elapsed));
            let status = response.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        InvalidRedirect("Download redirect has no valid Location".into())
                    })?;
                let mut next = response
                    .url()
                    .join(location)
                    .map_err(|_| InvalidRedirect("Invalid download redirect URL".into()))?;
                next.set_fragment(None);
                previous = Some(response.url().clone());
                if (status == 303 && spec.method != Method::HEAD)
                    || (matches!(status, 301 | 302) && spec.method == Method::POST)
                {
                    spec.method = Method::GET;
                    spec.body = None;
                }
                // Release the old origin before waiting for the next origin.
                drop(response);
                drop(permit);
                url = next;
                continue;
            }
            if matches!(status, 429 | 503) {
                permit.refused(response.url().as_str());
            } else if response.status().is_success() {
                permit.response_started();
            }
            return Ok(Received {
                leased: LeasedResponse { response, permit },
                remaining_network_time: remaining,
            });
        }
        Err(InvalidRedirect("Too many download redirects".into()).into())
    }
}

pub async fn public_get(
    client: &Client,
    url: &str,
    headers: &[(&'static str, &'static str)],
    timeout: Duration,
) -> Result<LeasedResponse> {
    let context = RequestContext::new(Arc::new(client.clone()), url, HeaderMap::new())?;
    let mut spec = Spec::get(url);
    for (name, value) in headers {
        spec = spec.header(name, value)?;
    }
    Ok(context
        .send(
            spec,
            Control {
                network_timeout: Some(timeout),
                total_timeout: Some(timeout),
                ..Control::default()
            },
        )
        .await?
        .leased)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::StatusCode, routing::any};
    use std::sync::atomic::AtomicUsize;

    async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        (
            url,
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }),
        )
    }

    #[tokio::test]
    async fn malformed_http_does_not_infer_capacity_and_closed_forms_are_not_replayed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let posts = Arc::new(AtomicUsize::new(0));
        let received_posts = posts.clone();
        let transient = Arc::new(AtomicUsize::new(0));
        let permanent_closes = Arc::new(AtomicUsize::new(0));
        let closed = permanent_closes.clone();
        let single_use = Arc::new(AtomicUsize::new(0));
        let single = single_use.clone();
        let (release, released) = tokio::sync::watch::channel(false);
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let posts = received_posts.clone();
                let transient = transient.clone();
                let closed = closed.clone();
                let single = single.clone();
                let mut released = released.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                        let count = socket.read(&mut buffer).await.unwrap();
                        assert!(count > 0 && request.len() < 8192);
                        request.extend_from_slice(&buffer[..count]);
                    }
                    if request.starts_with(b"GET /healthy ") {
                        socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n")
                            .await
                            .unwrap();
                        released.wait_for(|value| *value).await.unwrap();
                        socket.write_all(b"data").await.unwrap();
                    } else if request.starts_with(b"GET /malformed ") {
                        socket
                            .write_all(b"invalid HTTP response\r\n")
                            .await
                            .unwrap();
                    } else if request.starts_with(b"GET /transient ") {
                        if transient.fetch_add(1, Ordering::Relaxed) > 0 {
                            socket
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndata")
                                .await
                                .unwrap();
                        }
                    } else if request.starts_with(b"GET /always-closed ") {
                        closed.fetch_add(1, Ordering::Relaxed);
                    } else if request.starts_with(b"GET /single-use ") {
                        single.fetch_add(1, Ordering::Relaxed);
                        socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndata")
                            .await
                            .unwrap();
                    } else {
                        assert!(request.starts_with(b"POST /closed "));
                        posts.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
        let budget = HostBudget::for_url(&base);
        let context = RequestContext::new(
            Arc::new(super::super::http_client().unwrap()),
            &base,
            HeaderMap::new(),
        )
        .unwrap();
        let LeasedResponse { response, permit } = context
            .send(Spec::get(&format!("{base}/healthy")), Control::default())
            .await
            .unwrap()
            .leased;
        assert_eq!(budget.counts(), (1, 32));
        assert!(
            context
                .send(Spec::get(&format!("{base}/transient")), Control::default())
                .await
                .is_err()
        );
        let recovered = tokio::time::timeout(
            Duration::from_millis(250),
            context.send(Spec::get(&format!("{base}/transient")), Control::default()),
        )
        .await;
        if recovered.is_err() {
            release.send(true).unwrap();
            let _ = response.bytes().await;
            drop(permit);
            server.abort();
            panic!(
                "An isolated closed connection must not block recovery behind an unrelated healthy stream"
            );
        }
        let LeasedResponse {
            response: recovered,
            permit: recovered_permit,
        } = recovered.unwrap().unwrap().leased;
        assert_eq!(recovered.bytes().await.unwrap().as_ref(), b"data");
        drop(recovered_permit);
        assert_eq!(budget.counts(), (1, 32));
        let error = context
            .send(Spec::get(&format!("{base}/malformed")), Control::default())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("HTTP request failed"));
        assert_eq!(
            budget.counts(),
            (1, 32),
            "Malformed HTTP must not be treated as a connection cap"
        );
        for attempt in 0..2 {
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                context.send(
                    Spec::form(&format!("{base}/closed"), format!("token={attempt}")),
                    Control::default(),
                ),
            )
            .await
            .unwrap();
            assert!(result.is_err());
        }
        assert_eq!(
            posts.load(Ordering::Relaxed),
            2,
            "A closed form request is never replayed"
        );
        assert_eq!(budget.counts(), (1, 1));
        assert!(
            tokio::time::timeout(
                Duration::from_millis(250),
                context.send(
                    Spec::form(&format!("{base}/closed"), "token=waiting".into()),
                    Control::default(),
                )
            )
            .await
            .is_err(),
            "A waiting form must not be used as a speculative connection probe"
        );
        assert_eq!(posts.load(Ordering::Relaxed), 2);
        let export_url = format!("{base}/single-use");
        let mut export =
            Box::pin(context.send(Spec::get(&export_url).single_use(), Control::default()));
        assert!(
            tokio::time::timeout(Duration::from_millis(250), &mut export)
                .await
                .is_err()
        );
        assert_eq!(
            single_use.load(Ordering::Relaxed),
            0,
            "A single-use GET must not probe occupied capacity"
        );
        let trial = tokio::time::timeout(
            Duration::from_secs(1),
            context.send(
                Spec::get_once(&format!("{base}/always-closed")),
                Control::default(),
            ),
        )
        .await
        .expect("A safe caller-controlled GET may test occupied capacity");
        assert!(trial.is_err());
        assert_eq!(
            permanent_closes.load(Ordering::Relaxed),
            1,
            "A caller-controlled trial must return its first connection failure without automatic replay"
        );
        assert_eq!(single_use.load(Ordering::Relaxed), 0);
        release.send(true).unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"data");
        drop(permit);
        let LeasedResponse {
            response: exported,
            permit: export_permit,
        } = tokio::time::timeout(Duration::from_secs(1), export)
            .await
            .unwrap()
            .unwrap()
            .leased;
        assert_eq!(exported.bytes().await.unwrap().as_ref(), b"data");
        drop(export_permit);
        assert_eq!(single_use.load(Ordering::Relaxed), 1);
        assert_eq!(budget.counts().0, 0);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            context.send(
                Spec::get(&format!("{base}/always-closed")),
                Control::default(),
            ),
        )
        .await
        .unwrap();
        assert!(
            result.is_err(),
            "Without healthy peers, closed connections remain a bounded failure"
        );
        assert_eq!(permanent_closes.load(Ordering::Relaxed), 2);
        server.abort();
    }

    #[tokio::test]
    async fn form_redirects_preserve_http_method_rules_and_origin_scoped_credentials() {
        for cross in [false, true] {
            for status in [301, 302, 303, 307, 308] {
                let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
                let captured = seen.clone();
                let target = any(
                    move |method: Method, headers: HeaderMap, body: bytes::Bytes| {
                        captured.lock().unwrap().push((method, headers, body));
                        async { "payload" }
                    },
                );
                let (target_url, target_server) =
                    serve(Router::new().route("/target", target.clone())).await;
                let source_hits = Arc::new(AtomicUsize::new(0));
                let hits = source_hits.clone();
                let location = if cross {
                    format!("{target_url}/target")
                } else {
                    "/target".into()
                };
                let (source, source_server) = serve(Router::new().route("/target", target).route(
                    "/start",
                    any(move |headers: HeaderMap, body: bytes::Bytes| {
                        assert_eq!(headers["cookie"], "session=source");
                        assert_eq!(headers["x-csrf-token"], "source-token");
                        assert_eq!(body.as_ref(), b"token=once");
                        hits.fetch_add(1, Ordering::Relaxed);
                        let location = location.clone();
                        async move {
                            (
                                StatusCode::from_u16(status).unwrap(),
                                [("location", location)],
                            )
                        }
                    }),
                ))
                .await;
                let source = format!("{source}/start");
                let mut headers = HeaderMap::new();
                headers.insert("cookie", "session=source".parse().unwrap());
                headers.insert("x-csrf-token", "source-token".parse().unwrap());
                let context = RequestContext::new(
                    Arc::new(super::super::http_client().unwrap()),
                    &source,
                    headers,
                )
                .unwrap();
                let received = context
                    .send(Spec::form(&source, "token=once".into()), Control::default())
                    .await
                    .unwrap();
                assert_eq!(
                    received.leased.response.bytes().await.unwrap().as_ref(),
                    b"payload"
                );
                assert_eq!(source_hits.load(Ordering::Relaxed), 1);
                let seen = seen.lock().unwrap();
                assert_eq!(seen.len(), 1);
                let (method, headers, body) = &seen[0];
                let preserves = matches!(status, 307 | 308);
                assert_eq!(*method, if preserves { Method::POST } else { Method::GET });
                assert_eq!(
                    body.as_ref(),
                    if preserves {
                        b"token=once".as_slice()
                    } else {
                        b"".as_slice()
                    }
                );
                assert_eq!(headers.contains_key("content-type"), preserves);
                for name in ["cookie", "x-csrf-token"] {
                    assert_eq!(headers.contains_key(name), !cross);
                }
                source_server.abort();
                target_server.abort();
            }
        }
    }

    #[tokio::test]
    async fn invalid_redirects_stop_without_replaying_a_form_or_leaking_leases() {
        for location in [
            Some("/start#fragment"),
            Some("ftp://example.test/file"),
            None,
        ] {
            let hits = Arc::new(AtomicUsize::new(0));
            let seen = hits.clone();
            let (base, server) = serve(Router::new().route(
                "/start",
                any(move || {
                    seen.fetch_add(1, Ordering::Relaxed);
                    async move {
                        let mut headers = HeaderMap::new();
                        if let Some(location) = location {
                            headers.insert("location", location.parse().unwrap());
                        }
                        (StatusCode::TEMPORARY_REDIRECT, headers, "")
                    }
                }),
            ))
            .await;
            let url = format!("{base}/start");
            let budget = HostBudget::for_url(&url);
            let context = RequestContext::new(
                Arc::new(super::super::http_client().unwrap()),
                &url,
                HeaderMap::new(),
            )
            .unwrap();
            let result = context
                .send(Spec::form(&url, "token=once".into()), Control::default())
                .await;
            assert!(result.err().unwrap().is::<InvalidRedirect>());
            assert_eq!(hits.load(Ordering::Relaxed), 1);
            assert_eq!(budget.counts().0, 0);
            server.abort();
        }
    }

    #[tokio::test]
    async fn redirects_release_old_origin_and_exclude_capacity_wait_from_network_time() {
        for interruption in 0..3 {
            let target_hits = Arc::new(AtomicUsize::new(0));
            let seen = target_hits.clone();
            let (target, target_server) = serve(Router::new().route(
                "/target",
                any(move || {
                    seen.fetch_add(1, Ordering::Relaxed);
                    async { "payload" }
                }),
            ))
            .await;
            let target_url = format!("{target}/target");
            let target_budget = HostBudget::for_url(&target_url);
            let mut held = Vec::new();
            for _ in 0..super::super::connections::MAX_CONNECTIONS {
                held.push(target_budget.acquire().await);
            }
            let ready = Arc::new(tokio::sync::Notify::new());
            let signal = ready.clone();
            let location = target_url.clone();
            let (source, source_server) = serve(Router::new().route(
                "/start",
                any(move || {
                    signal.notify_one();
                    let location = location.clone();
                    async move { (StatusCode::TEMPORARY_REDIRECT, [("location", location)]) }
                }),
            ))
            .await;
            let source_url = format!("{source}/start");
            let source_budget = HostBudget::for_url(&source_url);
            let cancel = Arc::new(AtomicBool::new(false));
            let pause = Arc::new(AtomicBool::new(false));
            let cancelled = cancel.clone();
            let paused = pause.clone();
            let work = tokio::spawn(async move {
                let context = RequestContext::new(
                    Arc::new(super::super::http_client().unwrap()),
                    &source_url,
                    HeaderMap::new(),
                )
                .unwrap();
                context
                    .send(
                        Spec::get(&source_url),
                        Control {
                            cancel: Some(&cancelled),
                            pause: Some(&paused),
                            network_timeout: Some(Duration::from_millis(100)),
                            ..Control::default()
                        },
                    )
                    .await
            });
            tokio::time::timeout(Duration::from_secs(1), ready.notified())
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(350)).await;
            assert_eq!(
                source_budget.counts().0,
                0,
                "Never hold the first origin while waiting for the next"
            );
            assert_eq!(target_hits.load(Ordering::Relaxed), 0);
            assert!(
                !work.is_finished(),
                "Capacity wait is not a network timeout"
            );
            if interruption == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
            if interruption == 2 {
                pause.store(true, Ordering::Relaxed);
            }
            if interruption == 0 {
                drop(held);
                held = Vec::new();
            }
            let result = tokio::time::timeout(Duration::from_secs(1), work)
                .await
                .unwrap()
                .unwrap();
            if interruption == 0 {
                let LeasedResponse { response, permit } = result.unwrap().leased;
                assert_eq!(response.bytes().await.unwrap().as_ref(), b"payload");
                drop(permit);
            } else if interruption == 2 {
                assert!(result.err().unwrap().is::<PausedRequest>());
            } else {
                assert!(result.err().unwrap().to_string().contains("cancelled"));
            }
            drop(held);
            assert_eq!(target_budget.counts().0, 0);
            source_server.abort();
            target_server.abort();
        }
    }

    #[tokio::test]
    async fn updater_body_timeout_survives_explicit_cross_origin_redirects() {
        let (target, target_server) = serve(Router::new().route(
            "/target",
            any(|| async {
                let body = Body::from_stream(futures::stream::once(async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"late"))
                }));
                ([("content-length", "4")], body)
            }),
        ))
        .await;
        let (source, source_server) = serve(Router::new().route(
            "/start",
            any(move || {
                let location = format!("{target}/target");
                async move { (StatusCode::FOUND, [("location", location)]) }
            }),
        ))
        .await;
        let leased = public_get(
            &super::super::http_client().unwrap(),
            &format!("{source}/start"),
            &[],
            Duration::from_millis(500),
        )
        .await
        .unwrap();
        assert!(leased.response.bytes().await.unwrap_err().is_timeout());
        source_server.abort();
        target_server.abort();
    }

    #[tokio::test]
    async fn url_authentication_stays_scoped_and_automatic_referrer_omits_secrets() {
        for (username, password, expected) in [
            ("", "pass", "Basic OnBhc3M="),
            ("user", "pass", "Basic dXNlcjpwYXNz"),
            (
                "user name",
                "p@ss:word",
                "Basic dXNlciBuYW1lOnBAc3M6d29yZA==",
            ),
        ] {
            let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured = seen.clone();
            let (target, target_server) = serve(Router::new().route(
                "/target",
                any(move |headers: HeaderMap| {
                    captured.lock().unwrap().push(headers);
                    async { "payload" }
                }),
            ))
            .await;
            let (source, source_server) = serve(Router::new().route(
                "/start",
                any(move |headers: HeaderMap| {
                    assert_eq!(headers["authorization"], expected);
                    let location = format!("{target}/target");
                    async move { (StatusCode::FOUND, [("location", location)]) }
                }),
            ))
            .await;
            let mut authenticated =
                Url::parse(&format!("{source}/start?token=source-secret")).unwrap();
            authenticated.set_username(username).unwrap();
            authenticated.set_password(Some(password)).unwrap();
            let context = RequestContext::new(
                Arc::new(super::super::http_client().unwrap()),
                authenticated.as_str(),
                HeaderMap::new(),
            )
            .unwrap();
            // The second request models a resolved same-origin URL without userinfo.
            for url in [
                authenticated.to_string(),
                format!("{source}/start?token=source-secret"),
            ] {
                let received = context
                    .send(Spec::get(&url), Control::default())
                    .await
                    .unwrap();
                assert_eq!(
                    received.leased.response.bytes().await.unwrap().as_ref(),
                    b"payload"
                );
            }
            for headers in seen.lock().unwrap().iter() {
                assert!(!headers.contains_key("authorization"));
                assert_eq!(headers["referer"], format!("{source}/"));
            }
            let secure =
                Url::parse("https://user:pass@example.test/path?token=secret#fragment").unwrap();
            assert!(
                automatic_referer(&secure, &Url::parse("http://example.test/").unwrap()).is_none()
            );
            assert_eq!(
                automatic_referer(&secure, &Url::parse("https://example.test/other").unwrap())
                    .unwrap(),
                "https://example.test/path?token=secret"
            );
            source_server.abort();
            target_server.abort();
        }
    }
}
