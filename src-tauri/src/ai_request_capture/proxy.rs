use super::{
    validation_errors, AiRequestCaptureConfig, AiRequestCaptureHeader, CaptureFinish, CaptureStart,
    CaptureState, CaptureStore, CapturedBody,
};
use bytes::Bytes;
use futures_util::Stream;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, HOST,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
};
use hyper::http::request::Parts;
use hyper::{Method, Request, Response, StatusCode, Version};
use std::collections::HashSet;
use std::convert::Infallible;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use uuid::Uuid;

type ProxyBodyError = io::Error;
type ProxyResponse = Response<UnsyncBoxBody<Bytes, ProxyBodyError>>;

struct CaptureBuffer {
    data: Vec<u8>,
    total_bytes: u64,
}

impl CaptureBuffer {
    fn push(&mut self, chunk: &[u8]) {
        self.total_bytes += chunk.len() as u64;
        let remaining = super::CAPTURE_BODY_LIMIT_BYTES.saturating_sub(self.data.len());
        self.data
            .extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }

    fn into_body(self) -> CapturedBody {
        CapturedBody::from_bytes(self.data, self.total_bytes)
    }
}

struct RequestTee<S> {
    inner: S,
    capture: CaptureBuffer,
    store: CaptureStore,
    id: String,
    persisted: bool,
    completed: Option<oneshot::Sender<()>>,
    transfer_error: Arc<Mutex<Option<String>>>,
}

impl<S> RequestTee<S> {
    fn schedule_persist(&mut self) {
        if self.persisted {
            return;
        }
        self.persisted = true;
        let store = self.store.clone();
        let id = self.id.clone();
        let body = std::mem::replace(
            &mut self.capture,
            CaptureBuffer {
                data: Vec::new(),
                total_bytes: 0,
            },
        )
        .into_body();
        let completed = self.completed.take();
        tokio::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || store.update_request_body(&id, body)).await;
            if let Some(completed) = completed {
                let _ = completed.send(());
            }
        });
    }
}

impl<S> Stream for RequestTee<S>
where
    S: Stream<Item = Result<Bytes, hyper::Error>> + Unpin,
{
    type Item = Result<Bytes, ProxyBodyError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => {
                this.capture.push(&chunk);
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(error))) => {
                *this
                    .transfer_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(format!("failed to read proxy request body: {error}"));
                this.schedule_persist();
                Poll::Ready(Some(Err(io::Error::other(error))))
            }
            Poll::Ready(None) => {
                this.schedule_persist();
                Poll::Ready(None)
            }
        }
    }
}

impl<S> Drop for RequestTee<S> {
    fn drop(&mut self) {
        self.schedule_persist();
    }
}

struct ResponseTee {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    capture: CaptureBuffer,
    request_completed: Option<oneshot::Receiver<()>>,
    store: CaptureStore,
    id: String,
    response_status: u16,
    response_headers: Vec<AiRequestCaptureHeader>,
    captured: bool,
    app: Option<AppHandle>,
    finished: bool,
}

impl ResponseTee {
    fn schedule_finish(&mut self, state: CaptureState, error: Option<String>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let request_completed = self.request_completed.take();
        let store = self.store.clone();
        let id = self.id.clone();
        let response_status = self.response_status;
        let response_headers = std::mem::take(&mut self.response_headers);
        let response_body = std::mem::replace(
            &mut self.capture,
            CaptureBuffer {
                data: Vec::new(),
                total_bytes: 0,
            },
        )
        .into_body();
        let captured = self.captured;
        let app = self.app.clone();
        tokio::spawn(async move {
            if let Some(request_completed) = request_completed {
                let _ = request_completed.await;
            }
            finish_capture(
                store,
                &id,
                state,
                Some(response_status),
                response_headers,
                response_body,
                error,
                captured,
                &app,
            )
            .await;
        });
    }
}

impl Stream for ResponseTee {
    type Item = Result<Frame<Bytes>, ProxyBodyError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        match this.inner.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => {
                this.capture.push(&chunk);
                Poll::Ready(Some(Ok(Frame::data(chunk))))
            }
            Poll::Ready(Some(Err(error))) => {
                let message = format!("failed to read upstream response body: {error}");
                this.schedule_finish(CaptureState::Failed, Some(message.clone()));
                Poll::Ready(Some(Err(io::Error::other(message))))
            }
            Poll::Ready(None) => {
                this.schedule_finish(CaptureState::Completed, None);
                Poll::Ready(None)
            }
        }
    }
}

impl Drop for ResponseTee {
    fn drop(&mut self) {
        self.schedule_finish(
            CaptureState::Failed,
            Some("client disconnected before response transfer completed".to_string()),
        );
    }
}

pub(crate) async fn forward(
    request: Request<Incoming>,
    config: AiRequestCaptureConfig,
    store: CaptureStore,
    app: Option<AppHandle>,
) -> Result<ProxyResponse, Infallible> {
    let (parts, body) = request.into_parts();
    let request_path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    let enabled_config = AiRequestCaptureConfig {
        enabled: true,
        ..config.clone()
    };
    if let Some(error) = validation_errors(&enabled_config).first() {
        return Ok(failed_response(
            &parts,
            &config,
            request_path_and_query,
            store,
            &app,
            StatusCode::BAD_REQUEST,
            error.message.clone(),
        )
        .await);
    }
    if parts.method == Method::CONNECT {
        return Ok(failed_response(
            &parts,
            &config,
            request_path_and_query,
            store,
            &app,
            StatusCode::METHOD_NOT_ALLOWED,
            "CONNECT is not supported by AI request capture".to_string(),
        )
        .await);
    }
    if websocket_upgrade(&parts.headers) {
        return Ok(failed_response(
            &parts,
            &config,
            request_path_and_query,
            store,
            &app,
            StatusCode::BAD_REQUEST,
            "WebSocket Upgrade is not supported by AI request capture".to_string(),
        )
        .await);
    }
    let upstream_url = match mapped_upstream_url(&config, &request_path_and_query) {
        Ok(url) => url,
        Err(error) => {
            return Ok(failed_response(
                &parts,
                &config,
                request_path_and_query,
                store,
                &app,
                StatusCode::BAD_REQUEST,
                error,
            )
            .await)
        }
    };
    let id = Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().timestamp_millis();
    let started = CaptureStart {
        id: id.clone(),
        started_at,
        http_version: http_version(parts.version).to_string(),
        method: parts.method.to_string(),
        request_path_and_query,
        upstream_url: upstream_url.clone(),
        request_headers: capture_headers(&parts.headers),
        request_body: CapturedBody::from_bytes(Vec::new(), 0),
        provider: None,
        model: None,
    };
    let captured = begin_capture(store.clone(), started).await;
    if captured {
        emit_capture_update(&app, "created", Some(&id));
    }

    let (request_completed_tx, request_completed_rx) = oneshot::channel();
    let request_transfer_error = Arc::new(Mutex::new(None));
    let request_stream = RequestTee {
        inner: body.into_data_stream(),
        capture: CaptureBuffer {
            data: Vec::new(),
            total_bytes: 0,
        },
        store: store.clone(),
        id: id.clone(),
        persisted: false,
        completed: Some(request_completed_tx),
        transfer_error: Arc::clone(&request_transfer_error),
    };
    let upstream_response = reqwest::Client::new()
        .request(parts.method.clone(), &upstream_url)
        .headers(forward_headers(&parts.headers))
        .body(reqwest::Body::wrap_stream(request_stream))
        .send()
        .await;
    let upstream_response = match upstream_response {
        Ok(response) => response,
        Err(error) => {
            let request_error = request_transfer_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            let (state, message) = match request_error {
                Some(message) => (CaptureState::Failed, message),
                None => (
                    CaptureState::Failed,
                    format!("upstream connection failed: {error}"),
                ),
            };
            schedule_finish_after_request(
                request_completed_rx,
                store,
                id.clone(),
                state,
                Some(StatusCode::BAD_GATEWAY.as_u16()),
                Vec::new(),
                CapturedBody::from_bytes(Vec::new(), 0),
                Some(message.clone()),
                captured,
                app.clone(),
            );
            return Ok(text_response(StatusCode::BAD_GATEWAY, message));
        }
    };
    let status = upstream_response.status();
    let response_headers = capture_headers(upstream_response.headers());
    let forwarded_headers = response_headers_for_forwarding(upstream_response.headers());
    let response_stream = ResponseTee {
        inner: Box::pin(upstream_response.bytes_stream()),
        capture: CaptureBuffer {
            data: Vec::new(),
            total_bytes: 0,
        },
        request_completed: Some(request_completed_rx),
        store,
        id,
        response_status: status.as_u16(),
        response_headers,
        captured,
        app,
        finished: false,
    };
    let mut response = Response::new(BodyExt::boxed_unsync(StreamBody::new(response_stream)));
    *response.status_mut() = status;
    for (name, value) in &forwarded_headers {
        response.headers_mut().append(name.clone(), value.clone());
    }
    Ok(response)
}

async fn failed_response(
    parts: &Parts,
    config: &AiRequestCaptureConfig,
    request_path_and_query: String,
    store: CaptureStore,
    app: &Option<AppHandle>,
    status: StatusCode,
    message: String,
) -> ProxyResponse {
    let id = Uuid::new_v4().to_string();
    let started = CaptureStart {
        id: id.clone(),
        started_at: chrono::Utc::now().timestamp_millis(),
        http_version: http_version(parts.version).to_string(),
        method: parts.method.to_string(),
        request_path_and_query,
        upstream_url: config.upstream_base_url.clone(),
        request_headers: capture_headers(&parts.headers),
        request_body: CapturedBody::from_bytes(Vec::new(), 0),
        provider: None,
        model: None,
    };
    let captured = begin_capture(store.clone(), started).await;
    if captured {
        emit_capture_update(app, "created", Some(&id));
    }
    finish_capture(
        store,
        &id,
        CaptureState::Failed,
        Some(status.as_u16()),
        Vec::new(),
        CapturedBody::from_bytes(Vec::new(), 0),
        Some(message.clone()),
        captured,
        app,
    )
    .await;
    text_response(status, message)
}

pub(crate) fn mapped_upstream_url(
    config: &AiRequestCaptureConfig,
    request_path_and_query: &str,
) -> Result<String, String> {
    if !request_path_and_query.starts_with('/') {
        return Err("proxy requests must use an origin-form path".to_string());
    }
    let base = config.upstream_base_url.trim().trim_end_matches('/');
    let target = format!("{base}{request_path_and_query}");
    url::Url::parse(&target)
        .map(|url| url.to_string())
        .map_err(|_| "mapped upstream URL is invalid".to_string())
}

fn forward_headers(headers: &HeaderMap) -> HeaderMap {
    let mut forwarded = remove_hop_by_hop_headers(headers);
    forwarded.remove(HOST);
    forwarded.remove(CONTENT_LENGTH);
    forwarded.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    forwarded
}

fn response_headers_for_forwarding(headers: &HeaderMap) -> HeaderMap {
    let mut forwarded = remove_hop_by_hop_headers(headers);
    forwarded.remove(CONTENT_LENGTH);
    forwarded
}

fn remove_hop_by_hop_headers(headers: &HeaderMap) -> HeaderMap {
    let mut excluded = HashSet::new();
    for name in [
        CONNECTION,
        HeaderName::from_static("keep-alive"),
        PROXY_AUTHENTICATE,
        PROXY_AUTHORIZATION,
        TE,
        TRAILER,
        TRANSFER_ENCODING,
        UPGRADE,
    ] {
        excluded.insert(name);
    }
    for value in headers.get_all(CONNECTION) {
        if let Ok(value) = value.to_str() {
            for name in value
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                if let Ok(name) = HeaderName::from_bytes(name.as_bytes()) {
                    excluded.insert(name);
                }
            }
        }
    }
    let mut forwarded = HeaderMap::new();
    for (name, value) in headers {
        if !excluded.contains(name) {
            forwarded.append(name.clone(), value.clone());
        }
    }
    forwarded
}

fn websocket_upgrade(headers: &HeaderMap) -> bool {
    headers.contains_key(UPGRADE)
        || headers.get_all(CONNECTION).iter().any(|value| {
            value
                .to_str()
                .map(|value| {
                    value
                        .split(',')
                        .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
                })
                .unwrap_or(false)
        })
}

fn capture_headers(headers: &HeaderMap) -> Vec<AiRequestCaptureHeader> {
    let mut captured: Vec<AiRequestCaptureHeader> = Vec::new();
    for (name, value) in headers {
        let name = name.as_str();
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        if let Some(existing) = captured.iter_mut().find(|header| header.name == name) {
            existing.values.push(value);
        } else {
            captured.push(AiRequestCaptureHeader {
                name: name.to_string(),
                values: vec![value],
            });
        }
    }
    captured
}

async fn begin_capture(store: CaptureStore, start: CaptureStart) -> bool {
    tokio::task::spawn_blocking(move || store.begin(start))
        .await
        .ok()
        .and_then(Result::ok)
        .is_some()
}

#[allow(clippy::too_many_arguments)]
async fn finish_capture(
    store: CaptureStore,
    id: &str,
    state: CaptureState,
    response_status: Option<u16>,
    response_headers: Vec<AiRequestCaptureHeader>,
    response_body: CapturedBody,
    error: Option<String>,
    captured: bool,
    app: &Option<AppHandle>,
) {
    if !captured {
        return;
    }
    let id = id.to_string();
    let capture_id = id.clone();
    let event_kind = if state == CaptureState::Completed {
        "completed"
    } else {
        "failed"
    };
    let completed_at = chrono::Utc::now().timestamp_millis();
    let finished = tokio::task::spawn_blocking(move || {
        store.finish(
            &capture_id,
            CaptureFinish {
                completed_at,
                state,
                response_status,
                response_headers,
                response_body,
                error,
            },
        )
    })
    .await
    .ok()
    .and_then(Result::ok)
    .is_some();
    if finished {
        emit_capture_update(app, event_kind, Some(id.as_str()));
    }
}

#[allow(clippy::too_many_arguments)]
fn schedule_finish_after_request(
    request_completed: oneshot::Receiver<()>,
    store: CaptureStore,
    id: String,
    state: CaptureState,
    response_status: Option<u16>,
    response_headers: Vec<AiRequestCaptureHeader>,
    response_body: CapturedBody,
    error: Option<String>,
    captured: bool,
    app: Option<AppHandle>,
) {
    tokio::spawn(async move {
        let _ = request_completed.await;
        finish_capture(
            store,
            &id,
            state,
            response_status,
            response_headers,
            response_body,
            error,
            captured,
            &app,
        )
        .await;
    });
}

fn emit_capture_update(app: &Option<AppHandle>, kind: &str, id: Option<&str>) {
    if let Some(app) = app {
        let _ = app.emit(
            "ai-request-capture-updated",
            serde_json::json!({ "kind": kind, "id": id }),
        );
    }
}

fn text_response(status: StatusCode, message: String) -> ProxyResponse {
    Response::builder()
        .status(status)
        .body(BodyExt::boxed_unsync(
            Full::new(Bytes::from(message)).map_err(|never| match never {}),
        ))
        .expect("build proxy error response")
}

fn http_version(version: Version) -> &'static str {
    match version {
        Version::HTTP_09 => "HTTP/0.9",
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_11 => "HTTP/1.1",
        Version::HTTP_2 => "HTTP/2",
        Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/unknown",
    }
}
