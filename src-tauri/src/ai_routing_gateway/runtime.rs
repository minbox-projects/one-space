use bytes::Bytes;
use chrono::{DateTime, Local, Utc};
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
    sync::{mpsc, oneshot, watch, Mutex},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;

use super::{
    accounts::{
        decrypt_api_key, decrypt_oauth_tokens, load_oauth_refresh_material, replace_oauth_tokens,
        OAuthTokenBundle,
    },
    gateway_key,
    protocol::{
        convert_request, convert_response, convert_sse_with_state, error_envelope,
        SseConversionState,
    },
    request_logs::{
        self, AttemptDraft, AttemptStatus, RequestCompletion, RequestLogDraft, RequestStatus,
    },
    router::{
        attempt_decision, candidates, routable_models, AttemptFailure, HealthTracker, QuotaScope,
    },
    security::RootKey,
    types::{AccountType, UpstreamProtocol},
};

pub(crate) const DEFAULT_PORT: u16 = 17_688;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(120);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_UPSTREAM_ERROR_BYTES: usize = 64 * 1024;

type HttpBody = UnsyncBoxBody<Bytes, Infallible>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeStatus {
    Stopped { port: u16 },
    Running { port: u16 },
    Error { port: u16, code: &'static str },
}

struct RunningRuntime {
    port: u16,
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct GatewayHttpRuntime {
    running: Mutex<Option<RunningRuntime>>,
}

impl GatewayHttpRuntime {
    pub(crate) async fn preflight_port(port: u16) -> Result<(), &'static str> {
        if port == 0 {
            return Err("invalid_port");
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| "port_conflict")?;
        drop(listener);
        Ok(())
    }

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
        let previous = {
            let mut running = self.running.lock().await;
            if running.as_ref().is_some_and(|current| current.port == port) {
                return Ok(RuntimeStatus::Running { port });
            }
            running.take()
        };
        if let Some(previous) = previous {
            stop_running(previous).await;
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| RuntimeStatus::Error {
                port,
                code: "port_conflict",
            })?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(run_listener(listener, service, shutdown_rx));
        self.running.lock().await.replace(RunningRuntime {
            port,
            shutdown: shutdown_tx,
            task,
        });
        Ok(RuntimeStatus::Running { port })
    }

    pub(crate) async fn stop(&self, fallback_port: u16) -> RuntimeStatus {
        let current = self.running.lock().await.take();
        let port = current
            .as_ref()
            .map_or(fallback_port, |current| current.port);
        if let Some(current) = current {
            stop_running(current).await;
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

async fn stop_running(mut running: RunningRuntime) {
    let _ = running.shutdown.send(());
    if tokio::time::timeout(DRAIN_TIMEOUT, &mut running.task)
        .await
        .is_err()
    {
        running.task.abort();
        let _ = running.task.await;
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
            return match routable_models(
                &connection,
                &grant,
                &self.root_key,
                &self.health,
                Instant::now(),
            ) {
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
        let collected = match read_request_body(request.into_body()).await {
            Ok(body) => body,
            Err(BodyReadError::TooLarge) => {
                return gateway_error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "invalid_request",
                    "Request body is too large",
                )
            }
            Err(BodyReadError::Invalid) => {
                return gateway_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Invalid request body",
                )
            }
        };
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
        let request_started = Utc::now();
        let request_id = format!("req_{}", uuid::Uuid::new_v4().simple());
        let local_time = request_started.with_timezone(&Local);
        let timezone_name = local_timezone_name(&local_time);
        let capabilities = requested_capabilities(input);
        let candidates = match candidates(
            connection,
            grant,
            public_model,
            endpoint,
            &capabilities,
            &self.root_key,
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
            let request_log = match request_logs::begin_unrouted_request(
                grant,
                endpoint,
                public_model,
                &request_id,
                request_started,
                local_time,
                &timezone_name,
            ) {
                Ok(request) => request,
                Err(_) => return logging_unavailable(),
            };
            if complete_runtime_request(
                connection,
                &request_log,
                &[],
                RequestStatus::Failed,
                Some("no_available_upstream"),
                request_logs::usage_from_response(&Value::Null),
            )
            .is_err()
            {
                return logging_unavailable();
            }
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
        let mut last_failure: Option<(StatusCode, AttemptFailure)> = None;
        let mut attempts = Vec::<AttemptDraft>::new();
        let mut last_request = None::<RequestLogDraft>;
        let mut preflight_request = None::<RequestLogDraft>;
        for candidate in candidates {
            let request_log = match request_logs::begin_request(
                connection,
                grant,
                &candidate,
                endpoint,
                public_model,
                &request_id,
                request_started,
                local_time,
                &timezone_name,
            ) {
                Ok(request) => request,
                Err(_) => return logging_unavailable(),
            };
            if preflight_request.is_none() {
                preflight_request = Some(request_log.clone());
            }
            let converted = match convert_request(
                requested_protocol,
                candidate.protocol,
                input,
                &candidate.upstream_model,
            ) {
                Ok(converted) => converted,
                Err(error) => {
                    if complete_runtime_request(
                        connection,
                        &request_log,
                        &attempts,
                        RequestStatus::Failed,
                        Some(error.code),
                        request_logs::usage_from_response(&Value::Null),
                    )
                    .is_err()
                    {
                        return logging_unavailable();
                    }
                    return gateway_error(StatusCode::BAD_REQUEST, error.code, error.message);
                }
            };
            let credential = match candidate.account_type {
                AccountType::ApiKey => {
                    decrypt_api_key(connection, &self.root_key, &candidate.account_id)
                        .map(|credential| String::from_utf8(credential).ok())
                }
                AccountType::OAuth => {
                    decrypt_oauth_tokens(connection, &self.root_key, &candidate.account_id)
                        .map(|bundle| Some(bundle.access_token))
                }
            };
            let mut credential = match credential {
                Ok(Some(credential)) => credential,
                Err(_) | Ok(None) => continue,
            };
            let url = format!(
                "{}/{}",
                candidate.base_url.trim_end_matches('/'),
                candidate.protocol.as_str().replace('_', "/")
            );
            let probe_reserved = candidate.is_probe
                && self
                    .health
                    .reserve_probe(&candidate.account_id, Instant::now());
            if candidate.is_probe && !probe_reserved {
                continue;
            }
            let mut oauth_refresh_already_attempted = false;
            loop {
                let invocation_started = Utc::now();
                let mut upstream = self
                    .client
                    .post(&url)
                    .header("x-request-id", &request_id)
                    .json(&converted);
                upstream = if candidate.auth_method == "api_key_header" {
                    upstream.header("x-api-key", &credential)
                } else {
                    upstream.bearer_auth(&credential)
                };
                let response = match upstream.send().await {
                    Ok(response) => response,
                    Err(_) => {
                        let failure = AttemptFailure::Network;
                        self.health
                            .record_failure(&candidate.account_id, failure, Instant::now());
                        if push_attempt(
                            &mut attempts,
                            &request_log,
                            &candidate,
                            invocation_started,
                            AttemptStatus::Failed,
                            Some(failure_code(failure)),
                            false,
                            true,
                        )
                        .is_err()
                        {
                            return logging_unavailable();
                        }
                        last_request = Some(request_log.clone());
                        last_failure = Some((StatusCode::BAD_GATEWAY, failure));
                        break;
                    }
                };
                let status = response.status();
                if !status.is_success() {
                    let error_info = upstream_error_info(response).await;
                    let failure = classify_status(
                        status,
                        error_info.retry_after,
                        error_info.reset_after,
                        error_info.scope,
                        &error_info.body,
                    );
                    let decision = attempt_decision(
                        candidate.account_type,
                        failure,
                        false,
                        oauth_refresh_already_attempted,
                    );
                    if decision.refresh_oauth_once {
                        if push_attempt(
                            &mut attempts,
                            &request_log,
                            &candidate,
                            invocation_started,
                            AttemptStatus::Failed,
                            Some(failure_code(failure)),
                            false,
                            false,
                        )
                        .is_err()
                        {
                            return logging_unavailable();
                        }
                        oauth_refresh_already_attempted = true;
                        match self.refresh_oauth_credential(connection, &candidate).await {
                            Ok(refreshed) => {
                                credential = refreshed;
                                continue;
                            }
                            Err(_) => {
                                let retry_decision = attempt_decision(
                                    candidate.account_type,
                                    failure,
                                    false,
                                    oauth_refresh_already_attempted,
                                );
                                if retry_decision.affects_health {
                                    self.health.record_failure(
                                        &candidate.account_id,
                                        failure,
                                        Instant::now(),
                                    );
                                    if let Some(attempt) = attempts.last_mut() {
                                        attempt.affected_health = true;
                                    }
                                } else if probe_reserved {
                                    self.health.release_probe(&candidate.account_id);
                                }
                                if failure == AttemptFailure::Authorization
                                    && retry_decision.affects_health
                                {
                                    let _ = connection.execute(
                                        "UPDATE ai_gateway_accounts SET health_status = 'authorization_invalid', health_reason_code = 'upstream_authorization_invalid', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                                        [&candidate.account_id],
                                    );
                                }
                                last_request = Some(request_log.clone());
                                last_failure = Some((status, failure));
                                if retry_decision.retry_different_account {
                                    break;
                                }
                                if complete_runtime_request(
                                    connection,
                                    &request_log,
                                    &attempts,
                                    RequestStatus::Failed,
                                    Some(failure_code(failure)),
                                    request_logs::usage_from_response(&Value::Null),
                                )
                                .is_err()
                                {
                                    return logging_unavailable();
                                }
                                return upstream_error(status, failure);
                            }
                        }
                    }
                    if decision.affects_health {
                        self.health
                            .record_failure(&candidate.account_id, failure, Instant::now());
                    } else if probe_reserved {
                        self.health.release_probe(&candidate.account_id);
                    }
                    if failure == AttemptFailure::Authorization
                        && (candidate.account_type == AccountType::ApiKey
                            || oauth_refresh_already_attempted)
                    {
                        let _ = connection.execute(
                            "UPDATE ai_gateway_accounts SET health_status = 'authorization_invalid', health_reason_code = 'upstream_authorization_invalid', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                            [&candidate.account_id],
                        );
                    }
                    if push_attempt(
                        &mut attempts,
                        &request_log,
                        &candidate,
                        invocation_started,
                        AttemptStatus::Failed,
                        Some(failure_code(failure)),
                        false,
                        decision.affects_health,
                    )
                    .is_err()
                    {
                        return logging_unavailable();
                    }
                    last_request = Some(request_log.clone());
                    last_failure = Some((status, failure));
                    if !decision.retry_different_account {
                        if complete_runtime_request(
                            connection,
                            &request_log,
                            &attempts,
                            RequestStatus::Failed,
                            Some(failure_code(failure)),
                            request_logs::usage_from_response(&Value::Null),
                        )
                        .is_err()
                        {
                            return logging_unavailable();
                        }
                        return upstream_error(status, failure);
                    }
                    break;
                }
                if stream {
                    match self
                        .stream_response(
                            response,
                            candidate.protocol,
                            requested_protocol,
                            public_model,
                            &candidate.account_id,
                            &request_id,
                            request_log.clone(),
                            attempts.clone(),
                            candidate.clone(),
                            invocation_started,
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
                        Err(failure) => {
                            let status = if failure == AttemptFailure::ClientCancelled {
                                AttemptStatus::Cancelled
                            } else {
                                AttemptStatus::Failed
                            };
                            if push_attempt(
                                &mut attempts,
                                &request_log,
                                &candidate,
                                invocation_started,
                                status,
                                Some(failure_code(failure)),
                                false,
                                !matches!(failure, AttemptFailure::ClientCancelled),
                            )
                            .is_err()
                                || complete_runtime_request(
                                    connection,
                                    &request_log,
                                    &attempts,
                                    if failure == AttemptFailure::ClientCancelled {
                                        RequestStatus::Cancelled
                                    } else {
                                        RequestStatus::Failed
                                    },
                                    Some(failure_code(failure)),
                                    request_logs::usage_from_response(&Value::Null),
                                )
                                .is_err()
                            {
                                return logging_unavailable();
                            }
                            if failure == AttemptFailure::ClientCancelled {
                                return upstream_error(StatusCode::BAD_GATEWAY, failure);
                            }
                            last_failure = Some((StatusCode::BAD_GATEWAY, failure));
                            break;
                        }
                    }
                }
                let body = match response.bytes().await {
                    Ok(body) => body,
                    Err(_) => {
                        let failure = AttemptFailure::Network;
                        self.health
                            .record_failure(&candidate.account_id, failure, Instant::now());
                        if push_attempt(
                            &mut attempts,
                            &request_log,
                            &candidate,
                            invocation_started,
                            AttemptStatus::Failed,
                            Some(failure_code(failure)),
                            false,
                            true,
                        )
                        .is_err()
                        {
                            return logging_unavailable();
                        }
                        last_request = Some(request_log.clone());
                        last_failure = Some((StatusCode::BAD_GATEWAY, failure));
                        break;
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
                        let failure = AttemptFailure::Server;
                        if push_attempt(
                            &mut attempts,
                            &request_log,
                            &candidate,
                            invocation_started,
                            AttemptStatus::Failed,
                            Some(failure_code(failure)),
                            false,
                            true,
                        )
                        .is_err()
                            || complete_runtime_request(
                                connection,
                                &request_log,
                                &attempts,
                                RequestStatus::Failed,
                                Some(failure_code(failure)),
                                request_logs::usage_from_response(&Value::Null),
                            )
                            .is_err()
                        {
                            return logging_unavailable();
                        }
                        return gateway_error(
                            StatusCode::BAD_GATEWAY,
                            "upstream_unavailable",
                            "Upstream returned an invalid response",
                        );
                    }
                };
                let converted = match convert_response(
                    candidate.protocol,
                    requested_protocol,
                    &value,
                    public_model,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        if push_attempt(
                            &mut attempts,
                            &request_log,
                            &candidate,
                            invocation_started,
                            AttemptStatus::Failed,
                            Some(error.code),
                            false,
                            false,
                        )
                        .is_err()
                            || complete_runtime_request(
                                connection,
                                &request_log,
                                &attempts,
                                RequestStatus::Failed,
                                Some(error.code),
                                request_logs::usage_from_response(&value),
                            )
                            .is_err()
                        {
                            return logging_unavailable();
                        }
                        return gateway_error(StatusCode::BAD_GATEWAY, error.code, error.message);
                    }
                };
                if push_attempt(
                    &mut attempts,
                    &request_log,
                    &candidate,
                    invocation_started,
                    AttemptStatus::Succeeded,
                    None,
                    false,
                    false,
                )
                .is_err()
                    || complete_runtime_request(
                        connection,
                        &request_log,
                        &attempts,
                        RequestStatus::Succeeded,
                        None,
                        request_logs::usage_from_response(&value),
                    )
                    .is_err()
                {
                    return logging_unavailable();
                }
                return json_response(StatusCode::OK, converted);
            }
        }
        match last_failure {
            Some((status, failure)) => {
                let Some(request) = last_request else {
                    return logging_unavailable();
                };
                if complete_runtime_request(
                    connection,
                    &request,
                    &attempts,
                    RequestStatus::Failed,
                    Some(failure_code(failure)),
                    request_logs::usage_from_response(&Value::Null),
                )
                .is_err()
                {
                    return logging_unavailable();
                }
                upstream_error(status, failure)
            }
            None => {
                let Some(request) = preflight_request else {
                    return gateway_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "no_available_upstream",
                        "No upstream is currently available",
                    );
                };
                if complete_runtime_request(
                    connection,
                    &request,
                    &[],
                    RequestStatus::Failed,
                    Some("no_available_upstream"),
                    request_logs::usage_from_response(&Value::Null),
                )
                .is_err()
                {
                    return logging_unavailable();
                }
                gateway_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no_available_upstream",
                    "No upstream is currently available",
                )
            }
        }
    }

    async fn refresh_oauth_credential(
        &self,
        connection: &mut Connection,
        candidate: &super::router::RouteCandidate,
    ) -> Result<String, ()> {
        let material =
            load_oauth_refresh_material(connection, &self.root_key, &candidate.account_id)
                .map_err(|_| ())?;
        let endpoint = material
            .token_endpoint
            .unwrap_or_else(|| format!("{}/oauth/token", candidate.base_url.trim_end_matches('/')));
        let mut form = vec![
            ("grant_type", "refresh_token".to_owned()),
            ("refresh_token", material.token_bundle.refresh_token.clone()),
        ];
        if let Some(client_id) = material.client_id {
            form.push(("client_id", client_id));
        }
        if let Some(client_secret) = material.client_secret {
            form.push(("client_secret", client_secret));
        }
        if !material.token_bundle.scope.is_empty() {
            form.push(("scope", material.token_bundle.scope.clone()));
        }
        let response = self
            .client
            .post(endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|_| ())?;
        if !response.status().is_success() {
            return Err(());
        }
        let body: Value = response.json().await.map_err(|_| ())?;
        let access_token = body
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .ok_or(())?;
        let refreshed = OAuthTokenBundle {
            access_token: access_token.to_owned(),
            refresh_token: body
                .get("refresh_token")
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty())
                .unwrap_or(&material.token_bundle.refresh_token)
                .to_owned(),
            expires_at: material.token_bundle.expires_at,
            token_type: body
                .get("token_type")
                .and_then(Value::as_str)
                .filter(|token_type| !token_type.is_empty())
                .unwrap_or(&material.token_bundle.token_type)
                .to_owned(),
            scope: body
                .get("scope")
                .and_then(Value::as_str)
                .filter(|scope| !scope.is_empty())
                .unwrap_or(&material.token_bundle.scope)
                .to_owned(),
        };
        replace_oauth_tokens(
            connection,
            &self.root_key,
            &candidate.account_id,
            &refreshed,
        )
        .map_err(|_| ())?;
        Ok(refreshed.access_token)
    }

    async fn stream_response(
        &self,
        response: reqwest::Response,
        source: UpstreamProtocol,
        target: UpstreamProtocol,
        public_model: &str,
        account_id: &str,
        request_id: &str,
        request_log: RequestLogDraft,
        previous_attempts: Vec<AttemptDraft>,
        candidate: super::router::RouteCandidate,
        attempt_started: DateTime<Utc>,
    ) -> Result<Response<HttpBody>, AttemptFailure> {
        let mut upstream = response.bytes_stream();
        let mut pending = Vec::new();
        let mut state = SseConversionState::new(source, target, public_model, request_id);
        let mut output_gate = StreamOutputGate::default();
        let mut usage_buffer = Vec::new();
        let mut usage = request_logs::usage_from_response(&Value::Null);
        let (sender, receiver) = mpsc::channel::<Result<Frame<Bytes>, Infallible>>(8);
        let first = loop {
            let next = tokio::select! {
                _ = sender.closed() => return Err(AttemptFailure::ClientCancelled),
                next = upstream.next() => next,
            };
            match next {
                Some(Ok(chunk)) => {
                    let converted = match convert_stream_chunk(
                        source,
                        target,
                        &mut pending,
                        &mut state,
                        &chunk,
                        false,
                    ) {
                        Ok(converted) => converted,
                        Err(_) => {
                            self.health.record_failure(
                                account_id,
                                AttemptFailure::Server,
                                Instant::now(),
                            );
                            return Err(AttemptFailure::Server);
                        }
                    };
                    observe_stream_usage(&mut usage_buffer, &converted, false, &mut usage);
                    let visible = output_gate.push(&converted);
                    if !visible.is_empty() {
                        break visible;
                    }
                }
                Some(Err(_)) => {
                    if sender.is_closed() {
                        self.health.release_probe(account_id);
                        return Err(AttemptFailure::ClientCancelled);
                    }
                    self.health
                        .record_failure(account_id, AttemptFailure::Network, Instant::now());
                    return Err(AttemptFailure::Network);
                }
                None => {
                    let converted =
                        convert_stream_chunk(source, target, &mut pending, &mut state, &[], true)
                            .map_err(|_| AttemptFailure::Server)?;
                    observe_stream_usage(&mut usage_buffer, &converted, true, &mut usage);
                    let mut visible = output_gate.push(&converted);
                    visible.extend(output_gate.finish());
                    if visible.is_empty() && !output_gate.has_terminal() {
                        self.health.record_failure(
                            account_id,
                            AttemptFailure::Network,
                            Instant::now(),
                        );
                        return Err(AttemptFailure::Network);
                    }
                    break visible;
                }
            }
        };
        let source_health = Arc::clone(&self.health);
        let account_id = account_id.to_owned();
        let database_path = self.database_path.clone();
        tokio::spawn(async move {
            if !first.is_empty() && !send_stream_frame(&sender, Bytes::from(first)).await {
                source_health.release_probe(&account_id);
                finish_stream_request(
                    &database_path,
                    &request_log,
                    previous_attempts,
                    &candidate,
                    attempt_started,
                    RequestStatus::Cancelled,
                    AttemptStatus::Cancelled,
                    "client_cancelled",
                    usage,
                    true,
                    false,
                );
                return;
            }
            let mut previous_attempts = previous_attempts;
            let mut output_gate = output_gate;
            loop {
                let next = tokio::select! {
                    _ = sender.closed() => {
                        source_health.release_probe(&account_id);
                        finish_stream_request(
                            &database_path,
                            &request_log,
                            previous_attempts,
                            &candidate,
                            attempt_started,
                            RequestStatus::Cancelled,
                            AttemptStatus::Cancelled,
                            "client_cancelled",
                            usage,
                            true,
                            false,
                        );
                        return;
                    }
                    next = upstream.next() => next,
                };
                let Some(next) = next else { break };
                let chunk = match next {
                    Ok(chunk) => chunk,
                    Err(_) => {
                        if sender.is_closed() {
                            source_health.release_probe(&account_id);
                            finish_stream_request(
                                &database_path,
                                &request_log,
                                previous_attempts,
                                &candidate,
                                attempt_started,
                                RequestStatus::Cancelled,
                                AttemptStatus::Cancelled,
                                "client_cancelled",
                                usage,
                                true,
                                false,
                            );
                        } else {
                            source_health.record_failure(
                                &account_id,
                                AttemptFailure::Network,
                                Instant::now(),
                            );
                            finish_stream_request(
                                &database_path,
                                &request_log,
                                previous_attempts,
                                &candidate,
                                attempt_started,
                                RequestStatus::Interrupted,
                                AttemptStatus::Interrupted,
                                "upstream_unavailable",
                                usage,
                                true,
                                true,
                            );
                        }
                        return;
                    }
                };
                let converted = match convert_stream_chunk(
                    source,
                    target,
                    &mut pending,
                    &mut state,
                    &chunk,
                    false,
                ) {
                    Ok(converted) => converted,
                    Err(_) => {
                        if !sender.is_closed() {
                            source_health.record_failure(
                                &account_id,
                                AttemptFailure::Server,
                                Instant::now(),
                            );
                            finish_stream_request(
                                &database_path,
                                &request_log,
                                previous_attempts,
                                &candidate,
                                attempt_started,
                                RequestStatus::Interrupted,
                                AttemptStatus::Interrupted,
                                "upstream_unavailable",
                                usage,
                                true,
                                true,
                            );
                        } else {
                            source_health.release_probe(&account_id);
                            finish_stream_request(
                                &database_path,
                                &request_log,
                                previous_attempts,
                                &candidate,
                                attempt_started,
                                RequestStatus::Cancelled,
                                AttemptStatus::Cancelled,
                                "client_cancelled",
                                usage,
                                true,
                                false,
                            );
                        }
                        return;
                    }
                };
                observe_stream_usage(&mut usage_buffer, &converted, false, &mut usage);
                let visible = output_gate.push(&converted);
                if !visible.is_empty() && !send_stream_frame(&sender, Bytes::from(visible)).await {
                    source_health.release_probe(&account_id);
                    finish_stream_request(
                        &database_path,
                        &request_log,
                        previous_attempts,
                        &candidate,
                        attempt_started,
                        RequestStatus::Cancelled,
                        AttemptStatus::Cancelled,
                        "client_cancelled",
                        usage,
                        true,
                        false,
                    );
                    return;
                }
            }
            let final_bytes =
                match convert_stream_chunk(source, target, &mut pending, &mut state, &[], true) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        if !sender.is_closed() {
                            source_health.record_failure(
                                &account_id,
                                AttemptFailure::Server,
                                Instant::now(),
                            );
                            finish_stream_request(
                                &database_path,
                                &request_log,
                                previous_attempts,
                                &candidate,
                                attempt_started,
                                RequestStatus::Interrupted,
                                AttemptStatus::Interrupted,
                                "upstream_unavailable",
                                usage,
                                true,
                                true,
                            );
                        } else {
                            source_health.release_probe(&account_id);
                            finish_stream_request(
                                &database_path,
                                &request_log,
                                previous_attempts,
                                &candidate,
                                attempt_started,
                                RequestStatus::Cancelled,
                                AttemptStatus::Cancelled,
                                "client_cancelled",
                                usage,
                                true,
                                false,
                            );
                        }
                        return;
                    }
                };
            observe_stream_usage(&mut usage_buffer, &final_bytes, true, &mut usage);
            let mut visible = output_gate.push(&final_bytes);
            visible.extend(output_gate.finish());
            if !visible.is_empty() && !send_stream_frame(&sender, Bytes::from(visible)).await {
                source_health.release_probe(&account_id);
                finish_stream_request(
                    &database_path,
                    &request_log,
                    previous_attempts,
                    &candidate,
                    attempt_started,
                    RequestStatus::Cancelled,
                    AttemptStatus::Cancelled,
                    "client_cancelled",
                    usage,
                    true,
                    false,
                );
                return;
            }
            if !output_gate.has_terminal() {
                source_health.record_failure(&account_id, AttemptFailure::Network, Instant::now());
                finish_stream_request(
                    &database_path,
                    &request_log,
                    previous_attempts,
                    &candidate,
                    attempt_started,
                    RequestStatus::Interrupted,
                    AttemptStatus::Interrupted,
                    "upstream_unavailable",
                    usage,
                    true,
                    true,
                );
                return;
            }
            if push_attempt(
                &mut previous_attempts,
                &request_log,
                &candidate,
                attempt_started,
                AttemptStatus::Succeeded,
                None,
                true,
                false,
            )
            .is_err()
            {
                source_health.release_probe(&account_id);
                return;
            }
            let mut connection = match crate::shared_sqlite::open_at(&database_path) {
                Ok(connection) => connection,
                Err(_) => {
                    source_health.release_probe(&account_id);
                    return;
                }
            };
            if complete_runtime_request(
                &mut connection,
                &request_log,
                &previous_attempts,
                RequestStatus::Succeeded,
                None,
                usage,
            )
            .is_err()
            {
                source_health.release_probe(&account_id);
                return;
            }
            source_health.record_success(&account_id);
            let terminal = output_gate.take_terminal();
            if !terminal.is_empty() && !send_stream_frame(&sender, Bytes::from(terminal)).await {
                return;
            }
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

#[derive(Debug, Default)]
struct StreamOutputGate {
    pending: Vec<u8>,
    terminal: Vec<u8>,
    terminal_seen: bool,
}

impl StreamOutputGate {
    fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        if self.terminal_seen {
            self.terminal.extend_from_slice(bytes);
            return Vec::new();
        }
        self.pending.extend_from_slice(bytes);
        let mut visible = Vec::new();
        while let Some(position) = self.pending.windows(2).position(|window| window == b"\n\n") {
            let end = position + 2;
            let block = self.pending.drain(..end).collect::<Vec<_>>();
            if is_terminal_sse_block(&block) {
                self.terminal_seen = true;
                self.terminal.extend(block);
                self.terminal.extend(self.pending.drain(..));
                break;
            }
            visible.extend(block);
        }
        visible
    }

    fn finish(&mut self) -> Vec<u8> {
        let pending = std::mem::take(&mut self.pending);
        if self.terminal_seen {
            self.terminal.extend(pending);
            return Vec::new();
        }
        if pending.is_empty() {
            return Vec::new();
        }
        if is_terminal_sse_block(&pending) {
            self.terminal_seen = true;
            self.terminal.extend(pending);
            Vec::new()
        } else {
            pending
        }
    }

    fn has_terminal(&self) -> bool {
        self.terminal_seen
    }

    fn take_terminal(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.terminal)
    }
}

fn is_terminal_sse_block(block: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(block) else {
        return false;
    };
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        return false;
    }
    if data == "[DONE]" {
        return true;
    }
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("response.completed") | Some("response.incomplete")
    ) || value
        .pointer("/choices/0/finish_reason")
        .is_some_and(|finish_reason| !finish_reason.is_null())
}

fn local_timezone_name(local_time: &DateTime<Local>) -> String {
    std::env::var("TZ")
        .ok()
        .and_then(|value| {
            value
                .parse::<chrono_tz::Tz>()
                .ok()
                .map(|zone| zone.name().to_owned())
        })
        .unwrap_or_else(|| format!("local:{}", local_time.format("%:z")))
}

fn push_attempt(
    attempts: &mut Vec<AttemptDraft>,
    request: &RequestLogDraft,
    candidate: &super::router::RouteCandidate,
    started_at: DateTime<Utc>,
    status: AttemptStatus,
    error_code: Option<&str>,
    emitted_client_bytes: bool,
    affected_health: bool,
) -> Result<(), ()> {
    let attempt_number = u8::try_from(attempts.len() + 1).map_err(|_| ())?;
    let item = request_logs::attempt(
        request,
        candidate,
        attempt_number,
        started_at,
        Utc::now(),
        status,
        error_code,
        emitted_client_bytes,
        affected_health,
    )
    .map_err(|_| ())?;
    attempts.push(item);
    Ok(())
}

fn complete_runtime_request(
    connection: &mut Connection,
    request: &RequestLogDraft,
    attempts: &[AttemptDraft],
    status: RequestStatus,
    error_code: Option<&str>,
    usage: super::pricing::TokenUsage,
) -> Result<(), ()> {
    request_logs::complete_request(
        connection,
        request,
        attempts,
        &RequestCompletion {
            completed_at: Utc::now().to_rfc3339(),
            status,
            error_code: error_code.map(str::to_owned),
            usage,
        },
    )
    .map_err(|_| ())
}

#[allow(clippy::too_many_arguments)]
fn finish_stream_request(
    database_path: &std::path::Path,
    request: &RequestLogDraft,
    mut attempts: Vec<AttemptDraft>,
    candidate: &super::router::RouteCandidate,
    attempt_started: DateTime<Utc>,
    request_status: RequestStatus,
    attempt_status: AttemptStatus,
    error_code: &str,
    usage: super::pricing::TokenUsage,
    emitted_client_bytes: bool,
    affected_health: bool,
) {
    let result = (|| -> Result<(), ()> {
        push_attempt(
            &mut attempts,
            request,
            candidate,
            attempt_started,
            attempt_status,
            Some(error_code),
            emitted_client_bytes,
            affected_health,
        )?;
        let mut connection = crate::shared_sqlite::open_at(database_path).map_err(|_| ())?;
        complete_runtime_request(
            &mut connection,
            request,
            &attempts,
            request_status,
            Some(error_code),
            usage,
        )
    })();
    if result.is_err() {
        eprintln!(
            "ai gateway stream log persistence failed for request {}",
            request.request_id
        );
    }
}

fn observe_stream_usage(
    pending: &mut Vec<u8>,
    chunk: &[u8],
    finished: bool,
    usage: &mut super::pricing::TokenUsage,
) {
    const MAX_USAGE_EVENT_BYTES: usize = 64 * 1024;
    pending.extend_from_slice(chunk);
    if pending.len() > MAX_USAGE_EVENT_BYTES {
        let excess = pending.len() - MAX_USAGE_EVENT_BYTES;
        pending.drain(..excess);
    }
    let complete_end = pending
        .windows(2)
        .rposition(|window| window == b"\n\n")
        .map(|position| position + 2)
        .or_else(|| finished.then_some(pending.len()))
        .unwrap_or(0);
    if complete_end == 0 {
        return;
    }
    let complete = pending.drain(..complete_end).collect::<Vec<_>>();
    let Ok(text) = std::str::from_utf8(&complete) else {
        return;
    };
    for block in text.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            let observed = request_logs::usage_from_response(&value);
            usage.input_tokens = observed.input_tokens.or(usage.input_tokens);
            usage.output_tokens = observed.output_tokens.or(usage.output_tokens);
            usage.cache_read_tokens = observed.cache_read_tokens.or(usage.cache_read_tokens);
            usage.cache_write_tokens = observed.cache_write_tokens.or(usage.cache_write_tokens);
            usage.total_tokens = observed.total_tokens.or(usage.total_tokens);
        }
    }
}

fn failure_code(failure: AttemptFailure) -> &'static str {
    match failure {
        AttemptFailure::Authorization => "upstream_authorization_invalid",
        AttemptFailure::QuotaExhausted { .. } | AttemptFailure::RateLimited { .. } => {
            "upstream_rate_limited"
        }
        AttemptFailure::SemanticClientError => "invalid_request",
        AttemptFailure::ClientCancelled => "client_cancelled",
        AttemptFailure::Network | AttemptFailure::Server => "upstream_unavailable",
    }
}

fn logging_unavailable() -> Response<HttpBody> {
    gateway_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "gateway_not_ready",
        "Gateway request logging is unavailable",
    )
}

async fn send_stream_frame(
    sender: &mpsc::Sender<Result<Frame<Bytes>, Infallible>>,
    bytes: Bytes,
) -> bool {
    tokio::select! {
        _ = sender.closed() => false,
        result = sender.send(Ok(Frame::data(bytes))) => result.is_ok(),
    }
}

async fn run_listener(
    listener: TcpListener,
    service: Arc<GatewayHttpService>,
    mut shutdown: oneshot::Receiver<()>,
) {
    let mut connections = JoinSet::new();
    let (connection_shutdown, _) = watch::channel(false);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, address)) if is_loopback(address) => {
                    let service = Arc::clone(&service);
                    let mut connection_shutdown = connection_shutdown.subscribe();
                    connections.spawn(async move {
                        let io = TokioIo::new(stream);
                        let handler = service_fn(move |request| {
                            let service = Arc::clone(&service);
                            async move { Ok::<_, Infallible>(service.handle(request).await) }
                        });
                        let connection = http1::Builder::new().serve_connection(io, handler);
                        tokio::pin!(connection);
                        tokio::select! {
                            _ = &mut connection => {}
                            _ = connection_shutdown.changed() => {
                                connection.as_mut().graceful_shutdown();
                                let _ = connection.await;
                            }
                        }
                    });
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
    let _ = connection_shutdown.send(true);
    if tokio::time::timeout(DRAIN_TIMEOUT, async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyReadError {
    TooLarge,
    Invalid,
}

async fn read_request_body(mut body: Incoming) -> Result<Vec<u8>, BodyReadError> {
    let mut collected = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| BodyReadError::Invalid)?;
        let Some(data) = frame.data_ref() else {
            continue;
        };
        if collected
            .len()
            .checked_add(data.len())
            .is_none_or(|length| length > MAX_BODY_BYTES)
        {
            return Err(BodyReadError::TooLarge);
        }
        collected.extend_from_slice(data);
    }
    Ok(collected)
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

#[derive(Debug, Default)]
struct UpstreamErrorInfo {
    body: Vec<u8>,
    retry_after: Option<Duration>,
    reset_after: Option<Duration>,
    scope: Option<QuotaScope>,
}

async fn upstream_error_info(response: reqwest::Response) -> UpstreamErrorInfo {
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(parse_duration_header);
    let reset_after_header = [
        "x-ratelimit-reset",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ]
    .into_iter()
    .find_map(|name| response.headers().get(name).and_then(parse_duration_header));
    let header_scope = response
        .headers()
        .get("x-ratelimit-scope")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_quota_scope);
    let body = response
        .bytes()
        .await
        .map(|body| {
            body.into_iter()
                .take(MAX_UPSTREAM_ERROR_BYTES)
                .collect::<Vec<u8>>()
        })
        .unwrap_or_default();
    let reset_after = reset_after_header.or_else(|| parse_body_reset(&body));
    let scope = header_scope.or_else(|| {
        let scope = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/scope")
                    .or_else(|| value.get("scope"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })?;
        parse_quota_scope(&scope)
    });
    UpstreamErrorInfo {
        body,
        retry_after,
        reset_after,
        scope,
    }
}

fn parse_body_reset(body: &[u8]) -> Option<Duration> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let reset = value
        .pointer("/error/reset_after")
        .or_else(|| value.pointer("/error/reset_in"))
        .or_else(|| value.get("reset_after"))
        .or_else(|| value.get("reset_in"))?;
    reset
        .as_u64()
        .or_else(|| reset.as_str()?.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn parse_duration_header(value: &reqwest::header::HeaderValue) -> Option<Duration> {
    value
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

fn parse_quota_scope(value: &str) -> Option<QuotaScope> {
    Some(match value.to_ascii_lowercase().as_str() {
        "global" | "account" => QuotaScope::Global,
        "model" => QuotaScope::Model,
        "endpoint" => QuotaScope::Endpoint,
        "capability" | "feature" => QuotaScope::Capability,
        _ => QuotaScope::Unknown,
    })
}

fn classify_status(
    status: StatusCode,
    retry_after: Option<Duration>,
    reset_after: Option<Duration>,
    scope: Option<QuotaScope>,
    body: &[u8],
) -> AttemptFailure {
    let reset_after = reset_after.or_else(|| parse_body_reset(body));
    let scope = scope.or_else(|| parse_body_scope(body));
    if is_quota_exhausted(body) {
        return AttemptFailure::QuotaExhausted { reset_after, scope };
    }
    match status.as_u16() {
        401 | 403 => AttemptFailure::Authorization,
        429 => AttemptFailure::RateLimited { retry_after },
        500..=599 => AttemptFailure::Server,
        _ => AttemptFailure::SemanticClientError,
    }
}

fn parse_body_scope(body: &[u8]) -> Option<QuotaScope> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let scope = value
        .pointer("/error/scope")
        .or_else(|| value.get("scope"))
        .and_then(Value::as_str)?;
    parse_quota_scope(scope)
}

fn is_quota_exhausted(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    let error = value.get("error").unwrap_or(&value);
    ["code", "type", "message"]
        .into_iter()
        .filter_map(|field| error.get(field).and_then(Value::as_str))
        .map(str::to_ascii_lowercase)
        .any(|value| {
            value.contains("insufficient_quota")
                || value.contains("quota_exceeded")
                || value.contains("quota exhausted")
        })
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
        AttemptFailure::QuotaExhausted { .. } => gateway_error(
            StatusCode::TOO_MANY_REQUESTS,
            "upstream_rate_limited",
            "Upstream quota is exhausted",
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
    state: &mut SseConversionState,
    chunk: &[u8],
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
    convert_sse_with_state(state, &complete, finished).map_err(|_| ())
}

fn is_loopback(address: SocketAddr) -> bool {
    address.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_routing_gateway::{
        accounts::{
            create_api_key_account, decrypt_oauth_tokens, set_model_mapping, upsert_oauth_account,
            CreateApiKeyAccount, OAuthTokenBundle, UpsertOAuthAccount,
        },
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

    async fn oauth_mock_upstream(
        refresh_succeeds: bool,
    ) -> (u16, Arc<AtomicUsize>, Arc<AtomicUsize>, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let upstream_calls = Arc::new(AtomicUsize::new(0));
        let refresh_calls = Arc::new(AtomicUsize::new(0));
        let task_upstream_calls = Arc::clone(&upstream_calls);
        let task_refresh_calls = Arc::clone(&refresh_calls);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let upstream_calls = Arc::clone(&task_upstream_calls);
                let refresh_calls = Arc::clone(&task_refresh_calls);
                tokio::spawn(async move {
                    let handler = service_fn(move |request: Request<Incoming>| {
                        let upstream_calls = Arc::clone(&upstream_calls);
                        let refresh_calls = Arc::clone(&refresh_calls);
                        async move {
                            let path = request.uri().path().to_owned();
                            let authorization = request
                                .headers()
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default()
                                .to_owned();
                            let _ = request.into_body().collect().await;
                            if path == "/oauth/token" {
                                refresh_calls.fetch_add(1, Ordering::SeqCst);
                                let (status, body) = if refresh_succeeds {
                                    (
                                        StatusCode::OK,
                                        serde_json::to_vec(&json!({
                                            "access_token": "new-access",
                                            "refresh_token": "new-refresh",
                                            "token_type": "Bearer",
                                            "scope": "fixture"
                                        }))
                                        .unwrap(),
                                    )
                                } else {
                                    (StatusCode::BAD_GATEWAY, Vec::new())
                                };
                                return Ok::<_, Infallible>(
                                    Response::builder()
                                        .status(status)
                                        .header(CONTENT_TYPE, "application/json")
                                        .body(Full::new(Bytes::from(body)))
                                        .unwrap(),
                                );
                            }
                            upstream_calls.fetch_add(1, Ordering::SeqCst);
                            if authorization == "Bearer old-access" {
                                return Ok::<_, Infallible>(
                                    Response::builder()
                                        .status(StatusCode::UNAUTHORIZED)
                                        .header(CONTENT_TYPE, "application/json")
                                        .body(Full::new(Bytes::from_static(
                                            br#"{"error":{"code":"invalid_token","message":"expired"}}"#,
                                        )))
                                        .unwrap(),
                                );
                            }
                            Ok::<_, Infallible>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header(CONTENT_TYPE, "application/json")
                                    .body(Full::new(Bytes::from(
                                        serde_json::to_vec(&json!({
                                            "id": "resp-oauth",
                                            "object": "response",
                                            "status": "completed",
                                            "model": "vendor-model",
                                            "output": [{"type":"message","content":[{"type":"output_text","text":"oauth-ok"}]}],
                                            "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
                                        }))
                                        .unwrap(),
                                    )))
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
        (port, upstream_calls, refresh_calls, task)
    }

    async fn delayed_stream_upstream() -> (u16, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let handler = service_fn(move |request: Request<Incoming>| async move {
                        let _ = request.into_body().collect().await;
                        let (sender, receiver) =
                            mpsc::channel::<Result<Frame<Bytes>, Infallible>>(2);
                        tokio::spawn(async move {
                            let _ = sender
                                .send(Ok(Frame::data(Bytes::from_static(
                                    b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"first\"}\n\n",
                                ))))
                                .await;
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            let _ = sender
                                .send(Ok(Frame::data(Bytes::from_static(
                                    b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\ndata: [DONE]\n\n",
                                ))))
                                .await;
                        });
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .header(CONTENT_TYPE, "text/event-stream")
                                .body(StreamBody::new(ReceiverStream::new(receiver)))
                                .unwrap(),
                        )
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), handler)
                        .await;
                });
            }
        });
        (port, task)
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

    #[tokio::test]
    async fn stop_drains_listener_and_releases_port_before_returning() {
        let port = loopback_port().await;
        let path = std::env::temp_dir().join(format!("unused-{}.sqlite3", uuid::Uuid::new_v4()));
        let service = Arc::new(
            GatewayHttpService::new(path, Arc::new(RootKey::try_from(vec![2; 32]).unwrap()))
                .unwrap(),
        );
        let runtime = GatewayHttpRuntime::default();
        assert_eq!(
            runtime.start(port, service).await.unwrap(),
            RuntimeStatus::Running { port }
        );
        assert_eq!(runtime.stop(port).await, RuntimeStatus::Stopped { port });
        let rebound = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await;
        assert!(rebound.is_ok());
    }

    #[tokio::test]
    async fn rebind_conflict_releases_old_listener_stays_stopped_and_recovers_manually() {
        let old_port = loopback_port().await;
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let new_port = occupied.local_addr().unwrap().port();
        let path = std::env::temp_dir().join(format!("unused-{}.sqlite3", uuid::Uuid::new_v4()));
        let service = Arc::new(
            GatewayHttpService::new(path, Arc::new(RootKey::try_from(vec![3; 32]).unwrap()))
                .unwrap(),
        );
        let runtime = GatewayHttpRuntime::default();

        assert_eq!(
            runtime.start(old_port, Arc::clone(&service)).await,
            Ok(RuntimeStatus::Running { port: old_port })
        );
        assert_eq!(
            runtime.start(new_port, Arc::clone(&service)).await,
            Err(RuntimeStatus::Error {
                port: new_port,
                code: "port_conflict"
            })
        );
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, old_port))
            .await
            .is_ok());
        assert_eq!(
            runtime.status(new_port).await,
            RuntimeStatus::Stopped { port: new_port }
        );

        drop(occupied);
        tokio::task::yield_now().await;
        assert_eq!(
            runtime.status(new_port).await,
            RuntimeStatus::Stopped { port: new_port }
        );
        assert_eq!(
            runtime.start(new_port, service).await,
            Ok(RuntimeStatus::Running { port: new_port })
        );
        runtime.stop(new_port).await;
    }

    #[tokio::test]
    async fn oauth_authorization_refreshes_once_then_retries_the_same_account() {
        let (upstream_port, upstream_calls, refresh_calls, upstream_task) =
            oauth_mock_upstream(true).await;
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-oauth-refresh-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let root_key = Arc::new(RootKey::try_from(vec![23; 32]).unwrap());
        let mut connection = crate::shared_sqlite::open_at(&path).unwrap();
        let account = upsert_oauth_account(
            &mut connection,
            &root_key,
            UpsertOAuthAccount {
                stable_external_id: "oauth-fixture-user",
                name: "OAuth Fixture",
                token_bundle: &OAuthTokenBundle {
                    access_token: "old-access".into(),
                    refresh_token: "old-refresh".into(),
                    expires_at: None,
                    token_type: "Bearer".into(),
                    scope: "fixture".into(),
                },
                metadata_json: &format!(
                    "{{\"token_endpoint\":\"http://127.0.0.1:{upstream_port}/oauth/token\"}}"
                ),
            },
        )
        .unwrap();
        connection
            .execute(
                "UPDATE ai_gateway_accounts SET base_url = ?1, auth_method = 'bearer', upstream_protocol = 'responses' WHERE id = ?2",
                rusqlite::params![format!("http://127.0.0.1:{upstream_port}/v1"), account.id],
            )
            .unwrap();
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                account_id: account.id.clone(),
                public_model_id: "gpt-5.6-sol".into(),
                upstream_model_id: "vendor-model".into(),
                enabled: true,
            },
        )
        .unwrap();
        let key = gateway_key::create(
            &mut connection,
            "oauth fixture client",
            &["default".into()],
            &["gpt-5.6-sol".into()],
            None,
        )
        .unwrap();
        drop(connection);

        let gateway_port = loopback_port().await;
        let runtime = GatewayHttpRuntime::default();
        let service = Arc::new(GatewayHttpService::new(path.clone(), root_key.clone()).unwrap());
        runtime.start(gateway_port, service).await.unwrap();
        let response: Value = Client::new()
            .post(format!("http://127.0.0.1:{gateway_port}/v1/responses"))
            .bearer_auth(&key.plaintext)
            .json(&json!({"model":"gpt-5.6-sol","input":"hello"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["output"][0]["content"][0]["text"], "oauth-ok");
        assert_eq!(upstream_calls.load(Ordering::SeqCst), 2);
        assert_eq!(refresh_calls.load(Ordering::SeqCst), 1);

        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        assert_eq!(
            decrypt_oauth_tokens(&connection, &root_key, &account.id)
                .unwrap()
                .access_token,
            "new-access"
        );
        let health_status: String = connection
            .query_row(
                "SELECT health_status FROM ai_gateway_accounts WHERE id = ?1",
                [&account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(health_status, "unknown");
        let request_log: (String, Option<i64>) = connection
            .query_row(
                "SELECT status, total_tokens FROM ai_gateway_request_logs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(request_log, ("succeeded".into(), Some(2)));
        let attempts: Vec<(i64, String, Option<String>)> = connection
            .prepare("SELECT attempt_number, status, error_code FROM ai_gateway_request_attempts ORDER BY attempt_number")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(
            attempts[0].2.as_deref(),
            Some("upstream_authorization_invalid")
        );
        assert_eq!(attempts[1], (2, "succeeded".into(), None));
        let aggregate_total: Option<i64> = connection
            .query_row(
                "SELECT total_tokens FROM ai_gateway_daily_aggregates",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(aggregate_total, Some(2));
        drop(connection);
        runtime.stop(gateway_port).await;
        upstream_task.abort();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[tokio::test]
    async fn oauth_refresh_failure_logs_one_attempt_without_replaying_old_token() {
        let (upstream_port, upstream_calls, refresh_calls, upstream_task) =
            oauth_mock_upstream(false).await;
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-oauth-refresh-failure-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let root_key = Arc::new(RootKey::try_from(vec![31; 32]).unwrap());
        let mut connection = crate::shared_sqlite::open_at(&path).unwrap();
        let account = upsert_oauth_account(
            &mut connection,
            &root_key,
            UpsertOAuthAccount {
                stable_external_id: "oauth-failure-fixture-user",
                name: "OAuth Failure Fixture",
                token_bundle: &OAuthTokenBundle {
                    access_token: "old-access".into(),
                    refresh_token: "old-refresh".into(),
                    expires_at: None,
                    token_type: "Bearer".into(),
                    scope: "fixture".into(),
                },
                metadata_json: &format!(
                    "{{\"token_endpoint\":\"http://127.0.0.1:{upstream_port}/oauth/token\"}}"
                ),
            },
        )
        .unwrap();
        connection
            .execute(
                "UPDATE ai_gateway_accounts SET base_url = ?1, auth_method = 'bearer', upstream_protocol = 'responses' WHERE id = ?2",
                rusqlite::params![format!("http://127.0.0.1:{upstream_port}/v1"), account.id],
            )
            .unwrap();
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                account_id: account.id.clone(),
                public_model_id: "gpt-5.6-sol".into(),
                upstream_model_id: "vendor-model".into(),
                enabled: true,
            },
        )
        .unwrap();
        let key = gateway_key::create(
            &mut connection,
            "oauth failure fixture client",
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
        let response = Client::new()
            .post(format!("http://127.0.0.1:{gateway_port}/v1/responses"))
            .bearer_auth(&key.plaintext)
            .json(&json!({"model":"gpt-5.6-sol","input":"hello"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(upstream_calls.load(Ordering::SeqCst), 1);
        assert_eq!(refresh_calls.load(Ordering::SeqCst), 1);

        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let attempt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_request_attempts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_count, 1);
        let attempt: (i64, String, Option<String>) = connection
            .query_row(
                "SELECT attempt_number, status, error_code FROM ai_gateway_request_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            attempt,
            (
                1,
                "failed".into(),
                Some("upstream_authorization_invalid".into())
            )
        );
        let health: (String, Option<String>) = connection
            .query_row(
                "SELECT health_status, health_reason_code FROM ai_gateway_accounts WHERE id = ?1",
                [&account.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            health,
            (
                "authorization_invalid".into(),
                Some("upstream_authorization_invalid".into())
            )
        );
        drop(connection);
        runtime.stop(gateway_port).await;
        upstream_task.abort();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[tokio::test]
    async fn client_disconnect_releases_stream_probe_without_health_failure() {
        let (upstream_port, upstream_task) = delayed_stream_upstream().await;
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-stream-cancel-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let root_key = Arc::new(RootKey::try_from(vec![29; 32]).unwrap());
        let mut connection = crate::shared_sqlite::open_at(&path).unwrap();
        let account = create_api_key_account(
            &mut connection,
            &root_key,
            CreateApiKeyAccount {
                name: "Stream Fixture",
                base_url: &format!("http://127.0.0.1:{upstream_port}/v1"),
                api_key: "stream-secret",
                auth_method: "bearer",
                upstream_protocol: UpstreamProtocol::Responses,
                note: "",
            },
        )
        .unwrap();
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                account_id: account.id.clone(),
                public_model_id: "gpt-5.6-sol".into(),
                upstream_model_id: "vendor-model".into(),
                enabled: true,
            },
        )
        .unwrap();
        let key = gateway_key::create(
            &mut connection,
            "stream fixture client",
            &["default".into()],
            &["gpt-5.6-sol".into()],
            None,
        )
        .unwrap();
        drop(connection);

        let gateway_port = loopback_port().await;
        let runtime = GatewayHttpRuntime::default();
        let service = Arc::new(GatewayHttpService::new(path.clone(), root_key).unwrap());
        runtime
            .start(gateway_port, Arc::clone(&service))
            .await
            .unwrap();
        let response = Client::new()
            .post(format!(
                "http://127.0.0.1:{gateway_port}/v1/chat/completions"
            ))
            .bearer_auth(&key.plaintext)
            .json(&json!({
                "model": "gpt-5.6-sol",
                "messages": [{"role":"user","content":"hello"}],
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        let mut stream = response.bytes_stream();
        let first = stream.next().await.unwrap().unwrap();
        assert!(first.windows(5).any(|window| window == b"first"));
        drop(stream);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (consecutive_failures, probe_in_flight) = service
            .health
            .state_snapshot(&account.id)
            .expect("stream health state");
        assert_eq!(consecutive_failures, 0);
        assert!(!probe_in_flight);
        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let cancelled: (String, Option<i64>) = connection
            .query_row(
                "SELECT status, total_tokens FROM ai_gateway_request_logs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(cancelled, ("cancelled".into(), None));
        let attempt_status: String = connection
            .query_row(
                "SELECT status FROM ai_gateway_request_attempts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_status, "cancelled");
        drop(connection);

        let draining_response = Client::new()
            .post(format!(
                "http://127.0.0.1:{gateway_port}/v1/chat/completions"
            ))
            .bearer_auth(&key.plaintext)
            .json(&json!({
                "model": "gpt-5.6-sol",
                "messages": [{"role":"user","content":"drain"}],
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        let body = tokio::spawn(async move { draining_response.text().await.unwrap() });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let drain_started = Instant::now();
        assert_eq!(
            runtime.stop(gateway_port).await,
            RuntimeStatus::Stopped { port: gateway_port }
        );
        assert!(drain_started.elapsed() < DRAIN_TIMEOUT);
        let body = body.await.unwrap();
        assert!(body.contains("\"finish_reason\":\"stop\""));
        assert!(body.contains("[DONE]"));
        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let statuses = connection
            .prepare("SELECT status, COUNT(*) FROM ai_gateway_request_logs GROUP BY status ORDER BY status")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            statuses,
            vec![("cancelled".to_owned(), 1), ("succeeded".to_owned(), 1)]
        );
        drop(connection);
        upstream_task.abort();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
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
    async fn failure_classification_preserves_quota_reset_scope_and_rate_limit_code() {
        let failure = classify_status(
            StatusCode::TOO_MANY_REQUESTS,
            None,
            None,
            None,
            br#"{"error":{"code":"insufficient_quota","scope":"model","reset_after":3600}}"#,
        );
        assert_eq!(
            failure,
            AttemptFailure::QuotaExhausted {
                reset_after: Some(Duration::from_secs(3600)),
                scope: Some(QuotaScope::Model),
            }
        );
        let response = upstream_error(
            StatusCode::TOO_MANY_REQUESTS,
            AttemptFailure::RateLimited { retry_after: None },
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "upstream_rate_limited");
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

        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let (log_count, attempt_count, total_tokens): (i64, i64, i64) = connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM ai_gateway_request_logs), (SELECT COUNT(*) FROM ai_gateway_request_attempts), (SELECT SUM(total_tokens) FROM ai_gateway_request_logs)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((log_count, attempt_count, total_tokens), (3, 3, 9));
        let aggregate: (i64, Option<i64>) = connection
            .query_row(
                "SELECT request_count, total_tokens FROM ai_gateway_daily_aggregates",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(aggregate, (3, Some(9)));
        drop(connection);

        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_stream_aggregate BEFORE INSERT ON ai_gateway_daily_aggregates BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
            )
            .unwrap();
        drop(connection);
        let persisted_failure_stream = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(&key.plaintext)
            .json(&json!({ "model": "gpt-5.6-sol", "messages": [{ "role": "user", "content": "hello" }], "stream": true }))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(persisted_failure_stream.contains("streamed"));
        assert!(!persisted_failure_stream.contains("\"finish_reason\":\"stop\""));
        assert!(!persisted_failure_stream.contains("[DONE]"));
        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let failed_persistence_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_gateway_request_logs", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(failed_persistence_count, 3);
        connection
            .execute_batch("DROP TRIGGER reject_stream_aggregate")
            .unwrap();
        drop(connection);

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
        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let rejected_log: (String, Option<String>) = connection
            .query_row(
                "SELECT status, error_code FROM ai_gateway_request_logs ORDER BY started_at DESC, id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            rejected_log,
            (
                "failed".into(),
                Some("lossless_conversion_unsupported".into())
            )
        );
        drop(connection);

        let oversized = reqwest::Body::wrap_stream(tokio_stream::iter(vec![
            Ok::<_, std::io::Error>(Bytes::from(vec![b'a'; MAX_BODY_BYTES])),
            Ok::<_, std::io::Error>(Bytes::from_static(b"overflow")),
        ]));
        let oversized_response = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(&key.plaintext)
            .body(oversized)
            .send()
            .await
            .unwrap();
        assert_eq!(oversized_response.status(), StatusCode::PAYLOAD_TOO_LARGE);
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

    #[tokio::test]
    async fn sensitive_fixture_material_is_never_persisted() {
        let (upstream_port, upstream_calls, upstream_task) = mock_upstream().await;
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-sensitive-fixture-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let root_key = Arc::new(RootKey::try_from(vec![37; 32]).unwrap());
        let api_key_secret = "fixture-api-key-secret";
        let prompt_secret = "prompt fixture body";
        let token_secret = "oauth-access-token";
        let sensitive_header = "sensitive-header-value";
        let mut connection = crate::shared_sqlite::open_at(&path).unwrap();
        let account = create_api_key_account(
            &mut connection,
            &root_key,
            CreateApiKeyAccount {
                name: "Sensitive fixture account",
                base_url: &format!("http://127.0.0.1:{upstream_port}/v1"),
                api_key: api_key_secret,
                auth_method: "bearer",
                upstream_protocol: UpstreamProtocol::Responses,
                note: "fixture note",
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
            "sensitive fixture client",
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
        let response = Client::new()
            .post(format!("http://127.0.0.1:{gateway_port}/v1/responses"))
            .bearer_auth(&key.plaintext)
            .header("x-sensitive-fixture", sensitive_header)
            .json(&json!({
                "model": "gpt-5.6-sol",
                "input": format!("{prompt_secret} {token_secret}")
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(upstream_calls.load(Ordering::SeqCst), 1);

        let connection = crate::shared_sqlite::open_at(&path).unwrap();
        let persisted_text: String = connection
            .query_row(
                "SELECT COALESCE((SELECT group_concat(COALESCE(id, '') || COALESCE(request_id, '') || COALESCE(started_at, '') || COALESCE(completed_at, '') || COALESCE(local_date, '') || COALESCE(timezone_name, '') || COALESCE(endpoint, '') || COALESCE(public_model_id, '') || COALESCE(upstream_model_id_snapshot, '') || COALESCE(api_key_id_snapshot, '') || COALESCE(api_key_name_snapshot, '') || COALESCE(account_id_snapshot, '') || COALESCE(account_name_snapshot, '') || COALESCE(group_id_snapshot, '') || COALESCE(group_name_snapshot, '') || COALESCE(status, '') || COALESCE(error_code, '') || COALESCE(price_snapshot_json, '') || COALESCE(estimated_cost_usd, '')) FROM ai_gateway_request_logs), '') || COALESCE((SELECT group_concat(COALESCE(id, '') || COALESCE(request_log_id, '') || COALESCE(account_id, '') || COALESCE(account_name_snapshot, '') || COALESCE(group_id_snapshot, '') || COALESCE(group_name_snapshot, '') || COALESCE(upstream_model_id_snapshot, '') || COALESCE(started_at, '') || COALESCE(completed_at, '') || COALESCE(status, '') || COALESCE(error_code, '')) FROM ai_gateway_request_attempts), '') || COALESCE((SELECT group_concat(COALESCE(account_id, '') || COALESCE(record_type, '') || COALESCE(metadata_json, '')) FROM ai_gateway_credentials), '') || COALESCE((SELECT group_concat(COALESCE(id, '') || COALESCE(name, '') || COALESCE(note, '') || COALESCE(base_url, '') || COALESCE(auth_method, '')) FROM ai_gateway_accounts), '') || COALESCE((SELECT group_concat(COALESCE(local_date, '') || COALESCE(timezone_name, '') || COALESCE(account_id_snapshot, '') || COALESCE(account_name_snapshot, '') || COALESCE(group_id_snapshot, '') || COALESCE(group_name_snapshot, '') || COALESCE(public_model_id, '') || COALESCE(estimated_cost_usd, '')) FROM ai_gateway_daily_aggregates), '')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        for secret in [
            prompt_secret,
            token_secret,
            api_key_secret,
            &key.plaintext,
            &format!("Bearer {}", key.plaintext),
            sensitive_header,
        ] {
            assert!(
                !persisted_text.contains(secret),
                "persisted secret: {secret}"
            );
        }
        drop(connection);
        runtime.stop(gateway_port).await;
        upstream_task.abort();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }
}
