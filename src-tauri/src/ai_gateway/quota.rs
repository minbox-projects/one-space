use super::{storage, GatewayConfig, GatewayUpstreamProvider};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const COMMANDCODE_QUOTA_URL: &str = "https://api.commandcode.ai/alpha/billing/credits";
pub const QUOTA_CACHE_TTL_MS: u64 = 300_000;
const QUOTA_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

fn deserialize_default_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuota {
    pub credits: QuotaCredits,
    #[serde(default)]
    pub window_limits: Option<QuotaWindowLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaCredits {
    #[serde(default, deserialize_with = "deserialize_default_f64")]
    pub monthly_credits: f64,
    #[serde(default, deserialize_with = "deserialize_default_f64")]
    pub purchased_credits: f64,
    #[serde(default, deserialize_with = "deserialize_default_f64")]
    pub free_credits: f64,
    #[serde(default)]
    pub below_threshold: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowLimits {
    #[serde(default)]
    pub limited: bool,
    #[serde(default)]
    pub five_hour: Option<QuotaWindow>,
    #[serde(default)]
    pub weekly: Option<QuotaWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    #[serde(default, deserialize_with = "deserialize_default_f64")]
    pub used: f64,
    #[serde(default, deserialize_with = "deserialize_default_f64")]
    pub cap: f64,
    #[serde(default)]
    pub exceeded: bool,
    #[serde(default)]
    pub reset_at: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderQuotaPayload {
    credits: Option<QuotaCredits>,
    #[serde(default)]
    window_limits: Option<QuotaWindowLimits>,
}

pub fn parse_provider_quota(body: &str) -> Result<ProviderQuota, String> {
    let payload: ProviderQuotaPayload = serde_json::from_str(body)
        .map_err(|error| format!("invalid quota response: {error}"))?;
    let credits = payload
        .credits
        .ok_or_else(|| "quota response is missing a credits object".to_string())?;
    Ok(ProviderQuota {
        credits,
        window_limits: payload.window_limits,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaRequest {
    pub url: String,
    pub api_key: String,
}

pub fn resolve_quota_request(
    provider: &GatewayUpstreamProvider,
) -> Result<QuotaRequest, String> {
    if provider.api_key.trim().is_empty() {
        return Err("no API key configured for this provider".to_string());
    }

    let base_url = provider.base_url.trim();
    if base_url.is_empty() {
        return Err("provider base URL is blank".to_string());
    }
    let parsed = url::Url::parse(base_url)
        .map_err(|_| "provider base URL is invalid".to_string())?;
    if !parsed
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("api.commandcode.ai"))
    {
        return Err("provider is not hosted by api.commandcode.ai".to_string());
    }

    Ok(QuotaRequest {
        url: COMMANDCODE_QUOTA_URL.to_string(),
        api_key: provider.api_key.clone(),
    })
}

pub fn is_quota_cache_fresh(cached_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(cached_at_ms) < QUOTA_CACHE_TTL_MS
}

struct CachedQuota {
    api_key: String,
    base_url: String,
    cached_at_ms: u64,
    snapshot: ProviderQuota,
}

pub struct QuotaCache {
    entries: HashMap<String, CachedQuota>,
}

impl QuotaCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn get_fresh(
        &self,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        force_refresh: bool,
    ) -> Option<ProviderQuota> {
        if force_refresh {
            return None;
        }
        self.entries.get(provider_id).and_then(|entry| {
            (entry.api_key == api_key
                && entry.base_url == base_url
                && is_quota_cache_fresh(entry.cached_at_ms, now_ms))
                .then(|| entry.snapshot.clone())
        })
    }

    pub fn store(
        &mut self,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        snapshot: ProviderQuota,
    ) {
        self.entries.insert(
            provider_id.to_string(),
            CachedQuota {
                api_key: api_key.to_string(),
                base_url: base_url.to_string(),
                cached_at_ms: now_ms,
                snapshot,
            },
        );
    }
}

pub async fn provider_quota_with<F, Fut>(
    config: &GatewayConfig,
    provider_id: &str,
    force_refresh: bool,
    now_ms: u64,
    cache: &Mutex<QuotaCache>,
    fetch: F,
) -> Result<ProviderQuota, String>
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;
    let request = resolve_quota_request(provider)?;

    {
        let cache = cache
            .lock()
            .map_err(|_| "quota cache is unavailable".to_string())?;
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
    let snapshot = parse_provider_quota(&body)?;
    cache
        .lock()
        .map_err(|_| "quota cache is unavailable".to_string())?
        .store(
            provider_id,
            &provider.api_key,
            &provider.base_url,
            now_ms,
            snapshot.clone(),
        );
    Ok(snapshot)
}

static QUOTA_CACHE: OnceLock<Mutex<QuotaCache>> = OnceLock::new();

#[tauri::command]
pub async fn ai_gateway_provider_quota(
    provider_id: String,
    force_refresh: Option<bool>,
) -> Result<ProviderQuota, String> {
    let config = storage::read_config()?;
    let cache = QUOTA_CACHE.get_or_init(|| Mutex::new(QuotaCache::new()));
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_string())?
        .as_millis() as u64;
    provider_quota_with(
        &config,
        &provider_id,
        force_refresh.unwrap_or(false),
        now_ms,
        cache,
        fetch_quota_body,
    )
    .await
}

async fn fetch_quota_body(url: String, api_key: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(QUOTA_REQUEST_TIMEOUT)
        .build()
        .map_err(|_| format!("failed to prepare a request for {url}"))?;
    let response = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|_| format!("failed to fetch {url}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("failed to fetch {url}: HTTP {status}"));
    }
    response
        .text()
        .await
        .map_err(|_| format!("failed to read response from {url}"))
}
