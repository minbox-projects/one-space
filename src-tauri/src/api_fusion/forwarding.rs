use super::FusionUpstreamProvider;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

/// Parsed upstream response for a non-streaming attempt.
pub(in crate::api_fusion) struct UpstreamJsonResponse {
    pub(in crate::api_fusion) status: u16,
    pub(in crate::api_fusion) body: Vec<u8>,
    pub(in crate::api_fusion) parsed: bool,
    pub(in crate::api_fusion) headers: HeaderMap,
}

/// Bound only the connect phase (the repo's `proxy.rs` uses 10s) so a
/// blackholed/unroutable upstream fails fast instead of hanging.
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Idle read timeout (waiting for the first byte, then per read): applied to
/// each read and reset after every successful read. Measured slow reasoning
/// models take 2.7s-10.8s to emit their first byte, so the budget must
/// accommodate that long and jittery first-byte latency; 10s classified those
/// live upstreams as failures. The cost, accepted deliberately: an upstream
/// that connects but never sends anything is only judged a retryable failure
/// after 60s, so switching to the next candidate can take that long. Because
/// the budget resets on every successful read, an active long response or
/// stream is still never truncated by a fixed total deadline.
const UPSTREAM_READ_TIMEOUT: Duration = Duration::from_secs(60);

fn shared_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
            .read_timeout(UPSTREAM_READ_TIMEOUT)
            .build()
            .expect("build API Fusion upstream HTTP client")
    })
}

pub(in crate::api_fusion) fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    // Providers are commonly configured with a base URL that already ends in
    // `/v1` while the canonical inbound path is versioned as well; drop the
    // duplicated version segment so upstream never sees `.../v1/v1/...`.
    let path = if base.ends_with("/v1") {
        path.strip_prefix("v1/").unwrap_or(path)
    } else {
        path
    };
    format!("{base}/{path}")
}

/// Hop-by-hop and credential headers that must never be copied from the local
/// client onto the upstream request. `authorization` / `x-api-key` carry the
/// LOCAL relay key; the provider credential is set explicitly afterwards.
fn is_forwardable_client_header(name: &str) -> bool {
    !matches!(
        name,
        "host"
            | "content-length"
            | "connection"
            | "accept-encoding"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authorization"
            | "authorization"
            | "x-api-key"
            | "content-type"
            | "accept"
    )
}

/// Rewrite only the top-level `model` field, leaving all other fields equivalent.
pub(in crate::api_fusion) fn rewrite_body_model(
    body: &[u8],
    model: &str,
) -> Result<Vec<u8>, String> {
    let mut value: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| e.to_string())?;
    match value.as_object_mut() {
        Some(object) => {
            object.insert(
                "model".to_string(),
                serde_json::Value::String(model.to_string()),
            );
            serde_json::to_vec(&value).map_err(|e| e.to_string())
        }
        None => Err("request body must be a JSON object".to_string()),
    }
}

fn build_request(
    provider: &FusionUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    stream: bool,
    client_headers: &HashMap<String, String>,
) -> Result<reqwest::RequestBuilder, String> {
    let url = join_url(&provider.base_url, path);
    let rewritten = rewrite_body_model(body, model)?;
    let mut request = shared_client().post(url);
    // Forward unknown client headers (e.g. vendor session headers) so upstream
    // requirements like `x-opencode-session` survive the relay. Invalid names
    // or values are skipped rather than failing the whole request.
    for (name, value) in client_headers {
        if !is_forwardable_client_header(name) {
            continue;
        }
        if let (Ok(header_name), Ok(header_value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            request = request.header(header_name, header_value);
        }
    }
    request = request
        .header("content-type", "application/json")
        .header(
            "accept",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .body(rewritten);
    let api_key = provider.api_key.trim();
    if !api_key.is_empty() {
        request = request.header("authorization", format!("Bearer {api_key}"));
    }
    Ok(request)
}

/// Send a non-streaming upstream request and return status plus raw body.
pub(in crate::api_fusion) async fn forward_non_streaming(
    provider: &FusionUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
) -> Result<UpstreamJsonResponse, String> {
    let request = build_request(provider, path, body, model, false, client_headers)?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes.to_vec(),
        // Preserve explicit HTTP failure policies even when the error body
        // cannot be read. Successful responses still require a complete body.
        Err(_) if status >= 400 => Vec::new(),
        Err(error) => return Err(error.to_string()),
    };
    let parsed = serde_json::from_slice::<serde_json::Value>(&bytes).is_ok();
    Ok(UpstreamJsonResponse {
        status,
        body: bytes,
        parsed,
        headers,
    })
}

/// Open an upstream streaming response. Callers stream bytes and enforce the
/// first-byte switching boundary themselves.
pub(in crate::api_fusion) async fn open_streaming_response(
    provider: &FusionUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
) -> Result<reqwest::Response, String> {
    let request = build_request(provider, path, body, model, true, client_headers)?;
    request.send().await.map_err(|e| e.to_string())
}
