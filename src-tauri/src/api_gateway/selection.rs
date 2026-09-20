use super::{GatewayUpstreamProvider, UpstreamProtocol, FAILURE_THRESHOLD};
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::Instant;

/// Outcome class for an upstream attempt, driving switching and auto-disable decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::api_gateway) enum FailureClass {
    /// Auth failures (401/403) disable the provider immediately and switch.
    DisableImmediately,
    /// Counts toward consecutive failures; switches and auto-disables at the threshold.
    Retryable,
    /// Switches without counting as a failure (429/404).
    Transient,
    /// Returns the upstream error to the caller without switching or disabling (400/422/other 4xx).
    ReturnToClient,
}

/// Outcome of resolving a request model against one provider for an inbound protocol.
#[derive(Debug)]
pub(in crate::api_gateway) enum ModelResolution {
    /// Forward using this upstream model.
    Serve(String),
    /// A valid mapping row matches the requested model but declares another
    /// protocol. The provider must not serve this request and must never fall
    /// back to `default_model`; the payload carries the protocol it targets so
    /// the error can name the required endpoint.
    ProtocolMismatch(UpstreamProtocol),
    /// Neither a matching row nor an eligible default model.
    NoMatch,
}

/// Resolve the upstream model for a provider under the inbound `protocol`.
///
/// Rows with an empty `upstream_model` are discarded and count as no match.
/// Disabled rows never serve and never cause a protocol mismatch, but a request
/// that matches only disabled rows is a `NoMatch` and cannot fall back to the
/// default model. An enabled matching row is served with its own remote model
/// only when the row's effective protocol (its own declaration, else the
/// provider protocol) equals `protocol`; when enabled matching rows exist but
/// none matches, the request is a `ProtocolMismatch` and the default model is
/// not used as a fallback. The default model serves an unmapped model only when
/// the provider protocol itself matches.
pub(in crate::api_gateway) fn resolve_model_for_protocol(
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> ModelResolution {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    let mut configured: Option<UpstreamProtocol> = None;
    let mut disabled_match = false;
    if let Some(requested) = requested {
        for mapping in provider.mappings.iter().filter(|mapping| {
            mapping.local_model.trim() == requested && !mapping.upstream_model.trim().is_empty()
        }) {
            // A disabled row never serves and never produces a protocol
            // mismatch; it only records that the requested model is mapped but
            // switched off, which later blocks the default-model fallback.
            if !mapping.enabled {
                disabled_match = true;
                continue;
            }
            let effective = mapping.effective_protocol(provider.protocol);
            if effective == protocol {
                return ModelResolution::Serve(mapping.upstream_model.trim().to_string());
            }
            configured.get_or_insert(effective);
        }
    }
    if let Some(configured) = configured {
        return ModelResolution::ProtocolMismatch(configured);
    }
    if disabled_match {
        return ModelResolution::NoMatch;
    }
    provider
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|_| provider.protocol == protocol)
        .map(|value| ModelResolution::Serve(value.to_string()))
        .unwrap_or(ModelResolution::NoMatch)
}

/// Candidate set: enabled, not auto-disabled, and able to serve the request
/// model under the inbound protocol. A provider is not filtered by its own
/// protocol alone, because a mapping row may declare the inbound protocol.
pub(in crate::api_gateway) fn candidate_providers<'a>(
    providers: &'a [GatewayUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> Vec<&'a GatewayUpstreamProvider> {
    providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && !provider.auto_disabled
                && matches!(
                    resolve_model_for_protocol(provider, requested, protocol),
                    ModelResolution::Serve(_)
                )
        })
        .collect()
}

/// Uniformly shuffle a copy of the candidate list for one request attempt pass.
pub(in crate::api_gateway) fn shuffled_candidates(
    candidates: &[GatewayUpstreamProvider],
) -> Vec<GatewayUpstreamProvider> {
    let mut ordered = candidates.to_vec();
    ordered.shuffle(&mut rand::thread_rng());
    ordered
}

/// Client headers that may carry a session identity, highest precedence first.
/// The inbound header map is already lower-cased before it is looked up.
pub(in crate::api_gateway) const SESSION_ID_HEADERS: [&str; 7] = [
    "x-session-affinity",
    "x-opencode-session",
    "session-id",
    "session_id",
    "conversation_id",
    "thread-id",
    "x-session-id",
];

/// Idle time after which a session binding is treated as absent. Recency is
/// measured on `tokio::time::Instant`, so a paused-clock test can assert
/// expiry without real waiting.
pub(in crate::api_gateway) const SESSION_BINDING_IDLE_TIMEOUT: Duration =
    Duration::from_secs(30 * 60);

/// Maximum live bindings; the least recently used entry is evicted first.
pub(in crate::api_gateway) const SESSION_BINDING_CAPACITY: usize = 1024;

/// Consecutive misses after which a binding migrates to the serving provider.
pub(in crate::api_gateway) const SESSION_BINDING_MISS_THRESHOLD: u32 = 2;

/// Resolve the session identity as the first header of [`SESSION_ID_HEADERS`]
/// present with a non-empty trimmed value. A header whose value is empty or
/// whitespace-only counts as absent and the walk continues, and headers outside
/// the list are never consulted.
pub(in crate::api_gateway) fn resolve_session_id(
    headers: &HashMap<String, String>,
) -> Option<String> {
    SESSION_ID_HEADERS.iter().find_map(|header| {
        headers
            .get(*header)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// One live binding: the provider that should be attempted first and the
/// consecutive misses recorded since its last bound success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::api_gateway) struct SessionBinding {
    pub(in crate::api_gateway) provider_id: String,
    pub(in crate::api_gateway) misses: u32,
}

/// A request's candidate order together with the binding that ordered it.
#[derive(Debug)]
pub(in crate::api_gateway) struct SessionOrder {
    pub(in crate::api_gateway) ordered: Vec<GatewayUpstreamProvider>,
    pub(in crate::api_gateway) bound_provider_id: Option<String>,
}

/// One entry of [`SessionAffinityStore`].
#[derive(Debug)]
struct SessionBindingEntry {
    provider_id: String,
    misses: u32,
    last_used: Instant,
}

/// Process-memory binding table keyed by the session value plus the trimmed
/// local model name. An entry idle for more than
/// [`SESSION_BINDING_IDLE_TIMEOUT`] is treated as absent, the table holds at
/// most [`SESSION_BINDING_CAPACITY`] entries and evicts the least recently used
/// one, and nothing is ever persisted.
#[derive(Debug, Default)]
pub(in crate::api_gateway) struct SessionAffinityStore {
    entries: HashMap<(String, String), SessionBindingEntry>,
}

impl SessionAffinityStore {
    pub(in crate::api_gateway) fn new() -> Self {
        Self::default()
    }

    /// Read a live binding and refresh its recency; an entry idle for more than
    /// the timeout is dropped and reported as absent.
    pub(in crate::api_gateway) fn lookup(
        &mut self,
        session: &str,
        model: &str,
    ) -> Option<SessionBinding> {
        let model = model.trim();
        if session.trim().is_empty() || model.is_empty() {
            return None;
        }
        self.prune_expired();
        let entry = self
            .entries
            .get_mut(&(session.to_string(), model.to_string()))?;
        entry.last_used = Instant::now();
        Some(SessionBinding {
            provider_id: entry.provider_id.clone(),
            misses: entry.misses,
        })
    }

    /// The atomic selection operation the runtime holds the store lock around:
    /// look the binding up, shuffle once, write the first-request binding and
    /// reorder a bound provider to the front. A request without a session or a
    /// non-empty trimmed model reads and writes nothing.
    pub(in crate::api_gateway) fn resolve_order<F>(
        &mut self,
        session: Option<&str>,
        model: Option<&str>,
        shuffle: F,
    ) -> SessionOrder
    where
        F: FnOnce() -> Vec<GatewayUpstreamProvider>,
    {
        let session = session.filter(|value| !value.trim().is_empty());
        let model = model.map(str::trim).filter(|value| !value.is_empty());
        let ordered = shuffle();
        let (Some(session), Some(model)) = (session, model) else {
            return SessionOrder {
                ordered,
                bound_provider_id: None,
            };
        };
        self.prune_expired();
        let key = (session.to_string(), model.to_string());
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = Instant::now();
            let bound_provider_id = entry.provider_id.clone();
            let ordered = reorder_bound_first(
                ordered,
                Some(session),
                Some(model),
                Some(&bound_provider_id),
            );
            return SessionOrder {
                ordered,
                bound_provider_id: Some(bound_provider_id),
            };
        }
        let Some(first) = ordered.first() else {
            return SessionOrder {
                ordered,
                bound_provider_id: None,
            };
        };
        let provider_id = first.id.clone();
        self.insert_binding(key, provider_id.clone());
        SessionOrder {
            ordered,
            bound_provider_id: Some(provider_id),
        }
    }

    /// Settle one request after its terminal outcome. Nothing changes without a
    /// live binding or a served provider; a binding that was not an eligible
    /// candidate is replaced immediately at zero misses; a different serving
    /// provider records one miss and migrates the binding at
    /// [`SESSION_BINDING_MISS_THRESHOLD`].
    pub(in crate::api_gateway) fn settle(
        &mut self,
        session: &str,
        model: &str,
        bound_was_eligible: bool,
        served_provider_id: Option<&str>,
    ) {
        let model = model.trim();
        if session.trim().is_empty() || model.is_empty() {
            return;
        }
        let served = served_provider_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(served) = served else {
            return;
        };
        self.prune_expired();
        let Some(entry) = self
            .entries
            .get_mut(&(session.to_string(), model.to_string()))
        else {
            return;
        };
        entry.last_used = Instant::now();
        if !bound_was_eligible || entry.provider_id == served {
            entry.provider_id = served.to_string();
            entry.misses = 0;
        } else {
            entry.misses = entry.misses.saturating_add(1);
            if entry.misses >= SESSION_BINDING_MISS_THRESHOLD {
                entry.provider_id = served.to_string();
                entry.misses = 0;
            }
        }
    }

    /// Live binding count after dropping idle-expired entries (test-only).
    #[cfg(test)]
    pub(in crate::api_gateway) fn live_len(&mut self) -> usize {
        self.prune_expired();
        self.entries.len()
    }

    /// Drop every entry idle for more than the timeout.
    fn prune_expired(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| {
            now.duration_since(entry.last_used) <= SESSION_BINDING_IDLE_TIMEOUT
        });
    }

    /// Insert a fresh binding, evicting the least recently used live entry when
    /// the table is already full. Refreshing an existing key never evicts.
    fn insert_binding(&mut self, key: (String, String), provider_id: String) {
        if !self.entries.contains_key(&key) && self.entries.len() >= SESSION_BINDING_CAPACITY {
            self.evict_least_recently_used();
        }
        self.entries.insert(
            key,
            SessionBindingEntry {
                provider_id,
                misses: 0,
                last_used: Instant::now(),
            },
        );
    }

    fn evict_least_recently_used(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            self.entries.remove(&key);
        }
    }
}

/// Move the provider bound to a session and model to the front of `ordered`,
/// keeping the relative order of every other candidate. The list is returned
/// unchanged when there is no session, no non-empty trimmed model, no bound id
/// or no matching candidate, so a binding never adds, drops or widens a
/// candidate.
pub(in crate::api_gateway) fn reorder_bound_first(
    mut ordered: Vec<GatewayUpstreamProvider>,
    session: Option<&str>,
    model: Option<&str>,
    bound_provider_id: Option<&str>,
) -> Vec<GatewayUpstreamProvider> {
    let has_value =
        |value: Option<&str>| value.map(str::trim).is_some_and(|value| !value.is_empty());
    if !has_value(session) || !has_value(model) {
        return ordered;
    }
    let Some(bound_provider_id) = bound_provider_id.filter(|value| !value.trim().is_empty()) else {
        return ordered;
    };
    let Some(index) = ordered.iter().position(|item| item.id == bound_provider_id) else {
        return ordered;
    };
    let bound = ordered.remove(index);
    ordered.insert(0, bound);
    ordered
}

static SESSION_AFFINITY: OnceLock<Mutex<SessionAffinityStore>> = OnceLock::new();

/// The process-global binding table of the running gateway. Bindings live in
/// process memory only, so a restart starts with none.
pub(in crate::api_gateway) fn session_affinity() -> &'static Mutex<SessionAffinityStore> {
    SESSION_AFFINITY.get_or_init(|| Mutex::new(SessionAffinityStore::new()))
}

/// Drop every binding of the process-global table (test-only reset; used by the
/// behavior tests that touch the global store).
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::api_gateway) fn reset_session_affinity_for_test() {
    *session_affinity()
        .lock()
        .expect("session affinity store lock") = SessionAffinityStore::new();
}

/// Choose a single candidate uniformly at random (used by coverage-sensitive tests).
#[cfg(test)]
pub(in crate::api_gateway) fn pick_candidate(
    candidates: &[GatewayUpstreamProvider],
) -> Option<GatewayUpstreamProvider> {
    shuffled_candidates(candidates).into_iter().next()
}

/// Classify an upstream failure.
///
/// Status semantics take priority over the response body shape, so an HTML 401
/// still disables immediately and a 5xx is retryable even without JSON. The
/// caller treats a parsed `< 400` response as success before classifying, so a
/// non-JSON success body falls through to retryable; network errors are always
/// retryable.
pub(in crate::api_gateway) fn classify_failure(
    status: u16,
    network_error: bool,
    _body_parsed: bool,
) -> FailureClass {
    if network_error {
        return FailureClass::Retryable;
    }
    match status {
        401 | 403 => FailureClass::DisableImmediately,
        404 => FailureClass::Transient,
        // 429 rate limits are retried by the scheduler but never count toward
        // provider health; 404 and 429 share the "no health count" class.
        429 => FailureClass::Transient,
        408 => FailureClass::Retryable,
        // 413/422 and every other client error are returned unchanged.
        400..=499 => FailureClass::ReturnToClient,
        500..=599 => FailureClass::Retryable,
        // A 2xx/3xx that did not parse as JSON is unusable and worth another
        // bounded attempt; a parsed 2xx is handled as success before this.
        _ => FailureClass::Retryable,
    }
}

/// Maximum bounded retries after a provider's first attempt in one request
/// (so a provider is contacted at most `1 + MAX_RETRIES_PER_PROVIDER` times).
pub(in crate::api_gateway) const MAX_RETRIES_PER_PROVIDER: u32 = 5;

const RETRY_BASE_DELAY_MILLIS: u128 = 2_000;
const RETRY_MAX_DELAY_MILLIS: u128 = 30_000;
const RETRY_JITTER_RATIO: f64 = 0.25;

/// Default delay before the `retry`-th retry (1-based): the spec's
/// `min(2000ms * 2^(retry-1) * (1 + random[0,1]*0.25), 30000ms)`. Header-driven
/// overrides take priority over this.
pub(in crate::api_gateway) fn default_retry_delay(retry: u32) -> Duration {
    let exponent = retry.saturating_sub(1).min(20);
    let base = RETRY_BASE_DELAY_MILLIS.saturating_mul(1u128 << exponent);
    let jitter = 1.0 + rand::random::<f64>() * RETRY_JITTER_RATIO;
    let millis = (base as f64 * jitter).min(RETRY_MAX_DELAY_MILLIS as f64) as u64;
    Duration::from_millis(millis)
}

/// Read the first value of each header, falling through invalid values.
pub(in crate::api_gateway) fn retry_header_delay(
    headers: &reqwest::header::HeaderMap,
) -> Option<Duration> {
    fn numeric_delay(value: &str, divisor: f64) -> Option<Duration> {
        let number = value.trim().parse::<f64>().ok()?;
        if !number.is_finite() || number < 0.0 {
            return None;
        }
        // Finite but unrepresentably large delays remain valid header values;
        // the request's wait budget will reject them instead of using backoff.
        Some(Duration::try_from_secs_f64(number / divisor).unwrap_or(Duration::MAX))
    }

    if let Some(delay) = headers
        .get("retry-after-ms")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| numeric_delay(value, 1000.0))
    {
        return Some(delay);
    }
    let value = headers.get("retry-after")?.to_str().ok()?.trim();
    if let Some(delay) = numeric_delay(value, 1.0) {
        return Some(delay);
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let delay = date.signed_duration_since(chrono::Utc::now());
    if delay <= chrono::Duration::zero() {
        return None;
    }
    delay.to_std().ok()
}

/// Whether a classified failure is worth another bounded attempt. `404` is a
/// permanent per-request skip and `ReturnToClient`/`DisableImmediately` never
/// retry; `429` (classified `Transient` so it skips health) is still retried.
pub(in crate::api_gateway) fn is_retryable_failure(class: FailureClass, status: u16) -> bool {
    match class {
        FailureClass::Retryable => true,
        FailureClass::Transient => status == 429,
        FailureClass::DisableImmediately | FailureClass::ReturnToClient => false,
    }
}

/// Record a failure on a provider. Returns `true` when the provider is (or becomes)
/// auto-disabled. `Transient` and `ReturnToClient` never count as failures.
pub(in crate::api_gateway) fn register_failure(
    provider: &mut GatewayUpstreamProvider,
    class: FailureClass,
    reason: &str,
    at: u64,
) -> bool {
    let counts = matches!(
        class,
        FailureClass::Retryable | FailureClass::DisableImmediately
    );
    if !counts {
        return false;
    }
    provider.consecutive_failures = provider.consecutive_failures.saturating_add(1);
    provider.last_error_at = Some(at);
    let should_disable = class == FailureClass::DisableImmediately
        || provider.consecutive_failures >= FAILURE_THRESHOLD;
    if should_disable && !provider.auto_disabled {
        provider.auto_disabled = true;
        provider.disabled_reason = Some(reason.to_string());
        provider.disabled_at = Some(at);
    }
    should_disable
}

/// A successful attempt resets the consecutive failure counter.
pub(in crate::api_gateway) fn register_success(provider: &mut GatewayUpstreamProvider) {
    provider.consecutive_failures = 0;
    provider.last_error_at = None;
}

/// Manual re-enable clears only the auto-disabled runtime state; user intent is untouched.
pub(in crate::api_gateway) fn manual_reenable(provider: &mut GatewayUpstreamProvider) {
    provider.auto_disabled = false;
    provider.disabled_reason = None;
    provider.disabled_at = None;
    provider.consecutive_failures = 0;
    provider.last_error_at = None;
}

/// User toggle touches only the `enabled` intent flag.
pub(in crate::api_gateway) fn set_user_enabled(
    provider: &mut GatewayUpstreamProvider,
    enabled: bool,
) {
    provider.enabled = enabled;
}
