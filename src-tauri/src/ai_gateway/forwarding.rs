use super::anthropic::{convert_request, is_conversion_path, is_messages_path, DEFAULT_ANTHROPIC_VERSION};
use super::openai_conversion;
use super::{GatewayUpstreamProvider, UpstreamProtocol};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

/// Parsed upstream response for a non-streaming attempt.
pub(in crate::ai_gateway) struct UpstreamJsonResponse {
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) body: Vec<u8>,
    pub(in crate::ai_gateway) parsed: bool,
    pub(in crate::ai_gateway) headers: HeaderMap,
}

#[derive(Clone, PartialEq, Eq)]
struct ClientConfigKey {
    connect_timeout_secs: u32,
    read_timeout_secs: u32,
    proxy_url: Option<String>,
}

fn get_shared_client(settings: &super::GatewaySettings) -> Arc<Client> {
    static CACHED_CLIENT: OnceLock<RwLock<(ClientConfigKey, Arc<Client>)>> = OnceLock::new();
    let cache = CACHED_CLIENT.get_or_init(|| {
        let default_settings = super::GatewaySettings::default();
        let key = ClientConfigKey {
            connect_timeout_secs: default_settings.connect_timeout_secs,
            read_timeout_secs: default_settings.read_timeout_secs,
            proxy_url: default_settings.proxy_url.clone(),
        };
        let client = build_client_from_key(&key);
        RwLock::new((key, Arc::new(client)))
    });

    let current_key = ClientConfigKey {
        connect_timeout_secs: settings.connect_timeout_secs,
        read_timeout_secs: settings.read_timeout_secs,
        proxy_url: settings.proxy_url.clone(),
    };

    if let Ok(read) = cache.read() {
        if read.0 == current_key {
            return Arc::clone(&read.1);
        }
    }

    if let Ok(mut write) = cache.write() {
        if write.0 == current_key {
            return Arc::clone(&write.1);
        }
        let new_client = Arc::new(build_client_from_key(&current_key));
        *write = (current_key, Arc::clone(&new_client));
        return new_client;
    }

    Arc::new(build_client_from_key(&current_key))
}

fn build_client_from_key(key: &ClientConfigKey) -> Client {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(key.connect_timeout_secs as u64))
        .read_timeout(Duration::from_secs(key.read_timeout_secs as u64));

    if let Some(ref proxy_url) = key.proxy_url {
        let trimmed = proxy_url.trim();
        if !trimmed.is_empty() {
            if let Ok(proxy) = reqwest::Proxy::all(trimmed) {
                builder = builder.proxy(proxy);
            }
        }
    }

    builder.build().unwrap_or_else(|_| Client::new())
}

#[allow(dead_code)]
fn shared_client() -> Arc<Client> {
    get_shared_client(&super::GatewaySettings::default())
}

pub(in crate::ai_gateway) fn join_url(base: &str, path: &str) -> String {
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
pub(in crate::ai_gateway) fn rewrite_body_model(
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
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    stream: bool,
    client_headers: &HashMap<String, String>,
    upstream_protocol: UpstreamProtocol,
    settings: &super::GatewaySettings,
) -> Result<reqwest::RequestBuilder, String> {
    let converted = upstream_protocol == UpstreamProtocol::AnthropicMessages
        && is_conversion_path(path);
    // A prepared model-pool candidate may declare the opposite OpenAI endpoint
    // from the client; that body must be translated to the candidate's protocol.
    let openai_converted = provider.pool_candidate.is_some()
        && openai_conversion::is_openai_path(path)
        && upstream_protocol != UpstreamProtocol::AnthropicMessages
        && openai_conversion::client_protocol(path) != Some(upstream_protocol);
    let upstream_path = if converted {
        "/v1/messages"
    } else if openai_converted {
        openai_conversion::endpoint_path(upstream_protocol)
    } else {
        path
    };
    let url = join_url(&provider.base_url, upstream_path);
    // A pool candidate removes its recognized reasoning entry controls before
    // the adapter sees them, so the ordinary Anthropic adapter keeps rejecting
    // the controls on every ordinary route.
    let native = provider
        .pool_candidate
        .as_ref()
        .and_then(|identity| identity.reasoning.as_ref());
    let stripped = match native {
        Some(_) => Some(super::reasoning::strip_pool_entry_controls_bytes(path, body)?),
        None => None,
    };
    let body: &[u8] = stripped.as_deref().unwrap_or(body);
    let rewritten = if converted {
        convert_request(path, body, model)?
    } else if openai_converted {
        openai_conversion::convert_request(path, body, model, upstream_protocol)?
    } else {
        rewrite_body_model(body, model)?
    };
    // Inject exactly this candidate's native control after conversion. `auto`
    // carries no parameters and leaves the body unchanged.
    let rewritten = match native {
        Some(native) => super::reasoning::inject_native_controls(rewritten, native)?,
        None => rewritten,
    };
    let client = get_shared_client(settings);
    let mut request = client.post(url);
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
    let attempt_key = provider.attempt_key.trim();
    if !attempt_key.is_empty() {
        if upstream_protocol == UpstreamProtocol::AnthropicMessages || is_messages_path(upstream_path) {
            request = request.header("x-api-key", attempt_key);
        } else {
            request = request.header("authorization", format!("Bearer {attempt_key}"));
        }
    }
    if upstream_protocol == UpstreamProtocol::AnthropicMessages
        && !client_headers.contains_key("anthropic-version")
    {
        request = request.header("anthropic-version", DEFAULT_ANTHROPIC_VERSION);
    }
    Ok(request)
}

/// Send a non-streaming upstream request and return status plus raw body.
pub(in crate::ai_gateway) async fn forward_non_streaming_with_protocol(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
    upstream_protocol: UpstreamProtocol,
    settings: &super::GatewaySettings,
) -> Result<UpstreamJsonResponse, String> {
    let request = build_request(
        provider,
        path,
        body,
        model,
        false,
        client_headers,
        upstream_protocol,
        settings,
    )?;
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
pub(in crate::ai_gateway) async fn open_streaming_response_with_protocol(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
    upstream_protocol: UpstreamProtocol,
    settings: &super::GatewaySettings,
) -> Result<reqwest::Response, String> {
    let request = build_request(
        provider,
        path,
        body,
        model,
        true,
        client_headers,
        upstream_protocol,
        settings,
    )?;
    request.send().await.map_err(|e| e.to_string())
}

#[cfg(test)]
pub(in crate::ai_gateway) async fn forward_non_streaming(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
) -> Result<UpstreamJsonResponse, String> {
    let default_settings = super::GatewaySettings::default();
    forward_non_streaming_with_protocol(
        provider,
        path,
        body,
        model,
        client_headers,
        if is_messages_path(path) {
            UpstreamProtocol::AnthropicMessages
        } else if path.split('?').next().unwrap_or(path) == "/v1/responses" {
            UpstreamProtocol::Responses
        } else {
            UpstreamProtocol::ChatCompletions
        },
        &default_settings,
    )
    .await
}
