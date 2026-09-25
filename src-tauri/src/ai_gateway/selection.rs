use super::{
    GatewayUpstreamProvider, KeyFailureKind, ModelMapping, UpstreamKey, UpstreamProtocol,
    AUTO_DISABLE_PROBE_COOLDOWN_SECS, FAILURE_THRESHOLD, KEY_PROBE_COOLDOWN_SECS,
};
#[cfg(test)]
use rand::seq::SliceRandom;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::Instant;

/// Outcome class for an upstream attempt, driving switching and auto-disable decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) enum FailureClass {
    /// Auth failures (401/403) disable the provider immediately and switch.
    DisableImmediately,
    /// Counts toward consecutive failures; switches and auto-disables at the threshold.
    /// Quota-exhausted 429s are classified here (via
    /// [`classify_failure_with_message`); plain rate-limit 429s stay [`FailureClass::Transient`].
    Retryable,
    /// Switches without counting as a failure (404 / rate-limit 429).
    Transient,
    /// Returns the upstream error to the caller without switching or disabling (400/422/other 4xx).
    ReturnToClient,
}

/// Outcome of resolving a request model against one provider for an inbound protocol.
#[derive(Debug)]
pub(in crate::ai_gateway) enum ModelResolution {
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
/// Disabled and auto-disabled rows never serve and never cause a protocol
/// mismatch, but a request that matches only such rows is a `NoMatch` and cannot
/// fall back to the default model. An enabled matching row is served with its own
/// remote model only when the row's effective protocol (its own declaration, else
/// the provider protocol) equals `protocol`; when enabled matching rows exist but
/// none matches, the request is a `ProtocolMismatch` and the default model is
/// not used as a fallback. The default model serves an unmapped model only when
/// the provider protocol itself matches.
pub(in crate::ai_gateway) fn resolve_model_for_protocol(
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
            // A disabled or auto-disabled row never serves and never produces a
            // protocol mismatch; it only records that the requested model is
            // mapped but switched off, which later blocks the default-model
            // fallback.
            if !mapping.enabled || mapping.auto_disabled {
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

/// Candidate set: enabled and able to serve the request model under the inbound
/// protocol. A provider is not filtered by its own protocol alone, because a
/// mapping row may declare the inbound protocol. The provider-level
/// `auto_disabled` field is legacy and never filters; only an auto-disabled row
/// removes the model it maps.
pub(in crate::ai_gateway) fn candidate_providers<'a>(
    providers: &'a [GatewayUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> Vec<&'a GatewayUpstreamProvider> {
    providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && matches!(
                    resolve_model_for_protocol(provider, requested, protocol),
                    ModelResolution::Serve(_)
                )
        })
        .collect()
}

/// Uniformly shuffle a copy of the candidate list for one request attempt pass.
/// Test-only: production candidate ordering goes through [`weighted_candidates`].
#[cfg(test)]
pub(in crate::ai_gateway) fn shuffled_candidates(
    candidates: &[GatewayUpstreamProvider],
) -> Vec<GatewayUpstreamProvider> {
    let mut ordered = candidates.to_vec();
    ordered.shuffle(&mut rand::thread_rng());
    ordered
}

static WEIGHTED_SCHEDULER: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

fn weighted_scheduler() -> &'static Mutex<HashMap<String, i64>> {
    WEIGHTED_SCHEDULER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reset the global weighted round-robin scheduler state for tests.
#[cfg(test)]
pub(in crate::ai_gateway) fn reset_weighted_scheduler_for_test() {
    let mut map = weighted_scheduler().lock().unwrap_or_else(|e| e.into_inner());
    map.clear();
}

/// Smooth Weighted Round-Robin (SWRR) candidate scheduling.
///
/// Returns all candidates ordered with the SWRR primary candidate at index 0,
/// followed by remaining candidates sorted descending by updated current_weight
/// (with provider ID ascending as tie-breaker).
pub(in crate::ai_gateway) fn weighted_candidates(
    candidates: &[GatewayUpstreamProvider],
) -> Vec<GatewayUpstreamProvider> {
    if candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() == 1 {
        return candidates.to_vec();
    }
    let mut map = weighted_scheduler().lock().unwrap_or_else(|e| e.into_inner());
    let total_weight: i64 = candidates.iter().map(|c| c.weight.max(1) as i64).sum();

    // 1. current_weight += effective_weight
    for c in candidates {
        let cw = map.entry(c.id.clone()).or_insert(0);
        *cw += c.weight.max(1) as i64;
    }

    // 2. 选择 current_weight 最大的候选，平手按 provider.id 升序决胜
    let mut best_idx = 0;
    let mut best_val = (
        map.get(&candidates[0].id).copied().unwrap_or(0),
        Reverse(&candidates[0].id),
    );
    for (idx, c) in candidates.iter().enumerate().skip(1) {
        let val = (
            map.get(&c.id).copied().unwrap_or(0),
            Reverse(&c.id),
        );
        if val > best_val {
            best_val = val;
            best_idx = idx;
        }
    }

    // 3. 扣减选中者的 total_weight
    if let Some(cw) = map.get_mut(&candidates[best_idx].id) {
        *cw -= total_weight;
    }

    // 4. 剩余候选按更新后的 current_weight 降序排列（平手按 id 升序）
    let mut remaining: Vec<(usize, &GatewayUpstreamProvider)> = candidates
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != best_idx)
        .collect();

    remaining.sort_by(|(_, a), (_, b)| {
        let wa = map.get(&a.id).copied().unwrap_or(0);
        let wb = map.get(&b.id).copied().unwrap_or(0);
        wb.cmp(&wa).then_with(|| a.id.cmp(&b.id))
    });

    // 5. 组合主选 + 降序降级序列
    let mut result = Vec::with_capacity(candidates.len());
    result.push(candidates[best_idx].clone());
    for (_, c) in remaining {
        result.push((*c).clone());
    }
    result
}


/// Client headers that may carry a session identity, highest precedence first.
/// The inbound header map is already lower-cased before it is looked up.
pub(in crate::ai_gateway) const SESSION_ID_HEADERS: [&str; 7] = [
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
pub(in crate::ai_gateway) const SESSION_BINDING_IDLE_TIMEOUT: Duration =
    Duration::from_secs(30 * 60);

/// Maximum live bindings; the least recently used entry is evicted first.
pub(in crate::ai_gateway) const SESSION_BINDING_CAPACITY: usize = 1024;

/// Consecutive misses after which a binding migrates to the serving provider.
pub(in crate::ai_gateway) const SESSION_BINDING_MISS_THRESHOLD: u32 = 2;

/// Resolve the session identity as the first header of [`SESSION_ID_HEADERS`]
/// present with a non-empty trimmed value. A header whose value is empty or
/// whitespace-only counts as absent and the walk continues, and headers outside
/// the list are never consulted.
pub(in crate::ai_gateway) fn resolve_session_id(
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
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ai_gateway) struct SessionBinding {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) misses: u32,
}

/// A request's candidate order together with the binding that ordered it.
#[derive(Debug)]
pub(in crate::ai_gateway) struct SessionOrder {
    pub(in crate::ai_gateway) ordered: Vec<GatewayUpstreamProvider>,
    pub(in crate::ai_gateway) bound_provider_id: Option<String>,
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
pub(in crate::ai_gateway) struct SessionAffinityStore {
    entries: HashMap<(String, String), SessionBindingEntry>,
}

impl SessionAffinityStore {
    pub(in crate::ai_gateway) fn new() -> Self {
        Self::default()
    }

    /// Read a live binding and refresh its recency; an entry idle for more than
    /// the timeout is dropped and reported as absent.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn lookup(
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
    pub(in crate::ai_gateway) fn resolve_order<F>(
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
    pub(in crate::ai_gateway) fn settle(
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
    pub(in crate::ai_gateway) fn live_len(&mut self) -> usize {
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
pub(in crate::ai_gateway) fn reorder_bound_first(
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
pub(in crate::ai_gateway) fn session_affinity() -> &'static Mutex<SessionAffinityStore> {
    SESSION_AFFINITY.get_or_init(|| Mutex::new(SessionAffinityStore::new()))
}

/// Drop every binding of the process-global table (test-only reset; used by the
/// behavior tests that touch the global store).
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn reset_session_affinity_for_test() {
    *session_affinity()
        .lock()
        .expect("session affinity store lock") = SessionAffinityStore::new();
}

/// Choose a single candidate uniformly at random (used by coverage-sensitive tests).
#[cfg(test)]
pub(in crate::ai_gateway) fn pick_candidate(
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
pub(in crate::ai_gateway) fn classify_failure(
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
        // A bare 429 is a transient rate limit: retried by the scheduler but
        // never counts toward provider health. Quota-exhausted 429s are
        // upgraded to Retryable by classify_failure_with_message.
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
pub(in crate::ai_gateway) const MAX_RETRIES_PER_PROVIDER: u32 = 5;

const RETRY_BASE_DELAY_MILLIS: u128 = 2_000;
const RETRY_MAX_DELAY_MILLIS: u128 = 30_000;
const RETRY_JITTER_RATIO: f64 = 0.25;

/// Default delay before the `retry`-th retry (1-based): the spec's
/// `min(2000ms * 2^(retry-1) * (1 + random[0,1]*0.25), 30000ms)`. Header-driven
/// overrides take priority over this.
pub(in crate::ai_gateway) fn default_retry_delay(retry: u32) -> Duration {
    let exponent = retry.saturating_sub(1).min(20);
    let base = RETRY_BASE_DELAY_MILLIS.saturating_mul(1u128 << exponent);
    let jitter = 1.0 + rand::random::<f64>() * RETRY_JITTER_RATIO;
    let millis = (base as f64 * jitter).min(RETRY_MAX_DELAY_MILLIS as f64) as u64;
    Duration::from_millis(millis)
}

/// Read the first value of each header, falling through invalid values.
pub(in crate::ai_gateway) fn retry_header_delay(
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

/// Whether an upstream 429 error message describes quota exhaustion rather than
/// a transient rate limit.
///
/// Quota signals (case-insensitive): `quota`, `billing`, `insufficient`,
/// `usage limit`, weekly/monthly period limits, or an `upgrade plan` prompt.
/// A bare `limit`/`exceeded` (e.g. `Rate limit exceeded`) is NOT quota: it
/// stays a transient rate limit so ordinary throttling never disables a
/// mapping row.
pub(in crate::ai_gateway) fn is_quota_exceeded_message(message: Option<&str>) -> bool {
    let Some(message) = message.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    if lower.contains("quota")
        || lower.contains("billing")
        || lower.contains("insufficient")
        || lower.contains("usage limit")
        || lower.contains("usage_limit")
        || lower.contains("weekly")
        || lower.contains("monthly")
        || lower.contains("upgrade your plan")
        || lower.contains("upgrade plan")
    {
        return true;
    }
    if lower.contains("balance")
        && (lower.contains("exceed")
            || lower.contains("insufficient")
            || lower.contains("limit")
            || lower.contains("deplet")
            || lower.contains("empty")
            || lower.contains("zero"))
    {
        return true;
    }
    if lower.contains("plan")
        && (lower.contains("limit")
            || lower.contains("usage")
            || lower.contains("reset")
            || lower.contains("quota")
            || lower.contains("upgrade"))
    {
        return true;
    }
    false
}

/// Classify an upstream failure with the sanitized error text available.
///
/// A 429 whose message matches [`is_quota_exceeded_message`] is `Retryable`
/// (counts toward mapping health); any other 429 stays `Transient`.
/// Everything else delegates to [`classify_failure`].
pub(in crate::ai_gateway) fn classify_failure_with_message(
    status: u16,
    network_error: bool,
    body_parsed: bool,
    error_message: Option<&str>,
) -> FailureClass {
    if status == 429 && !network_error && is_quota_exceeded_message(error_message) {
        return FailureClass::Retryable;
    }
    classify_failure(status, network_error, body_parsed)
}

/// Whether a classified failure is worth another bounded attempt on the same
/// provider. `404` is a permanent per-request skip and
/// `ReturnToClient`/`DisableImmediately` never retry; a plain rate-limit 429
/// (classified `Transient`) is still retried. A quota-exhausted 429 is
/// classified `Retryable` for health counting but must NOT be retried on the
/// same provider (its quota will not recover inside the request budget), so
/// callers pass the same error text here to suppress the same-provider retry
/// while keeping the health count.
pub(in crate::ai_gateway) fn is_retryable_failure(class: FailureClass, status: u16) -> bool {
    match class {
        FailureClass::Retryable => true,
        FailureClass::Transient => status == 429,
        FailureClass::DisableImmediately | FailureClass::ReturnToClient => false,
    }
}

/// Quota-aware variant of [`is_retryable_failure`]: quota-exhausted 429s count
/// toward health (via their `Retryable` class) but never requeue the same
/// provider.
pub(in crate::ai_gateway) fn is_retryable_with_message(
    class: FailureClass,
    status: u16,
    error_message: Option<&str>,
) -> bool {
    if status == 429 && is_quota_exceeded_message(error_message) {
        return false;
    }
    is_retryable_failure(class, status)
}

/// Identity of one mapping row: the provider it belongs to plus its trimmed
/// `(local_model, upstream_model)` key.
///
/// Runtime health settles per row, so a failure of one row never touches a
/// sibling row or the provider's own legacy state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::ai_gateway) struct MappingTarget {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) local_model: String,
    pub(in crate::ai_gateway) upstream_model: String,
}

impl MappingTarget {
    /// Build a target, trimming every component.
    pub(in crate::ai_gateway) fn new(
        provider_id: &str,
        local_model: &str,
        upstream_model: &str,
    ) -> Self {
        Self {
            provider_id: provider_id.trim().to_string(),
            local_model: local_model.trim().to_string(),
            upstream_model: upstream_model.trim().to_string(),
        }
    }

    /// The row a served request settles on when it resolved through a mapping
    /// row rather than the provider's `default_model`.
    ///
    /// Returns `None` unless the trimmed requested model is non-empty, matches a
    /// row's trimmed `local_model` and that row's trimmed `upstream_model` equals
    /// the resolved upstream model. A `default_model` attempt therefore settles
    /// on no row and records no health outcome.
    pub(in crate::ai_gateway) fn for_request(
        provider: &GatewayUpstreamProvider,
        requested: Option<&str>,
        upstream_model: &str,
    ) -> Option<Self> {
        let requested = requested.map(str::trim).filter(|value| !value.is_empty())?;
        let upstream_model = upstream_model.trim();
        provider
            .mappings
            .iter()
            .find(|mapping| {
                mapping.local_model.trim() == requested
                    && mapping.upstream_model.trim() == upstream_model
            })
            .map(|_| Self::new(&provider.id, requested, upstream_model))
    }
}

/// One eligible half-open probe: the provider whose auto-disabled row may be
/// retried, the mapping row identity it settles on, and the upstream model to
/// forward. Returned by [`find_probe_candidate`].
#[derive(Debug, Clone)]
pub(in crate::ai_gateway) struct ProbeCandidate {
    pub(in crate::ai_gateway) provider: GatewayUpstreamProvider,
    pub(in crate::ai_gateway) target: MappingTarget,
    pub(in crate::ai_gateway) upstream_model: String,
}

/// Whether a row's trimmed `disabled_reason` denotes an immediate auth disable
/// (401/403) that must stay manual-only. Rows persisted by older builds could
/// have reached the threshold counter under the previous counting rule, so the
/// reason prefix is the discriminator for them; no threshold disable produces
/// this prefix.
fn is_immediate_auth_disable_reason(reason: Option<&str>) -> bool {
    let reason = reason.unwrap_or("").trim();
    reason.starts_with("HTTP 401") || reason.starts_with("HTTP 403")
}

/// Find at most one eligible half-open probe candidate for a request.
///
/// A row is eligible only when its provider and the row are both enabled, the
/// row is auto-disabled by a transient-failure threshold (`consecutive_failures
/// >= FAILURE_THRESHOLD`) with a `disabled_reason` that is not an immediate auth
/// disable, it has a non-empty trimmed `upstream_model`, its trimmed
/// `local_model` equals the requested model, its effective protocol equals the
/// inbound `protocol`, its `disabled_at` is present, and the cooldown
/// `now - disabled_at >= AUTO_DISABLE_PROBE_COOLDOWN_SECS` has elapsed. Rows
/// whose provider is already serving as a healthy candidate are excluded.
///
/// At most one candidate is returned: the oldest `disabled_at` first, breaking
/// ties by provider id, then `local_model`, then `upstream_model` (trimmed,
/// lexicographic). A missing or blank requested model yields no candidate.
pub(in crate::ai_gateway) fn find_probe_candidate(
    providers: &[GatewayUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
    healthy_provider_ids: &[String],
    now: u64,
) -> Option<ProbeCandidate> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty())?;
    let mut best: Option<((u64, String, String, String), ProbeCandidate)> = None;
    for provider in providers.iter().filter(|provider| provider.enabled) {
        if healthy_provider_ids
            .iter()
            .any(|id| id == &provider.id)
        {
            continue;
        }
        for mapping in provider
            .mappings
            .iter()
            .filter(|mapping| mapping.enabled && mapping.auto_disabled)
        {
            let Some(disabled_at) = mapping.disabled_at else {
                continue;
            };
            if now.saturating_sub(disabled_at) < AUTO_DISABLE_PROBE_COOLDOWN_SECS {
                continue;
            }
            if mapping.consecutive_failures < FAILURE_THRESHOLD {
                continue;
            }
            if is_immediate_auth_disable_reason(mapping.disabled_reason.as_deref()) {
                continue;
            }
            if mapping.local_model.trim() != requested {
                continue;
            }
            let upstream_model = mapping.upstream_model.trim().to_string();
            if upstream_model.is_empty() {
                continue;
            }
            if mapping.effective_protocol(provider.protocol) != protocol {
                continue;
            }
            let key = (
                disabled_at,
                provider.id.trim().to_string(),
                mapping.local_model.trim().to_string(),
                upstream_model.clone(),
            );
            let replace = match &best {
                None => true,
                Some((best_key, _)) => key < *best_key,
            };
            if replace {
                let target = MappingTarget::new(&provider.id, &mapping.local_model, &upstream_model);
                best = Some((
                    key,
                    ProbeCandidate {
                        provider: provider.clone(),
                        target,
                        upstream_model,
                    },
                ));
            }
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// Process-wide set of mapping targets with a probe currently in flight.
fn active_probes() -> &'static Mutex<HashSet<MappingTarget>> {
    static ACTIVE_PROBES: OnceLock<Mutex<HashSet<MappingTarget>>> = OnceLock::new();
    ACTIVE_PROBES.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Single-flight guard for one in-flight probe of a mapping target.
///
/// Process memory only; dropping it releases the target, including when the
/// request future is cancelled, because the release runs in [`Drop`].
pub(in crate::ai_gateway) struct ProbeGuard {
    target: MappingTarget,
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        active_probes()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.target);
    }
}

/// Try to become the single in-flight probe for `target`'s mapping key.
///
/// Returns `None` when another request already holds the guard, in which case
/// the caller must skip probing and continue on its exhausted path.
pub(in crate::ai_gateway) fn try_acquire_probe_guard(
    target: &MappingTarget,
) -> Option<ProbeGuard> {
    let mut active = active_probes()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if active.insert(target.clone()) {
        Some(ProbeGuard {
            target: target.clone(),
        })
    } else {
        None
    }
}

/// Re-arm a failed probe's cooldown: every row matching `target`'s trimmed key
/// stays auto-disabled, keeps its consecutive-failure counter, and moves
/// `disabled_at` to `at`. With `update_details` the failure also stamps
/// `last_error_at` and `disabled_reason`; a suppressed transport failure passes
/// `false` so only the cooldown moves.
pub(in crate::ai_gateway) fn rearm_mapping_probe_cooldown(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
    reason: &str,
    at: u64,
    update_details: bool,
) {
    for mapping in provider.mappings.iter_mut().filter(|mapping| {
        mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
    }) {
        mapping.auto_disabled = true;
        mapping.disabled_at = Some(at);
        if update_details {
            mapping.last_error_at = Some(at);
            mapping.disabled_reason = Some(reason.to_string());
        }
    }
}

/// Record a failure on every row matching `target`'s trimmed key. Returns `true`
/// when that key is (or becomes) auto-disabled. `Transient` and `ReturnToClient`
/// never count as failures.
pub(in crate::ai_gateway) fn register_mapping_failure(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
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
    let mut disabled = false;
    for mapping in provider.mappings.iter_mut().filter(|mapping| {
        mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
    }) {
        // Frozen Step 2 rule: an immediate auth disable records reason/at/
        // last_error_at but must not advance the transient-failure counter, so
        // the counter keeps meaning consecutive Retryable failures and a 401/403
        // row can never reach the probe threshold through the counter.
        if class == FailureClass::Retryable {
            mapping.consecutive_failures = mapping.consecutive_failures.saturating_add(1);
        }
        mapping.last_error_at = Some(at);
        let should_disable = class == FailureClass::DisableImmediately
            || mapping.consecutive_failures >= FAILURE_THRESHOLD;
        if should_disable && !mapping.auto_disabled {
            mapping.auto_disabled = true;
            mapping.disabled_reason = Some(reason.to_string());
            mapping.disabled_at = Some(at);
        }
        disabled |= mapping.auto_disabled;
    }
    disabled
}

/// A successful attempt resets the consecutive failure counter and last-error
/// value of every row matching `target`'s trimmed key.
pub(in crate::ai_gateway) fn register_mapping_success(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
) {
    for mapping in provider.mappings.iter_mut().filter(|mapping| {
        mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
    }) {
        mapping.consecutive_failures = 0;
        mapping.last_error_at = None;
    }
}

/// Clear a row's runtime health state only; the user's `enabled` intent is untouched.
pub(in crate::ai_gateway) fn clear_mapping_runtime_state(mapping: &mut ModelMapping) {
    mapping.auto_disabled = false;
    mapping.disabled_reason = None;
    mapping.disabled_at = None;
    mapping.consecutive_failures = 0;
    mapping.last_error_at = None;
}

/// Whether a row's trimmed `(local_model, upstream_model)` equals the given key.
pub(in crate::ai_gateway) fn mapping_matches_key(
    mapping: &ModelMapping,
    local_model: &str,
    upstream_model: &str,
) -> bool {
    mapping.local_model.trim() == local_model.trim()
        && mapping.upstream_model.trim() == upstream_model.trim()
}

/// Manual re-enable clears the runtime state of every auto-disabled row; the
/// user's `enabled` intent is untouched and a row that is not auto-disabled
/// keeps its counter.
pub(in crate::ai_gateway) fn manual_reenable(provider: &mut GatewayUpstreamProvider) {
    for mapping in provider.mappings.iter_mut() {
        if mapping.auto_disabled {
            clear_mapping_runtime_state(mapping);
        }
    }
}

/// User toggle touches only the `enabled` intent flag.
pub(in crate::ai_gateway) fn set_user_enabled(
    provider: &mut GatewayUpstreamProvider,
    enabled: bool,
) {
    provider.enabled = enabled;
}

/// First key usable for a normal upstream attempt: enabled and not
/// runtime-marked, in list order. A user-disabled or runtime-marked key never
/// participates in selection.
pub(in crate::ai_gateway) fn select_usable_key(
    provider: &GatewayUpstreamProvider,
) -> Option<&UpstreamKey> {
    provider
        .keys
        .iter()
        .find(|key| key.enabled && !key.auto_marked)
}

/// The single quota-marked key eligible for one half-open probe.
///
/// Eligibility requires the key to be enabled and runtime-marked with
/// [`KeyFailureKind::Quota`] at least [`KEY_PROBE_COOLDOWN_SECS`] seconds before
/// the explicit `now` (`now - marked_at >= cooldown`). Auth-marked keys are
/// never returned; a missing `marked_at` is never eligible. At most one key is
/// chosen: the oldest `marked_at` first, ties by list order.
pub(in crate::ai_gateway) fn find_key_probe_candidate(
    provider: &GatewayUpstreamProvider,
    now: u64,
) -> Option<&UpstreamKey> {
    provider
        .keys
        .iter()
        .filter(|key| key.enabled && key.auto_marked)
        .filter(|key| key.failure_kind == Some(KeyFailureKind::Quota))
        .filter(|key| {
            key.marked_at
                .is_some_and(|marked_at| now.saturating_sub(marked_at) >= KEY_PROBE_COOLDOWN_SECS)
        })
        .min_by_key(|key| key.marked_at.unwrap_or(u64::MAX))
}

/// Mark a key with a key-scoped failure: it leaves the usable pool and carries
/// the failure kind, marking time and readable reason. The user's `enabled`
/// intent is untouched.
pub(in crate::ai_gateway) fn mark_key_failure(
    key: &mut UpstreamKey,
    kind: KeyFailureKind,
    reason: &str,
    at: u64,
) {
    key.auto_marked = true;
    key.failure_kind = Some(kind);
    key.marked_at = Some(at);
    key.reason = Some(reason.to_string());
}

/// Re-arm a failed probe's cooldown: the key stays runtime-marked and moves its
/// `marked_at` to `at`. An auth failure switches the mark kind to
/// authentication; any other failure keeps the existing quota kind.
pub(in crate::ai_gateway) fn rearm_key_probe(
    key: &mut UpstreamKey,
    kind: Option<KeyFailureKind>,
    reason: &str,
    at: u64,
) {
    key.auto_marked = true;
    match kind {
        Some(kind) => key.failure_kind = Some(kind),
        None => {
            key.failure_kind.get_or_insert(KeyFailureKind::Quota);
        }
    }
    key.marked_at = Some(at);
    key.reason = Some(reason.to_string());
}

/// The key a read-only quota/usage query is pinned to: the first enabled key in
/// list order, or the first key when every key is disabled. An empty pool has no
/// pinned key. This source never follows the serving key.
pub(in crate::ai_gateway) fn pinned_key_value(
    provider: &GatewayUpstreamProvider,
) -> Option<&str> {
    provider
        .keys
        .iter()
        .find(|key| key.enabled)
        .or_else(|| provider.keys.first())
        .map(|key| key.value.as_str())
}

/// Clear a key's runtime state without touching the user's `enabled` flag.
pub(in crate::ai_gateway) fn clear_key_runtime_state(key: &mut UpstreamKey) {
    key.auto_marked = false;
    key.failure_kind = None;
    key.marked_at = None;
    key.reason = None;
}
