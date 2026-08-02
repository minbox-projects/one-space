use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::{
    sync::{watch, Mutex},
    task::JoinHandle,
};

use super::{
    accounts::{self, CreateApiKeyAccount, DeleteConfirmationStore, UpdateAccount},
    gateway_key, oauth, pricing,
    request_logs::{self, LogFilters, RetentionPolicy},
    runtime::{GatewayHttpRuntime, GatewayHttpService, RuntimeStatus},
    security::{
        initialize_security, MacOsKeychainStore, RootKey, SecurityLockReason, SecurityState,
    },
    storage,
    types::{
        AccountDto, GroupDto, ModelMappingDto, PriceSnapshot, QuotaWindowDto, UpstreamProtocol,
    },
};

const RUNTIME_EVENT: &str = "ai-routing-gateway-runtime";
const ACCOUNT_EVENT: &str = "ai-routing-gateway-account";
const OAUTH_EVENT: &str = "ai-routing-gateway-oauth";
const MAINTENANCE_EVENT: &str = "ai-routing-gateway-maintenance";

fn confirmations() -> &'static DeleteConfirmationStore {
    use std::sync::OnceLock;

    static CONFIRMATIONS: OnceLock<DeleteConfirmationStore> = OnceLock::new();
    CONFIRMATIONS.get_or_init(DeleteConfirmationStore::default)
}

#[derive(Default)]
pub(crate) struct GatewayLifecycle {
    runtime: GatewayHttpRuntime,
    operation: Mutex<()>,
    diagnostic: Mutex<Option<RuntimeStatus>>,
    lock_reason: Mutex<Option<String>>,
    schedulers: Mutex<Option<BackgroundSchedulers>>,
}

struct BackgroundSchedulers {
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl BackgroundSchedulers {
    fn start(database_path: PathBuf) -> Self {
        let (shutdown, _) = watch::channel(false);
        let quota_shutdown = shutdown.subscribe();
        let maintenance_shutdown = shutdown.subscribe();
        Self {
            shutdown,
            tasks: vec![
                tokio::spawn(run_quota_scheduler(quota_shutdown)),
                tokio::spawn(run_maintenance_scheduler(
                    database_path,
                    maintenance_shutdown,
                )),
            ],
        }
    }

    async fn stop(self) {
        let _ = self.shutdown.send(true);
        for mut task in self.tasks {
            if tokio::time::timeout(Duration::from_secs(5), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

async fn run_quota_scheduler(mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
    interval.tick().await;
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
            _ = interval.tick() => {
                // 官方 OAuth 契约仍处于发布门禁，禁止在此发起替代联网刷新。
            }
        }
    }
}

async fn run_maintenance_scheduler(database_path: PathBuf, mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
    interval.tick().await;
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
            _ = interval.tick() => {
                if let Ok(mut connection) = crate::shared_sqlite::open_at(&database_path) {
                    let _ = request_logs::cleanup_retained_details(&mut connection, chrono::Utc::now());
                }
            }
        }
    }
}

impl GatewayLifecycle {
    async fn status(&self, fallback_port: u16) -> RuntimeStatus {
        let active = self.runtime.status(fallback_port).await;
        if matches!(active, RuntimeStatus::Running { .. }) {
            return active;
        }
        self.diagnostic.lock().await.clone().unwrap_or(active)
    }

    async fn remember(&self, status: RuntimeStatus, lock_reason: Option<String>) {
        *self.diagnostic.lock().await = Some(status);
        *self.lock_reason.lock().await = lock_reason;
    }

    async fn clear_diagnostic(&self) {
        *self.diagnostic.lock().await = None;
        *self.lock_reason.lock().await = None;
    }

    async fn start_schedulers(&self, database_path: PathBuf) {
        let mut schedulers = self.schedulers.lock().await;
        if schedulers.is_none() {
            *schedulers = Some(BackgroundSchedulers::start(database_path));
        }
    }

    async fn stop_schedulers(&self) {
        let schedulers = self.schedulers.lock().await.take();
        if let Some(schedulers) = schedulers {
            schedulers.stop().await;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GatewayAvailability {
    Ready,
    Locked,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeDto {
    pub(crate) state: String,
    pub(crate) availability: GatewayAvailability,
    pub(crate) port: u16,
    pub(crate) run_enabled: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) lock_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsDto {
    pub(crate) port: u16,
    pub(crate) global_quota_threshold_percent: u8,
    pub(crate) log_retention_days: Option<u16>,
    pub(crate) run_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicModelDto {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayKeyDto {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) key_prefix: String,
    pub(crate) enabled: bool,
    pub(crate) expires_at: Option<String>,
    pub(crate) revoked_at: Option<String>,
    pub(crate) last_used_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) model_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OneTimeGatewayKeyDto {
    pub(crate) key: GatewayKeyDto,
    pub(crate) plaintext: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenUsageDto {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_write_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrendPointDto {
    pub(crate) local_date: String,
    pub(crate) request_count: u64,
    pub(crate) success_count: u64,
    pub(crate) failure_count: u64,
    pub(crate) usage: TokenUsageDto,
    pub(crate) estimated_cost_usd: Option<String>,
    pub(crate) cost_calculable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HomepageDto {
    pub(crate) account_count: u64,
    pub(crate) available_count: u64,
    pub(crate) unavailable_count: u64,
    pub(crate) stale_count: u64,
    pub(crate) today: TrendPointDto,
    pub(crate) trend: Vec<TrendPointDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BootstrapDto {
    pub(crate) runtime: RuntimeDto,
    pub(crate) settings: SettingsDto,
    pub(crate) groups: Vec<GroupDto>,
    pub(crate) accounts: Vec<AccountDto>,
    pub(crate) models: Vec<PublicModelDto>,
    pub(crate) keys: Vec<GatewayKeyDto>,
    pub(crate) homepage: HomepageDto,
    pub(crate) oauth_release_block_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HomepageFiltersInput {
    pub(crate) account_id: Option<String>,
    pub(crate) group_id: Option<String>,
    pub(crate) public_model_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateGroupInput {
    pub(crate) name: String,
    pub(crate) sort_order: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateAccountInput {
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) auth_method: String,
    pub(crate) upstream_protocol: UpstreamProtocol,
    pub(crate) note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateAccountInput {
    pub(crate) account_id: String,
    pub(crate) name: String,
    pub(crate) group_id: String,
    pub(crate) sort_order: i64,
    pub(crate) note: String,
    pub(crate) enabled: bool,
    pub(crate) quota_threshold_override_percent: Option<u8>,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MappingInput {
    pub(crate) account_id: String,
    pub(crate) public_model_id: String,
    pub(crate) upstream_model_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateKeyInput {
    pub(crate) name: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) model_ids: Vec<String>,
    pub(crate) expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogQueryInput {
    pub(crate) started_at_or_after: Option<String>,
    pub(crate) started_before: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) group_id: Option<String>,
    pub(crate) public_model_id: Option<String>,
    pub(crate) upstream_model_id: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) api_key_id: Option<String>,
    pub(crate) cursor: Option<String>,
    pub(crate) page_size: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogPageDto {
    pub(crate) items: Vec<request_logs::RequestLogRow>,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PriceInputDto {
    pub(crate) public_model_id: String,
    pub(crate) account_id: Option<String>,
    pub(crate) effective_at: String,
    pub(crate) input_per_million_usd: Option<String>,
    pub(crate) output_per_million_usd: Option<String>,
    pub(crate) cache_read_per_million_usd: Option<String>,
    pub(crate) cache_write_per_million_usd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceResultDto {
    pub(crate) operation: String,
    pub(crate) affected_rows: usize,
    pub(crate) expected_rows: Option<usize>,
    pub(crate) actual_rows: Option<usize>,
    pub(crate) mismatched_rows: Option<usize>,
}

fn error_code(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    let _ = app.emit(event, payload);
}

fn read_settings(connection: &Connection) -> Result<SettingsDto, String> {
    connection
        .query_row(
            "SELECT port, global_quota_threshold_percent, log_retention_days, run_enabled FROM ai_gateway_settings WHERE id = 1",
            [],
            |row| {
                Ok(SettingsDto {
                    port: row.get(0)?,
                    global_quota_threshold_percent: row.get(1)?,
                    log_retention_days: row.get(2)?,
                    run_enabled: row.get(3)?,
                })
            },
        )
        .map_err(|_| "storage_unavailable".to_owned())
}

fn security() -> Result<RootKey, String> {
    let connection = storage::open().map_err(error_code)?;
    match initialize_security(&connection, &MacOsKeychainStore) {
        SecurityState::Ready(key) => Ok(key),
        SecurityState::Locked(reason) => Err(lock_reason(reason)),
    }
}

fn lock_reason(reason: SecurityLockReason) -> String {
    match reason {
        SecurityLockReason::StorageUnavailable => "storage_unavailable",
        SecurityLockReason::RootKeyMissing => "root_key_missing",
        SecurityLockReason::CredentialStoreUnavailable => "credential_store_unavailable",
        SecurityLockReason::RootKeyInvalid => "root_key_invalid",
    }
    .to_owned()
}

fn groups(connection: &Connection) -> Result<Vec<GroupDto>, String> {
    let mut statement = connection
        .prepare("SELECT id, name, sort_order, is_default FROM ai_gateway_groups ORDER BY sort_order, id")
        .map_err(|_| "storage_unavailable".to_owned())?;
    let result = statement
        .query_map([], |row| {
            Ok(GroupDto {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                is_default: row.get(3)?,
            })
        })
        .map_err(|_| "storage_unavailable".to_owned())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "storage_unavailable".to_owned());
    result
}

fn accounts(connection: &Connection) -> Result<Vec<AccountDto>, String> {
    let ids = {
        let mut statement = connection
            .prepare("SELECT id FROM ai_gateway_accounts ORDER BY sort_order, id")
            .map_err(|_| "storage_unavailable".to_owned())?;
        let result = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| "storage_unavailable".to_owned())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "storage_unavailable".to_owned())?;
        result
    };
    ids.into_iter()
        .map(|id| accounts::get_account(connection, &id).map_err(error_code))
        .collect()
}

fn models(connection: &Connection) -> Result<Vec<PublicModelDto>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, display_name, enabled FROM ai_gateway_models ORDER BY display_name, id",
        )
        .map_err(|_| "storage_unavailable".to_owned())?;
    let result = statement
        .query_map([], |row| {
            Ok(PublicModelDto {
                id: row.get(0)?,
                display_name: row.get(1)?,
                enabled: row.get(2)?,
            })
        })
        .map_err(|_| "storage_unavailable".to_owned())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "storage_unavailable".to_owned());
    result
}

fn query_strings(connection: &Connection, sql: &str, id: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| "storage_unavailable".to_owned())?;
    let result = statement
        .query_map([id], |row| row.get(0))
        .map_err(|_| "storage_unavailable".to_owned())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "storage_unavailable".to_owned());
    result
}

fn keys(connection: &Connection) -> Result<Vec<GatewayKeyDto>, String> {
    let base = {
        let mut statement = connection
            .prepare("SELECT id, name, key_prefix, enabled, expires_at, revoked_at, last_used_at, created_at FROM ai_gateway_api_keys ORDER BY created_at DESC, id DESC")
            .map_err(|_| "storage_unavailable".to_owned())?;
        let result = statement
            .query_map([], |row| {
                Ok(GatewayKeyDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    key_prefix: row.get(2)?,
                    enabled: row.get(3)?,
                    expires_at: row.get(4)?,
                    revoked_at: row.get(5)?,
                    last_used_at: row.get(6)?,
                    created_at: row.get(7)?,
                    group_ids: Vec::new(),
                    model_ids: Vec::new(),
                })
            })
            .map_err(|_| "storage_unavailable".to_owned())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "storage_unavailable".to_owned())?;
        result
    };
    base.into_iter()
        .map(|mut key| {
            key.group_ids = query_strings(
                connection,
                "SELECT group_id FROM ai_gateway_api_key_groups WHERE api_key_id = ?1 ORDER BY group_id",
                &key.id,
            )?;
            key.model_ids = query_strings(
                connection,
                "SELECT model_id FROM ai_gateway_api_key_models WHERE api_key_id = ?1 ORDER BY model_id",
                &key.id,
            )?;
            Ok(key)
        })
        .collect()
}

fn token_usage(value: pricing::TokenUsage) -> TokenUsageDto {
    TokenUsageDto {
        input_tokens: value.input_tokens,
        output_tokens: value.output_tokens,
        cache_read_tokens: value.cache_read_tokens,
        cache_write_tokens: value.cache_write_tokens,
        total_tokens: value.total_tokens,
    }
}

fn trend_point(value: request_logs::TrendPoint) -> TrendPointDto {
    TrendPointDto {
        local_date: value.local_date,
        request_count: value.request_count,
        success_count: value.success_count,
        failure_count: value.failure_count,
        usage: token_usage(value.usage),
        estimated_cost_usd: value.estimated_cost_usd,
        cost_calculable: value.cost_calculable,
    }
}

fn filter_value(value: Option<&String>) -> Option<&str> {
    value.and_then(|value| (!value.trim().is_empty()).then_some(value.as_str()))
}

fn homepage(
    connection: &Connection,
    days: u8,
    filters: Option<&HomepageFiltersInput>,
) -> Result<HomepageDto, String> {
    let account_filter = filter_value(filters.and_then(|filters| filters.account_id.as_ref()));
    let group_filter = filter_value(filters.and_then(|filters| filters.group_id.as_ref()));
    let model_filter = filter_value(filters.and_then(|filters| filters.public_model_id.as_ref()));
    let accounts = accounts(connection)?;
    let selected_accounts = accounts.iter().filter(|account| {
        account_filter.map_or(true, |value| account.id == value)
            && group_filter.map_or(true, |value| account.group_id == value)
    });
    let selected_accounts = selected_accounts.collect::<Vec<_>>();
    let available_count = selected_accounts
        .iter()
        .filter(|account| {
            account.enabled
                && !matches!(
                    account.health_status.as_str(),
                    "unavailable" | "authorization_invalid"
                )
        })
        .count() as u64;
    let stale_count: u64 = connection
        .query_row(
            "SELECT COUNT(DISTINCT windows.account_id) FROM ai_gateway_quota_windows windows JOIN ai_gateway_accounts accounts ON accounts.id = windows.account_id WHERE windows.is_stale = 1 AND (?1 IS NULL OR windows.account_id = ?1) AND (?2 IS NULL OR accounts.group_id = ?2)",
            params![account_filter, group_filter],
            |row| row.get(0),
        )
        .map_err(|_| "storage_unavailable".to_owned())?;
    let trend = request_logs::trend(
        connection,
        Local::now().date_naive(),
        days,
        account_filter,
        group_filter,
        model_filter,
    )
    .map_err(error_code)?
    .into_iter()
    .map(trend_point)
    .collect::<Vec<_>>();
    let today = trend.last().cloned().unwrap_or(TrendPointDto {
        local_date: Local::now().date_naive().to_string(),
        request_count: 0,
        success_count: 0,
        failure_count: 0,
        usage: token_usage(pricing::TokenUsage {
            input_tokens: Some(0),
            output_tokens: Some(0),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            total_tokens: Some(0),
        }),
        estimated_cost_usd: Some("0".to_owned()),
        cost_calculable: true,
    });
    Ok(HomepageDto {
        account_count: selected_accounts.len() as u64,
        available_count,
        unavailable_count: selected_accounts.len() as u64 - available_count,
        stale_count,
        today,
        trend,
    })
}

async fn runtime_dto(lifecycle: &GatewayLifecycle, settings: &SettingsDto) -> RuntimeDto {
    let status = lifecycle.status(settings.port).await;
    let (state, error_code) = match status {
        RuntimeStatus::Stopped { .. } => ("stopped", None),
        RuntimeStatus::Running { .. } => ("running", None),
        RuntimeStatus::Error { code, .. } => ("error", Some(code.to_owned())),
    };
    let lock = lifecycle.lock_reason.lock().await.clone();
    RuntimeDto {
        state: if lock.is_some() {
            "locked".to_owned()
        } else {
            state.to_owned()
        },
        availability: if lock.is_some() {
            GatewayAvailability::Locked
        } else if error_code.is_some() {
            GatewayAvailability::Error
        } else {
            GatewayAvailability::Ready
        },
        port: settings.port,
        run_enabled: settings.run_enabled,
        error_code,
        lock_reason: lock,
    }
}

fn unavailable_runtime(port: u16, run_enabled: bool, code: &str) -> RuntimeDto {
    RuntimeDto {
        state: "error".to_owned(),
        availability: GatewayAvailability::Error,
        port,
        run_enabled,
        error_code: Some(code.to_owned()),
        lock_reason: None,
    }
}

fn locked_runtime(port: u16, run_enabled: bool, reason: String) -> RuntimeDto {
    RuntimeDto {
        state: "locked".to_owned(),
        availability: GatewayAvailability::Locked,
        port,
        run_enabled,
        error_code: None,
        lock_reason: Some(reason),
    }
}

async fn start_managed(lifecycle: &GatewayLifecycle, settings: &SettingsDto) -> RuntimeDto {
    let _operation = lifecycle.operation.lock().await;

    // open() performs SQLite bootstrap and migrations before security is touched.
    let connection = match storage::open() {
        Ok(connection) => connection,
        Err(_) => {
            lifecycle.runtime.stop(settings.port).await;
            lifecycle.stop_schedulers().await;
            let status = RuntimeStatus::Error {
                port: settings.port,
                code: "storage_unavailable",
            };
            lifecycle.remember(status, None).await;
            return unavailable_runtime(settings.port, settings.run_enabled, "storage_unavailable");
        }
    };
    let root_key = match initialize_security(&connection, &MacOsKeychainStore) {
        SecurityState::Ready(key) => Arc::new(key),
        SecurityState::Locked(reason) => {
            let reason = lock_reason(reason);
            lifecycle.runtime.stop(settings.port).await;
            lifecycle.stop_schedulers().await;
            lifecycle
                .remember(
                    RuntimeStatus::Stopped {
                        port: settings.port,
                    },
                    Some(reason.clone()),
                )
                .await;
            return locked_runtime(settings.port, settings.run_enabled, reason);
        }
    };
    drop(connection);
    let path = match crate::shared_sqlite::database_path() {
        Ok(path) => path,
        Err(_) => {
            lifecycle.runtime.stop(settings.port).await;
            lifecycle.stop_schedulers().await;
            let status = RuntimeStatus::Error {
                port: settings.port,
                code: "storage_unavailable",
            };
            lifecycle.remember(status, None).await;
            return unavailable_runtime(settings.port, settings.run_enabled, "storage_unavailable");
        }
    };
    let service = match GatewayHttpService::new(path.clone(), root_key) {
        Ok(service) => Arc::new(service),
        Err(_) => {
            lifecycle.runtime.stop(settings.port).await;
            lifecycle.stop_schedulers().await;
            let status = RuntimeStatus::Error {
                port: settings.port,
                code: "gateway_not_ready",
            };
            lifecycle.remember(status, None).await;
            return unavailable_runtime(settings.port, settings.run_enabled, "gateway_not_ready");
        }
    };
    match lifecycle.runtime.start(settings.port, service).await {
        Ok(status @ RuntimeStatus::Running { .. }) => {
            lifecycle.start_schedulers(path).await;
            lifecycle.clear_diagnostic().await;
            runtime_from_status(status, settings.run_enabled)
        }
        Err(status @ RuntimeStatus::Error { .. }) => {
            lifecycle.stop_schedulers().await;
            lifecycle.remember(status.clone(), None).await;
            runtime_from_status(status, settings.run_enabled)
        }
        _ => unreachable!("gateway runtime start only returns running or error"),
    }
}

fn runtime_from_status(status: RuntimeStatus, run_enabled: bool) -> RuntimeDto {
    match status {
        RuntimeStatus::Stopped { port } => RuntimeDto {
            state: "stopped".to_owned(),
            availability: GatewayAvailability::Ready,
            port,
            run_enabled,
            error_code: None,
            lock_reason: None,
        },
        RuntimeStatus::Running { port } => RuntimeDto {
            state: "running".to_owned(),
            availability: GatewayAvailability::Ready,
            port,
            run_enabled,
            error_code: None,
            lock_reason: None,
        },
        RuntimeStatus::Error { port, code } => unavailable_runtime(port, run_enabled, code),
    }
}

async fn stop_managed(lifecycle: &GatewayLifecycle, settings: &SettingsDto) -> RuntimeDto {
    let _operation = lifecycle.operation.lock().await;
    let status = lifecycle.runtime.stop(settings.port).await;
    lifecycle.stop_schedulers().await;
    lifecycle.clear_diagnostic().await;
    runtime_from_status(status, settings.run_enabled)
}

pub(crate) async fn initialize(app: AppHandle) {
    let lifecycle = app.state::<GatewayLifecycle>();
    let settings = match storage::open().and_then(|connection| {
        read_settings(&connection).map_err(|_| {
            super::error::GatewayError::new(
                super::error::GatewayErrorCategory::StorageUnavailable,
                None,
            )
        })
    }) {
        Ok(settings) => settings,
        Err(_) => {
            let status = RuntimeStatus::Error {
                port: super::runtime::DEFAULT_PORT,
                code: "storage_unavailable",
            };
            lifecycle.remember(status, None).await;
            emit(
                &app,
                RUNTIME_EVENT,
                unavailable_runtime(super::runtime::DEFAULT_PORT, true, "storage_unavailable"),
            );
            return;
        }
    };
    let dto = if settings.run_enabled {
        start_managed(&lifecycle, &settings).await
    } else {
        stop_managed(&lifecycle, &settings).await
    };
    emit(&app, RUNTIME_EVENT, dto);
}

pub(crate) async fn shutdown(app: &AppHandle) {
    let lifecycle = app.state::<GatewayLifecycle>();
    let port = storage::open()
        .ok()
        .and_then(|connection| read_settings(&connection).ok())
        .map_or(super::runtime::DEFAULT_PORT, |settings| settings.port);
    let _operation = lifecycle.operation.lock().await;
    lifecycle.runtime.stop(port).await;
    lifecycle.stop_schedulers().await;
    lifecycle.clear_diagnostic().await;
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_bootstrap(
    lifecycle: State<'_, GatewayLifecycle>,
    days: Option<u8>,
    filters: Option<HomepageFiltersInput>,
) -> Result<BootstrapDto, String> {
    let connection = storage::open().map_err(error_code)?;
    let settings = read_settings(&connection)?;
    Ok(BootstrapDto {
        runtime: runtime_dto(&lifecycle, &settings).await,
        settings,
        groups: groups(&connection)?,
        accounts: accounts(&connection)?,
        models: models(&connection)?,
        keys: keys(&connection)?,
        homepage: homepage(&connection, days.unwrap_or(7), filters.as_ref())?,
        oauth_release_block_reason: Some(oauth::OAUTH_RELEASE_BLOCK_REASON.to_owned()),
    })
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_runtime_status(
    lifecycle: State<'_, GatewayLifecycle>,
) -> Result<RuntimeDto, String> {
    let connection = storage::open().map_err(error_code)?;
    let settings = read_settings(&connection)?;
    Ok(runtime_dto(&lifecycle, &settings).await)
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_runtime_start(
    app: AppHandle,
    lifecycle: State<'_, GatewayLifecycle>,
) -> Result<RuntimeDto, String> {
    let connection = storage::open().map_err(error_code)?;
    let settings = read_settings(&connection)?;
    let dto = start_managed(&lifecycle, &settings).await;
    emit(&app, RUNTIME_EVENT, dto.clone());
    Ok(dto)
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_runtime_stop(
    app: AppHandle,
    lifecycle: State<'_, GatewayLifecycle>,
) -> Result<RuntimeDto, String> {
    let connection = storage::open().map_err(error_code)?;
    let settings = read_settings(&connection)?;
    let dto = stop_managed(&lifecycle, &settings).await;
    emit(&app, RUNTIME_EVENT, dto.clone());
    Ok(dto)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_settings_get() -> Result<SettingsDto, String> {
    read_settings(&storage::open().map_err(error_code)?)
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_settings_save(
    app: AppHandle,
    lifecycle: State<'_, GatewayLifecycle>,
    input: SettingsDto,
) -> Result<SettingsDto, String> {
    if input.port == 0
        || input.global_quota_threshold_percent > 100
        || !matches!(input.log_retention_days, Some(7 | 30 | 90 | 180) | None)
    {
        return Err("invalid_input".to_owned());
    }
    let previous = {
        let connection = storage::open().map_err(error_code)?;
        read_settings(&connection)?
    };
    if input.run_enabled && input.port != previous.port {
        if let Err(code) = GatewayHttpRuntime::preflight_port(input.port).await {
            return Err(code.to_owned());
        }
    }
    let dto = {
        let mut connection = storage::open().map_err(error_code)?;
        let transaction = connection
            .transaction()
            .map_err(|_| "storage_unavailable".to_owned())?;
        transaction.execute(
            "UPDATE ai_gateway_settings SET port = ?1, global_quota_threshold_percent = ?2, log_retention_days = ?3, run_enabled = ?4, updated_at = CURRENT_TIMESTAMP WHERE id = 1",
            params![input.port, input.global_quota_threshold_percent, input.log_retention_days, input.run_enabled],
        ).map_err(|_| "storage_unavailable".to_owned())?;
        transaction
            .commit()
            .map_err(|_| "storage_unavailable".to_owned())?;
        read_settings(&connection)?
    };
    let runtime = if dto.run_enabled {
        start_managed(&lifecycle, &dto).await
    } else {
        stop_managed(&lifecycle, &dto).await
    };
    emit(&app, RUNTIME_EVENT, runtime);
    Ok(dto)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_groups_list() -> Result<Vec<GroupDto>, String> {
    groups(&storage::open().map_err(error_code)?)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_group_create(input: CreateGroupInput) -> Result<GroupDto, String> {
    let connection = storage::open().map_err(error_code)?;
    accounts::create_group(&connection, &input.name, input.sort_order).map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_group_delete(group_id: String) -> Result<(), String> {
    let mut connection = storage::open().map_err(error_code)?;
    accounts::delete_group(&mut connection, &group_id).map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_accounts_list() -> Result<Vec<AccountDto>, String> {
    accounts(&storage::open().map_err(error_code)?)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_create_api_key(
    app: AppHandle,
    input: CreateAccountInput,
) -> Result<AccountDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let root_key = security()?;
    let account = accounts::create_api_key_account(
        &mut connection,
        &root_key,
        CreateApiKeyAccount {
            name: &input.name,
            base_url: &input.base_url,
            api_key: &input.api_key,
            auth_method: &input.auth_method,
            upstream_protocol: input.upstream_protocol,
            note: &input.note,
        },
    )
    .map_err(error_code)?;
    emit(&app, ACCOUNT_EVENT, &account);
    Ok(account)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_update(
    app: AppHandle,
    input: UpdateAccountInput,
) -> Result<AccountDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let transaction = connection
        .transaction()
        .map_err(|_| "storage_unavailable".to_owned())?;
    let account = accounts::update_account(
        &transaction,
        &input.account_id,
        UpdateAccount {
            name: &input.name,
            group_id: &input.group_id,
            sort_order: input.sort_order,
            note: &input.note,
            enabled: input.enabled,
            quota_threshold_override_percent: input.quota_threshold_override_percent,
        },
    )
    .map_err(error_code)?;
    accounts::replace_account_tags_in_transaction(&transaction, &input.account_id, &input.tags)
        .map_err(error_code)?;
    transaction
        .commit()
        .map_err(|_| "storage_unavailable".to_owned())?;
    emit(&app, ACCOUNT_EVENT, &account);
    Ok(accounts::get_account(&connection, &input.account_id).map_err(error_code)?)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_move(
    app: AppHandle,
    account_id: String,
    direction: i8,
) -> Result<AccountDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let account =
        accounts::move_account(&mut connection, &account_id, direction).map_err(error_code)?;
    emit(&app, ACCOUNT_EVENT, &account);
    Ok(account)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_delete_confirmation(
    account_id: String,
) -> Result<String, String> {
    confirmations().issue(&account_id).map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_delete(
    app: AppHandle,
    account_id: String,
    confirmation_token: String,
) -> Result<(), String> {
    let mut connection = storage::open().map_err(error_code)?;
    accounts::permanent_delete_account(
        &mut connection,
        confirmations(),
        &account_id,
        &confirmation_token,
    )
    .map_err(error_code)?;
    emit(
        &app,
        ACCOUNT_EVENT,
        serde_json::json!({ "accountId": account_id, "deleted": true }),
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_oauth_begin(
    app: AppHandle,
    store: State<'_, oauth::OAuthSessionStore>,
    method: String,
    callback_port: Option<u16>,
) -> Result<serde_json::Value, String> {
    let result = match method.as_str() {
        "loopback" | "manual" => store.begin_loopback(callback_port.unwrap_or(0)).map(|value| {
            serde_json::json!({ "sessionId": value.session_id, "authorizationUrl": value.authorization_url, "callbackUrl": value.callback_url })
        }),
        "device_code" => store.begin_device_code().map(|value| {
            serde_json::json!({ "sessionId": value.session_id, "userCode": value.user_code, "verificationUrl": value.verification_url, "intervalSeconds": value.interval.as_secs(), "expiresInSeconds": value.expires_in.as_secs() })
        }),
        _ => return Err("invalid_input".to_owned()),
    }
    .map_err(error_code)?;
    emit(&app, OAUTH_EVENT, result.clone());
    Ok(result)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_oauth_complete(
    app: AppHandle,
    store: State<'_, oauth::OAuthSessionStore>,
    session_id: String,
    callback_url: String,
) -> Result<(), String> {
    store
        .complete_callback(&session_id, &callback_url)
        .map_err(error_code)?;
    emit(
        &app,
        OAUTH_EVENT,
        serde_json::json!({ "sessionId": session_id, "state": "completed" }),
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_oauth_cancel(
    app: AppHandle,
    store: State<'_, oauth::OAuthSessionStore>,
    session_id: String,
) -> Result<(), String> {
    store.cancel(&session_id);
    emit(
        &app,
        OAUTH_EVENT,
        serde_json::json!({ "sessionId": session_id, "state": "cancelled" }),
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_quota_list(
    account_id: String,
) -> Result<Vec<QuotaWindowDto>, String> {
    super::quota::load_account_windows(&storage::open().map_err(error_code)?, &account_id)
        .map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_quota_refresh(_account_id: String) -> Result<(), String> {
    Err(oauth::OAUTH_RELEASE_BLOCK_REASON.to_owned())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_models_list() -> Result<Vec<PublicModelDto>, String> {
    models(&storage::open().map_err(error_code)?)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_mapping_list(
    account_id: String,
) -> Result<Vec<ModelMappingDto>, String> {
    let connection = storage::open().map_err(error_code)?;
    let mut statement = connection.prepare(
        "SELECT account_id, public_model_id, upstream_model_id, enabled FROM ai_gateway_account_model_mappings WHERE account_id = ?1 ORDER BY public_model_id",
    ).map_err(|_| "storage_unavailable".to_owned())?;
    let result = statement
        .query_map([account_id], |row| {
            Ok(ModelMappingDto {
                account_id: row.get(0)?,
                public_model_id: row.get(1)?,
                upstream_model_id: row.get(2)?,
                enabled: row.get(3)?,
            })
        })
        .map_err(|_| "storage_unavailable".to_owned())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "storage_unavailable".to_owned());
    result
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_mapping_save(input: MappingInput) -> Result<(), String> {
    accounts::set_model_mapping(
        &storage::open().map_err(error_code)?,
        &ModelMappingDto {
            account_id: input.account_id,
            public_model_id: input.public_model_id,
            upstream_model_id: input.upstream_model_id,
            enabled: input.enabled,
        },
    )
    .map_err(error_code)
}

fn created_key(
    connection: &Connection,
    value: gateway_key::CreatedGatewayKey,
) -> Result<OneTimeGatewayKeyDto, String> {
    let key = keys(connection)?
        .into_iter()
        .find(|item| item.id == value.grant.id)
        .ok_or_else(|| "not_found".to_owned())?;
    Ok(OneTimeGatewayKeyDto {
        key,
        plaintext: value.plaintext,
    })
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_keys_list() -> Result<Vec<GatewayKeyDto>, String> {
    keys(&storage::open().map_err(error_code)?)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_key_create(
    input: CreateKeyInput,
) -> Result<OneTimeGatewayKeyDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let value = gateway_key::create(
        &mut connection,
        &input.name,
        &input.group_ids,
        &input.model_ids,
        input.expires_at.as_deref(),
    )
    .map_err(error_code)?;
    created_key(&connection, value)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_key_regenerate(
    key_id: String,
) -> Result<OneTimeGatewayKeyDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let value = gateway_key::regenerate(&mut connection, &key_id).map_err(error_code)?;
    created_key(&connection, value)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_key_set_enabled(
    key_id: String,
    enabled: bool,
) -> Result<(), String> {
    gateway_key::set_enabled(&storage::open().map_err(error_code)?, &key_id, enabled)
        .map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_key_revoke(key_id: String) -> Result<(), String> {
    gateway_key::revoke(&storage::open().map_err(error_code)?, &key_id).map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_logs_query(input: LogQueryInput) -> Result<LogPageDto, String> {
    let page = request_logs::query_logs(
        &storage::open().map_err(error_code)?,
        &LogFilters {
            started_at_or_after: input.started_at_or_after,
            started_before: input.started_before,
            account_id: input.account_id,
            group_id: input.group_id,
            public_model_id: input.public_model_id,
            upstream_model_id: input.upstream_model_id,
            status: input.status,
            error_code: input.error_code,
            api_key_id: input.api_key_id,
        },
        input.cursor.as_deref(),
        input.page_size,
    )
    .map_err(error_code)?;
    Ok(LogPageDto {
        items: page.items,
        next_cursor: page.next_cursor,
    })
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_log_attempts(
    request_log_id: String,
) -> Result<Vec<request_logs::AttemptRow>, String> {
    request_logs::query_attempts(&storage::open().map_err(error_code)?, &request_log_id)
        .map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_logs_clear(app: AppHandle) -> Result<usize, String> {
    let deleted = request_logs::clear_details(&mut storage::open().map_err(error_code)?)
        .map_err(error_code)?;
    emit(
        &app,
        MAINTENANCE_EVENT,
        serde_json::json!({ "operation": "clear_logs", "state": "completed", "affectedRows": deleted }),
    );
    Ok(deleted)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_prices_list() -> Result<Vec<PriceSnapshot>, String> {
    let connection = storage::open().map_err(error_code)?;
    let mut statement = connection.prepare(
        "SELECT public_model_id, account_id, source, effective_at, input_per_million_usd, output_per_million_usd, cache_read_per_million_usd, cache_write_per_million_usd FROM ai_gateway_model_prices ORDER BY public_model_id, account_id, effective_at DESC",
    ).map_err(|_| "storage_unavailable".to_owned())?;
    let result = statement
        .query_map([], |row| {
            Ok(PriceSnapshot {
                public_model_id: row.get(0)?,
                account_id: row.get(1)?,
                source: row.get(2)?,
                effective_at: row.get(3)?,
                input_per_million_usd: row.get(4)?,
                output_per_million_usd: row.get(5)?,
                cache_read_per_million_usd: row.get(6)?,
                cache_write_per_million_usd: row.get(7)?,
            })
        })
        .map_err(|_| "storage_unavailable".to_owned())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "storage_unavailable".to_owned());
    result
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_price_save(input: PriceInputDto) -> Result<String, String> {
    pricing::save_price(
        &storage::open().map_err(error_code)?,
        pricing::PriceInput {
            public_model_id: &input.public_model_id,
            account_id: input.account_id.as_deref(),
            effective_at: &input.effective_at,
            input_per_million_usd: input.input_per_million_usd.as_deref(),
            output_per_million_usd: input.output_per_million_usd.as_deref(),
            cache_read_per_million_usd: input.cache_read_per_million_usd.as_deref(),
            cache_write_per_million_usd: input.cache_write_per_million_usd.as_deref(),
        },
    )
    .map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_stats_home(
    days: u8,
    filters: Option<HomepageFiltersInput>,
) -> Result<HomepageDto, String> {
    homepage(
        &storage::open().map_err(error_code)?,
        days,
        filters.as_ref(),
    )
}

fn retention(value: Option<u16>) -> Result<RetentionPolicy, String> {
    match value {
        Some(7) => Ok(RetentionPolicy::Days7),
        Some(30) => Ok(RetentionPolicy::Days30),
        Some(90) => Ok(RetentionPolicy::Days90),
        Some(180) => Ok(RetentionPolicy::Days180),
        None => Ok(RetentionPolicy::Forever),
        _ => Err("invalid_input".to_owned()),
    }
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_retention_save(days: Option<u16>) -> Result<(), String> {
    request_logs::set_retention_policy(&storage::open().map_err(error_code)?, retention(days)?)
        .map_err(error_code)
}

fn date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "invalid_input".to_owned())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_maintenance_run(
    app: AppHandle,
    operation: String,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<MaintenanceResultDto, String> {
    emit(
        &app,
        MAINTENANCE_EVENT,
        serde_json::json!({ "operation": operation, "state": "running" }),
    );
    let mut connection = storage::open().map_err(error_code)?;
    let mut result = MaintenanceResultDto {
        operation: operation.clone(),
        affected_rows: 0,
        expected_rows: None,
        actual_rows: None,
        mismatched_rows: None,
    };
    match operation.as_str() {
        "optimize" => request_logs::run_sqlite_maintenance(&connection).map_err(error_code)?,
        "cleanup" => {
            result.affected_rows =
                request_logs::cleanup_retained_details(&mut connection, chrono::Utc::now())
                    .map_err(error_code)?
        }
        "rebuild" => {
            result.affected_rows = request_logs::rebuild_aggregates(
                &mut connection,
                date(
                    start_date
                        .as_deref()
                        .ok_or_else(|| "invalid_input".to_owned())?,
                )?,
                date(
                    end_date
                        .as_deref()
                        .ok_or_else(|| "invalid_input".to_owned())?,
                )?,
            )
            .map_err(error_code)?
        }
        "validate" => {
            let value = request_logs::validate_aggregates(
                &connection,
                date(
                    start_date
                        .as_deref()
                        .ok_or_else(|| "invalid_input".to_owned())?,
                )?,
                date(
                    end_date
                        .as_deref()
                        .ok_or_else(|| "invalid_input".to_owned())?,
                )?,
            )
            .map_err(error_code)?;
            result.expected_rows = Some(value.expected_rows);
            result.actual_rows = Some(value.actual_rows);
            result.mismatched_rows = Some(value.mismatched_rows);
        }
        _ => return Err("invalid_input".to_owned()),
    }
    emit(
        &app,
        MAINTENANCE_EVENT,
        serde_json::json!({ "operation": operation, "state": "completed", "affectedRows": result.affected_rows }),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_sqlite;

    #[test]
    fn homepage_filters_apply_to_account_counts_and_trend_dto() {
        let path = std::env::temp_dir().join(format!(
            "onespace-homepage-filters-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_groups (id, name, sort_order) VALUES ('team', 'Team', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order) VALUES ('account-filtered', 'api_key', 'Filtered', 'team', 0)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order) VALUES ('account-other', 'api_key', 'Other', 'team', 1)",
                [],
            )
            .unwrap();
        let today = Local::now().date_naive().to_string();
        connection
            .execute(
                "INSERT INTO ai_gateway_daily_aggregates (local_date, timezone_name, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, public_model_id, request_count, success_count, failure_count, input_tokens, output_tokens, total_tokens, estimated_cost_usd, cost_calculable) VALUES (?1, 'UTC', 'account-filtered', 'Filtered', 'team', 'Team', 'model-filtered', 3, 2, 1, 10, 20, 30, NULL, 0)",
                [today.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_daily_aggregates (local_date, timezone_name, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, public_model_id, request_count, success_count, failure_count, input_tokens, output_tokens, total_tokens, estimated_cost_usd, cost_calculable) VALUES (?1, 'UTC', 'account-other', 'Other', 'team', 'Team', 'model-other', 9, 9, 0, 90, 90, 180, '2', 1)",
                [today.as_str()],
            )
            .unwrap();

        let homepage = homepage(
            &connection,
            7,
            Some(&HomepageFiltersInput {
                account_id: Some("account-filtered".to_owned()),
                group_id: Some("team".to_owned()),
                public_model_id: Some("model-filtered".to_owned()),
            }),
        )
        .unwrap();
        assert_eq!(homepage.account_count, 1);
        assert_eq!(homepage.available_count, 1);
        assert_eq!(homepage.today.request_count, 3);
        assert_eq!(homepage.today.usage.input_tokens, Some(10));
        assert_eq!(homepage.today.usage.output_tokens, Some(20));
        assert!(!homepage.today.cost_calculable);
        assert_eq!(homepage.today.estimated_cost_usd, None);

        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn all_public_commands_use_the_isolated_prefix() {
        let names = [
            "ai_routing_gateway_bootstrap",
            "ai_routing_gateway_runtime_status",
            "ai_routing_gateway_runtime_start",
            "ai_routing_gateway_runtime_stop",
            "ai_routing_gateway_settings_get",
            "ai_routing_gateway_settings_save",
            "ai_routing_gateway_groups_list",
            "ai_routing_gateway_group_create",
            "ai_routing_gateway_group_delete",
            "ai_routing_gateway_accounts_list",
            "ai_routing_gateway_account_create_api_key",
            "ai_routing_gateway_account_update",
            "ai_routing_gateway_account_move",
            "ai_routing_gateway_account_delete_confirmation",
            "ai_routing_gateway_account_delete",
            "ai_routing_gateway_oauth_begin",
            "ai_routing_gateway_oauth_complete",
            "ai_routing_gateway_oauth_cancel",
            "ai_routing_gateway_quota_list",
            "ai_routing_gateway_quota_refresh",
            "ai_routing_gateway_models_list",
            "ai_routing_gateway_mapping_list",
            "ai_routing_gateway_mapping_save",
            "ai_routing_gateway_keys_list",
            "ai_routing_gateway_key_create",
            "ai_routing_gateway_key_regenerate",
            "ai_routing_gateway_key_set_enabled",
            "ai_routing_gateway_key_revoke",
            "ai_routing_gateway_logs_query",
            "ai_routing_gateway_log_attempts",
            "ai_routing_gateway_logs_clear",
            "ai_routing_gateway_prices_list",
            "ai_routing_gateway_price_save",
            "ai_routing_gateway_stats_home",
            "ai_routing_gateway_retention_save",
            "ai_routing_gateway_maintenance_run",
        ];
        assert!(names
            .iter()
            .all(|name| name.starts_with("ai_routing_gateway_")));
        assert!(names
            .iter()
            .all(|name| !name.starts_with("protocol_router_")));
    }

    #[test]
    fn settings_and_retention_validation_match_the_plan() {
        assert!(retention(Some(7)).is_ok());
        assert!(retention(Some(30)).is_ok());
        assert!(retention(Some(90)).is_ok());
        assert!(retention(Some(180)).is_ok());
        assert!(retention(None).is_ok());
        assert!(retention(Some(14)).is_err());
    }

    #[tokio::test]
    async fn lifecycle_diagnostics_remain_stable_redacted_and_recoverable() {
        let lifecycle = GatewayLifecycle::default();
        let settings = SettingsDto {
            port: 17_688,
            global_quota_threshold_percent: 10,
            log_retention_days: Some(90),
            run_enabled: true,
        };
        lifecycle
            .remember(
                RuntimeStatus::Error {
                    port: settings.port,
                    code: "port_conflict",
                },
                None,
            )
            .await;
        let conflict = runtime_dto(&lifecycle, &settings).await;
        assert_eq!(conflict.state, "error");
        assert!(matches!(conflict.availability, GatewayAvailability::Error));
        assert_eq!(conflict.error_code.as_deref(), Some("port_conflict"));
        let serialized = serde_json::to_string(&conflict).unwrap();
        for forbidden in [
            "Authorization",
            "Bearer fixture-secret",
            "Cookie",
            "prompt body",
            "x-api-key",
        ] {
            assert!(!serialized.contains(forbidden));
        }

        lifecycle.clear_diagnostic().await;
        let recovered = runtime_dto(&lifecycle, &settings).await;
        assert_eq!(recovered.state, "stopped");
        assert!(recovered.error_code.is_none());
    }

    #[tokio::test]
    async fn quota_and_maintenance_schedulers_share_the_managed_lifecycle() {
        let lifecycle = GatewayLifecycle::default();
        let path = std::env::temp_dir().join(format!(
            "onespace-lifecycle-schedulers-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        lifecycle.start_schedulers(path.clone()).await;
        let scheduler_count = lifecycle
            .schedulers
            .lock()
            .await
            .as_ref()
            .map_or(0, |schedulers| schedulers.tasks.len());
        assert_eq!(scheduler_count, 2);
        lifecycle.start_schedulers(path.clone()).await;
        assert_eq!(
            lifecycle
                .schedulers
                .lock()
                .await
                .as_ref()
                .map_or(0, |schedulers| schedulers.tasks.len()),
            2
        );
        lifecycle.stop_schedulers().await;
        assert!(lifecycle.schedulers.lock().await.is_none());
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn managed_startup_dependency_order_is_fixed() {
        let source = include_str!("mod.rs");
        let startup = source
            .split_once("async fn start_managed")
            .expect("managed startup start")
            .1
            .split_once("fn runtime_from_status")
            .expect("managed startup end")
            .0;
        let sqlite = startup.find("storage::open()").expect("SQLite bootstrap");
        let keychain = startup
            .find("initialize_security")
            .expect("Keychain initialization");
        let service = startup
            .find("GatewayHttpService::new")
            .expect("HTTP service initialization");
        let listener = startup
            .find("runtime.start")
            .expect("loopback listener initialization");
        assert!(sqlite < keychain && keychain < service && service < listener);
    }
}
