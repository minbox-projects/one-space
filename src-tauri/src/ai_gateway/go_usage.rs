use super::templates::effective_quota_strategy;
use super::types_config::QuotaStrategy;
use super::{storage, GatewayConfig, GatewayUpstreamProvider};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;

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
    pub(in crate::ai_gateway) policy: QuotaStrategy,
}

/// Stable name of the effective policy inside a source identity.
fn go_usage_policy_identity(strategy: QuotaStrategy) -> &'static str {
    match strategy {
        QuotaStrategy::NoQuota => "none",
        QuotaStrategy::Commandcode => "commandcode",
        QuotaStrategy::OpencodeGo => "opencode_go",
    }
}

/// Resolve the OpenCode Go usage request for a provider strictly from its
/// effective template quota policy: a provider whose effective strategy is not
/// `opencode_go` — including an unbound provider, a broken binding or a
/// different specialized policy — is rejected before any key or base-URL is
/// consulted, so no upstream fetch can happen. The fixed endpoint and pinned
/// source key are otherwise resolved exactly as before.
pub(in crate::ai_gateway) fn resolve_go_usage_request(
    config: &GatewayConfig,
    provider: &GatewayUpstreamProvider,
) -> Result<GoUsageRequest, String> {
    let policy = effective_quota_strategy(config, provider);
    if policy != QuotaStrategy::OpencodeGo {
        return Err(
            "provider is not bound to an OpenCode Go quota policy; bind the OpenCode Go template to monitor usage"
                .to_string(),
        );
    }

    let api_key = super::selection::pinned_key_value(provider)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "no API key configured for this provider".to_string())?
        .to_string();

    Ok(GoUsageRequest {
        url: GO_USAGE_URL.to_string(),
        api_key,
        policy,
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

/// Exact source identity of one Go-usage lookup. Concurrent lookups may share a
/// fetch only when the provider, pinned key, base URL and effective policy all
/// match, so a rotated key or changed endpoint never receives another source's
/// result.
#[derive(Clone, PartialEq, Eq, Hash)]
struct GoUsageSourceIdentity {
    provider_id: String,
    api_key: String,
    base_url: String,
    policy: &'static str,
}

/// One in-flight Go-usage fetch. The watch channel publishes the settled result
/// to every follower. Its own keep-alive receiver keeps the channel open even
/// when a follower subscribes only after the owner settled, so the published
/// result can never be lost.
struct GoUsageFlight {
    tx: watch::Sender<Option<Result<ProviderGoUsage, String>>>,
    _keepalive: watch::Receiver<Option<Result<ProviderGoUsage, String>>>,
}

/// Ownership token of one in-flight Go-usage fetch. Dropping it before
/// completion (a cancelled caller) settles the flight with an error and removes
/// only its own entry, so a replacement flight is never cleared.
struct GoUsageFlightOwner<'a> {
    cache: &'a Mutex<GoUsageCache>,
    identity: GoUsageSourceIdentity,
    flight: Arc<GoUsageFlight>,
}

impl GoUsageFlightOwner<'_> {
    fn finish(self, result: Result<ProviderGoUsage, String>) {
        let _ = self.flight.tx.send(Some(result));
    }

    fn clear(&self) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache
            .flights
            .get(&self.identity)
            .is_some_and(|existing| Arc::ptr_eq(existing, &self.flight))
        {
            cache.flights.remove(&self.identity);
        }
    }
}

impl Drop for GoUsageFlightOwner<'_> {
    fn drop(&mut self) {
        let unfinished = self.flight.tx.borrow().is_none();
        if unfinished {
            let _ = self
                .flight
                .tx
                .send(Some(Err("Go usage lookup was cancelled".to_string())));
        }
        self.clear();
    }
}

/// Await and reuse the owner's in-flight result without another fetch.
async fn wait_for_go_usage_flight(flight: Arc<GoUsageFlight>) -> Result<ProviderGoUsage, String> {
    let mut rx = flight.tx.subscribe();
    if let Some(result) = rx.borrow().clone() {
        return result;
    }
    loop {
        if rx.changed().await.is_err() {
            return rx.borrow().clone().unwrap_or_else(|| {
                Err("Go usage lookup ended without a result".to_string())
            });
        }
        if let Some(result) = rx.borrow().clone() {
            return result;
        }
    }
}

enum GoUsageStart<'a> {
    Cached(ProviderGoUsage),
    Join(Arc<GoUsageFlight>),
    Own(GoUsageFlightOwner<'a>),
}

pub(in crate::ai_gateway) struct GoUsageCache {
    entries: HashMap<String, CachedGoUsage>,
    flights: HashMap<GoUsageSourceIdentity, Arc<GoUsageFlight>>,
}

impl GoUsageCache {
    pub(in crate::ai_gateway) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            flights: HashMap::new(),
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
    let GoUsageRequest {
        url,
        api_key,
        policy,
    } = resolve_go_usage_request(config, provider)?;
    let identity = GoUsageSourceIdentity {
        provider_id: provider_id.to_string(),
        api_key: api_key.clone(),
        base_url: provider.base_url.clone(),
        policy: go_usage_policy_identity(policy),
    };

    let start = {
        let mut guard = cache
            .lock()
            .map_err(|_| "Go usage cache is unavailable".to_string())?;
        if let Some(snapshot) = guard.get_fresh(
            provider_id,
            &api_key,
            &provider.base_url,
            now_ms,
            force_refresh,
        ) {
            GoUsageStart::Cached(snapshot)
        } else if let Some(flight) = guard.flights.get(&identity) {
            GoUsageStart::Join(Arc::clone(flight))
        } else {
            let (tx, keepalive) = watch::channel(None);
            let flight = Arc::new(GoUsageFlight {
                tx,
                _keepalive: keepalive,
            });
            guard
                .flights
                .insert(identity.clone(), Arc::clone(&flight));
            GoUsageStart::Own(GoUsageFlightOwner {
                cache,
                identity: identity.clone(),
                flight,
            })
        }
    };

    let owner = match start {
        GoUsageStart::Cached(snapshot) => return Ok(snapshot),
        GoUsageStart::Join(flight) => return wait_for_go_usage_flight(flight).await,
        GoUsageStart::Own(owner) => owner,
    };

    let result = match fetch(url, api_key.clone()).await {
        Ok(body) => parse_go_usage(&body),
        Err(error) => Err(error),
    };
    let result = match result {
        Ok(snapshot) => {
            let stored = cache
                .lock()
                .map_err(|_| "Go usage cache is unavailable".to_string())
                .map(|mut cache| {
                    cache.store(
                        provider_id,
                        &api_key,
                        &provider.base_url,
                        now_ms,
                        snapshot.clone(),
                    );
                });
            match stored {
                Ok(()) => Ok(snapshot),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    };

    owner.finish(result.clone());
    result
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

static GO_USAGE_HTTP_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

fn go_usage_http_client() -> Result<reqwest::Client, String> {
    GO_USAGE_HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(GO_USAGE_REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|error| error.to_string())
        })
        .clone()
}

async fn fetch_go_usage_body(url: String, api_key: String) -> Result<String, String> {
    let client = go_usage_http_client()
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
