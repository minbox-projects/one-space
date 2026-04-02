use crate::mcp_servers::{MCPServer, MCPServerTransport};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};

const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpRuntimeTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct McpToolCallOutput {
    pub text: String,
    pub structured_content: Option<Value>,
    pub raw_result: Value,
}

#[derive(Debug, Clone)]
struct RpcResponseEnvelope {
    payload: Value,
    session_id: Option<String>,
    protocol_version: Option<String>,
}

struct StdioSession {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

struct HttpSession {
    client: reqwest::Client,
    endpoint: String,
}

struct SseSession {
    client: reqwest::Client,
    endpoint: String,
    post_endpoint: String,
    receiver: mpsc::UnboundedReceiver<Value>,
}

enum McpSession {
    Stdio(StdioSession),
    Http(HttpSession),
    Sse(SseSession),
}

pub struct McpClient {
    server: MCPServer,
    session: McpSession,
    request_id: u64,
    protocol_version: String,
    session_id: Option<String>,
}

impl McpClient {
    pub async fn connect(server: &MCPServer) -> Result<Self, String> {
        let timeout_ms = timeout_ms(server);
        let session = match server.transport {
            MCPServerTransport::Stdio => McpSession::Stdio(connect_stdio(server).await?),
            MCPServerTransport::Http => McpSession::Http(connect_http(server)?),
            MCPServerTransport::Sse => McpSession::Sse(connect_sse(server, timeout_ms).await?),
        };

        let mut client = Self {
            server: server.clone(),
            session,
            request_id: 1,
            protocol_version: DEFAULT_PROTOCOL_VERSION.to_string(),
            session_id: None,
        };

        let initialize_result = client
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": DEFAULT_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "OneSpace",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;

        if let Some(version) = initialize_result
            .get("protocolVersion")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
        {
            client.protocol_version = version.to_string();
        }

        client
            .send_notification("notifications/initialized", Some(json!({})))
            .await?;

        Ok(client)
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpRuntimeTool>, String> {
        let result = self.send_request("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(tools
            .into_iter()
            .filter_map(|item| {
                let name = item
                    .get("name")
                    .and_then(|value| value.as_str())?
                    .trim()
                    .to_string();
                if name.is_empty() {
                    return None;
                }
                let description = item
                    .get("description")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let input_schema = item
                    .get("inputSchema")
                    .cloned()
                    .or_else(|| item.get("input_schema").cloned())
                    .filter(|value| value.is_object())
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
                Some(McpRuntimeTool {
                    name,
                    description,
                    input_schema,
                })
            })
            .collect())
    }

    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolCallOutput, String> {
        let result = self
            .send_request(
                "tools/call",
                json!({
                    "name": tool_name,
                    "arguments": normalize_tool_arguments(arguments),
                }),
            )
            .await?;

        if result
            .get("isError")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            return Err(render_tool_result_text(&result));
        }

        Ok(McpToolCallOutput {
            text: render_tool_result_text(&result),
            structured_content: result.get("structuredContent").cloned(),
            raw_result: result,
        })
    }

    pub async fn close(&mut self) {
        if let McpSession::Stdio(session) = &mut self.session {
            let _ = session.child.start_kill();
        }
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_request_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let envelope = self.send_message(request, true, id).await?;
        if let Some(session_id) = envelope.session_id {
            self.session_id = Some(session_id);
        }
        if let Some(protocol_version) = envelope.protocol_version {
            self.protocol_version = protocol_version;
        }

        if let Some(error) = envelope.payload.get("error") {
            return Err(format_json_rpc_error(error));
        }

        envelope
            .payload
            .get("result")
            .cloned()
            .ok_or_else(|| format!("MCP response for '{}' did not contain a result", method))
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), String> {
        let mut request = json!({
            "jsonrpc": "2.0",
            "method": method,
        });
        if let Some(params) = params {
            request["params"] = params;
        }
        let _ = self.send_message(request, false, 0).await?;
        Ok(())
    }

    async fn send_message(
        &mut self,
        payload: Value,
        expects_response: bool,
        request_id: u64,
    ) -> Result<RpcResponseEnvelope, String> {
        match &mut self.session {
            McpSession::Stdio(session) => {
                send_stdio_message(&self.server, session, payload, expects_response, request_id)
                    .await
            }
            McpSession::Http(session) => {
                send_http_message(
                    &self.server,
                    session,
                    &self.protocol_version,
                    self.session_id.as_deref(),
                    payload,
                    expects_response,
                    request_id,
                )
                .await
            }
            McpSession::Sse(session) => {
                send_sse_message(
                    &self.server,
                    session,
                    &self.protocol_version,
                    self.session_id.as_deref(),
                    payload,
                    expects_response,
                    request_id,
                )
                .await
            }
        }
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.request_id;
        self.request_id += 1;
        id
    }
}

fn timeout_ms(server: &MCPServer) -> u64 {
    server.timeout.unwrap_or(DEFAULT_TIMEOUT_MS as u32).max(1) as u64
}

fn normalize_tool_arguments(arguments: Value) -> Value {
    match arguments {
        Value::Object(_) => arguments,
        Value::Null => json!({}),
        other => json!({ "input": other }),
    }
}

fn build_reqwest_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|error| error.to_string())
}

fn build_server_headers(server: &MCPServer) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    if let Some(raw_headers) = &server.headers {
        for (key, value) in raw_headers {
            if key.trim().is_empty() || value.trim().is_empty() {
                continue;
            }
            let header_name =
                HeaderName::from_bytes(key.trim().as_bytes()).map_err(|error| error.to_string())?;
            let header_value =
                HeaderValue::from_str(value.trim()).map_err(|error| error.to_string())?;
            headers.insert(header_name, header_value);
        }
    }
    Ok(headers)
}

async fn connect_stdio(server: &MCPServer) -> Result<StdioSession, String> {
    let command = server
        .command
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("MCP server '{}' is missing a stdio command", server.name))?;

    let mut child = Command::new(command);
    if let Some(args) = &server.args {
        child.args(args);
    }
    if let Some(env) = &server.env {
        child.envs(env.iter().map(|(key, value)| (key, value)));
    }
    if let Some(cwd) = &server.cwd {
        if !cwd.trim().is_empty() {
            child.current_dir(cwd);
        }
    }
    child.stdin(Stdio::piped());
    child.stdout(Stdio::piped());
    child.stderr(Stdio::null());

    let mut child = child.spawn().map_err(|error| error.to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("Failed to open stdin for MCP server '{}'", server.name))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("Failed to open stdout for MCP server '{}'", server.name))?;

    Ok(StdioSession {
        child,
        stdin,
        stdout: BufReader::new(stdout).lines(),
    })
}

fn connect_http(server: &MCPServer) -> Result<HttpSession, String> {
    let endpoint = server
        .http_url
        .clone()
        .or_else(|| server.url.clone())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("MCP server '{}' is missing an HTTP endpoint", server.name))?;

    Ok(HttpSession {
        client: build_reqwest_client()?,
        endpoint,
    })
}

async fn connect_sse(server: &MCPServer, timeout_ms: u64) -> Result<SseSession, String> {
    let endpoint = server
        .http_url
        .clone()
        .or_else(|| server.url.clone())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("MCP server '{}' is missing an SSE endpoint", server.name))?;

    let client = build_reqwest_client()?;
    let mut request = client
        .get(endpoint.clone())
        .header(ACCEPT, "text/event-stream");
    let header_map = build_server_headers(server)?;
    if !header_map.is_empty() {
        request = request.headers(header_map);
    }
    let response = timeout(Duration::from_millis(timeout_ms), request.send())
        .await
        .map_err(|_| format!("Timed out connecting to MCP SSE server '{}'", server.name))?
        .map_err(|error| error.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!(
                "Failed to connect to MCP SSE server '{}': HTTP {}",
                server.name, status
            )
        } else {
            body
        });
    }

    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (endpoint_tx, endpoint_rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut response = response;
        let mut endpoint_tx = Some(endpoint_tx);
        let mut sent_endpoint = false;
        let mut buffer = String::new();
        while let Ok(Some(chunk)) = response.chunk().await {
            let normalized = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
            buffer.push_str(&normalized);
            while let Some(index) = buffer.find("\n\n") {
                let block = buffer[..index].to_string();
                buffer = buffer[index + 2..].to_string();
                if !dispatch_sse_block(&block, &event_tx, &mut endpoint_tx, &mut sent_endpoint) {
                    return;
                }
            }
        }
        if !buffer.trim().is_empty() {
            let _ = dispatch_sse_block(&buffer, &event_tx, &mut endpoint_tx, &mut sent_endpoint);
        }
    });

    let post_endpoint = timeout(Duration::from_millis(timeout_ms), endpoint_rx)
        .await
        .map_err(|_| {
            format!(
                "Timed out waiting for MCP SSE endpoint from '{}'",
                server.name
            )
        })?
        .map_err(|_| format!("MCP SSE endpoint closed early for '{}'", server.name))?;
    let post_endpoint = resolve_post_endpoint(&endpoint, &post_endpoint)?;

    Ok(SseSession {
        client,
        endpoint,
        post_endpoint,
        receiver: event_rx,
    })
}

fn dispatch_sse_block(
    block: &str,
    event_tx: &mpsc::UnboundedSender<Value>,
    endpoint_tx: &mut Option<oneshot::Sender<String>>,
    sent_endpoint: &mut bool,
) -> bool {
    let mut event_name: Option<String> = None;
    let mut data_lines = Vec::new();

    for raw_line in block.lines() {
        let line = raw_line.trim_end();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return true;
    }

    let data = data_lines.join("\n");
    if event_name.as_deref() == Some("endpoint") && !*sent_endpoint {
        if let Some(sender) = endpoint_tx.take() {
            let _ = sender.send(data.clone());
        }
        *sent_endpoint = true;
        return true;
    }

    if let Ok(value) = serde_json::from_str::<Value>(&data) {
        let _ = event_tx.send(value);
    }
    true
}

fn resolve_post_endpoint(base_endpoint: &str, candidate: &str) -> Result<String, String> {
    let base = Url::parse(base_endpoint).map_err(|error| error.to_string())?;
    if let Ok(url) = Url::parse(candidate) {
        return Ok(url.to_string());
    }
    base.join(candidate)
        .map(|url| url.to_string())
        .map_err(|error| error.to_string())
}

async fn send_stdio_message(
    server: &MCPServer,
    session: &mut StdioSession,
    payload: Value,
    expects_response: bool,
    request_id: u64,
) -> Result<RpcResponseEnvelope, String> {
    let timeout_ms = timeout_ms(server);
    let serialized = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
    timeout(
        Duration::from_millis(timeout_ms),
        session.stdin.write_all(serialized.as_bytes()),
    )
    .await
    .map_err(|_| format!("Timed out writing to MCP server '{}'", server.name))?
    .map_err(|error| error.to_string())?;
    timeout(
        Duration::from_millis(timeout_ms),
        session.stdin.write_all(b"\n"),
    )
    .await
    .map_err(|_| format!("Timed out writing newline to MCP server '{}'", server.name))?
    .map_err(|error| error.to_string())?;
    timeout(Duration::from_millis(timeout_ms), session.stdin.flush())
        .await
        .map_err(|_| format!("Timed out flushing MCP server '{}'", server.name))?
        .map_err(|error| error.to_string())?;

    if !expects_response {
        return Ok(RpcResponseEnvelope {
            payload: Value::Null,
            session_id: None,
            protocol_version: None,
        });
    }

    loop {
        let line = timeout(
            Duration::from_millis(timeout_ms),
            session.stdout.next_line(),
        )
        .await
        .map_err(|_| format!("Timed out waiting for MCP response from '{}'", server.name))?
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("MCP stdio server '{}' closed the stream", server.name))?;

        if line.trim().is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(&line)
            .map_err(|error| format!("Invalid MCP stdio JSON from '{}': {}", server.name, error))?;
        if matches_json_rpc_id(&value, request_id) {
            return Ok(RpcResponseEnvelope {
                payload: value,
                session_id: None,
                protocol_version: None,
            });
        }
    }
}

async fn send_http_message(
    server: &MCPServer,
    session: &mut HttpSession,
    protocol_version: &str,
    session_id: Option<&str>,
    payload: Value,
    expects_response: bool,
    request_id: u64,
) -> Result<RpcResponseEnvelope, String> {
    let response = send_http_post(
        server,
        &session.client,
        &session.endpoint,
        protocol_version,
        session_id,
        payload,
    )
    .await?;

    parse_http_response(server, response, expects_response, request_id).await
}

async fn send_sse_message(
    server: &MCPServer,
    session: &mut SseSession,
    protocol_version: &str,
    session_id: Option<&str>,
    payload: Value,
    expects_response: bool,
    request_id: u64,
) -> Result<RpcResponseEnvelope, String> {
    let response = send_http_post(
        server,
        &session.client,
        &session.post_endpoint,
        protocol_version,
        session_id,
        payload,
    )
    .await?;

    let status = response.status();
    let session_id = response
        .headers()
        .get("MCP-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!(
                "MCP SSE request to '{}' failed with HTTP {}",
                session.endpoint, status
            )
        } else {
            body
        });
    }

    if !expects_response {
        return Ok(RpcResponseEnvelope {
            payload: Value::Null,
            session_id,
            protocol_version: None,
        });
    }

    let timeout_ms = timeout_ms(server);
    loop {
        let next = timeout(Duration::from_millis(timeout_ms), session.receiver.recv())
            .await
            .map_err(|_| {
                format!(
                    "Timed out waiting for MCP SSE response from '{}'",
                    server.name
                )
            })?;
        let Some(value) = next else {
            return Err(format!(
                "MCP SSE stream '{}' closed unexpectedly",
                server.name
            ));
        };
        if matches_json_rpc_id(&value, request_id) {
            return Ok(RpcResponseEnvelope {
                payload: value,
                session_id,
                protocol_version: None,
            });
        }
    }
}

async fn send_http_post(
    server: &MCPServer,
    client: &reqwest::Client,
    endpoint: &str,
    protocol_version: &str,
    session_id: Option<&str>,
    payload: Value,
) -> Result<reqwest::Response, String> {
    let timeout_ms = timeout_ms(server);
    let mut request = client
        .post(endpoint)
        .header(ACCEPT, "application/json, text/event-stream")
        .header(CONTENT_TYPE, "application/json")
        .header("MCP-Protocol-Version", protocol_version)
        .json(&payload);

    if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
        request = request.header("MCP-Session-Id", session_id);
    }

    let header_map = build_server_headers(server)?;
    if !header_map.is_empty() {
        request = request.headers(header_map);
    }

    timeout(Duration::from_millis(timeout_ms), request.send())
        .await
        .map_err(|_| {
            format!(
                "Timed out waiting for MCP HTTP response from '{}'",
                server.name
            )
        })?
        .map_err(|error| error.to_string())
}

async fn parse_http_response(
    server: &MCPServer,
    response: reqwest::Response,
    expects_response: bool,
    request_id: u64,
) -> Result<RpcResponseEnvelope, String> {
    let status = response.status();
    let session_id = response
        .headers()
        .get("MCP-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let protocol_version = response
        .headers()
        .get("MCP-Protocol-Version")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!(
                "MCP HTTP request to '{}' failed with HTTP {}",
                server.name, status
            )
        } else {
            body
        });
    }

    if !expects_response || status.as_u16() == 202 {
        return Ok(RpcResponseEnvelope {
            payload: Value::Null,
            session_id,
            protocol_version,
        });
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if content_type.contains("application/json") {
        let payload = response
            .json::<Value>()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(RpcResponseEnvelope {
            payload,
            session_id,
            protocol_version,
        });
    }

    if content_type.contains("text/event-stream") {
        let payload = read_sse_json_response(server, response, request_id).await?;
        return Ok(RpcResponseEnvelope {
            payload,
            session_id,
            protocol_version,
        });
    }

    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "Unsupported MCP HTTP content type '{}' from '{}': {}",
        content_type, server.name, body
    ))
}

async fn read_sse_json_response(
    server: &MCPServer,
    mut response: reqwest::Response,
    request_id: u64,
) -> Result<Value, String> {
    let timeout_ms = timeout_ms(server);
    let future = async {
        let mut buffer = String::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            let normalized = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
            buffer.push_str(&normalized);
            while let Some(index) = buffer.find("\n\n") {
                let block = buffer[..index].to_string();
                buffer = buffer[index + 2..].to_string();
                if let Some(value) = parse_sse_json_block(&block, request_id)? {
                    return Ok(value);
                }
            }
        }
        if !buffer.trim().is_empty() {
            if let Some(value) = parse_sse_json_block(&buffer, request_id)? {
                return Ok(value);
            }
        }
        Err(format!(
            "MCP SSE response from '{}' ended before returning request id {}",
            server.name, request_id
        ))
    };

    timeout(Duration::from_millis(timeout_ms), future)
        .await
        .map_err(|_| format!("Timed out reading MCP SSE response from '{}'", server.name))?
}

fn parse_sse_json_block(block: &str, request_id: u64) -> Result<Option<Value>, String> {
    let mut data_lines = Vec::new();
    for raw_line in block.lines() {
        let line = raw_line.trim_end();
        if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return Ok(None);
    }

    let data = data_lines.join("\n");
    let value: Value = serde_json::from_str(&data).map_err(|error| error.to_string())?;
    if matches_json_rpc_id(&value, request_id) {
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

fn matches_json_rpc_id(payload: &Value, request_id: u64) -> bool {
    payload.get("id").and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    }) == Some(request_id)
}

fn format_json_rpc_error(error: &Value) -> String {
    let code = error.get("code").and_then(|value| value.as_i64());
    let message = error
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or("Unknown MCP error");
    let details = error.get("data").cloned().unwrap_or(Value::Null);
    if details.is_null() {
        match code {
            Some(code) => format!("{} ({})", message, code),
            None => message.to_string(),
        }
    } else {
        match code {
            Some(code) => format!("{} ({}): {}", message, code, details),
            None => format!("{}: {}", message, details),
        }
    }
}

fn render_tool_result_text(result: &Value) -> String {
    if let Some(content) = result.get("content").and_then(|value| value.as_array()) {
        let text_parts = content
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("content").and_then(|value| value.as_str()))
            })
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();

        if !text_parts.is_empty() {
            return text_parts.join("\n\n");
        }
    }

    if let Some(structured) = result.get("structuredContent") {
        if structured.is_string() {
            return structured.as_str().unwrap_or_default().to_string();
        }
        return serde_json::to_string_pretty(structured).unwrap_or_else(|_| structured.to_string());
    }

    result.to_string()
}

pub fn compose_mcp_tool_name(config_key: &str, tool_name: &str) -> String {
    format!(
        "mcp__{}__{}",
        sanitize_tool_component(config_key),
        sanitize_tool_component(tool_name)
    )
}

pub fn sanitize_tool_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('_').to_string()
}

pub fn server_signature(server: &MCPServer) -> HashMap<&'static str, String> {
    let mut signature = HashMap::new();
    signature.insert(
        "command",
        server
            .command
            .clone()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
    );
    signature.insert(
        "url",
        server
            .http_url
            .clone()
            .or_else(|| server.url.clone())
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
    );
    signature.insert(
        "args",
        server
            .args
            .clone()
            .unwrap_or_default()
            .join(" ")
            .trim()
            .to_lowercase(),
    );
    signature
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc as std_mpsc;
    use std::thread;

    fn stdio_test_server() -> MCPServer {
        MCPServer {
            id: "mcp-stdio-test".to_string(),
            name: "stdio-test".to_string(),
            config_key: Some("stdio_test".to_string()),
            description: None,
            transport: MCPServerTransport::Stdio,
            command: Some("/bin/sh".to_string()),
            args: Some(vec![
                "-c".to_string(),
                r#"while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","serverInfo":{"name":"mock","version":"1.0.0"},"capabilities":{}}}'
      ;;
    *'"method":"notifications/initialized"'*)
      ;;
    *'"method":"tools/list"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search_docs","description":"Search docs","inputSchema":{"type":"object","properties":{"q":{"type":"string"}}}}]}}'
      ;;
    *'"method":"tools/call"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"tool ok"}],"structuredContent":{"results":[{"title":"Example","url":"https://example.com","text":"Snippet"}]}}}'
      ;;
  esac
done"#.to_string(),
            ]),
            cwd: None,
            url: None,
            http_url: None,
            env: None,
            headers: None,
            timeout: Some(10_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn read_http_request(mut stream: &std::net::TcpStream) -> (String, String, String) {
        let mut buffer = [0u8; 8192];
        let size = stream.read(&mut buffer).expect("read request");
        let raw = String::from_utf8_lossy(&buffer[..size]).to_string();
        let mut lines = raw.lines();
        let request_line = lines.next().unwrap_or_default().to_string();
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
        (request_line, raw, body)
    }

    #[tokio::test]
    async fn stdio_transport_supports_initialize_list_and_call() {
        let server = stdio_test_server();
        let mut client = McpClient::connect(&server).await.expect("connect");
        let tools = client.list_tools().await.expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "search_docs");

        let output = client
            .call_tool("search_docs", json!({ "q": "mcp" }))
            .await
            .expect("call");
        assert_eq!(output.text, "tool ok");
        assert!(output.structured_content.is_some());
        client.close().await;
    }

    #[tokio::test]
    async fn http_transport_supports_json_responses() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (tx, rx) = std_mpsc::channel();

        thread::spawn(move || {
            for _ in 0..4 {
                let (stream, _) = listener.accept().expect("accept");
                let (request_line, raw, body) = read_http_request(&stream);
                tx.send((request_line.clone(), raw.clone(), body.clone()))
                    .ok();
                let method = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("method")
                            .and_then(|item| item.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                let id = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|value| value.get("id").cloned())
                    .unwrap_or(Value::Null);
                let response = match method.as_str() {
                    "initialize" => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMCP-Session-Id: session-1\r\n\r\n{}",
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "protocolVersion": "2025-03-26",
                                "serverInfo": { "name": "mock", "version": "1.0.0" },
                                "capabilities": {}
                            }
                        })
                    ),
                    "notifications/initialized" => "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".to_string(),
                    "tools/list" => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "tools": [
                                    {
                                        "name": "web_search_exa",
                                        "description": "Search",
                                        "inputSchema": { "type": "object", "properties": {} }
                                    }
                                ]
                            }
                        })
                    ),
                    _ => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [
                                    { "type": "text", "text": "http ok" }
                                ]
                            }
                        })
                    ),
                };
                let mut stream = stream;
                stream.write_all(response.as_bytes()).expect("write");
            }
        });

        let server = MCPServer {
            id: "mcp-http-test".to_string(),
            name: "http-test".to_string(),
            config_key: Some("http_test".to_string()),
            description: None,
            transport: MCPServerTransport::Http,
            command: None,
            args: None,
            cwd: None,
            url: Some(format!("http://{}/mcp", addr)),
            http_url: Some(format!("http://{}/mcp", addr)),
            env: None,
            headers: None,
            timeout: Some(10_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let mut client = McpClient::connect(&server).await.expect("connect");
        let tools = client.list_tools().await.expect("tools");
        assert_eq!(tools[0].name, "web_search_exa");
        let output = client
            .call_tool("web_search_exa", json!({}))
            .await
            .expect("call");
        assert_eq!(output.text, "http ok");

        let initialize_request = rx.recv().expect("initialize request");
        assert!(initialize_request.0.starts_with("POST /mcp"));
    }

    #[tokio::test]
    async fn legacy_sse_transport_supports_post_then_stream_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (notify_tx, notify_rx) = std_mpsc::channel::<String>();

        thread::spawn(move || {
            let (mut sse_stream, _) = listener.accept().expect("accept sse");
            let (request_line, _, _) = read_http_request(&sse_stream);
            assert!(request_line.starts_with("GET /sse"));
            let sse_headers =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\r\n";
            sse_stream
                .write_all(sse_headers.as_bytes())
                .expect("write headers");
            sse_stream
                .write_all(b"event: endpoint\ndata: /messages\n\n")
                .expect("write endpoint");

            for _ in 0..4 {
                let (mut post_stream, _) = listener.accept().expect("accept post");
                let (_, _, body) = read_http_request(&post_stream);
                let payload: Value = serde_json::from_str(&body).expect("json");
                let method = payload
                    .get("method")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let id = payload.get("id").cloned().unwrap_or(Value::Null);
                notify_tx.send(method.to_string()).ok();
                let ack = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n";
                post_stream.write_all(ack.as_bytes()).expect("ack");

                if method == "notifications/initialized" {
                    continue;
                }

                let response_json = match method {
                    "initialize" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2025-03-26",
                            "serverInfo": { "name": "mock", "version": "1.0.0" },
                            "capabilities": {}
                        }
                    }),
                    "tools/list" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": [
                                {
                                    "name": "legacy_tool",
                                    "description": "Legacy",
                                    "inputSchema": { "type": "object", "properties": {} }
                                }
                            ]
                        }
                    }),
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                { "type": "text", "text": "legacy ok" }
                            ]
                        }
                    }),
                };
                let event = format!("data: {}\n\n", response_json);
                sse_stream.write_all(event.as_bytes()).expect("write sse");
            }
        });

        let server = MCPServer {
            id: "mcp-sse-test".to_string(),
            name: "sse-test".to_string(),
            config_key: Some("sse_test".to_string()),
            description: None,
            transport: MCPServerTransport::Sse,
            command: None,
            args: None,
            cwd: None,
            url: Some(format!("http://{}/sse", addr)),
            http_url: Some(format!("http://{}/sse", addr)),
            env: None,
            headers: None,
            timeout: Some(10_000),
            trust: Some(false),
            linked_provider_ids: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let mut client = McpClient::connect(&server).await.expect("connect");
        let tools = client.list_tools().await.expect("tools");
        assert_eq!(tools[0].name, "legacy_tool");
        let output = client
            .call_tool("legacy_tool", json!({}))
            .await
            .expect("call");
        assert_eq!(output.text, "legacy ok");

        let methods = vec![
            notify_rx.recv().expect("initialize"),
            notify_rx.recv().expect("initialized notification"),
            notify_rx.recv().expect("tools list"),
            notify_rx.recv().expect("tools call"),
        ];
        assert_eq!(
            methods,
            vec![
                "initialize".to_string(),
                "notifications/initialized".to_string(),
                "tools/list".to_string(),
                "tools/call".to_string()
            ]
        );
    }

    #[test]
    fn compose_mcp_tool_name_sanitizes_components() {
        assert_eq!(
            compose_mcp_tool_name("exa.ai", "web-search"),
            "mcp__exa_ai__web_search"
        );
    }
}
