use super::FusionUpstreamProvider;
use reqwest::Client;
use std::sync::OnceLock;
use std::time::Duration;

/// Parsed upstream response for a non-streaming attempt.
pub(in crate::api_fusion) struct UpstreamJsonResponse {
    pub(in crate::api_fusion) status: u16,
    pub(in crate::api_fusion) body: Vec<u8>,
    pub(in crate::api_fusion) parsed: bool,
}

/// Bound only the connect phase (the repo's `proxy.rs` uses 10s) so a
/// blackholed/unroutable upstream fails fast instead of hanging.
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Idle read timeout: applied to each read and reset after every successful
/// read, so an upstream that connects but never answers is a retryable failure
/// while an active long response or stream is never truncated by a fixed total
/// deadline.
const UPSTREAM_READ_TIMEOUT: Duration = Duration::from_secs(10);

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
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
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
) -> Result<reqwest::RequestBuilder, String> {
    let url = join_url(&provider.base_url, path);
    let rewritten = rewrite_body_model(body, model)?;
    let mut request = shared_client()
        .post(url)
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
) -> Result<UpstreamJsonResponse, String> {
    let request = build_request(provider, path, body, model, false)?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
    let parsed = serde_json::from_slice::<serde_json::Value>(&bytes).is_ok();
    Ok(UpstreamJsonResponse {
        status,
        body: bytes,
        parsed,
    })
}

/// Open an upstream streaming response. Callers stream bytes and enforce the
/// first-byte switching boundary themselves.
pub(in crate::api_fusion) async fn open_streaming_response(
    provider: &FusionUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
) -> Result<reqwest::Response, String> {
    let request = build_request(provider, path, body, model, true)?;
    request.send().await.map_err(|e| e.to_string())
}
