use super::anthropic::{convert_request, is_conversion_path, is_messages_path, DEFAULT_ANTHROPIC_VERSION};
use super::openai_conversion;
use super::{GatewayUpstreamProvider, TransportStrategy, UpstreamProtocol};
use bytes::Bytes;
use futures_util::Stream;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;

/// Parsed upstream response for a non-streaming attempt.
pub(in crate::ai_gateway) struct UpstreamJsonResponse {
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) body: Vec<u8>,
    pub(in crate::ai_gateway) parsed: bool,
    pub(in crate::ai_gateway) headers: HeaderMap,
}

/// The shared gateway client cache key: caller timeouts plus the global proxy
/// generation the client was prepared for.
#[derive(Clone, PartialEq, Eq)]
struct ClientConfigKey {
    connect_timeout_secs: u32,
    read_timeout_secs: u32,
    proxy_generation: u64,
}

/// Return one shared client for the current global proxy generation. A request
/// admitted under a new generation never reuses the previous transport; a
/// preparation failure is returned instead of falling back to a direct client.
fn get_shared_client(settings: &super::GatewaySettings) -> Result<Arc<Client>, String> {
    static CACHED_CLIENT: OnceLock<RwLock<Option<(ClientConfigKey, Arc<Client>)>>> = OnceLock::new();
    let cache = CACHED_CLIENT.get_or_init(|| RwLock::new(None));
    let snapshot = super::current_proxy_snapshot();
    let current_key = ClientConfigKey {
        connect_timeout_secs: settings.connect_timeout_secs,
        read_timeout_secs: settings.read_timeout_secs,
        proxy_generation: snapshot.generation,
    };

    {
        let read = cache
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((key, client)) = read.as_ref() {
            if *key == current_key {
                return Ok(Arc::clone(client));
            }
        }
    }

    let client = Arc::new(build_client_from_key(&current_key, &snapshot)?);
    let mut write = cache
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *write = Some((current_key, Arc::clone(&client)));
    Ok(client)
}

fn build_client_from_key(
    key: &ClientConfigKey,
    snapshot: &crate::proxy::ProxySnapshot,
) -> Result<Client, String> {
    let builder = Client::builder()
        .connect_timeout(Duration::from_secs(key.connect_timeout_secs as u64))
        .read_timeout(Duration::from_secs(key.read_timeout_secs as u64));
    crate::proxy::apply_proxy_policy(builder, snapshot)?
        .build()
        .map_err(|_| "failed to prepare the gateway transport".to_string())
}

/// The sidecar is a loopback process, so its client must never inherit the
/// user's upstream proxy. SSE responses may stay open far longer than the
/// gateway's ordinary read timeout.
fn sidecar_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(3600))
            .build()
            .unwrap_or_else(|_| Client::new())
    })
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
            | "x-onespace-sidecar-token"
            | "x-onespace-upstream-base"
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

async fn build_request(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    stream: bool,
    client_headers: &HashMap<String, String>,
    upstream_protocol: UpstreamProtocol,
    settings: &super::GatewaySettings,
) -> Result<(reqwest::RequestBuilder, Option<super::zen_sidecar::StartedSidecar>), String> {
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
    // Sidecar providers always go through the local relay and never fall back
    // to the direct upstream URL. Direct providers keep the shared client and
    // the existing URL joining.
    let (client, url, sidecar_identity, lease) = match provider.transport {
        TransportStrategy::OpencodeZenSidecar => {
            let started = super::zen_sidecar::ensure_started(settings).await?;
            let endpoint = started.endpoint_base.trim_end_matches('/');
            (
                sidecar_client().clone(),
                format!("{endpoint}{upstream_path}"),
                Some((started.token.clone(), provider.base_url.trim().to_string())),
                Some(started),
            )
        }
        TransportStrategy::Direct => (
            get_shared_client(settings)?.as_ref().clone(),
            join_url(&provider.base_url, upstream_path),
            None,
            None,
        ),
    };
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
    if let Some((token, upstream_base)) = sidecar_identity {
        // Added after client-header forwarding so a client value can never
        // override the gateway's sidecar-only headers.
        request = request
            .header("x-onespace-sidecar-token", token)
            .header("x-onespace-upstream-base", upstream_base);
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
    Ok((request, lease))
}

/// A streaming upstream response together with the sidecar lease that keeps a
/// retiring relay process alive until the response body finishes.
pub(in crate::ai_gateway) struct UpstreamStreamResponse {
    response: reqwest::Response,
    lease: Option<super::zen_sidecar::StartedSidecar>,
}

impl UpstreamStreamResponse {
    pub(in crate::ai_gateway) fn status(&self) -> reqwest::StatusCode {
        self.response.status()
    }

    pub(in crate::ai_gateway) fn headers(&self) -> &HeaderMap {
        self.response.headers()
    }

    /// Read the complete body while retaining the response lease.
    pub(in crate::ai_gateway) async fn bytes(self) -> Result<Bytes, reqwest::Error> {
        let Self { response, lease } = self;
        let result = response.bytes().await;
        drop(lease);
        result
    }

    /// Convert the response into a byte stream that owns the lease for as long
    /// as the stream lives.
    pub(in crate::ai_gateway) fn bytes_stream(self) -> UpstreamByteStream {
        let Self { response, lease } = self;
        UpstreamByteStream {
            inner: Box::pin(response.bytes_stream()),
            _lease: lease,
        }
    }
}

/// A response byte stream that keeps the sidecar lease alive until the stream is
/// dropped.
pub(in crate::ai_gateway) struct UpstreamByteStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    _lease: Option<super::zen_sidecar::StartedSidecar>,
}

impl Stream for UpstreamByteStream {
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.as_mut().poll_next(cx)
    }
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
    let (request, lease) = build_request(
        provider,
        path,
        body,
        model,
        false,
        client_headers,
        upstream_protocol,
        settings,
    )
    .await?;
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
    drop(lease);
    let parsed = serde_json::from_slice::<serde_json::Value>(&bytes).is_ok();
    Ok(UpstreamJsonResponse {
        status,
        body: bytes,
        parsed,
        headers,
    })
}

/// Open an upstream streaming response. Callers stream bytes and enforce the
/// first-byte switching boundary themselves. The returned value owns the relay
/// lease until the response body is consumed.
pub(in crate::ai_gateway) async fn open_streaming_response_with_protocol(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
    upstream_protocol: UpstreamProtocol,
    settings: &super::GatewaySettings,
) -> Result<UpstreamStreamResponse, String> {
    let (request, lease) = build_request(
        provider,
        path,
        body,
        model,
        true,
        client_headers,
        upstream_protocol,
        settings,
    )
    .await?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    Ok(UpstreamStreamResponse { response, lease })
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


#[cfg(test)]
mod proxy_switch_named {
    use crate::proxy::test_support::{DirectUpstream, HttpProxyRecorder, SERIAL_GUARD};
    use std::ffi::OsString;

    const PROXY_ENV_KEYS: [&str; 6] = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ];
    const NO_PROXY_ENV_KEYS: [&str; 2] = ["NO_PROXY", "no_proxy"];

    struct EnvRestore {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvRestore {
        fn apply(proxy_url: &str) -> Self {
            let mut saved = Vec::new();
            for key in PROXY_ENV_KEYS {
                saved.push((key, std::env::var_os(key)));
                std::env::set_var(key, proxy_url);
            }
            for key in NO_PROXY_ENV_KEYS {
                saved.push((key, std::env::var_os(key)));
                std::env::remove_var(key);
            }
            Self { saved }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    /// REQ-002 / AC-002: the loopback relay client is explicitly direct, so
    /// inherited proxy environment variables never reroute local sidecar calls.
    #[tokio::test(flavor = "current_thread")]
    async fn zen_sidecar_local_client_ignores_inherited_proxy() {
        let _guard = SERIAL_GUARD.lock().unwrap_or_else(|error| error.into_inner());
        let _home = crate::lock_test_home_env();
        let inherited_proxy = HttpProxyRecorder::new("inherited").await;
        let upstream = DirectUpstream::new("loopback-ok").await;
        let _env = EnvRestore::apply(&inherited_proxy.url());

        let response = super::sidecar_client()
            .get(upstream.url())
            .send()
            .await
            .expect("the loopback sidecar request must complete");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response.text().await.expect("loopback body"),
            "loopback-ok"
        );

        assert_eq!(
            inherited_proxy.hits(),
            0,
            "the sidecar local client must never send loopback traffic through an inherited proxy"
        );
        assert!(
            upstream.hits() >= 1,
            "the request must reach the loopback endpoint directly"
        );
    }
}
