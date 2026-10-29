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
    generation: u64,
    api_key: String,
    base_url: String,
    cached_at_ms: u64,
    snapshot: ProviderGoUsage,
}

/// Exact source identity of one Go-usage lookup. Concurrent lookups may share a
/// fetch only when the provider, pinned key, base URL, effective policy and
/// global proxy generation all match, so a rotated key, changed endpoint or
/// switched transport never receives another source's result.
#[derive(Clone, PartialEq, Eq, Hash)]
struct GoUsageSourceIdentity {
    provider_id: String,
    api_key: String,
    base_url: String,
    policy: &'static str,
    generation: u64,
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

    #[allow(dead_code)]
    pub(in crate::ai_gateway) fn get_fresh(
        &self,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        force_refresh: bool,
    ) -> Option<ProviderGoUsage> {
        self.get_fresh_at(
            super::current_proxy_generation(),
            provider_id,
            api_key,
            base_url,
            now_ms,
            force_refresh,
        )
    }

    fn get_fresh_at(
        &self,
        generation: u64,
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
            (entry.generation == generation
                && entry.api_key == api_key
                && entry.base_url == base_url
                && is_go_usage_cache_fresh(entry.cached_at_ms, now_ms))
            .then(|| entry.snapshot.clone())
        })
    }

    #[allow(dead_code)]
    pub(in crate::ai_gateway) fn store(
        &mut self,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        snapshot: ProviderGoUsage,
    ) {
        self.store_at(
            super::current_proxy_generation(),
            provider_id,
            api_key,
            base_url,
            now_ms,
            snapshot,
        );
    }

    fn store_at(
        &mut self,
        generation: u64,
        provider_id: &str,
        api_key: &str,
        base_url: &str,
        now_ms: u64,
        snapshot: ProviderGoUsage,
    ) {
        self.entries.insert(
            provider_id.to_string(),
            CachedGoUsage {
                generation,
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
    let generation = super::current_proxy_generation();
    let identity = GoUsageSourceIdentity {
        provider_id: provider_id.to_string(),
        api_key: api_key.clone(),
        base_url: provider.base_url.clone(),
        policy: go_usage_policy_identity(policy),
        generation,
    };

    let start = {
        let mut guard = cache
            .lock()
            .map_err(|_| "Go usage cache is unavailable".to_string())?;
        if let Some(snapshot) = guard.get_fresh_at(
            generation,
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
                    // A late result may settle its own callers, but it must
                    // never replace a newer generation's cache entry.
                    if super::current_proxy_generation() == generation {
                        cache.store_at(
                            generation,
                            provider_id,
                            &api_key,
                            &provider.base_url,
                            now_ms,
                            snapshot.clone(),
                        );
                    }
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

/// Build a Go-usage request client for the current global proxy generation.
/// Each request resolves transport anew; a preparation failure is reported
/// instead of silently using a direct client.
fn go_usage_http_client() -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .timeout(GO_USAGE_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    crate::proxy::apply_current_proxy_to_builder(builder)?
        .build()
        .map_err(|_| "failed to prepare a Go usage request".to_string())
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

#[cfg(test)]
mod proxy_switch_named {
    #[tokio::test]
    async fn gateway_go_usage_proxy_switch() {
        use crate::proxy::test_support::{HttpProxyRecorder, SERIAL_GUARD};
        let _g = SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let proxy_a = HttpProxyRecorder::new("A").await;
        let proxy_b = HttpProxyRecorder::new("B").await;
        fn port_of(url: &str) -> u16 {
            url.trim_start_matches("http://").split(':').nth(1).unwrap().parse().unwrap()
        }
        let manager = crate::proxy::PROXY_MANAGER.get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
        manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(&proxy_a.url()),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        })).unwrap());

        let _ = super::fetch_go_usage_body("http://usage.example.test/x".to_string(), "SAFE_FIXTURE_key".to_string()).await;
        assert_eq!(proxy_a.targets(), vec!["http://usage.example.test/x".to_string()]);

        manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(&proxy_b.url()),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        })).unwrap());

        let _ = super::fetch_go_usage_body("http://usage.example.test/x".to_string(), "SAFE_FIXTURE_key".to_string()).await;
        assert_eq!(proxy_b.targets(), vec!["http://usage.example.test/x".to_string()]);
    }

    use super::{provider_go_usage_with, GoUsageCache};
    use crate::ai_gateway::{GatewayConfig, GatewayUpstreamProvider};
    use crate::proxy::test_support::{HttpProxyRecorder, SERIAL_GUARD};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::{Notify, Semaphore};

    fn port_of(url: &str) -> u16 {
        url.trim_start_matches("http://")
            .split(':')
            .nth(1)
            .expect("recorder URL has a port")
            .parse()
            .expect("recorder URL port is numeric")
    }

    fn proxy_config(url: &str) -> crate::config::ProxyConfig {
        crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(url),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        }
    }

    fn bound_go_usage_provider() -> GatewayUpstreamProvider {
        serde_json::from_value(json!({
            "id": "go-provider",
            "name": "Provider go-provider",
            "base_url": "https://opencode.ai/zen/go/v1",
            "keys": [{
                "id": "key-default",
                "name": "Default",
                "value": "SAFE_FIXTURE_go_usage_key",
                "enabled": true,
            }],
            "template_id": "opencode-go",
            "protocol": "chat_completions",
            "enabled": true,
        }))
        .expect("Go usage provider fixture must deserialize")
    }

    fn go_usage_body(percent: f64) -> String {
        format!(
            r#"{{"usage":{{"rolling":{{"percent":{percent}}},"weekly":{{"percent":{percent}}},"monthly":{{"percent":{percent}}}}}}}"#
        )
    }

    /// A fetch parked until the test releases it. Counting invocations is how
    /// the test observes flight ownership without touching cache internals.
    struct FetchGate {
        calls: AtomicUsize,
        arrived: Notify,
        release: Semaphore,
    }

    impl FetchGate {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                arrived: Notify::new(),
                release: Semaphore::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn release(&self) {
            self.release.add_permits(1);
        }

        async fn enter(&self) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.arrived.notify_one();
            let permit = self
                .release
                .acquire()
                .await
                .expect("Go usage flight release semaphore must stay open");
            permit.forget();
        }
    }

    fn spawn_lookup(
        config: Arc<GatewayConfig>,
        cache: Arc<Mutex<GoUsageCache>>,
        gate: Arc<FetchGate>,
        body: String,
        force_refresh: bool,
    ) -> tokio::task::JoinHandle<Result<super::ProviderGoUsage, String>> {
        tokio::spawn(async move {
            provider_go_usage_with(
                config.as_ref(),
                "go-provider",
                force_refresh,
                1_000,
                cache.as_ref(),
                move |_url, _key| {
                    let gate = Arc::clone(&gate);
                    let body = body.clone();
                    async move {
                        gate.enter().await;
                        Ok(body)
                    }
                },
            )
            .await
        })
    }

    fn spawn_panicking_lookup(
        config: Arc<GatewayConfig>,
        cache: Arc<Mutex<GoUsageCache>>,
    ) -> tokio::task::JoinHandle<Result<super::ProviderGoUsage, String>> {
        tokio::spawn(async move {
            provider_go_usage_with(
                config.as_ref(),
                "go-provider",
                false,
                1_000,
                cache.as_ref(),
                |_url, _key| async move {
                    panic!("a same-generation follower or a fresh cache hit must not fetch Go usage")
                },
            )
            .await
        })
    }

    async fn wait_for_fetch(gate: &FetchGate) {
        tokio::time::timeout(Duration::from_secs(2), gate.arrived.notified())
            .await
            .expect("the Go usage flight must start its own fetch");
    }

    /// REQ-004 / AC-006: a flight is keyed by the global proxy generation, so a
    /// call admitted after a switch never joins the previous transport's flight.
    /// The late old-generation success settles its own callers but must not
    /// replace the newer generation's cache entry.
    #[tokio::test(flavor = "current_thread")]
    async fn gateway_go_usage_flight_separates_generations_and_gates_late_result() {
        let _guard = SERIAL_GUARD.lock().unwrap_or_else(|error| error.into_inner());
        let _home = crate::lock_test_home_env();
        let proxy_a = HttpProxyRecorder::new("A").await;
        let proxy_b = HttpProxyRecorder::new("B").await;
        let manager = crate::proxy::PROXY_MANAGER
            .get_or_init(|| Arc::new(crate::proxy::ProxyManager::new()));

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_a.url())))
                .expect("proxy A prepares"),
        );
        let generation_a = manager.generation();

        let config = Arc::new(GatewayConfig {
            providers: vec![bound_go_usage_provider()],
            ..GatewayConfig::default()
        });
        let cache = Arc::new(Mutex::new(GoUsageCache::new()));
        let gate_a = Arc::new(FetchGate::new());
        let gate_b = Arc::new(FetchGate::new());

        let owner_a = spawn_lookup(
            Arc::clone(&config),
            Arc::clone(&cache),
            Arc::clone(&gate_a),
            go_usage_body(1.0),
            false,
        );
        wait_for_fetch(&gate_a).await;

        // Same generation: this caller must join the parked flight and never
        // invoke its own fetch.
        let follower_a = spawn_panicking_lookup(Arc::clone(&config), Arc::clone(&cache));
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            gate_a.calls(),
            1,
            "a same-generation follower must share the owner's single fetch"
        );

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_b.url())))
                .expect("proxy B prepares"),
        );
        let generation_b = manager.generation();
        assert_ne!(
            generation_a, generation_b,
            "publishing a different proxy endpoint must advance the generation"
        );

        let owner_b = spawn_lookup(
            Arc::clone(&config),
            Arc::clone(&cache),
            Arc::clone(&gate_b),
            go_usage_body(2.0),
            false,
        );
        wait_for_fetch(&gate_b).await;
        assert_eq!(
            gate_a.calls(),
            1,
            "the new-generation call must not join the old-generation flight"
        );
        assert_eq!(
            gate_b.calls(),
            1,
            "the new-generation call must run exactly one fetch of its own"
        );

        // Settle the newer generation first; it owns the cache entry.
        gate_b.release();
        let result_b = owner_b
            .await
            .expect("generation B task must not panic")
            .expect("generation B lookup must succeed");
        assert_eq!(result_b.usage.rolling.percent, 2.0);

        // The old generation settles late: its own callers still receive its
        // result, but the store gate must reject the stale entry.
        gate_a.release();
        let result_a_owner = owner_a
            .await
            .expect("generation A owner task must not panic")
            .expect("generation A owner lookup must succeed");
        let result_a_follower = follower_a
            .await
            .expect("generation A follower task must not panic")
            .expect("generation A follower lookup must succeed");
        assert_eq!(
            result_a_owner.usage.rolling.percent, 1.0,
            "the old-generation owner must receive its own result"
        );
        assert_eq!(
            result_a_follower.usage.rolling.percent, 1.0,
            "the same-generation follower must receive the owner's result"
        );

        // B's cache entry survived A's late success: a plain lookup in the
        // current generation is served without invoking the panicking fetch.
        let cached = provider_go_usage_with(
            config.as_ref(),
            "go-provider",
            false,
            1_001,
            cache.as_ref(),
            |_url, _key| async move {
                panic!("a fresh current-generation cache entry must not fetch Go usage")
            },
        )
        .await
        .expect("generation B cache entry must remain readable");
        assert_eq!(
            cached.usage.rolling.percent, 2.0,
            "the late old-generation result must not replace generation B's cache"
        );
    }
}
