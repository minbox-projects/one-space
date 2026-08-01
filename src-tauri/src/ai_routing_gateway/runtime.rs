use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    header::{AUTHORIZATION, CONTENT_TYPE},
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use reqwest::Client;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot, Mutex},
};
use tokio_stream::wrappers::ReceiverStream;

use super::{
    accounts::{decrypt_api_key, decrypt_oauth_tokens},
    gateway_key,
    protocol::{convert_request, convert_response, convert_sse, error_envelope},
    router::{attempt_decision, candidates, routable_models, AttemptFailure, HealthTracker},
    security::RootKey,
    types::{AccountType, UpstreamProtocol},
};

pub(crate) const DEFAULT_PORT: u16 = 17_688;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(120);

type HttpBody = UnsyncBoxBody<Bytes, Infallible>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeStatus {
    Stopped { port: u16 },
    Running { port: u16 },
    Error { port: u16, code: &'static str },
}

struct RunningRuntime {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
pub(crate) struct GatewayHttpRuntime {
    running: Mutex<Option<RunningRuntime>>,
}

impl GatewayHttpRuntime {
    pub(crate) async fn start(
        &self,
        port: u16,
        service: Arc<GatewayHttpService>,
    ) -> Result<RuntimeStatus, RuntimeStatus> {
        if port == 0 {
            return Err(RuntimeStatus::Error {
                port,
                code: "invalid_port",
            });
        }
        let mut running = self.running.lock().await;
        if running.as_ref().is_some_and(|current| current.port == port) {
            return Ok(RuntimeStatus::Running { port });
        }
        if let Some(mut current) = running.take() {
            if let Some(shutdown) = current.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| RuntimeStatus::Error {
                port,
                code: "port_conflict",
            })?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        tokio::spawn(run_listener(listener, service, shutdown_rx));
        *running = Some(RunningRuntime {
            port,
            shutdown: Some(shutdown_tx),
        });
        Ok(RuntimeStatus::Running { port })
    }

    pub(crate) async fn stop(&self, fallback_port: u16) -> RuntimeStatus {
        let mut running = self.running.lock().await;
        let port = running
            .as_ref()
            .map_or(fallback_port, |current| current.port);
        if let Some(mut current) = running.take() {
            if let Some(shutdown) = current.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
        RuntimeStatus::Stopped { port }
    }

    pub(crate) async fn status(&self, fallback_port: u16) -> RuntimeStatus {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|running| RuntimeStatus::Running { port: running.port })
            .unwrap_or(RuntimeStatus::Stopped {
                port: fallback_port,
            })
    }
}

pub(crate) struct GatewayHttpService {
    database_path: PathBuf,
    root_key: Arc<RootKey>,
    client: Client,
    health: Arc<HealthTracker>,
}

impl GatewayHttpService {
    pub(crate) fn new(database_path: PathBuf, root_key: Arc<RootKey>) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(UPSTREAM_TIMEOUT)
            .build()
            .map_err(|_| "gateway_not_ready".to_owned())?;
        Ok(Self {
            database_path,
            root_key,
            client,
            health: Arc::new(HealthTracker::default()),
        })
    }

    async fn handle(&self, request: Request<Incoming>) -> Response<HttpBody> {
        let method = request.method().clone();
        let path = request.uri().path().to_owned();
        if method == Method::GET && path == "/health" {
            return json_response(
                StatusCode::OK,
                json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }),
            );
        }
        if !matches!(
            path.as_str(),
            "/v1/models" | "/v1/responses" | "/v1/chat/completions"
        ) {
            return gateway_error(StatusCode::NOT_FOUND, "invalid_request", "Route not found");
        }
        let expected_method = if path == "/v1/models" {
            Method::GET
        } else {
            Method::POST
        };
        if method != expected_method {
            return gateway_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "invalid_request",
                "Method not allowed",
            );
        }
        let token = match bearer_token(
            request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
        ) {
            Some(token) => token.to_owned(),
            None => {
                return gateway_error(
                    StatusCode::UNAUTHORIZED,
                    "authentication_failed",
                    "Invalid gateway key",
                )
            }
        };
        let mut connection = match crate::shared_sqlite::open_at(&self.database_path) {
            Ok(connection) => connection,
            Err(_) => {
                return gateway_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "gateway_not_ready",
                    "Gateway is not ready",
                )
            }
        };
        let grant = match gateway_key::authenticate(&connection, &token) {
            Ok(grant) => grant,
            Err(_) => {
                return gateway_error(
                    StatusCode::UNAUTHORIZED,
                    "authentication_failed",
                    "Invalid gateway key",
                )
            }
        };
        if path == "/v1/models" {
            return match routable_models(&connection, &grant, &self.health, Instant::now()) {
                Ok(models) => json_response(
                    StatusCode::OK,
                    json!({ "object": "list", "data": models.into_iter().map(|id| json!({ "id": id, "object": "model", "owned_by": "onespace" })).collect::<Vec<_>>() }),
                ),
                Err(_) => gateway_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "gateway_not_ready",
                    "Gateway is not ready",
                ),
            };
        }
        if request
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_BODY_BYTES)
        {
            return gateway_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "invalid_request",
                "Request body is too large",
            );
        }
        let collected = match request.into_body().collect().await {
            Ok(body) => body.to_bytes(),
            Err(_) => {
                return gateway_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Invalid request body",
                )
            }
        };
        if collected.len() > MAX_BODY_BYTES {
            return gateway_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "invalid_request",
                "Request body is too large",
            );
        }
        let input: Value = match serde_json::from_slice(&collected) {
            Ok(value) => value,
            Err(_) => {
                return gateway_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Invalid JSON body",
                )
            }
        };
        let public_model = match input.get("model").and_then(Value::as_str) {
            Some(model) if !model.trim().is_empty() => model.to_owned(),
            _ => {
                return gateway_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Model is required",
                )
            }
        };
        if !grant.model_ids.iter().any(|model| model == &public_model) {
            return gateway_error(
                StatusCode::FORBIDDEN,
                "permission_denied",
                "The gateway key cannot access this model",
            );
        }
        let requested_protocol = if path == "/v1/responses" {
            UpstreamProtocol::Responses
        } else {
            UpstreamProtocol::ChatCompletions
        };
        self.route_upstream(
            &mut connection,
            &grant,
            requested_protocol,
            &public_model,
            &input,
        )
        .await
    }

    async fn route_upstream(
        &self,
        connection: &mut Connection,
        grant: &gateway_key::GatewayKeyGrant,
        requested_protocol: UpstreamProtocol,
        public_model: &str,
        input: &Value,
    ) -> Response<HttpBody> {
        let endpoint = requested_protocol.as_str();
        let capabilities = requested_capabilities(input);
        let candidates = match candidates(
            connection,
            grant,
            public_model,
            endpoint,
            &capabilities,
            &self.health,
            Instant::now(),
        ) {
            Ok(candidates) => candidates,
            Err(_) => {
                return gateway_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "gateway_not_ready",
                    "Gateway is not ready",
                )
            }
        };
        if candidates.is_empty() {
            return gateway_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_available_upstream",
                "No upstream is currently available",
            );
        }
        let stream = input
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let request_id = format!("req_{}", uuid::Uuid::new_v4().simple());
        for candidate in candidates {
            let converted = match convert_request(
                requested_protocol,
                candidate.protocol,
                input,
                &candidate.upstream_model,
            ) {
                Ok(converted) => converted,
                Err(error) => {
                    return gateway_error(StatusCode::BAD_REQUEST, error.code, error.message)
                }
            };
            let credential = match candidate.account_type {
                AccountType::ApiKey => {
                    decrypt_api_key(connection, &self.root_key, &candidate.account_id)
                }
                AccountType::OAuth => {
                    decrypt_oauth_tokens(connection, &self.root_key, &candidate.account_id)
                        .map(|bundle| bundle.access_token.into_bytes())
                }
            };
            let credential = match credential {
                Ok(credential) => credential,
                Err(_) => continue,
            };
            let credential = match String::from_utf8(credential) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let url = format!(
                "{}/{}",
                candidate.base_url.trim_end_matches('/'),
                candidate.protocol.as_str().replace('_', "/")
            );
            let mut upstream = self
                .client
                .post(url)
                .header("x-request-id", &request_id)
                .json(&converted);
            upstream = if candidate.auth_method == "api_key_header" {
                upstream.header("x-api-key", credential)
            } else {
                upstream.bearer_auth(credential)
            };
            let response = match upstream.send().await {
                Ok(response) => response,
                Err(_) => {
                    self.health.record_failure(
                        &candidate.account_id,
                        AttemptFailure::Network,
                        Instant::now(),
                    );
                    continue;
                }
            };
            let status = response.status();
            if !status.is_success() {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_secs);
                let failure = classify_status(status, retry_after);
                let decision = attempt_decision(candidate.account_type, failure, false, false);
                if decision.affects_health {
                    self.health
                        .record_failure(&candidate.account_id, failure, Instant::now());
                }
                if failure == AttemptFailure::Authorization {
                    let _ = connection.execute(
                        "UPDATE ai_gateway_accounts SET health_status = 'authorization_invalid', health_reason_code = 'upstream_authorization_invalid', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                        [&candidate.account_id],
                    );
                }
                if !decision.retry_different_account {
                    return upstream_error(status, failure);
                }
                continue;
            }
            if stream {
                match self
                    .stream_response(
                        response,
                        candidate.protocol,
                        requested_protocol,
                        public_model,
                        &candidate.account_id,
                    )
                    .await
                {
                    Ok(response) => {
                        let _ = connection.execute(
                            "UPDATE ai_gateway_accounts SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?1",
                            [&candidate.account_id],
                        );
                        return response;
                    }
                    Err(()) => continue,
                }
            }
            let body = match response.bytes().await {
                Ok(body) => body,
                Err(_) => {
                    self.health.record_failure(
                        &candidate.account_id,
                        AttemptFailure::Network,
                        Instant::now(),
                    );
                    continue;
                }
            };
            self.health.record_success(&candidate.account_id);
            let _ = connection.execute(
                "UPDATE ai_gateway_accounts SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?1",
                [&candidate.account_id],
            );
            let value: Value = match serde_json::from_slice(&body) {
                Ok(value) => value,
                Err(_) => {
                    return gateway_error(
                        StatusCode::BAD_GATEWAY,
                        "upstream_unavailable",
                        "Upstream returned an invalid response",
                    )
                }
            };
            return match convert_response(
                candidate.protocol,
                requested_protocol,
                &value,
                public_model,
            ) {
                Ok(value) => json_response(StatusCode::OK, value),
                Err(error) => gateway_error(StatusCode::BAD_GATEWAY, error.code, error.message),
            };
        }
        gateway_error(
            StatusCode::BAD_GATEWAY,
            "upstream_unavailable",
            "All upstream attempts failed",
        )
    }

    async fn stream_response(
        &self,
        response: reqwest::Response,
        source: UpstreamProtocol,
        target: UpstreamProtocol,
        public_model: &str,
        account_id: &str,
    ) -> Result<Response<HttpBody>, ()> {
        let mut upstream = response.bytes_stream();
        let mut pending = Vec::new();
        let first = loop {
            match upstream.next().await {
                Some(Ok(chunk)) => {
                    let converted = match convert_stream_chunk(
                        source,
                        target,
                        &mut pending,
                        &chunk,
                        public_model,
                        false,
                    ) {
                        Ok(converted) => converted,
                        Err(_) => {
                            self.health.record_failure(
                                account_id,
                                AttemptFailure::Server,
                                Instant::now(),
                            );
                            return Err(());
                        }
                    };
                    if !converted.is_empty() {
                        break converted;
                    }
                }
                Some(Err(_)) | None => {
                    self.health
                        .record_failure(account_id, AttemptFailure::Network, Instant::now());
                    return Err(());
                }
            }
        };
        let (sender, receiver) = mpsc::channel::<Result<Frame<Bytes>, Infallible>>(8);
        let source_health = Arc::clone(&self.health);
        let account_id = account_id.to_owned();
        let public_model = public_model.to_owned();
        tokio::spawn(async move {
            if sender
                .send(Ok(Frame::data(Bytes::from(first))))
                .await
                .is_err()
            {
                return;
            }
            while let Some(next) = upstream.next().await {
                let chunk = match next {
                    Ok(chunk) => chunk,
                    Err(_) => {
                        source_health.record_failure(
                            &account_id,
                            AttemptFailure::Network,
                            Instant::now(),
                        );
                        return;
                    }
                };
                let converted = match convert_stream_chunk(
                    source,
                    target,
                    &mut pending,
                    &chunk,
                    &public_model,
                    false,
                ) {
                    Ok(converted) => converted,
                    Err(_) => {
                        source_health.record_failure(
                            &account_id,
                            AttemptFailure::Server,
                            Instant::now(),
                        );
                        return;
                    }
                };
                if !converted.is_empty()
                    && sender
                        .send(Ok(Frame::data(Bytes::from(converted))))
                        .await
                        .is_err()
                {
                    // Receiver closure is the cancellation signal; dropping `upstream` aborts I/O.
                    return;
                }
            }
            if let Ok(final_bytes) =
                convert_stream_chunk(source, target, &mut pending, &[], &public_model, true)
            {
                if !final_bytes.is_empty()
                    && sender
                        .send(Ok(Frame::data(Bytes::from(final_bytes))))
                        .await
                        .is_err()
                {
                    return;
                }
            }
            source_health.record_success(&account_id);
        });
        let body = StreamBody::new(ReceiverStream::new(receiver)).boxed_unsync();
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/event-stream")
            .header("cache-control", "no-store")
            .body(body)
            .unwrap())
    }
}

async fn run_listener(
    listener: TcpListener,
    service: Arc<GatewayHttpService>,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, address)) if is_loopback(address) => {
                    let service = Arc::clone(&service);
                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);
                        let handler = service_fn(move |request| {
                            let service = Arc::clone(&service);
                            async move { Ok::<_, Infallible>(service.handle(request).await) }
                        });
                        let _ = http1::Builder::new().serve_connection(io, handler).await;
                    });
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
}

fn bearer_token(value: Option<&str>) -> Option<&str> {
    value?
        .strip_prefix("Bearer ")
        .filter(|token| !token.trim().is_empty())
}

fn requested_capabilities(input: &Value) -> Vec<&str> {
    let mut capabilities = Vec::new();
    if input.get("tools").is_some() {
        capabilities.push("tools");
    }
    if input.get("reasoning").is_some() || input.get("reasoning_effort").is_some() {
        capabilities.push("reasoning");
    }
    capabilities
}

fn classify_status(status: StatusCode, retry_after: Option<Duration>) -> AttemptFailure {
    match status.as_u16() {
        401 | 403 => AttemptFailure::Authorization,
        429 => AttemptFailure::RateLimited { retry_after },
        500..=599 => AttemptFailure::Server,
        _ => AttemptFailure::SemanticClientError,
    }
}

fn upstream_error(status: StatusCode, failure: AttemptFailure) -> Response<HttpBody> {
    match failure {
        AttemptFailure::Authorization => gateway_error(
            StatusCode::BAD_GATEWAY,
            "upstream_authorization_invalid",
            "Upstream authorization is invalid",
        ),
        AttemptFailure::RateLimited { .. } => gateway_error(
            StatusCode::TOO_MANY_REQUESTS,
            "upstream_rate_limited",
            "Upstream is rate limited",
        ),
        AttemptFailure::SemanticClientError => {
            gateway_error(status, "invalid_request", "Upstream rejected the request")
        }
        _ => gateway_error(
            StatusCode::BAD_GATEWAY,
            "upstream_unavailable",
            "Upstream is unavailable",
        ),
    }
}

fn gateway_error(status: StatusCode, code: &str, message: &str) -> Response<HttpBody> {
    json_response(status, error_envelope(code, message))
}

fn json_response(status: StatusCode, value: Value) -> Response<HttpBody> {
    bytes_response(
        status,
        "application/json",
        serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
    )
}

fn bytes_response(
    status: StatusCode,
    content_type: &'static str,
    bytes: Vec<u8>,
) -> Response<HttpBody> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .header("cache-control", "no-store")
        .body(Full::new(Bytes::from(bytes)).boxed_unsync())
        .unwrap()
}

fn convert_stream_chunk(
    source: UpstreamProtocol,
    target: UpstreamProtocol,
    pending: &mut Vec<u8>,
    chunk: &[u8],
    public_model: &str,
    finished: bool,
) -> Result<Vec<u8>, ()> {
    if source == target {
        return Ok(chunk.to_vec());
    }
    pending.extend_from_slice(chunk);
    let complete_end = pending
        .windows(2)
        .rposition(|window| window == b"\n\n")
        .map(|position| position + 2)
        .or_else(|| finished.then_some(pending.len()))
        .unwrap_or(0);
    if complete_end == 0 {
        return Ok(Vec::new());
    }
    let complete = pending.drain(..complete_end).collect::<Vec<_>>();
    convert_sse(source, target, &complete, public_model).map_err(|_| ())
}

fn is_loopback(address: SocketAddr) -> bool {
    address.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_routing_gateway::{
        accounts::{create_api_key_account, set_model_mapping, CreateApiKeyAccount},
        gateway_key,
        types::ModelMappingDto,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinHandle;

    async fn loopback_port() -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    async fn mock_upstream() -> (u16, Arc<AtomicUsize>, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(AtomicUsize::new(0));
        let task_calls = Arc::clone(&calls);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let calls = Arc::clone(&task_calls);
                tokio::spawn(async move {
                    let handler = service_fn(move |request: Request<Incoming>| {
                        let calls = Arc::clone(&calls);
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            let body = request.into_body().collect().await.unwrap().to_bytes();
                            let input: Value = serde_json::from_slice(&body).unwrap();
                            let stream = input
                                .get("stream")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            let (content_type, body) = if stream {
                                (
                                    "text/event-stream",
                                    concat!(
                                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"streamed\"}\n\n",
                                        "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n",
                                        "data: [DONE]\n\n"
                                    )
                                    .as_bytes()
                                    .to_vec(),
                                )
                            } else {
                                (
                                    "application/json",
                                    serde_json::to_vec(&json!({
                                        "id": "resp_fixture", "object": "response", "status": "completed",
                                        "model": "vendor-model", "output": [{ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "fixture" }] }],
                                        "usage": { "input_tokens": 1, "output_tokens": 2, "total_tokens": 3 }
                                    }))
                                    .unwrap(),
                                )
                            };
                            Ok::<_, Infallible>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header(CONTENT_TYPE, content_type)
                                    .body(Full::new(Bytes::from(body)))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), handler)
                        .await;
                });
            }
        });
        (port, calls, task)
    }

    #[tokio::test]
    async fn runtime_binds_only_ipv4_loopback_and_conflict_stays_stopped() {
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        let runtime = GatewayHttpRuntime::default();
        let path = std::env::temp_dir().join(format!("unused-{}.sqlite3", uuid::Uuid::new_v4()));
        let service = Arc::new(
            GatewayHttpService::new(path, Arc::new(RootKey::try_from(vec![1; 32]).unwrap()))
                .unwrap(),
        );
        assert_eq!(
            runtime.start(port, service).await.unwrap_err(),
            RuntimeStatus::Error {
                port,
                code: "port_conflict"
            }
        );
        assert_eq!(runtime.status(port).await, RuntimeStatus::Stopped { port });
        drop(occupied);
    }

    #[test]
    fn endpoints_and_error_envelopes_are_fixed() {
        assert_eq!(bearer_token(Some("Bearer secret")), Some("secret"));
        assert_eq!(bearer_token(Some("Basic secret")), None);
        let response = gateway_error(
            StatusCode::UNAUTHORIZED,
            "authentication_failed",
            "Invalid gateway key",
        );
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
    }

    #[tokio::test]
    async fn loopback_fixture_covers_four_endpoints_json_sse_and_preflight_rejection() {
        let (upstream_port, upstream_calls, upstream_task) = mock_upstream().await;
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-http-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let root_key = Arc::new(RootKey::try_from(vec![17; 32]).unwrap());
        let mut connection = crate::shared_sqlite::open_at(&path).unwrap();
        let account = create_api_key_account(
            &mut connection,
            &root_key,
            CreateApiKeyAccount {
                name: "Loopback fixture",
                base_url: &format!("http://127.0.0.1:{upstream_port}/v1"),
                api_key: "fixture-upstream-secret",
                auth_method: "bearer",
                upstream_protocol: UpstreamProtocol::Responses,
                note: "",
            },
        )
        .unwrap();
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                account_id: account.id,
                public_model_id: "gpt-5.6-sol".into(),
                upstream_model_id: "vendor-model".into(),
                enabled: true,
            },
        )
        .unwrap();
        let key = gateway_key::create(
            &mut connection,
            "fixture client",
            &["default".into()],
            &["gpt-5.6-sol".into()],
            None,
        )
        .unwrap();
        drop(connection);

        let gateway_port = loopback_port().await;
        let runtime = GatewayHttpRuntime::default();
        let service = Arc::new(GatewayHttpService::new(path.clone(), root_key).unwrap());
        runtime.start(gateway_port, service).await.unwrap();
        let client = Client::new();
        let base = format!("http://127.0.0.1:{gateway_port}");

        let health: Value = client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(health["status"], "ok");
        assert_eq!(health.as_object().unwrap().len(), 2);

        let unauthorized = client
            .get(format!("{base}/v1/models"))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let models: Value = client
            .get(format!("{base}/v1/models"))
            .bearer_auth(&key.plaintext)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(models["data"][0]["id"], "gpt-5.6-sol");

        let responses: Value = client
            .post(format!("{base}/v1/responses"))
            .bearer_auth(&key.plaintext)
            .json(&json!({ "model": "gpt-5.6-sol", "input": "hello" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(responses["model"], "gpt-5.6-sol");

        let chat: Value = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(&key.plaintext)
            .json(&json!({ "model": "gpt-5.6-sol", "messages": [{ "role": "user", "content": "hello" }] }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(chat["choices"][0]["message"]["content"], "fixture");

        let stream = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(&key.plaintext)
            .json(&json!({ "model": "gpt-5.6-sol", "messages": [{ "role": "user", "content": "hello" }], "stream": true }))
            .send()
            .await
            .unwrap();
        assert_eq!(stream.headers()[CONTENT_TYPE], "text/event-stream");
        let stream_body = stream.text().await.unwrap();
        assert!(stream_body.contains("streamed"));
        assert!(stream_body.contains("total_tokens"));

        let calls_before_rejection = upstream_calls.load(Ordering::SeqCst);
        let rejected = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(&key.plaintext)
            .json(&json!({ "model": "gpt-5.6-sol", "messages": [], "response_format": { "type": "json_schema" } }))
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        let rejected_body: Value = rejected.json().await.unwrap();
        assert_eq!(
            rejected_body["error"]["code"],
            "lossless_conversion_unsupported"
        );
        assert_eq!(
            upstream_calls.load(Ordering::SeqCst),
            calls_before_rejection
        );

        runtime.stop(gateway_port).await;
        upstream_task.abort();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }
}
