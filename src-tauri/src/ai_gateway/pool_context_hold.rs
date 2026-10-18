//! Same-turn model-pool continuation preference.
//!
//! Within one recognized user turn a model pool keeps sending a recognized tool
//! continuation to the candidate that successfully delivered the preceding tool
//! call, instead of advancing the ordinary per-request weighted rotation. This
//! module owns only the pool-local continuity decision:
//!
//! - a bounded, process-memory store of retained request context and the tool
//!   identities observed in the delivered response;
//! - a direct structural prefix and tool-identity comparison that never hashes
//!   the retained context;
//! - a monotonic-clock expiry and payload budget so no request body is ever
//!   persisted or truncated;
//! - a monotonic admission token that stops an older overlapping request from
//!   publishing over a newer admitted turn in the same scope.
//!
//! Candidate eligibility, weighted ordering, probes, key rotation, protocol
//! conversion and transport stay in their existing modules. The store keeps the
//! matched local-key *identity* rather than its credential, and every request
//! that cannot supply the required scope or evidence follows normal routing with
//! no new error.

use super::model_pools::pool_candidate_cooling;
use super::selection::{resolve_session_id, select_usable_key};
use super::{now_ts, GatewayConfig, GatewayUpstreamProvider, PoolCandidateIdentity, UpstreamProtocol};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::Instant;

/// Idle time after which a completed hold is treated as absent. A hold idle for
/// exactly this long stays eligible; only a strictly longer idle interval
/// expires it. Recency is measured on a monotonic clock, so the boundary can be
/// asserted deterministically without waiting on real time.
const HOLD_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Maximum retained holds; the least recently used completed entry is evicted
/// when inserting a new hold would exceed the cap.
const HOLD_CAPACITY: usize = 1024;

/// Maximum retained serialized payload of one hold entry.
const HOLD_MAX_ENTRY_BYTES: usize = 1024 * 1024;

/// Maximum retained serialized payload across every hold entry.
const HOLD_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;

/// Identity of one pool candidate for continuity comparison: the pool, the real
/// provider, the original local/upstream mapping pair and the effective upstream
/// protocol. The pool-only native reasoning control is deliberately excluded
/// because it is resolved per request and never changes which source is meant.
#[derive(Debug, Clone)]
struct CandidateIdentity {
    pool_id: String,
    provider_id: String,
    local_model: String,
    upstream_model: String,
    protocol: UpstreamProtocol,
}

impl CandidateIdentity {
    fn from_pool_candidate(identity: &PoolCandidateIdentity) -> Self {
        Self {
            pool_id: identity.pool_id.trim().to_string(),
            provider_id: identity.provider_id.trim().to_string(),
            local_model: identity.local_model.trim().to_string(),
            upstream_model: identity.upstream_model.trim().to_string(),
            protocol: identity.protocol,
        }
    }
}

impl PartialEq for CandidateIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.pool_id == other.pool_id
            && self.provider_id == other.provider_id
            && self.local_model == other.local_model
            && self.upstream_model == other.upstream_model
            && self.protocol == other.protocol
    }
}

impl Eq for CandidateIdentity {}

impl Hash for CandidateIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pool_id.hash(state);
        self.provider_id.hash(state);
        self.local_model.hash(state);
        self.upstream_model.hash(state);
        self.protocol.endpoint_path().hash(state);
    }
}

/// The scope a continuation preference is confined to. The matched enabled
/// local-key identity, resolved session, supplied thread identity, pool,
/// public alias and ingress protocol all participate, so a colliding history in
/// any other scope can never inherit a hold. A request without a recognized
/// session header produces no scope and keeps ordinary routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::ai_gateway) struct HoldScope {
    key_id: String,
    session: String,
    thread: Option<String>,
    pool_id: String,
    alias: String,
    ingress: &'static str,
}

/// The caller-protocol request facts a continuation is compared against: the
/// caller's own history plus the instruction/tool context it was sent with.
pub(in crate::ai_gateway) struct CallerHistory {
    history: Value,
    context: Value,
}

impl CallerHistory {
    /// Parse the caller-protocol history and instruction/tool context of one
    /// request body. Returns `None` when the body carries no history array for
    /// its ingress (for example a Responses string input), so the request keeps
    /// ordinary routing. Only already-supported representations are inspected;
    /// the forwarded request is never rewritten.
    pub(in crate::ai_gateway) fn parse(path: &str, body: &Value) -> Option<Self> {
        let responses = path.split('?').next().unwrap_or(path) == "/v1/responses";
        let key = if responses { "input" } else { "messages" };
        let array = body.get(key)?.as_array()?;
        let mut context = serde_json::Map::new();
        if let Some(tools) = body.get("tools") {
            context.insert("tools".to_string(), tools.clone());
        }
        if responses {
            if let Some(instructions) = body.get("instructions") {
                context.insert("instructions".to_string(), instructions.clone());
            }
        }
        Some(Self {
            history: Value::Array(array.clone()),
            context: Value::Object(context),
        })
    }
}

/// One completed hold: the request snapshot, instruction/tool context and tool
/// identities that a later continuation must match, plus the exact candidate
/// identity and resolved endpoint that delivered it.
struct HoldEntry {
    history: Value,
    context: Value,
    tool_ids: Vec<String>,
    identity: CandidateIdentity,
    endpoint: String,
    last_used: Instant,
    bytes: usize,
}

/// Process-memory hold table plus the per-scope latest-admission map. Nothing is
/// ever written to disk, emitted to logs or read back after a restart, so a new
/// process begins without holds.
#[derive(Default)]
struct HoldStore {
    entries: HashMap<HoldScope, HoldEntry>,
    admissions: HashMap<HoldScope, u64>,
    next_admission: u64,
    total_bytes: usize,
}

impl HoldStore {
    /// Drop every hold idle for strictly more than the timeout.
    fn prune_expired(&mut self, now: Instant) {
        let mut removed = 0usize;
        self.entries.retain(|_, entry| {
            let live = now.duration_since(entry.last_used) <= HOLD_IDLE_TIMEOUT;
            if !live {
                removed = removed.saturating_add(entry.bytes);
            }
            live
        });
        self.total_bytes = self.total_bytes.saturating_sub(removed);
    }

    /// Admit one request as the newest turn of its scope and return its token.
    fn admit(&mut self, scope: &HoldScope) -> u64 {
        let token = self.next_admission;
        self.next_admission = self.next_admission.wrapping_add(1);
        self.admissions.insert(scope.clone(), token);
        if self.admissions.len() > HOLD_CAPACITY {
            let excess = self.admissions.len() - HOLD_CAPACITY;
            let mut tokens: Vec<(u64, HoldScope)> = self
                .admissions
                .iter()
                .map(|(scope, token)| (*token, scope.clone()))
                .collect();
            tokens.sort_by_key(|(token, _)| *token);
            for (_, scope) in tokens.into_iter().take(excess) {
                self.admissions.remove(&scope);
            }
        }
        token
    }

    fn remove(&mut self, scope: &HoldScope) {
        if let Some(entry) = self.entries.remove(scope) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.bytes);
        }
    }

    /// Store a completed hold, replacing any previous entry for the scope, then
    /// evict least recently used entries until both budgets hold.
    fn insert_completed(&mut self, scope: HoldScope, entry: HoldEntry) {
        if let Some(previous) = self.entries.remove(&scope) {
            self.total_bytes = self.total_bytes.saturating_sub(previous.bytes);
        }
        self.total_bytes = self.total_bytes.saturating_add(entry.bytes);
        self.entries.insert(scope, entry);
        while self.entries.len() > HOLD_CAPACITY {
            self.evict_least_recently_used();
        }
        while self.total_bytes > HOLD_MAX_TOTAL_BYTES && !self.entries.is_empty() {
            self.evict_least_recently_used();
        }
    }

    fn evict_least_recently_used(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(scope, _)| scope.clone());
        if let Some(scope) = oldest {
            if let Some(entry) = self.entries.remove(&scope) {
                self.total_bytes = self.total_bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

static POOL_HOLDS: OnceLock<Mutex<HoldStore>> = OnceLock::new();

fn hold_store() -> &'static Mutex<HoldStore> {
    POOL_HOLDS.get_or_init(|| Mutex::new(HoldStore::default()))
}

/// Resolve this request's hold scope: the first configured enabled local key
/// matching either accepted credential header (identity only, never the
/// credential), the resolved session, the supplied nonblank `thread-id`, the
/// pool, its public alias and the ingress protocol. Returns `None` when the
/// model is not a current pool or no recognized session header is present.
pub(in crate::ai_gateway) fn resolve_scope(
    headers: &HashMap<String, String>,
    config: &GatewayConfig,
    alias: &str,
    protocol: UpstreamProtocol,
) -> Option<HoldScope> {
    let alias = alias.trim();
    if alias.is_empty() {
        return None;
    }
    let pool = config
        .model_pools
        .iter()
        .find(|pool| pool.model.trim() == alias)?;
    let session = resolve_session_id(headers)?;
    let key_id = matched_key_id(headers, config)?;
    let thread = headers
        .get("thread-id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(HoldScope {
        key_id,
        session,
        thread,
        pool_id: pool.id.trim().to_string(),
        alias: alias.to_string(),
        ingress: protocol.endpoint_path(),
    })
}

/// The identity of the first configured enabled key that matches either accepted
/// credential header, mirroring authorization acceptance without retaining or
/// returning the credential value.
fn matched_key_id(headers: &HashMap<String, String>, config: &GatewayConfig) -> Option<String> {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.trim().strip_prefix("Bearer "))
        .map(str::trim);
    let x_api_key = headers.get("x-api-key").map(|value| value.trim());
    config
        .keys
        .iter()
        .filter(|key| key.enabled && !key.value.trim().is_empty())
        .find(|key| {
            let value = key.value.trim();
            bearer == Some(value) || x_api_key == Some(value)
        })
        .map(|key| key.id.trim().to_string())
}

/// Admit one request as the newest turn of its scope and return its monotonic
/// admission token.
pub(in crate::ai_gateway) fn admit(scope: &HoldScope) -> u64 {
    hold_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .admit(scope)
}

/// Reorder a currently healthy held candidate to the front of `ordered` when the
/// incoming request is a recognized continuation of this scope, preserving the
/// relative order of the remaining candidates. The list is returned unchanged
/// for a missing, expired, non-matching or no-longer-eligible hold, so a hold
/// never adds, drops, widens or promotes an unavailable candidate.
pub(in crate::ai_gateway) fn prefer(
    config: &GatewayConfig,
    scope: &HoldScope,
    caller: &CallerHistory,
    ordered: &mut Vec<GatewayUpstreamProvider>,
) {
    if ordered.is_empty() {
        return;
    }
    let now = Instant::now();
    let mut store = hold_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    store.prune_expired(now);
    let Some(entry) = store.entries.get(scope) else {
        return;
    };
    let identity = entry.identity.clone();
    let endpoint = entry.endpoint.clone();
    if !is_continuation(&entry.history, &entry.context, &entry.tool_ids, caller) {
        return;
    }
    if !candidate_is_currently_healthy(config, &identity, &endpoint) {
        return;
    }
    let Some(index) = ordered
        .iter()
        .position(|provider| matches_candidate(provider, &identity, &endpoint))
    else {
        return;
    };
    if index != 0 {
        let candidate = ordered.remove(index);
        ordered.insert(0, candidate);
    }
    // Refresh recency only after a real continuation match, so unrelated scope
    // traffic never extends an idle hold's lifetime.
    if let Some(entry) = store.entries.get_mut(scope) {
        entry.last_used = now;
    }
}

/// Publish, replace or close the continuation preference after a delivered pool
/// outcome. The call is a no-op unless the caller holds the latest admission
/// token of its scope, so an older overlapping request can never publish over a
/// newer admitted turn. A response with no recognizable tool identity closes the
/// hold, and an over-budget payload is never retained.
pub(in crate::ai_gateway) fn record(
    scope: &HoldScope,
    admission: u64,
    caller: &CallerHistory,
    identity: &PoolCandidateIdentity,
    endpoint: &str,
    tool_ids: &[String],
) {
    let now = Instant::now();
    let mut store = hold_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    store.prune_expired(now);
    if store.admissions.get(scope).copied() != Some(admission) {
        return;
    }
    if tool_ids.is_empty() {
        store.remove(scope);
        return;
    }
    let identity = CandidateIdentity::from_pool_candidate(identity);
    let bytes = entry_bytes(caller, &identity, endpoint, tool_ids);
    if bytes > HOLD_MAX_ENTRY_BYTES {
        // Over-budget evidence cannot be retained, and no stale hold is kept.
        store.remove(scope);
        return;
    }
    let entry = HoldEntry {
        history: caller.history.clone(),
        context: caller.context.clone(),
        tool_ids: tool_ids.to_vec(),
        identity,
        endpoint: endpoint.to_string(),
        last_used: now,
        bytes,
    };
    store.insert_completed(scope.clone(), entry);
}

/// Tool-call identities of a successfully delivered caller-protocol response,
/// in response order. Chat reads `choices[].message.tool_calls[].id`; Responses
/// reads `output[]` items whose `type` is `function_call` and uses their
/// `call_id`. Absent, non-string or blank identities are ignored and never count
/// as tool evidence, so a completion without a recognizable tool call closes the
/// hold.
pub(in crate::ai_gateway) fn tool_identities(path: &str, body: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    if path.split('?').next().unwrap_or(path) == "/v1/responses" {
        if let Some(output) = body.get("output").and_then(Value::as_array) {
            for item in output {
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    if let Some(id) = nonblank(item.get("call_id")) {
                        ids.push(id);
                    }
                }
            }
        }
        return ids;
    }
    if let Some(choices) = body.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let Some(calls) = choice
                .get("message")
                .and_then(|message| message.get("tool_calls"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for call in calls {
                if let Some(id) = nonblank(call.get("id")) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

fn nonblank(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Whether the incoming request is a recognized continuation of the retained
/// hold: unchanged history as a structural prefix, identical instruction/tool
/// context, no appended user message, and appended assistant/function-call and
/// tool-result facts whose call identities all match the tool identities the
/// preceding delivered response reported, with at least one tool result.
fn is_continuation(
    retained_history: &Value,
    retained_context: &Value,
    retained_ids: &[String],
    caller: &CallerHistory,
) -> bool {
    if retained_context != &caller.context {
        return false;
    }
    let (Some(retained), Some(incoming)) =
        (retained_history.as_array(), caller.history.as_array())
    else {
        return false;
    };
    if incoming.len() <= retained.len() {
        return false;
    }
    if incoming[..retained.len()] != retained[..] {
        return false;
    }
    let suffix = &incoming[retained.len()..];
    let mut saw_result = false;
    for item in suffix {
        if item.get("role").and_then(Value::as_str) == Some("user") {
            return false;
        }
        let Some((ids, is_result)) = item_tool_facts(item) else {
            // An unrecognized appended item makes the history ambiguous.
            return false;
        };
        for id in ids {
            if id.is_empty() || !retained_ids.iter().any(|retained| retained == &id) {
                return false;
            }
        }
        if is_result {
            saw_result = true;
        }
    }
    saw_result
}

/// The call identities one appended history item carries and whether it is a
/// tool result. Chat reads `role: tool` results and `role: assistant`
/// `tool_calls`; Responses reads `function_call` and `function_call_output`
/// items by `call_id`. Anything else returns `None`.
fn item_tool_facts(item: &Value) -> Option<(Vec<String>, bool)> {
    if let Some(role) = item.get("role").and_then(Value::as_str) {
        match role {
            "tool" => {
                let id = item
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                return Some((vec![id], true));
            }
            "assistant" => {
                let calls = item.get("tool_calls").and_then(Value::as_array)?;
                let ids = calls
                    .iter()
                    .map(|call| {
                        call.get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    })
                    .collect();
                return Some((ids, false));
            }
            _ => return None,
        }
    }
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => Some((
            vec![item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()],
            false,
        )),
        Some("function_call_output") => Some((
            vec![item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()],
            true,
        )),
        _ => None,
    }
}

/// Whether one prepared pool candidate is the held source: same complete
/// identity and same resolved endpoint, so a changed source mapping or endpoint
/// invalidates the preference.
fn matches_candidate(
    provider: &GatewayUpstreamProvider,
    identity: &CandidateIdentity,
    endpoint: &str,
) -> bool {
    provider.base_url == endpoint
        && provider
            .pool_candidate
            .as_ref()
            .is_some_and(|candidate| CandidateIdentity::from_pool_candidate(candidate) == *identity)
}

/// Re-check current eligibility of a held candidate through existing resolution:
/// the provider and its mapping row are enabled, the mapping still resolves to
/// the same effective upstream protocol, a usable key exists, the endpoint is
/// unchanged and the candidate is not cooling.
fn candidate_is_currently_healthy(
    config: &GatewayConfig,
    identity: &CandidateIdentity,
    endpoint: &str,
) -> bool {
    let Some(provider) = config
        .providers
        .iter()
        .find(|provider| provider.id == identity.provider_id)
    else {
        return false;
    };
    if !provider.enabled || provider.base_url != endpoint {
        return false;
    }
    let Some(mapping) = provider.mappings.iter().find(|mapping| {
        mapping.local_model.trim() == identity.local_model
            && mapping.upstream_model.trim() == identity.upstream_model
    }) else {
        return false;
    };
    if !mapping.enabled || mapping.auto_disabled {
        return false;
    }
    if mapping.effective_protocol(provider.protocol) != identity.protocol {
        return false;
    }
    if select_usable_key(provider, now_ts(), &HashSet::new()).is_none() {
        return false;
    }
    let full = PoolCandidateIdentity {
        pool_id: identity.pool_id.clone(),
        provider_id: identity.provider_id.clone(),
        local_model: identity.local_model.clone(),
        upstream_model: identity.upstream_model.clone(),
        protocol: identity.protocol,
        reasoning: None,
    };
    pool_candidate_cooling(&full).is_none()
}

/// The retained serialized payload of one hold entry, including identity, the
/// instruction/tool context and the observed tool identities.
fn entry_bytes(
    caller: &CallerHistory,
    identity: &CandidateIdentity,
    endpoint: &str,
    tool_ids: &[String],
) -> usize {
    let history = serde_json::to_vec(&caller.history)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    let context = serde_json::to_vec(&caller.context)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    let tools = tool_ids.iter().map(|id| id.len() + 16).sum::<usize>();
    history
        + context
        + tools
        + identity.provider_id.len()
        + identity.local_model.len()
        + identity.upstream_model.len()
        + endpoint.len()
        + 64
}
