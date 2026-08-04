use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::{
    cmp::Ordering,
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::{
    accounts::{decrypt_api_key, decrypt_oauth_tokens},
    error::{GatewayError, GatewayErrorCategory},
    gateway_key::GatewayKeyGrant,
    quota::{evaluate_quota, load_account_windows, QuotaContext},
    security::RootKey,
    types::{AccountType, UpstreamProtocol},
};

pub(crate) const MAX_ATTEMPTS: usize = 3;
const INITIAL_COOLDOWN: Duration = Duration::from_secs(60);
const MAX_COOLDOWN: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RouteCandidate {
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) group_id: String,
    pub(crate) account_type: AccountType,
    pub(crate) base_url: String,
    pub(crate) auth_method: String,
    pub(crate) protocol: UpstreamProtocol,
    pub(crate) upstream_model: String,
    pub(crate) sort_order: i64,
    pub(crate) quota_fresh: bool,
    pub(crate) minimum_remaining_percent: Option<f64>,
    pub(crate) last_used_at: Option<String>,
    pub(crate) is_probe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptFailure {
    Authorization,
    QuotaExhausted {
        reset_after: Option<Duration>,
        scope: Option<QuotaScope>,
    },
    RateLimited {
        retry_after: Option<Duration>,
    },
    SemanticClientError,
    Network,
    Server,
    ClientCancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuotaScope {
    Global,
    Model,
    Endpoint,
    Capability,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttemptDecision {
    pub(crate) retry_different_account: bool,
    pub(crate) affects_health: bool,
    pub(crate) refresh_oauth_once: bool,
}

#[derive(Debug, Default)]
pub(crate) struct HealthTracker {
    states: Mutex<HashMap<String, HealthState>>,
}

#[derive(Debug, Clone)]
struct HealthState {
    consecutive_failures: u32,
    cooldown_level: u32,
    blocked_until: Option<Instant>,
    probe_in_flight: bool,
}

impl Default for HealthState {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            cooldown_level: 0,
            blocked_until: None,
            probe_in_flight: false,
        }
    }
}

impl HealthTracker {
    pub(crate) fn reset(&self) {
        self.states
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    fn eligibility(&self, account_id: &str, now: Instant, reserve_probe: bool) -> Eligibility {
        let mut states = self
            .states
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = states.entry(account_id.to_owned()).or_default();
        match state.blocked_until {
            Some(until) if now < until => Eligibility::Blocked,
            Some(_) if state.probe_in_flight => Eligibility::Blocked,
            Some(_) => {
                if reserve_probe {
                    state.probe_in_flight = true;
                }
                Eligibility::Probe
            }
            None => Eligibility::Available,
        }
    }

    pub(crate) fn record_success(&self, account_id: &str) {
        self.states
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(account_id.to_owned(), HealthState::default());
    }

    pub(crate) fn record_failure(&self, account_id: &str, failure: AttemptFailure, now: Instant) {
        if matches!(
            failure,
            AttemptFailure::SemanticClientError | AttemptFailure::ClientCancelled
        ) {
            return;
        }
        let mut states = self
            .states
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = states.entry(account_id.to_owned()).or_default();
        let failed_probe = state.probe_in_flight;
        state.probe_in_flight = false;
        match failure {
            AttemptFailure::RateLimited { retry_after } => {
                let delay = retry_after.unwrap_or_else(|| cooldown(state.cooldown_level));
                state.cooldown_level = state.cooldown_level.saturating_add(1);
                state.blocked_until = now.checked_add(delay);
            }
            AttemptFailure::QuotaExhausted { reset_after, .. } => {
                state.blocked_until = now.checked_add(reset_after.unwrap_or(MAX_COOLDOWN));
            }
            AttemptFailure::Authorization => {
                state.blocked_until = Some(now + MAX_COOLDOWN);
            }
            AttemptFailure::Network | AttemptFailure::Server => {
                state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                if failed_probe || state.consecutive_failures >= 3 {
                    state.cooldown_level = state.cooldown_level.saturating_add(1);
                    state.blocked_until = now.checked_add(cooldown(state.cooldown_level - 1));
                }
            }
            AttemptFailure::SemanticClientError | AttemptFailure::ClientCancelled => {}
        }
    }

    pub(crate) fn reserve_probe(&self, account_id: &str, now: Instant) -> bool {
        self.eligibility(account_id, now, true) == Eligibility::Probe
    }

    pub(crate) fn release_probe(&self, account_id: &str) {
        let mut states = self
            .states
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(state) = states.get_mut(account_id) {
            state.probe_in_flight = false;
        }
    }

    pub(crate) fn state_snapshot(&self, account_id: &str) -> Option<(u32, bool)> {
        self.states
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(account_id)
            .map(|state| (state.consecutive_failures, state.probe_in_flight))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Eligibility {
    Available,
    Probe,
    Blocked,
}

pub(crate) fn candidates(
    connection: &Connection,
    grant: &GatewayKeyGrant,
    public_model: &str,
    endpoint: &str,
    capabilities: &[&str],
    root_key: &RootKey,
    health: &HealthTracker,
    now: Instant,
) -> Result<Vec<RouteCandidate>, GatewayError> {
    candidates_with_probe_mode(
        connection,
        grant,
        public_model,
        endpoint,
        capabilities,
        root_key,
        health,
        now,
        false,
        Some(MAX_ATTEMPTS),
    )
}

fn candidates_with_probe_mode(
    connection: &Connection,
    grant: &GatewayKeyGrant,
    public_model: &str,
    endpoint: &str,
    capabilities: &[&str],
    root_key: &RootKey,
    health: &HealthTracker,
    now: Instant,
    reserve_probe: bool,
    max_attempts: Option<usize>,
) -> Result<Vec<RouteCandidate>, GatewayError> {
    if !grant.model_ids.iter().any(|model| model == public_model) {
        return Ok(Vec::new());
    }
    let global_threshold: u8 = connection
        .query_row(
            "SELECT global_quota_threshold_percent FROM ai_gateway_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| storage_error())?;
    let mut statement = connection
        .prepare(
            "SELECT account.id, account.name, account.group_id, account.account_type, account.base_url, account.auth_method, account.upstream_protocol, mapping.upstream_model_id, account.sort_order, account.last_used_at, account.quota_threshold_override_percent
             FROM ai_gateway_accounts account
             JOIN ai_gateway_credentials credential ON credential.account_id = account.id
             JOIN ai_gateway_models model ON model.id = ?1
             LEFT JOIN ai_gateway_account_model_mappings mapping ON mapping.account_id = account.id AND mapping.public_model_id = model.id
             WHERE (mapping.account_id IS NULL OR mapping.enabled = 1) AND model.enabled = 1 AND account.enabled = 1
               AND account.health_status NOT IN ('unavailable', 'authorization_invalid')",
        )
        .map_err(|_| storage_error())?;
    let rows = statement
        .query_map([public_model], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<u8>>(10)?,
            ))
        })
        .map_err(|_| storage_error())?;
    let mut output = Vec::new();
    for row in rows {
        let (
            account_id,
            account_name,
            group_id,
            account_type,
            base_url,
            auth_method,
            protocol,
            upstream_model,
            sort_order,
            last_used_at,
            threshold_override,
        ) = row.map_err(|_| storage_error())?;
        if !grant.group_ids.iter().any(|group| group == &group_id) {
            continue;
        }
        let (Some(base_url), Some(auth_method), Some(protocol)) = (base_url, auth_method, protocol)
        else {
            continue;
        };
        let credential_is_valid = if account_type == "oauth" {
            decrypt_oauth_tokens(connection, root_key, &account_id).is_ok_and(|bundle| {
                !bundle.access_token.trim().is_empty() && !bundle.refresh_token.trim().is_empty()
            })
        } else {
            decrypt_api_key(connection, root_key, &account_id)
                .ok()
                .and_then(|credential| String::from_utf8(credential).ok())
                .is_some_and(|credential| !credential.trim().is_empty())
        };
        if !credential_is_valid {
            continue;
        }
        let eligibility = health.eligibility(&account_id, now, reserve_probe);
        if eligibility == Eligibility::Blocked {
            continue;
        }
        let windows = load_account_windows(connection, &account_id)?;
        let quota = evaluate_quota(
            &windows,
            &QuotaContext {
                model: public_model,
                endpoint,
                capabilities,
            },
            global_threshold,
            threshold_override,
        );
        if !quota.available {
            continue;
        }
        output.push(RouteCandidate {
            account_id,
            account_name,
            group_id,
            account_type: if account_type == "oauth" {
                AccountType::OAuth
            } else {
                AccountType::ApiKey
            },
            base_url,
            auth_method,
            protocol: if protocol == "responses" {
                UpstreamProtocol::Responses
            } else {
                UpstreamProtocol::ChatCompletions
            },
            upstream_model: upstream_model.unwrap_or_else(|| public_model.to_owned()),
            sort_order,
            quota_fresh: quota.fresh,
            minimum_remaining_percent: quota.minimum_remaining_percent,
            last_used_at,
            is_probe: eligibility == Eligibility::Probe,
        });
    }
    output.sort_by(compare_candidates);
    if let Some(max_attempts) = max_attempts {
        output.truncate(max_attempts);
    }
    Ok(output)
}

pub(crate) fn routable_models(
    connection: &Connection,
    grant: &GatewayKeyGrant,
    root_key: &RootKey,
    health: &HealthTracker,
    now: Instant,
) -> Result<Vec<String>, GatewayError> {
    let mut models = Vec::new();
    for model in &grant.model_ids {
        if !candidates_with_probe_mode(
            connection,
            grant,
            model,
            "models",
            &[],
            root_key,
            health,
            now,
            false,
            Some(MAX_ATTEMPTS),
        )?
        .is_empty()
        {
            models.push(model.clone());
        }
    }
    models.sort();
    Ok(models)
}

pub(crate) fn available_account_ids(
    connection: &Connection,
    public_model: &str,
    root_key: &RootKey,
    health: &HealthTracker,
    now: Instant,
) -> Result<Vec<String>, GatewayError> {
    let mut statement = connection
        .prepare("SELECT id FROM ai_gateway_groups ORDER BY id")
        .map_err(|_| storage_error())?;
    let group_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| storage_error())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| storage_error())?;
    let grant = GatewayKeyGrant {
        id: "homepage".to_owned(),
        name: "Homepage".to_owned(),
        group_ids,
        model_ids: vec![public_model.to_owned()],
    };
    Ok(candidates_with_probe_mode(
        connection,
        &grant,
        public_model,
        "models",
        &[],
        root_key,
        health,
        now,
        false,
        None,
    )?
    .into_iter()
    .map(|candidate| candidate.account_id)
    .collect())
}

pub(crate) fn attempt_decision(
    account_type: AccountType,
    failure: AttemptFailure,
    emitted_client_bytes: bool,
    oauth_refresh_already_attempted: bool,
) -> AttemptDecision {
    if emitted_client_bytes || failure == AttemptFailure::ClientCancelled {
        return AttemptDecision {
            retry_different_account: false,
            affects_health: false,
            refresh_oauth_once: false,
        };
    }
    match failure {
        AttemptFailure::Authorization => AttemptDecision {
            retry_different_account: account_type == AccountType::ApiKey
                || oauth_refresh_already_attempted,
            affects_health: account_type == AccountType::ApiKey || oauth_refresh_already_attempted,
            refresh_oauth_once: account_type == AccountType::OAuth
                && !oauth_refresh_already_attempted,
        },
        AttemptFailure::SemanticClientError => AttemptDecision {
            retry_different_account: false,
            affects_health: false,
            refresh_oauth_once: false,
        },
        AttemptFailure::QuotaExhausted { .. }
        | AttemptFailure::RateLimited { .. }
        | AttemptFailure::Network
        | AttemptFailure::Server => AttemptDecision {
            retry_different_account: true,
            affects_health: true,
            refresh_oauth_once: false,
        },
        AttemptFailure::ClientCancelled => unreachable!(),
    }
}

fn compare_candidates(left: &RouteCandidate, right: &RouteCandidate) -> Ordering {
    left.sort_order
        .cmp(&right.sort_order)
        .then_with(|| right.quota_fresh.cmp(&left.quota_fresh))
        .then_with(|| {
            right
                .minimum_remaining_percent
                .partial_cmp(&left.minimum_remaining_percent)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| compare_last_used(&left.last_used_at, &right.last_used_at))
        .then_with(|| left.account_id.cmp(&right.account_id))
}

fn compare_last_used(left: &Option<String>, right: &Option<String>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => parse_time(left).cmp(&parse_time(right)),
    }
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
}

fn cooldown(level: u32) -> Duration {
    INITIAL_COOLDOWN
        .checked_mul(1u32.checked_shl(level.min(4)).unwrap_or(16))
        .unwrap_or(MAX_COOLDOWN)
        .min(MAX_COOLDOWN)
}

fn storage_error() -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai_routing_gateway::{gateway_key::GatewayKeyGrant, security::encrypt_credential},
        shared_sqlite,
    };

    fn candidate(
        id: &str,
        sort: i64,
        fresh: bool,
        remaining: f64,
        last: Option<&str>,
    ) -> RouteCandidate {
        RouteCandidate {
            account_id: id.into(),
            account_name: id.into(),
            group_id: "default".into(),
            account_type: AccountType::ApiKey,
            base_url: "http://127.0.0.1".into(),
            auth_method: "bearer".into(),
            protocol: UpstreamProtocol::Responses,
            upstream_model: "upstream".into(),
            sort_order: sort,
            quota_fresh: fresh,
            minimum_remaining_percent: Some(remaining),
            last_used_at: last.map(str::to_owned),
            is_probe: false,
        }
    }

    #[test]
    fn sorting_and_attempt_boundaries_are_deterministic() {
        let mut values = vec![
            candidate("d", 1, true, 90.0, None),
            candidate("c", 0, false, 100.0, None),
            candidate("b", 0, true, 50.0, Some("2026-08-01T00:00:00Z")),
            candidate("a", 0, true, 50.0, None),
            candidate("e", 0, true, 80.0, None),
        ];
        values.sort_by(compare_candidates);
        assert_eq!(
            values
                .iter()
                .map(|item| item.account_id.as_str())
                .collect::<Vec<_>>(),
            vec!["e", "a", "b", "c", "d"]
        );
        assert!(
            attempt_decision(AccountType::ApiKey, AttemptFailure::Network, false, false)
                .retry_different_account
        );
        assert!(
            !attempt_decision(AccountType::ApiKey, AttemptFailure::Network, true, false)
                .retry_different_account
        );
        assert!(
            attempt_decision(
                AccountType::OAuth,
                AttemptFailure::Authorization,
                false,
                false
            )
            .refresh_oauth_once
        );
        assert!(
            !attempt_decision(
                AccountType::ApiKey,
                AttemptFailure::ClientCancelled,
                false,
                false
            )
            .affects_health
        );
        assert!(
            !attempt_decision(
                AccountType::OAuth,
                AttemptFailure::Authorization,
                false,
                false
            )
            .affects_health
        );
        let second_oauth_failure = attempt_decision(
            AccountType::OAuth,
            AttemptFailure::Authorization,
            false,
            true,
        );
        assert!(second_oauth_failure.affects_health);
        assert!(second_oauth_failure.retry_different_account);
    }

    #[test]
    fn explicit_retry_after_is_not_capped_by_local_backoff_limit() {
        let tracker = HealthTracker::default();
        let now = Instant::now();
        tracker.record_failure(
            "rate-limited",
            AttemptFailure::RateLimited {
                retry_after: Some(Duration::from_secs(60 * 60)),
            },
            now,
        );
        assert_eq!(
            tracker.eligibility("rate-limited", now + Duration::from_secs(16 * 60), true),
            Eligibility::Blocked
        );
    }

    #[test]
    fn probe_reservation_is_atomic_and_can_be_released_before_execution() {
        let tracker = HealthTracker::default();
        let now = Instant::now();
        for _ in 0..3 {
            tracker.record_failure("probe", AttemptFailure::Network, now);
        }
        let later = now + INITIAL_COOLDOWN + Duration::from_millis(1);
        assert_eq!(
            tracker.eligibility("probe", later, false),
            Eligibility::Probe
        );
        assert!(tracker.reserve_probe("probe", later));
        assert!(!tracker.reserve_probe("probe", later));
        tracker.release_probe("probe");
        assert!(tracker.reserve_probe("probe", later));
    }

    #[test]
    fn circuit_breaker_allows_only_one_probe_and_resets_on_success() {
        let tracker = HealthTracker::default();
        let now = Instant::now();
        for _ in 0..3 {
            tracker.record_failure("a", AttemptFailure::Network, now);
        }
        assert_eq!(tracker.eligibility("a", now, true), Eligibility::Blocked);
        let later = now + INITIAL_COOLDOWN + Duration::from_millis(1);
        assert_eq!(tracker.eligibility("a", later, false), Eligibility::Probe);
        assert_eq!(tracker.eligibility("a", later, true), Eligibility::Probe);
        assert_eq!(tracker.eligibility("a", later, true), Eligibility::Blocked);
        tracker.record_success("a");
        assert_eq!(
            tracker.eligibility("a", later, true),
            Eligibility::Available
        );
    }

    #[test]
    fn legacy_accounts_route_without_mapping_and_preserve_explicit_mapping_state() {
        let path = std::env::temp_dir().join(format!(
            "onespace-router-legacy-account-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = RootKey::try_from(vec![61; 32]).unwrap();
        let encrypted = encrypt_credential(
            &root_key,
            "third_party_api_key",
            "legacy-account",
            b"legacy-secret",
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, base_url, auth_method, upstream_protocol) VALUES ('legacy-account', 'api_key', 'Legacy Account', 'default', 'http://127.0.0.1:1/v1', 'bearer', 'responses')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES ('legacy-account', 'third_party_api_key', ?1, ?2, ?3)",
                rusqlite::params![
                    encrypted.ciphertext,
                    encrypted.nonce.as_slice(),
                    encrypted.cipher_version
                ],
            )
            .unwrap();
        let grant = GatewayKeyGrant {
            id: "key".into(),
            name: "Key".into(),
            group_ids: vec!["default".into()],
            model_ids: vec!["gpt-5.6-sol".into()],
        };

        let legacy = candidates(
            &connection,
            &grant,
            "gpt-5.6-sol",
            "responses",
            &[],
            &root_key,
            &HealthTracker::default(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].upstream_model, "gpt-5.6-sol");

        connection
            .execute(
                "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id, enabled) VALUES ('legacy-account', 'gpt-5.6-sol', 'gpt-5.6-sol', 0)",
                [],
            )
            .unwrap();
        assert!(candidates(
            &connection,
            &grant,
            "gpt-5.6-sol",
            "responses",
            &[],
            &root_key,
            &HealthTracker::default(),
            Instant::now(),
        )
        .unwrap()
        .is_empty());

        connection
            .execute(
                "UPDATE ai_gateway_account_model_mappings SET upstream_model_id = 'vendor-model', enabled = 1 WHERE account_id = 'legacy-account' AND public_model_id = 'gpt-5.6-sol'",
                [],
            )
            .unwrap();
        let explicit = candidates(
            &connection,
            &grant,
            "gpt-5.6-sol",
            "responses",
            &[],
            &root_key,
            &HealthTracker::default(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0].upstream_model, "vendor-model");

        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn database_filtering_honors_permissions_mapping_quota_and_three_account_limit() {
        let path = std::env::temp_dir().join(format!(
            "onespace-router-candidates-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = RootKey::try_from(vec![53; 32]).unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_groups (id, name, sort_order) VALUES ('other', 'Other', 1)",
                [],
            )
            .unwrap();
        for (id, group, sort) in [
            ("account-a", "default", 0),
            ("account-b", "default", 1),
            ("account-c", "default", 2),
            ("account-d", "default", 3),
            ("account-other", "other", -1),
        ] {
            let encrypted =
                encrypt_credential(&root_key, "third_party_api_key", id, id.as_bytes()).unwrap();
            connection.execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order, base_url, auth_method, upstream_protocol) VALUES (?1, 'api_key', ?1, ?2, ?3, 'http://127.0.0.1:1/v1', 'bearer', 'responses')",
                rusqlite::params![id, group, sort],
            ).unwrap();
            connection.execute(
                "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, "third_party_api_key", encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
            ).unwrap();
            connection.execute(
                "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id) VALUES (?1, 'gpt-5.6-sol', ?1)",
                [id],
            ).unwrap();
        }
        let grant = GatewayKeyGrant {
            id: "key".into(),
            name: "Key".into(),
            group_ids: vec!["default".into()],
            model_ids: vec!["gpt-5.6-sol".into()],
        };
        let selected = candidates(
            &connection,
            &grant,
            "gpt-5.6-sol",
            "responses",
            &[],
            &root_key,
            &HealthTracker::default(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|candidate| candidate.account_id.as_str())
                .collect::<Vec<_>>(),
            vec!["account-a", "account-b", "account-c"]
        );
        connection
            .execute(
                "UPDATE ai_gateway_credentials SET ciphertext = X'01' WHERE account_id = 'account-a'",
                [],
            )
            .unwrap();
        let invalid_is_skipped_before_truncation = candidates(
            &connection,
            &GatewayKeyGrant {
                id: "key".into(),
                name: "Key".into(),
                group_ids: vec!["default".into()],
                model_ids: vec!["gpt-5.6-sol".into()],
            },
            "gpt-5.6-sol",
            "responses",
            &[],
            &root_key,
            &HealthTracker::default(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(
            invalid_is_skipped_before_truncation
                .iter()
                .map(|candidate| candidate.account_id.as_str())
                .collect::<Vec<_>>(),
            vec!["account-b", "account-c", "account-d"]
        );
        let denied = GatewayKeyGrant {
            model_ids: vec!["gpt-5.6-terra".into()],
            ..grant
        };
        assert!(candidates(
            &connection,
            &denied,
            "gpt-5.6-sol",
            "responses",
            &[],
            &root_key,
            &HealthTracker::default(),
            Instant::now(),
        )
        .unwrap()
        .is_empty());
        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }
}
