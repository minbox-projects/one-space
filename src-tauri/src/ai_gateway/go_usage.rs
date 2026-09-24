use super::{storage, GatewayConfig, GatewayUpstreamProvider};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(in crate::ai_gateway) const GO_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
pub(in crate::ai_gateway) const GO_USAGE_CACHE_TTL_MS: u64 = 300_000;
const GO_USAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

fn deserialize_default_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_status() -> String {
    "ok".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GoUsageWindow {
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default, deserialize_with = "deserialize_default_f64")]
    pub percent: f64,
    #[serde(default)]
    pub resets_at: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GoUsage {
    pub rolling: GoUsageWindow,
    pub weekly: GoUsageWindow,
    pub monthly: GoUsageWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderGoUsage {
    pub usage: GoUsage,
}

#[derive(Deserialize)]
struct GoUsagePayload {
    usage: Option<GoUsage>,
}

pub(in crate::ai_gateway) fn parse_go_usage(body: &str) -> Result<ProviderGoUsage, String> {
    let payload: GoUsagePayload = serde_json::from_str(body)
        .map_err(|error| format!("invalid Go usage response: {error}"))?;
    let usage = payload
        .usage
        .ok_or_else(|| "Go usage response is missing a usage object".to_string())?;
    Ok(ProviderGoUsage { usage })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ai_gateway) struct GoUsageRequest {
    pub(in crate::ai_gateway) url: String,
    pub(in crate::ai_gateway) api_key: String,
}

pub(in crate::ai_gateway) fn resolve_go_usage_request(
    provider: &GatewayUpstreamProvider,
) -> Result<GoUsageRequest, String> {
    if provider.api_key.trim().is_empty() {
        return Err("no API key configured for this provider".to_string());
    }

    let base_url = provider.base_url.trim();
    if base_url.is_empty() {
        return Err("provider base URL is blank".to_string());
    }
    let parsed = url::Url::parse(base_url)
        .map_err(|_| "provider base URL is invalid".to_string())?;
    let is_opencode_go = parsed
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("opencode.ai"))
        && parsed.path().to_ascii_lowercase().contains("/zen/go");
    if !is_opencode_go {
        return Err("provider is not an OpenCode Go endpoint".to_string());
    }

    Ok(GoUsageRequest {
        url: GO_USAGE_URL.to_string(),
        api_key: provider.api_key.clone(),
    })
}

pub(in crate::ai_gateway) fn is_go_usage_cache_fresh(cached_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(cached_at_ms) < GO_USAGE_CACHE_TTL_MS
}

struct CachedGoUsage {
    api_key: String,
    base_url: String,
    cached_at_ms: u64,
    snapshot: ProviderGoUsage,
}

pub(in crate::ai_gateway) struct GoUsageCache {
    entries: HashMap<String, CachedGoUsage>,
}

impl GoUsageCache {
    pub(in crate::ai_gateway) fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub(in crate::ai_gateway) fn get_fresh(
        &self,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        force_refresh: bool,
    ) -> Option<ProviderGoUsage> {
        if force_refresh {
            return None;
        }
        self.entries.get(provider_id).and_then(|entry| {
            (entry.api_key == api_key
                && entry.base_url == base_url
                && is_go_usage_cache_fresh(entry.cached_at_ms, now_ms))
            .then(|| entry.snapshot.clone())
        })
    }

    pub(in crate::ai_gateway) fn store(
        &mut self,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        snapshot: ProviderGoUsage,
    ) {
        self.entries.insert(
            provider_id.to_string(),
            CachedGoUsage {
                api_key: api_key.to_string(),
                base_url: base_url.to_string(),
                cached_at_ms: now_ms,
                snapshot,
            },
        );
    }
}

pub(in crate::ai_gateway) async fn provider_go_usage_with<F, Fut>(
    config: &GatewayConfig,
    provider_id: &str,
    force_refresh: bool,
    now_ms: u64,
    cache: &Mutex<GoUsageCache>,
    fetch: F,
) -> Result<ProviderGoUsage, String>
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;
    let request = resolve_go_usage_request(provider)?;

    {
        let cache = cache
            .lock()
            .map_err(|_| "Go usage cache is unavailable".to_string())?;
        if let Some(snapshot) = cache.get_fresh(
            provider_id,
            &request.api_key,
            &provider.base_url,
            now_ms,
            force_refresh,
        ) {
            return Ok(snapshot);
        }
    }

    let body = fetch(request.url, request.api_key).await?;
    let snapshot = parse_go_usage(&body)?;
    cache
        .lock()
        .map_err(|_| "Go usage cache is unavailable".to_string())?
        .store(
            provider_id,
            &provider.api_key,
            &provider.base_url,
            now_ms,
            snapshot.clone(),
        );
    Ok(snapshot)
}

static GO_USAGE_CACHE: OnceLock<Mutex<GoUsageCache>> = OnceLock::new();

pub(in crate::ai_gateway) async fn ai_gateway_provider_go_usage_with<F, Fut>(
    provider_id: String,
    force_refresh: Option<bool>,
    now_ms: u64,
    cache: &Mutex<GoUsageCache>,
    fetch: F,
) -> Result<ProviderGoUsage, String>
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let config = storage::read_config()?;
    provider_go_usage_with(
        &config,
        &provider_id,
        force_refresh.unwrap_or(false),
        now_ms,
        cache,
        fetch,
    )
    .await
}

#[tauri::command]
pub async fn ai_gateway_provider_go_usage(
    provider_id: String,
    force_refresh: Option<bool>,
) -> Result<ProviderGoUsage, String> {
    let cache = GO_USAGE_CACHE.get_or_init(|| Mutex::new(GoUsageCache::new()));
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_string())?
        .as_millis() as u64;
    ai_gateway_provider_go_usage_with(
        provider_id,
        force_refresh,
        now_ms,
        cache,
        fetch_go_usage_body,
    )
    .await
}

async fn fetch_go_usage_body(url: String, api_key: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(GO_USAGE_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| format!("failed to prepare a request for {url}"))?;
    let response = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                format!("timed out fetching {url}")
            } else {
                format!("failed to fetch {url}")
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("failed to fetch {url}: HTTP {status}"));
    }
    response.text().await.map_err(|error| {
        if error.is_timeout() {
            format!("timed out reading response from {url}")
        } else {
            format!("failed to read response from {url}")
        }
    })
}
