use rusqlite::{params, Connection};
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use super::{
    error::{GatewayError, GatewayErrorCategory},
    types::{QuotaScopeType, QuotaWindowDto},
};

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuotaContext<'a> {
    pub(crate) model: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) capabilities: &'a [&'a str],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QuotaDecision {
    pub(crate) available: bool,
    pub(crate) fresh: bool,
    pub(crate) minimum_remaining_percent: Option<f64>,
    pub(crate) blocking_window_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuotaHomepageSummary {
    pub(crate) account_count: usize,
    pub(crate) available_count: usize,
    pub(crate) unavailable_count: usize,
    pub(crate) stale_count: usize,
}

pub(crate) fn replace_account_windows(
    connection: &mut Connection,
    account_id: &str,
    windows: &[QuotaWindowDto],
) -> Result<(), GatewayError> {
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(account_id))?;
    let account_type: String = transaction
        .query_row(
            "SELECT account_type FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .map_err(|_| storage_error(account_id))?;
    if account_type != "oauth" {
        return Err(GatewayError::new(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    transaction
        .execute(
            "DELETE FROM ai_gateway_quota_windows WHERE account_id = ?1",
            [account_id],
        )
        .map_err(|_| storage_error(account_id))?;
    for window in windows {
        validate_window(account_id, window)?;
        transaction
            .execute(
                "INSERT INTO ai_gateway_quota_windows (id, account_id, upstream_window_id, name, scope_type, scope_value, used_percent, remaining_percent, resets_at, duration_seconds, last_succeeded_at, is_stale, raw_kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    window.id,
                    account_id,
                    window.upstream_window_id,
                    window.name,
                    window.scope_type.as_str(),
                    window.scope_value,
                    window.used_percent,
                    window.remaining_percent,
                    window.resets_at,
                    window.duration_seconds,
                    window.last_succeeded_at,
                    window.is_stale,
                    window.raw_kind,
                ],
            )
            .map_err(|_| storage_error(account_id))?;
    }
    transaction.commit().map_err(|_| storage_error(account_id))
}

pub(crate) fn load_account_windows(
    connection: &Connection,
    account_id: &str,
) -> Result<Vec<QuotaWindowDto>, GatewayError> {
    let mut statement = connection
        .prepare(
            "SELECT id, account_id, upstream_window_id, name, scope_type, scope_value, used_percent, remaining_percent, resets_at, duration_seconds, last_succeeded_at, is_stale, raw_kind FROM ai_gateway_quota_windows WHERE account_id = ?1 ORDER BY name, id",
        )
        .map_err(|_| storage_error(account_id))?;
    let windows = statement
        .query_map([account_id], |row| {
            let scope: String = row.get(4)?;
            Ok(QuotaWindowDto {
                id: row.get(0)?,
                account_id: row.get(1)?,
                upstream_window_id: row.get(2)?,
                name: row.get(3)?,
                scope_type: parse_scope(&scope),
                scope_value: row.get(5)?,
                used_percent: row.get(6)?,
                remaining_percent: row.get(7)?,
                resets_at: row.get(8)?,
                duration_seconds: row.get(9)?,
                last_succeeded_at: row.get(10)?,
                is_stale: row.get(11)?,
                raw_kind: row.get(12)?,
            })
        })
        .map_err(|_| storage_error(account_id))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| storage_error(account_id))?;
    Ok(windows)
}

pub(crate) fn evaluate_quota(
    windows: &[QuotaWindowDto],
    context: &QuotaContext<'_>,
    global_threshold: u8,
    account_override: Option<u8>,
) -> QuotaDecision {
    let threshold = account_override.unwrap_or(global_threshold).min(100) as f64;
    let mut fresh = true;
    let mut minimum: Option<f64> = None;
    let mut blocking_window: Option<(&str, f64)> = None;
    for window in windows.iter().filter(|window| applies(window, context)) {
        if window.is_stale {
            fresh = false;
            continue;
        }
        let Some(remaining) = window.remaining_percent else {
            continue;
        };
        minimum = Some(minimum.map_or(remaining, |current| current.min(remaining)));
        let blocked = remaining <= 0.0 || remaining < threshold;
        if blocked
            && blocking_window.is_none_or(|(current_id, current_remaining)| {
                remaining < current_remaining
                    || (remaining == current_remaining && window.id.as_str() < current_id)
            })
        {
            blocking_window = Some((window.id.as_str(), remaining));
        }
    }
    QuotaDecision {
        available: blocking_window.is_none(),
        fresh,
        minimum_remaining_percent: minimum,
        blocking_window_id: blocking_window.map(|(id, _)| id.to_owned()),
    }
}

pub(crate) fn homepage_summary(decisions: &[QuotaDecision]) -> QuotaHomepageSummary {
    let available_count = decisions
        .iter()
        .filter(|decision| decision.available)
        .count();
    QuotaHomepageSummary {
        account_count: decisions.len(),
        available_count,
        unavailable_count: decisions.len() - available_count,
        stale_count: decisions.iter().filter(|decision| !decision.fresh).count(),
    }
}

fn applies(window: &QuotaWindowDto, context: &QuotaContext<'_>) -> bool {
    match window.scope_type {
        QuotaScopeType::Global => true,
        QuotaScopeType::Model => window.scope_value.as_deref() == Some(context.model),
        QuotaScopeType::Endpoint => window.scope_value.as_deref() == Some(context.endpoint),
        QuotaScopeType::Capability => window
            .scope_value
            .as_deref()
            .is_some_and(|value| context.capabilities.contains(&value)),
        QuotaScopeType::Unknown => window.scope_value.is_some(),
    }
}

#[derive(Debug, Default)]
pub(crate) struct QuotaRefreshCoordinator {
    slots: Mutex<HashMap<String, Arc<RefreshSlot>>>,
}

#[derive(Debug)]
struct RefreshSlot {
    state: Mutex<RefreshState>,
    finished: Condvar,
}

#[derive(Debug, Clone)]
struct RefreshState {
    in_flight: bool,
    generation: u64,
    last_result: Option<Result<Vec<QuotaWindowDto>, GatewayError>>,
    consecutive_failures: u32,
    next_allowed_at: Option<Instant>,
}

impl Default for RefreshState {
    fn default() -> Self {
        Self {
            in_flight: false,
            generation: 0,
            last_result: None,
            consecutive_failures: 0,
            next_allowed_at: None,
        }
    }
}

impl QuotaRefreshCoordinator {
    pub(crate) fn refresh<F>(
        &self,
        account_id: &str,
        now: Instant,
        operation: F,
    ) -> Result<Vec<QuotaWindowDto>, GatewayError>
    where
        F: FnOnce() -> Result<Vec<QuotaWindowDto>, GatewayError>,
    {
        let slot = {
            let mut slots = self
                .slots
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(slots.entry(account_id.to_owned()).or_insert_with(|| {
                Arc::new(RefreshSlot {
                    state: Mutex::new(RefreshState::default()),
                    finished: Condvar::new(),
                })
            }))
        };
        let mut state = slot
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.in_flight {
            let generation = state.generation;
            while state.in_flight && state.generation == generation {
                state = slot
                    .finished
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            return state.last_result.clone().unwrap_or_else(|| {
                Err(GatewayError::new(
                    GatewayErrorCategory::StorageUnavailable,
                    Some(account_id),
                ))
            });
        }
        if state.next_allowed_at.is_some_and(|deadline| now < deadline) {
            return Err(GatewayError::new(
                GatewayErrorCategory::QuotaRefreshBackoff,
                Some(account_id),
            ));
        }
        state.in_flight = true;
        drop(state);

        let operation_started_at = Instant::now();
        let result = operation();
        let operation_duration = operation_started_at.elapsed();
        let completed_at = now
            .checked_add(operation_duration)
            .unwrap_or_else(Instant::now);
        let mut state = slot
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.in_flight = false;
        state.generation += 1;
        state.last_result = Some(result.clone());
        if result.is_ok() {
            state.consecutive_failures = 0;
            state.next_allowed_at = None;
        } else {
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            let multiplier = 1u32
                .checked_shl(state.consecutive_failures.saturating_sub(1).min(6))
                .unwrap_or(64);
            let backoff = (INITIAL_BACKOFF * multiplier).min(MAX_BACKOFF);
            state.next_allowed_at = completed_at.checked_add(backoff);
        }
        slot.finished.notify_all();
        result
    }

    pub(crate) fn refresh_with_storage<F>(
        &self,
        connection: &mut Connection,
        account_id: &str,
        now: Instant,
        operation: F,
    ) -> Result<Vec<QuotaWindowDto>, GatewayError>
    where
        F: FnOnce() -> Result<Vec<QuotaWindowDto>, GatewayError>,
    {
        match self.refresh(account_id, now, operation) {
            Ok(windows) => {
                replace_account_windows(connection, account_id, &windows)?;
                Ok(windows)
            }
            Err(error) => {
                mark_account_windows_stale(connection, account_id)?;
                Err(error)
            }
        }
    }
}

fn mark_account_windows_stale(
    connection: &mut Connection,
    account_id: &str,
) -> Result<(), GatewayError> {
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(account_id))?;
    let account_type: String = transaction
        .query_row(
            "SELECT account_type FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .map_err(|_| storage_error(account_id))?;
    if account_type != "oauth" {
        return Err(GatewayError::new(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    transaction
        .execute(
            "UPDATE ai_gateway_quota_windows SET is_stale = 1, updated_at = CURRENT_TIMESTAMP WHERE account_id = ?1",
            [account_id],
        )
        .map_err(|_| storage_error(account_id))?;
    transaction.commit().map_err(|_| storage_error(account_id))
}

fn validate_window(account_id: &str, window: &QuotaWindowDto) -> Result<(), GatewayError> {
    let percentages = [window.used_percent, window.remaining_percent];
    if window.account_id != account_id
        || window.id.trim().is_empty()
        || window.name.trim().is_empty()
        || percentages
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
        || window.duration_seconds.is_some_and(|value| value <= 0)
    {
        return Err(GatewayError::new(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    Ok(())
}

fn parse_scope(value: &str) -> QuotaScopeType {
    match value {
        "global" => QuotaScopeType::Global,
        "model" => QuotaScopeType::Model,
        "endpoint" => QuotaScopeType::Endpoint,
        "capability" => QuotaScopeType::Capability,
        _ => QuotaScopeType::Unknown,
    }
}

fn storage_error(account_id: &str) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, Some(account_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_sqlite;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Barrier,
    };

    fn window(
        id: &str,
        scope_type: QuotaScopeType,
        scope_value: Option<&str>,
        remaining: f64,
    ) -> QuotaWindowDto {
        QuotaWindowDto {
            id: id.into(),
            account_id: "account-1".into(),
            upstream_window_id: Some(id.into()),
            name: id.into(),
            scope_type,
            scope_value: scope_value.map(str::to_owned),
            used_percent: Some(100.0 - remaining),
            remaining_percent: Some(remaining),
            resets_at: None,
            duration_seconds: Some(18_000),
            last_succeeded_at: Some("2026-08-01T00:00:00Z".into()),
            is_stale: false,
            raw_kind: None,
        }
    }

    fn context<'a>() -> QuotaContext<'a> {
        QuotaContext {
            model: "gpt-test",
            endpoint: "responses",
            capabilities: &["code_review"],
        }
    }

    #[test]
    fn threshold_boundaries_inheritance_stale_and_recovery_are_exact() {
        let windows = vec![window("global", QuotaScopeType::Global, None, 10.0)];
        assert!(evaluate_quota(&windows, &context(), 10, None).available);
        assert!(!evaluate_quota(&windows, &context(), 100, None).available);
        assert!(evaluate_quota(&windows, &context(), 100, Some(0)).available);
        let exhausted = vec![window("global", QuotaScopeType::Global, None, 0.0)];
        assert!(!evaluate_quota(&exhausted, &context(), 0, None).available);
        let recovered = vec![window("global", QuotaScopeType::Global, None, 10.0)];
        assert!(evaluate_quota(&recovered, &context(), 10, None).available);
        let mut stale = window("stale", QuotaScopeType::Global, None, 0.0);
        stale.is_stale = true;
        let decision = evaluate_quota(&[stale], &context(), 10, None);
        assert!(decision.available);
        assert!(!decision.fresh);
    }

    #[test]
    fn dynamic_scopes_and_unknown_window_rules_apply_to_matching_requests_only() {
        let unrelated = vec![
            window("model", QuotaScopeType::Model, Some("other"), 0.0),
            window(
                "endpoint",
                QuotaScopeType::Endpoint,
                Some("chat_completions"),
                0.0,
            ),
            window("capability", QuotaScopeType::Capability, Some("spark"), 0.0),
            window("unknown-display", QuotaScopeType::Unknown, None, 0.0),
        ];
        assert!(evaluate_quota(&unrelated, &context(), 10, None).available);
        for scoped in [
            window("model", QuotaScopeType::Model, Some("gpt-test"), 0.0),
            window("endpoint", QuotaScopeType::Endpoint, Some("responses"), 0.0),
            window(
                "capability",
                QuotaScopeType::Capability,
                Some("code_review"),
                0.0,
            ),
            window(
                "unknown-scoped",
                QuotaScopeType::Unknown,
                Some("future-scope"),
                0.0,
            ),
        ] {
            assert!(!evaluate_quota(&[scoped], &context(), 10, None).available);
        }
    }

    #[test]
    fn quota_aggregation_is_independent_of_window_order() {
        let windows = vec![
            window("global", QuotaScopeType::Global, None, 80.0),
            window(
                "endpoint-block",
                QuotaScopeType::Endpoint,
                Some("responses"),
                8.0,
            ),
            window("model-block", QuotaScopeType::Model, Some("gpt-test"), 5.0),
        ];
        let mut reversed = windows.clone();
        reversed.reverse();

        let expected = QuotaDecision {
            available: false,
            fresh: true,
            minimum_remaining_percent: Some(5.0),
            blocking_window_id: Some("model-block".into()),
        };
        assert_eq!(evaluate_quota(&windows, &context(), 10, None), expected);
        assert_eq!(evaluate_quota(&reversed, &context(), 10, None), expected);
    }

    #[test]
    fn homepage_denominator_counts_accounts_not_windows() {
        let decisions = vec![
            QuotaDecision {
                available: true,
                fresh: true,
                minimum_remaining_percent: Some(80.0),
                blocking_window_id: None,
            },
            QuotaDecision {
                available: false,
                fresh: true,
                minimum_remaining_percent: Some(2.0),
                blocking_window_id: Some("w".into()),
            },
            QuotaDecision {
                available: true,
                fresh: false,
                minimum_remaining_percent: None,
                blocking_window_id: None,
            },
        ];
        assert_eq!(
            homepage_summary(&decisions),
            QuotaHomepageSummary {
                account_count: 3,
                available_count: 2,
                unavailable_count: 1,
                stale_count: 1
            }
        );
    }

    #[test]
    fn oauth_dynamic_windows_persist_and_api_key_accounts_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("onespace-quota-{}.sqlite3", uuid::Uuid::new_v4()));
        let mut connection = shared_sqlite::open_at(&path).unwrap();
        connection.execute_batch(
            "INSERT INTO ai_gateway_accounts (id, stable_external_id, account_type, name, group_id) VALUES ('account-1', 'oauth-user', 'oauth', 'OAuth', 'default');
             INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('api-account', 'api_key', 'API', 'default');",
        ).unwrap();
        let windows = vec![
            window(
                "code-review",
                QuotaScopeType::Capability,
                Some("code_review"),
                40.0,
            ),
            window("five-hour", QuotaScopeType::Global, None, 72.5),
            window(
                "future",
                QuotaScopeType::Unknown,
                Some("future-scope"),
                90.0,
            ),
        ];
        replace_account_windows(&mut connection, "account-1", &windows).unwrap();
        assert_eq!(
            load_account_windows(&connection, "account-1").unwrap(),
            windows
        );
        assert_eq!(
            replace_account_windows(&mut connection, "api-account", &[])
                .unwrap_err()
                .category(),
            GatewayErrorCategory::InvalidInput
        );
        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn concurrent_refreshes_coalesce_and_failures_back_off_with_cap() {
        let coordinator = Arc::new(QuotaRefreshCoordinator::default());
        let barrier = Arc::new(Barrier::new(8));
        let calls = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();
        let mut threads = Vec::new();
        for _ in 0..8 {
            let coordinator = Arc::clone(&coordinator);
            let barrier = Arc::clone(&barrier);
            let calls = Arc::clone(&calls);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                coordinator.refresh("account-1", now, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(30));
                    Ok(vec![window("global", QuotaScopeType::Global, None, 50.0)])
                })
            }));
        }
        for thread in threads {
            assert_eq!(thread.join().unwrap().unwrap().len(), 1);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let error = GatewayError::new(GatewayErrorCategory::StorageUnavailable, Some("account-2"));
        assert!(coordinator
            .refresh("account-2", now, || Err(error))
            .is_err());
        assert_eq!(
            coordinator
                .refresh("account-2", now + Duration::from_secs(1), || Ok(Vec::new()))
                .unwrap_err()
                .category(),
            GatewayErrorCategory::QuotaRefreshBackoff
        );
        assert!(coordinator
            .refresh(
                "account-2",
                now + Duration::from_secs(5) + Duration::from_millis(1),
                || Ok(Vec::new()),
            )
            .is_ok());
    }

    #[test]
    fn failed_refresh_preserves_windows_as_stale_in_one_transaction() {
        let path = std::env::temp_dir().join(format!(
            "onespace-quota-stale-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut connection = shared_sqlite::open_at(&path).unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, stable_external_id, account_type, name, group_id) VALUES ('account-1', 'oauth-user', 'oauth', 'OAuth', 'default')",
                [],
            )
            .unwrap();
        let windows = vec![
            window("global", QuotaScopeType::Global, None, 0.0),
            window("model", QuotaScopeType::Model, Some("gpt-test"), 40.0),
        ];
        replace_account_windows(&mut connection, "account-1", &windows).unwrap();

        let coordinator = QuotaRefreshCoordinator::default();
        let error = GatewayError::new(GatewayErrorCategory::StorageUnavailable, Some("account-1"));
        assert_eq!(
            coordinator
                .refresh_with_storage(&mut connection, "account-1", Instant::now(), || Err(error),)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::StorageUnavailable
        );
        let stored = load_account_windows(&connection, "account-1").unwrap();
        assert_eq!(stored.len(), windows.len());
        assert!(stored.iter().all(|window| window.is_stale));
        assert_eq!(stored[0].remaining_percent, windows[0].remaining_percent);
        assert_eq!(stored[1].remaining_percent, windows[1].remaining_percent);
        assert!(evaluate_quota(&stored, &context(), 10, None).available);
        assert!(!evaluate_quota(&stored, &context(), 10, None).fresh);

        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn refresh_backoff_starts_after_a_long_failed_operation_finishes() {
        let coordinator = QuotaRefreshCoordinator::default();
        let started = Instant::now();
        let error = GatewayError::new(GatewayErrorCategory::StorageUnavailable, Some("account-1"));
        assert!(coordinator
            .refresh("account-1", started, || {
                std::thread::sleep(Duration::from_millis(50));
                Err(error)
            })
            .is_err());

        assert_eq!(
            coordinator
                .refresh(
                    "account-1",
                    started + INITIAL_BACKOFF + Duration::from_millis(10),
                    || Ok(Vec::new()),
                )
                .unwrap_err()
                .category(),
            GatewayErrorCategory::QuotaRefreshBackoff
        );
    }
}
