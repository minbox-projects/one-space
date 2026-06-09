fn status_from_config(config: &ProtocolRouterConfig, running: bool) -> ProtocolRouterStatus {
    ProtocolRouterStatus {
        running,
        enabled: config.enabled,
        port: config.port,
        route_count: config.routes.len(),
    }
}

async fn run_server(listener: TcpListener, mut shutdown: oneshot::Receiver<()>) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        tauri::async_runtime::spawn(async move {
                            let _ = handle_connection(stream).await;
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    let request = read_http_request(&mut stream).await?;
    let response = match route_request(request).await {
        Ok(response) => response,
        Err(response) => response,
    };
    let payload = http_response_bytes(response);
    stream
        .write_all(&payload)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

enum UpstreamResult {
    Json { status: u16, body: Value },
    Stream { status: u16, body: Vec<u8> },
}

fn summarize_non_json_response(status: u16, body: &[u8]) -> String {
    let snippet = String::from_utf8_lossy(body)
        .replace('\n', " ")
        .replace('\r', " ")
        .chars()
        .take(240)
        .collect::<String>();
    if snippet.trim().is_empty() {
        format!("upstream returned HTTP {status} with a non-JSON body")
    } else {
        format!(
            "upstream returned HTTP {status} with a non-JSON body: {}",
            snippet.trim()
        )
    }
}

async fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut header_end = None;
    loop {
        let n = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_header_end(&buf) {
            header_end = Some(pos);
            break;
        }
        if buf.len() > 1024 * 1024 {
            return Err("request headers too large".to_string());
        }
    }
    let header_end = header_end.ok_or_else(|| "invalid http request".to_string())?;
    let headers_raw = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = headers_raw.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let path = request_parts.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[(header_end + 4)..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn route_request(request: HttpRequest) -> Result<HttpResponse, HttpResponse> {
    if request.method != "POST" {
        return Err(json_response(
            405,
            json!({ "error": { "message": "method not allowed" } }),
        ));
    }
    let Some(route_id) = parse_anthropic_route_id(&request.path) else {
        return Err(json_response(
            404,
            json!({ "error": { "message": "route not found" } }),
        ));
    };
    let config =
        read_config().map_err(|e| json_response(500, json!({ "error": { "message": e } })))?;
    if !is_authorized(&request, &config.token) {
        return Err(json_response(
            401,
            json!({ "error": { "message": "invalid router token" } }),
        ));
    }
    let route = resolve_runtime_route(&route_id)
        .map_err(|e| json_response(404, json!({ "error": { "message": e } })))?;
    let input: Value = serde_json::from_slice(&request.body)
        .map_err(|e| json_response(400, json!({ "error": { "message": e.to_string() } })))?;
    let started = Instant::now();
    let model = resolve_model(&route, input.get("model").and_then(|v| v.as_str()));
    if model.trim().is_empty() {
        return Err(json_response(
            400,
            json!({ "error": { "message": "model is required" } }),
        ));
    }
    let result = forward_request(&route, &input, &model).await;
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(UpstreamResult::Json {
            status,
            body: upstream_body,
        }) => {
            let response_body = upstream_to_anthropic(&upstream_body, &model);
            let (input_tokens, output_tokens, total_tokens) = usage_from_value(&upstream_body);
            record_call(
                ProtocolRouterCallRecord {
                    ts: now_ts(),
                    route_id: route.id,
                    provider: route.upstream_provider_name,
                    model,
                    endpoint: "/v1/messages".to_string(),
                    wire_api: route.wire_api,
                    status,
                    latency_ms,
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    error_summary: if status >= 400 {
                        Some(error_summary(&upstream_body))
                    } else {
                        None
                    },
                },
                config.retention_days,
            );
            Ok(json_response(status, response_body))
        }
        Ok(UpstreamResult::Stream { status, body }) => {
            record_call(
                ProtocolRouterCallRecord {
                    ts: now_ts(),
                    route_id: route.id,
                    provider: route.upstream_provider_name,
                    model,
                    endpoint: "/v1/messages".to_string(),
                    wire_api: route.wire_api,
                    status,
                    latency_ms,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    error_summary: if status >= 400 {
                        Some("streaming upstream request failed".to_string())
                    } else {
                        None
                    },
                },
                config.retention_days,
            );
            Ok(sse_response(status, body))
        }
        Err(error) => {
            record_call(
                ProtocolRouterCallRecord {
                    ts: now_ts(),
                    route_id: route.id,
                    provider: route.upstream_provider_name,
                    model,
                    endpoint: "/v1/messages".to_string(),
                    wire_api: route.wire_api,
                    status: 502,
                    latency_ms,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    error_summary: Some(error.clone()),
                },
                config.retention_days,
            );
            Err(json_response(502, json!({ "error": { "message": error } })))
        }
    }
}

fn parse_anthropic_route_id(path: &str) -> Option<String> {
    let clean = path.split('?').next().unwrap_or(path);
    let parts = clean.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() == 4 && parts[0] == "anthropic" && parts[2] == "v1" && parts[3] == "messages" {
        Some(parts[1].to_string())
    } else {
        None
    }
}

fn resolve_runtime_route(route_id: &str) -> Result<ProtocolRoute, String> {
    let routes = derived_routes()?;
    let route = routes
        .into_iter()
        .find(|route| route.id == route_id)
        .ok_or_else(|| format!("route_not_found: route not configured: {route_id}"))?;
    if !route.enabled {
        let reason = if route.upstream_provider_name.trim().is_empty() {
            "route is disabled".to_string()
        } else {
            route.upstream_provider_name.clone()
        };
        return Err(format!(
            "route_unavailable: route '{}' is unavailable: {reason}",
            route.id
        ));
    }
    validate_http_url(&route.base_url, "upstream base URL")
        .map_err(|e| format!("upstream_config_error: {e}"))?;
    if route
        .default_model
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
        && route.mappings.is_empty()
    {
        return Err(format!(
            "upstream_model_error: route '{}' has no upstream model mapping",
            route.id
        ));
    }
    Ok(route)
}

pub(crate) fn route_id_for_claude_provider(provider_id: &str) -> String {
    format!("service-provider-{}", safe_id(provider_id))
}

fn is_authorized(request: &HttpRequest, token: &str) -> bool {
    if token.trim().is_empty() {
        return false;
    }
    let bearer_ok = request
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v == token)
        .unwrap_or(false);
    let anthropic_ok = request
        .headers
        .get("x-api-key")
        .map(|v| v == token)
        .unwrap_or(false);
    bearer_ok || anthropic_ok
}

fn resolve_model(route: &ProtocolRoute, requested: Option<&str>) -> String {
    let raw = requested
        .filter(|m| !m.trim().is_empty())
        .or(route.default_model.as_deref())
        .unwrap_or("")
        .trim()
        .to_string();
    route
        .mappings
        .iter()
        .find(|mapping| mapping.claude_model == raw)
        .map(|mapping| mapping.upstream_model.clone())
        .unwrap_or(raw)
}

async fn forward_request(
    route: &ProtocolRoute,
    input: &Value,
    model: &str,
) -> Result<UpstreamResult, String> {
    let client = Client::new();
    let endpoint = match route.wire_api {
        WireApi::OpenAiChat => "chat/completions",
        WireApi::OpenAiResponses => "responses",
    };
    let url = join_url(&route.base_url, endpoint);
    let upstream_body = match route.wire_api {
        WireApi::OpenAiChat => anthropic_to_openai_chat(input, model),
        WireApi::OpenAiResponses => anthropic_to_openai_responses(input, model),
    };
    let wants_stream = input
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut req = client.post(url).json(&upstream_body);
    let route_api_key = route.api_key.clone();
    if !route_api_key.trim().is_empty() {
        let header = route.auth_header.as_deref().unwrap_or("Authorization");
        if header.eq_ignore_ascii_case("x-api-key") {
            req = req.header(header, route_api_key.trim().to_string());
        } else {
            req = req.header(header, format!("Bearer {}", route_api_key.trim()));
        }
    }
    let response = req
        .send()
        .await
        .map_err(|e| format!("upstream_network_error: {e}"))?;
    let status = response.status().as_u16();
    if wants_stream {
        let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let body = openai_sse_to_anthropic_sse(&bytes, model);
        return Ok(UpstreamResult::Stream { status, body });
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let body = serde_json::from_slice::<Value>(&bytes).map_err(|_| {
        format!(
            "upstream_http_error: {}",
            summarize_non_json_response(status, &bytes)
        )
    })?;
    Ok(UpstreamResult::Json { status, body })
}
