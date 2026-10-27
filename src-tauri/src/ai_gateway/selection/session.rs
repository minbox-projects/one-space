use super::super::GatewayUpstreamProvider;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::Instant;

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
    resolve_session_id_with_custom(headers, &[])
}

pub(in crate::ai_gateway) fn resolve_session_id_with_custom(
    headers: &HashMap<String, String>,
    custom_headers: &[String],
) -> Option<String> {
    for header in custom_headers {
        let key = header.trim().to_ascii_lowercase();
        if let Some(val) = headers.get(&key).map(String::as_str).map(str::trim).filter(|v| !v.is_empty()) {
            return Some(val.to_string());
        }
    }
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
