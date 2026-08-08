use chrono::{DateTime, Days, Local, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::{
    sync::{watch, Mutex},
    task::JoinHandle,
};

use super::{
    accounts::{
        self, CreateApiKeyAccount, CreateApiKeyAccountWithConfiguration, CreateModelMapping,
        CreateModelPrice, DeleteConfirmationStore, UpdateAccount, UpdateApiKeyConnection,
    },
    gateway_key, oauth, pricing,
    request_logs::{self, LogFilters, RetentionPolicy},
    router::{self, HealthTracker},
    runtime::{GatewayHttpRuntime, GatewayHttpService, RuntimeStatus},
    security::{LocalRootKeyStore, RootKey, RootKeyStore, SecurityLockReason, SecurityState},
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

pub(crate) struct GatewayLifecycle {
    runtime: GatewayHttpRuntime,
    operation: Mutex<()>,
    diagnostic: Mutex<Option<RuntimeStatus>>,
    lock_reason: Mutex<Option<String>>,
    schedulers: Mutex<Option<BackgroundSchedulers>>,
    security_store: Arc<StdMutex<Box<dyn RootKeyStore + Send>>>,
    health: Arc<HealthTracker>,
    root_key: StdMutex<Option<Arc<RootKey>>>,
    #[cfg(test)]
    startup_trace: Arc<StdMutex<Vec<&'static str>>>,
}

impl Default for GatewayLifecycle {
    fn default() -> Self {
        Self::with_security_store(Box::new(LocalRootKeyStore::default()))
    }
}

impl GatewayLifecycle {
    fn with_security_store(security_store: Box<dyn RootKeyStore + Send>) -> Self {
        Self {
            runtime: GatewayHttpRuntime::default(),
            operation: Mutex::new(()),
            diagnostic: Mutex::new(None),
            lock_reason: Mutex::new(None),
            schedulers: Mutex::new(None),
            security_store: Arc::new(StdMutex::new(security_store)),
            health: Arc::new(HealthTracker::default()),
            root_key: StdMutex::new(None),
            #[cfg(test)]
            startup_trace: Arc::new(StdMutex::new(Vec::new())),
        }
    }

    fn record_startup_step(&self, step: &'static str) {
        #[cfg(test)]
        self.startup_trace
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(step);
        #[cfg(not(test))]
        let _ = step;
    }

    #[cfg(test)]
    fn startup_trace(&self) -> Vec<&'static str> {
        self.startup_trace
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[cfg(test)]
    fn clear_startup_trace(&self) {
        self.startup_trace
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

impl GatewayLifecycle {
    fn set_root_key(&self, root_key: Arc<RootKey>) {
        *self
            .root_key
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(root_key);
    }

    fn root_key(&self) -> Option<Arc<RootKey>> {
        self.root_key
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn clear_root_key(&self) {
        *self
            .root_key
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
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
    pub(crate) masked_key: String,
    pub(crate) enabled: bool,
    pub(crate) expires_at: Option<String>,
    pub(crate) revoked_at: Option<String>,
    pub(crate) last_used_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) model_ids: Vec<String>,
    pub(crate) today: KeyUsageDto,
    pub(crate) last_30_days: KeyUsageDto,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyUsageDto {
    pub(crate) request_count: u64,
    pub(crate) total_tokens: u64,
    pub(crate) estimated_cost_usd: Option<String>,
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
pub(crate) struct RenameGroupInput {
    pub(crate) group_id: String,
    pub(crate) name: String,
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
pub(crate) struct CreateAccountMappingInput {
    pub(crate) public_model_id: String,
    pub(crate) upstream_model_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateAccountPriceInput {
    pub(crate) public_model_id: String,
    pub(crate) input_per_million_usd: Option<String>,
    pub(crate) output_per_million_usd: Option<String>,
    pub(crate) cache_read_per_million_usd: Option<String>,
    pub(crate) cache_write_per_million_usd: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateAccountWithConfigurationInput {
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) auth_method: String,
    pub(crate) upstream_protocol: UpstreamProtocol,
    #[serde(default)]
    pub(crate) group_id: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    pub(crate) quota_threshold_override_percent: Option<u8>,
    pub(crate) note: String,
    #[serde(default)]
    pub(crate) mappings: Vec<CreateAccountMappingInput>,
    #[serde(default)]
    pub(crate) prices: Vec<CreateAccountPriceInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountIdsInput {
    pub(crate) account_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteAccountsInput {
    pub(crate) account_ids: Vec<String>,
    pub(crate) confirmation_token: String,
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
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
    pub(crate) auth_method: Option<String>,
    pub(crate) upstream_protocol: Option<UpstreamProtocol>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateKeyGroupsInput {
    pub(crate) key_id: String,
    pub(crate) group_ids: Vec<String>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountDeletedEvent {
    account_id: String,
    deleted: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AccountEventPayload<'a> {
    Updated(&'a AccountDto),
    Deleted(AccountDeletedEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OAuthBeginEvent {
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    authorization_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthStateEvent {
    session_id: String,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OAuthEventPayload {
    Begin(OAuthBeginEvent),
    State(OAuthStateEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct MaintenanceEvent {
    operation: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    affected_rows: Option<usize>,
}

enum GatewayEvent<'a> {
    Runtime(&'a RuntimeDto),
    Account(AccountEventPayload<'a>),
    OAuth(OAuthEventPayload),
    Maintenance(MaintenanceEvent),
}

fn serialize_event(event: GatewayEvent<'_>) -> Option<(&'static str, serde_json::Value)> {
    let (name, payload) = match event {
        GatewayEvent::Runtime(payload) => (RUNTIME_EVENT, serde_json::to_value(payload).ok()?),
        GatewayEvent::Account(payload) => (ACCOUNT_EVENT, serde_json::to_value(payload).ok()?),
        GatewayEvent::OAuth(payload) => (OAUTH_EVENT, serde_json::to_value(payload).ok()?),
        GatewayEvent::Maintenance(payload) => {
            (MAINTENANCE_EVENT, serde_json::to_value(payload).ok()?)
        }
    };
    Some((name, payload))
}

fn emit_event<R: Runtime>(app: &AppHandle<R>, event: GatewayEvent<'_>) {
    let Some((name, payload)) = serialize_event(event) else {
        return;
    };
    let _ = app.emit(name, payload);
}

fn error_code(error: impl std::fmt::Display) -> String {
    error.to_string()
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

fn security(lifecycle: &GatewayLifecycle) -> Result<Arc<RootKey>, String> {
    lifecycle
        .root_key()
        .ok_or_else(|| "root_key_missing".to_owned())
}

fn lock_reason(reason: SecurityLockReason) -> String {
    match reason {
        SecurityLockReason::StorageUnavailable => "storage_unavailable",
        SecurityLockReason::RootKeyMissing => "root_key_missing",
        SecurityLockReason::CredentialStoreUnavailable => "credential_store_unavailable",
        SecurityLockReason::RootKeyInvalid => "root_key_invalid",
        SecurityLockReason::MigrationUnavailable => "root_key_migration_unavailable",
        SecurityLockReason::MigrationValidationFailed => "root_key_migration_validation_failed",
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
    let timezone = current_app_timezone();
    let today = timezone.local_date(Utc::now());
    let last_30_days = today
        .checked_sub_days(Days::new(29))
        .ok_or_else(|| "invalid_input".to_owned())?;
    let base = {
        let mut statement = connection
            .prepare("SELECT id, name, key_prefix, key_suffix, enabled, expires_at, revoked_at, last_used_at, created_at FROM ai_gateway_api_keys ORDER BY created_at DESC, id DESC")
            .map_err(|_| "storage_unavailable".to_owned())?;
        let result = statement
            .query_map([], |row| {
                Ok(GatewayKeyDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    masked_key: gateway_key::masked_value(
                        &row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?.as_deref(),
                    ),
                    enabled: row.get(4)?,
                    expires_at: row.get(5)?,
                    revoked_at: row.get(6)?,
                    last_used_at: row.get(7)?,
                    created_at: row.get(8)?,
                    group_ids: Vec::new(),
                    model_ids: Vec::new(),
                    today: KeyUsageDto::default(),
                    last_30_days: KeyUsageDto::default(),
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
            key.today = key_usage(connection, &key.id, today, today, &timezone)?;
            key.last_30_days = key_usage(
                connection,
                &key.id,
                last_30_days,
                today,
                &timezone,
            )?;
            Ok(key)
        })
        .collect()
}

enum AppTimeZone {
    Named(chrono_tz::Tz),
    System,
}

impl AppTimeZone {
    fn local_date(&self, value: DateTime<Utc>) -> NaiveDate {
        match self {
            Self::Named(zone) => value.with_timezone(zone).date_naive(),
            Self::System => value.with_timezone(&Local).date_naive(),
        }
    }
}

fn current_app_timezone() -> AppTimeZone {
    std::env::var("TZ")
        .ok()
        .and_then(|value| value.parse::<chrono_tz::Tz>().ok())
        .map_or(AppTimeZone::System, AppTimeZone::Named)
}

fn key_usage(
    connection: &Connection,
    key_id: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
    timezone: &AppTimeZone,
) -> Result<KeyUsageDto, String> {
    let mut statement = connection
        .prepare("SELECT started_at, total_tokens, estimated_cost_usd, cost_calculable FROM ai_gateway_request_logs WHERE api_key_id_snapshot = ?1")
        .map_err(|_| "storage_unavailable".to_owned())?;
    let rows = statement
        .query_map([key_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(|_| "storage_unavailable".to_owned())?;
    let mut requests = 0u64;
    let mut tokens = 0u64;
    let mut cost = 0.0f64;
    let mut cost_calculable = true;
    for row in rows {
        let (started_at, row_tokens, row_cost, row_calculable) =
            row.map_err(|_| "storage_unavailable".to_owned())?;
        let started_at = DateTime::parse_from_rfc3339(&started_at)
            .map_err(|_| "storage_unavailable".to_owned())?
            .with_timezone(&Utc);
        let local_date = timezone.local_date(started_at);
        if local_date < start_date || local_date > end_date {
            continue;
        }
        requests = requests.saturating_add(1);
        tokens = tokens.saturating_add(
            row_tokens
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(0),
        );
        match (
            row_calculable,
            row_cost.and_then(|value| value.parse::<f64>().ok()),
        ) {
            (true, Some(value)) => cost += value,
            _ => cost_calculable = false,
        }
    }
    Ok(KeyUsageDto {
        request_count: requests,
        total_tokens: tokens,
        estimated_cost_usd: cost_calculable.then(|| format_cost(cost)),
    })
}

fn format_cost(value: f64) -> String {
    let value = format!("{value:.9}");
    value.trim_end_matches('0').trim_end_matches('.').to_owned()
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
    root_key: Option<&RootKey>,
    health: &HealthTracker,
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
    let available_account_ids =
        homepage_available_account_ids(connection, model_filter, root_key, health)?;
    let available_count = selected_accounts
        .iter()
        .filter(|account| available_account_ids.contains(&account.id))
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

fn homepage_available_account_ids(
    connection: &Connection,
    model_filter: Option<&str>,
    root_key: Option<&RootKey>,
    health: &HealthTracker,
) -> Result<HashSet<String>, String> {
    let Some(root_key) = root_key else {
        return Ok(HashSet::new());
    };
    let model_ids = if let Some(model_id) = model_filter {
        vec![model_id.to_owned()]
    } else {
        let mut statement = connection
            .prepare("SELECT id FROM ai_gateway_models WHERE enabled = 1 ORDER BY id")
            .map_err(|_| "storage_unavailable".to_owned())?;
        let result = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| "storage_unavailable".to_owned())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "storage_unavailable".to_owned())?;
        result
    };
    model_ids
        .into_iter()
        .try_fold(HashSet::new(), |mut ids, model_id| {
            let candidates = router::available_account_ids(
                connection,
                &model_id,
                root_key,
                health,
                Instant::now(),
            )
            .map_err(error_code)?;
            ids.extend(candidates);
            Ok(ids)
        })
}

async fn runtime_dto(lifecycle: &GatewayLifecycle, settings: &SettingsDto) -> RuntimeDto {
    let status = lifecycle.status(settings.port).await;
    let (state, port, error_code) = match status {
        RuntimeStatus::Stopped { port } => ("stopped", port, None),
        RuntimeStatus::Running { port } => ("running", port, None),
        RuntimeStatus::Error { port, code } => ("error", port, Some(code.to_owned())),
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
        port,
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

struct PreparedStartup {
    settings: SettingsDto,
    database_path: PathBuf,
    root_key: Arc<RootKey>,
}

async fn storage_failure(
    lifecycle: &GatewayLifecycle,
    port: u16,
    run_enabled: bool,
    code: &'static str,
) -> RuntimeDto {
    lifecycle.runtime.stop(port).await;
    lifecycle.stop_schedulers().await;
    lifecycle.health.reset();
    lifecycle
        .remember(RuntimeStatus::Error { port, code }, None)
        .await;
    unavailable_runtime(port, run_enabled, code)
}

async fn security_failure(
    lifecycle: &GatewayLifecycle,
    settings: SettingsDto,
    reason: String,
) -> RuntimeDto {
    lifecycle.runtime.stop(settings.port).await;
    lifecycle.stop_schedulers().await;
    lifecycle.health.reset();
    lifecycle.clear_root_key();
    lifecycle
        .remember(
            RuntimeStatus::Stopped {
                port: settings.port,
            },
            Some(reason.clone()),
        )
        .await;
    locked_runtime(settings.port, settings.run_enabled, reason)
}

async fn prepare_startup(lifecycle: &GatewayLifecycle) -> Result<PreparedStartup, RuntimeDto> {
    // shared_sqlite::open 负责数据库引导并原子应用迁移。
    let connection = match storage::open() {
        Ok(connection) => {
            lifecycle.record_startup_step("database_migrations");
            connection
        }
        Err(_) => {
            return Err(storage_failure(
                lifecycle,
                super::runtime::DEFAULT_PORT,
                true,
                "storage_unavailable",
            )
            .await)
        }
    };

    let security_state = {
        let key_store = lifecycle
            .security_store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        initialize_lifecycle_security(&connection, key_store.as_ref())
    };
    lifecycle.record_startup_step("local_root_key");

    let settings = match read_settings(&connection) {
        Ok(settings) => {
            lifecycle.record_startup_step("settings");
            settings
        }
        Err(_) => {
            return Err(storage_failure(
                lifecycle,
                super::runtime::DEFAULT_PORT,
                true,
                "storage_unavailable",
            )
            .await)
        }
    };

    let root_key = match security_state {
        SecurityState::Ready(root_key) => root_key,
        SecurityState::Locked(reason) => {
            return Err(security_failure(lifecycle, settings, lock_reason(reason)).await)
        }
    };
    drop(connection);

    let database_path = match crate::shared_sqlite::database_path() {
        Ok(path) => path,
        Err(_) => {
            return Err(storage_failure(
                lifecycle,
                settings.port,
                settings.run_enabled,
                "storage_unavailable",
            )
            .await)
        }
    };
    let root_key = Arc::new(root_key);
    lifecycle.set_root_key(Arc::clone(&root_key));
    Ok(PreparedStartup {
        settings,
        database_path,
        root_key,
    })
}

#[cfg(target_os = "macos")]
fn initialize_lifecycle_security(
    connection: &Connection,
    key_store: &dyn RootKeyStore,
) -> SecurityState {
    super::security::initialize_security_with_migration(
        connection,
        key_store,
        Some(&super::security::MacOsKeychainStore),
    )
}

#[cfg(not(target_os = "macos"))]
fn initialize_lifecycle_security(
    connection: &Connection,
    key_store: &dyn RootKeyStore,
) -> SecurityState {
    super::security::initialize_security(connection, key_store)
}

async fn start_prepared(lifecycle: &GatewayLifecycle, prepared: PreparedStartup) -> RuntimeDto {
    let PreparedStartup {
        settings,
        database_path,
        root_key,
    } = prepared;
    lifecycle.health.reset();
    let service = match GatewayHttpService::new(
        database_path.clone(),
        root_key,
        Arc::clone(&lifecycle.health),
    ) {
        Ok(service) => Arc::new(service),
        Err(_) => {
            return storage_failure(
                lifecycle,
                settings.port,
                settings.run_enabled,
                "gateway_not_ready",
            )
            .await
        }
    };
    lifecycle.record_startup_step("runtime");
    match lifecycle.runtime.start(settings.port, service).await {
        Ok(status @ RuntimeStatus::Running { .. }) => {
            lifecycle.record_startup_step("scheduler");
            lifecycle.start_schedulers(database_path).await;
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

async fn stop_prepared(lifecycle: &GatewayLifecycle, settings: SettingsDto) -> RuntimeDto {
    let status = lifecycle.runtime.stop(settings.port).await;
    lifecycle.stop_schedulers().await;
    lifecycle.health.reset();
    lifecycle.clear_diagnostic().await;
    runtime_from_status(status, settings.run_enabled)
}

async fn initialize_managed(lifecycle: &GatewayLifecycle) -> RuntimeDto {
    let _operation = lifecycle.operation.lock().await;
    let prepared = match prepare_startup(lifecycle).await {
        Ok(prepared) => prepared,
        Err(runtime) => return runtime,
    };
    if prepared.settings.run_enabled {
        start_prepared(lifecycle, prepared).await
    } else {
        stop_prepared(lifecycle, prepared.settings).await
    }
}

async fn start_managed(lifecycle: &GatewayLifecycle) -> RuntimeDto {
    let _operation = lifecycle.operation.lock().await;
    let prepared = match prepare_startup(lifecycle).await {
        Ok(prepared) => prepared,
        Err(runtime) => return runtime,
    };
    start_prepared(lifecycle, prepared).await
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
    lifecycle.health.reset();
    lifecycle.clear_diagnostic().await;
    runtime_from_status(status, settings.run_enabled)
}

pub(crate) async fn initialize<R: Runtime>(app: AppHandle<R>) -> RuntimeDto {
    let lifecycle = app.state::<GatewayLifecycle>();
    let dto = initialize_managed(&lifecycle).await;
    emit_event(&app, GatewayEvent::Runtime(&dto));
    dto
}

pub(crate) async fn shutdown<R: Runtime>(app: &AppHandle<R>) {
    let lifecycle = app.state::<GatewayLifecycle>();
    let port = storage::open()
        .ok()
        .and_then(|connection| read_settings(&connection).ok())
        .map_or(super::runtime::DEFAULT_PORT, |settings| settings.port);
    let _operation = lifecycle.operation.lock().await;
    lifecycle.runtime.stop(port).await;
    lifecycle.stop_schedulers().await;
    lifecycle.health.reset();
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
    let root_key = lifecycle.root_key();
    Ok(BootstrapDto {
        runtime: runtime_dto(&lifecycle, &settings).await,
        settings,
        groups: groups(&connection)?,
        accounts: accounts(&connection)?,
        models: models(&connection)?,
        keys: keys(&connection)?,
        homepage: homepage(
            &connection,
            days.unwrap_or(7),
            filters.as_ref(),
            root_key.as_deref(),
            &lifecycle.health,
        )?,
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
pub(crate) async fn ai_routing_gateway_runtime_start<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, GatewayLifecycle>,
) -> Result<RuntimeDto, String> {
    let dto = start_managed(&lifecycle).await;
    emit_event(&app, GatewayEvent::Runtime(&dto));
    Ok(dto)
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_runtime_stop<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, GatewayLifecycle>,
) -> Result<RuntimeDto, String> {
    let connection = storage::open().map_err(error_code)?;
    let settings = read_settings(&connection)?;
    let dto = stop_managed(&lifecycle, &settings).await;
    emit_event(&app, GatewayEvent::Runtime(&dto));
    Ok(dto)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_settings_get() -> Result<SettingsDto, String> {
    read_settings(&storage::open().map_err(error_code)?)
}

#[tauri::command]
pub(crate) async fn ai_routing_gateway_settings_save<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, GatewayLifecycle>,
    input: SettingsDto,
) -> Result<SettingsDto, String> {
    if input.port == 0
        || input.global_quota_threshold_percent > 100
        || !matches!(input.log_retention_days, Some(7 | 30 | 90 | 180) | None)
    {
        return Err("invalid_input".to_owned());
    }
    let _operation = lifecycle.operation.lock().await;
    let previous = {
        let connection = storage::open().map_err(error_code)?;
        read_settings(&connection)?
    };
    let port_changed = input.port != previous.port;
    if port_changed {
        // 端口变更即使目标端口不可用，也必须先执行受控停止。
        lifecycle.runtime.stop(previous.port).await;
        lifecycle.stop_schedulers().await;
        lifecycle.clear_diagnostic().await;
        if input.run_enabled {
            if let Err(code) = GatewayHttpRuntime::preflight_port(input.port).await {
                let runtime =
                    storage_failure(&lifecycle, input.port, input.run_enabled, code).await;
                emit_event(&app, GatewayEvent::Runtime(&runtime));
                return Err(code.to_owned());
            }
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
        match prepare_startup(&lifecycle).await {
            Ok(prepared) => start_prepared(&lifecycle, prepared).await,
            Err(runtime) => runtime,
        }
    } else {
        stop_prepared(&lifecycle, dto.clone()).await
    };
    emit_event(&app, GatewayEvent::Runtime(&runtime));
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
pub(crate) fn ai_routing_gateway_group_rename(input: RenameGroupInput) -> Result<GroupDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    accounts::rename_group(&mut connection, &input.group_id, &input.name).map_err(error_code)
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
pub(crate) fn ai_routing_gateway_account_create_api_key<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, GatewayLifecycle>,
    input: CreateAccountInput,
) -> Result<AccountDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let root_key = security(&lifecycle)?;
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
    emit_event(
        &app,
        GatewayEvent::Account(AccountEventPayload::Updated(&account)),
    );
    Ok(account)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_create_api_key_with_configuration(
    app: AppHandle,
    lifecycle: State<'_, GatewayLifecycle>,
    input: CreateAccountWithConfigurationInput,
) -> Result<AccountDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let root_key = security(&lifecycle)?;
    let account = accounts::create_api_key_account_with_configuration(
        &mut connection,
        &root_key,
        CreateApiKeyAccountWithConfiguration {
            account: CreateApiKeyAccount {
                name: &input.name,
                base_url: &input.base_url,
                api_key: &input.api_key,
                auth_method: &input.auth_method,
                upstream_protocol: input.upstream_protocol,
                note: &input.note,
            },
            group_id: input.group_id.as_deref(),
            tags: input.tags,
            quota_threshold_override_percent: input.quota_threshold_override_percent,
            mappings: input
                .mappings
                .into_iter()
                .map(|mapping| CreateModelMapping {
                    public_model_id: mapping.public_model_id,
                    upstream_model_id: mapping.upstream_model_id,
                    enabled: mapping.enabled,
                })
                .collect(),
            prices: input
                .prices
                .into_iter()
                .map(|price| CreateModelPrice {
                    public_model_id: price.public_model_id,
                    input_per_million_usd: price.input_per_million_usd,
                    output_per_million_usd: price.output_per_million_usd,
                    cache_read_per_million_usd: price.cache_read_per_million_usd,
                    cache_write_per_million_usd: price.cache_write_per_million_usd,
                })
                .collect(),
        },
    )
    .map_err(error_code)?;
    emit_event(
        &app,
        GatewayEvent::Account(AccountEventPayload::Updated(&account)),
    );
    Ok(account)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_account_update(
    app: AppHandle,
    lifecycle: State<'_, GatewayLifecycle>,
    input: UpdateAccountInput,
) -> Result<AccountDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let transaction = connection
        .transaction()
        .map_err(|_| "storage_unavailable".to_owned())?;
    if input.base_url.is_some()
        || input
            .api_key
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        || input.auth_method.is_some()
        || input.upstream_protocol.is_some()
    {
        let root_key = security(&lifecycle)?;
        accounts::update_api_key_connection(
            &transaction,
            &root_key,
            &input.account_id,
            UpdateApiKeyConnection {
                base_url: input
                    .base_url
                    .as_deref()
                    .ok_or_else(|| "invalid_input".to_owned())?,
                api_key: input.api_key.as_deref(),
                auth_method: input
                    .auth_method
                    .as_deref()
                    .ok_or_else(|| "invalid_input".to_owned())?,
                upstream_protocol: input
                    .upstream_protocol
                    .ok_or_else(|| "invalid_input".to_owned())?,
            },
        )
        .map_err(error_code)?;
    }
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
    emit_event(
        &app,
        GatewayEvent::Account(AccountEventPayload::Updated(&account)),
    );
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
    emit_event(
        &app,
        GatewayEvent::Account(AccountEventPayload::Updated(&account)),
    );
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
    emit_event(
        &app,
        GatewayEvent::Account(AccountEventPayload::Deleted(AccountDeletedEvent {
            account_id,
            deleted: true,
        })),
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_accounts_disable<R: Runtime>(
    app: AppHandle<R>,
    input: AccountIdsInput,
) -> Result<Vec<AccountDto>, String> {
    let accounts = accounts::disable_accounts(
        &mut storage::open().map_err(error_code)?,
        &input.account_ids,
    )
    .map_err(error_code)?;
    for account in &accounts {
        emit_event(
            &app,
            GatewayEvent::Account(AccountEventPayload::Updated(account)),
        );
    }
    Ok(accounts)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_accounts_delete_confirmation(
    input: AccountIdsInput,
) -> Result<String, String> {
    confirmations()
        .issue_batch(&input.account_ids)
        .map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_accounts_delete<R: Runtime>(
    app: AppHandle<R>,
    input: DeleteAccountsInput,
) -> Result<(), String> {
    let account_ids = input.account_ids;
    accounts::permanent_delete_accounts(
        &mut storage::open().map_err(error_code)?,
        confirmations(),
        &account_ids,
        &input.confirmation_token,
    )
    .map_err(error_code)?;
    for account_id in account_ids {
        emit_event(
            &app,
            GatewayEvent::Account(AccountEventPayload::Deleted(AccountDeletedEvent {
                account_id,
                deleted: true,
            })),
        );
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_oauth_begin(
    app: AppHandle,
    store: State<'_, oauth::OAuthSessionStore>,
    method: String,
    callback_port: Option<u16>,
) -> Result<OAuthBeginEvent, String> {
    let result = match method.as_str() {
        "loopback" | "manual" => store
            .begin_loopback(callback_port.unwrap_or(0))
            .map(|value| OAuthBeginEvent {
                session_id: value.session_id,
                authorization_url: Some(value.authorization_url),
                callback_url: Some(value.callback_url),
                user_code: None,
                verification_url: None,
                interval_seconds: None,
                expires_in_seconds: None,
            }),
        "device_code" => store.begin_device_code().map(|value| OAuthBeginEvent {
            session_id: value.session_id,
            authorization_url: None,
            callback_url: None,
            user_code: Some(value.user_code),
            verification_url: Some(value.verification_url),
            interval_seconds: Some(value.interval.as_secs()),
            expires_in_seconds: Some(value.expires_in.as_secs()),
        }),
        _ => return Err("invalid_input".to_owned()),
    }
    .map_err(error_code)?;
    emit_event(
        &app,
        GatewayEvent::OAuth(OAuthEventPayload::Begin(result.clone())),
    );
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
    emit_event(
        &app,
        GatewayEvent::OAuth(OAuthEventPayload::State(OAuthStateEvent {
            session_id,
            state: "completed".to_owned(),
        })),
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
    emit_event(
        &app,
        GatewayEvent::OAuth(OAuthEventPayload::State(OAuthStateEvent {
            session_id,
            state: "cancelled".to_owned(),
        })),
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
        "SELECT ?1, model.id, COALESCE(mapping.upstream_model_id, model.id), COALESCE(mapping.enabled, 1) FROM ai_gateway_models model LEFT JOIN ai_gateway_account_model_mappings mapping ON mapping.account_id = ?1 AND mapping.public_model_id = model.id WHERE model.source = 'official' ORDER BY model.id",
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
    accounts::save_api_key_model_mapping(
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
    lifecycle: State<'_, GatewayLifecycle>,
    input: CreateKeyInput,
) -> Result<OneTimeGatewayKeyDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let root_key = security(&lifecycle)?;
    let value = gateway_key::create(
        &mut connection,
        &root_key,
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
    lifecycle: State<'_, GatewayLifecycle>,
    key_id: String,
) -> Result<OneTimeGatewayKeyDto, String> {
    let mut connection = storage::open().map_err(error_code)?;
    let root_key = security(&lifecycle)?;
    let value = gateway_key::regenerate(&mut connection, &root_key, &key_id).map_err(error_code)?;
    created_key(&connection, value)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_key_copy(
    lifecycle: State<'_, GatewayLifecycle>,
    key_id: String,
) -> Result<String, String> {
    let connection = storage::open().map_err(error_code)?;
    let root_key = security(&lifecycle)?;
    gateway_key::copy_plaintext(&connection, &root_key, &key_id).map_err(error_code)
}

#[tauri::command]
pub(crate) fn ai_routing_gateway_key_groups_update(
    input: UpdateKeyGroupsInput,
) -> Result<Vec<String>, String> {
    gateway_key::replace_groups(
        &mut storage::open().map_err(error_code)?,
        &input.key_id,
        &input.group_ids,
    )
    .map_err(error_code)
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
    emit_event(
        &app,
        GatewayEvent::Maintenance(MaintenanceEvent {
            operation: "clear_logs".to_owned(),
            state: "completed".to_owned(),
            affected_rows: Some(deleted),
        }),
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
    lifecycle: State<'_, GatewayLifecycle>,
    days: u8,
    filters: Option<HomepageFiltersInput>,
) -> Result<HomepageDto, String> {
    let connection = storage::open().map_err(error_code)?;
    let root_key = lifecycle.root_key();
    homepage(
        &connection,
        days,
        filters.as_ref(),
        root_key.as_deref(),
        &lifecycle.health,
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
    emit_event(
        &app,
        GatewayEvent::Maintenance(MaintenanceEvent {
            operation: operation.clone(),
            state: "running".to_owned(),
            affected_rows: None,
        }),
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
    emit_event(
        &app,
        GatewayEvent::Maintenance(MaintenanceEvent {
            operation,
            state: "completed".to_owned(),
            affected_rows: Some(result.affected_rows),
        }),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::security::InitializationLock;
    use super::*;
    use crate::ai_routing_gateway::error::{GatewayError, GatewayErrorCategory};
    use crate::shared_sqlite;
    use std::{
        ffi::OsString,
        path::PathBuf,
        sync::{Mutex as TestMutex, MutexGuard},
    };
    use tauri::Listener;

    #[derive(Default)]
    struct TestKeyStore {
        stored: TestMutex<Option<Vec<u8>>>,
    }

    struct TestInitializationLock;

    impl InitializationLock for TestInitializationLock {}

    impl RootKeyStore for TestKeyStore {
        fn load(&self) -> Result<Option<Vec<u8>>, super::super::error::GatewayError> {
            Ok(self
                .stored
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone())
        }

        fn store(&self, key: &[u8]) -> Result<(), super::super::error::GatewayError> {
            *self
                .stored
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(key.to_vec());
            Ok(())
        }

        fn acquire_initialization_lock(
            &self,
        ) -> Result<Box<dyn InitializationLock + '_>, super::super::error::GatewayError> {
            Ok(Box::new(TestInitializationLock))
        }
    }

    struct TestHome {
        _guard: MutexGuard<'static, ()>,
        original: Option<OsString>,
        path: PathBuf,
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            if let Some(original) = self.original.as_ref() {
                std::env::set_var("HOME", original);
            } else {
                std::env::remove_var("HOME");
            }
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn test_home() -> TestHome {
        let guard = crate::lock_test_home_env();
        let original = std::env::var_os("HOME");
        let path =
            std::env::temp_dir().join(format!("onespace-gateway-command-{}", uuid::Uuid::new_v4()));
        std::env::set_var("HOME", &path);
        TestHome {
            _guard: guard,
            original,
            path,
        }
    }

    async fn free_loopback_port() -> u16 {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    fn write_settings(port: u16, run_enabled: bool) {
        let connection = storage::open().unwrap();
        connection
            .execute(
                "UPDATE ai_gateway_settings SET port = ?1, run_enabled = ?2 WHERE id = 1",
                params![port, run_enabled],
            )
            .unwrap();
    }

    fn test_app() -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_app();
        app.manage(GatewayLifecycle::with_security_store(Box::new(
            TestKeyStore {
                stored: TestMutex::new(Some(vec![17; 32])),
            },
        )));
        app
    }

    #[test]
    fn homepage_filters_apply_to_account_counts_and_trend_dto() {
        let path = std::env::temp_dir().join(format!(
            "onespace-homepage-filters-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = RootKey::try_from(vec![61; 32]).unwrap();
        let health = HealthTracker::default();
        connection
            .execute(
                "INSERT INTO ai_gateway_groups (id, name, sort_order) VALUES ('team', 'Team', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order, base_url, auth_method, upstream_protocol) VALUES ('account-filtered', 'api_key', 'Filtered', 'team', 0, 'http://127.0.0.1:1/v1', 'bearer', 'responses')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order) VALUES ('account-other', 'api_key', 'Other', 'team', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_models (id, display_name, source, capabilities_json) VALUES ('model-filtered', 'Filtered Model', 'official', '{}')",
                [],
            )
            .unwrap();
        let encrypted = super::super::security::encrypt_credential(
            &root_key,
            "third_party_api_key",
            "account-filtered",
            b"SAFE_FIXTURE_API_KEY",
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES (?1, 'third_party_api_key', ?2, ?3, ?4)",
                params![
                    "account-filtered",
                    encrypted.ciphertext,
                    encrypted.nonce.as_slice(),
                    encrypted.cipher_version
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id) VALUES ('account-filtered', 'model-filtered', 'upstream-filtered')",
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
            Some(&root_key),
            &health,
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
    fn gateway_key_usage_uses_the_requested_current_timezone_and_local_date_window() {
        let path = std::env::temp_dir().join(format!(
            "onespace-key-usage-timezone-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        connection.execute("INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('key-stats', 'Stats', 'osk_stats000', X'01', X'02')", []).unwrap();
        for (id, started_at, local_date, timezone, tokens, cost) in [
            (
                "today",
                "2026-08-02T16:30:00Z",
                "2026-08-03",
                "Asia/Shanghai",
                30,
                "0.3",
            ),
            (
                "prior",
                "2026-07-04T16:30:00Z",
                "2026-07-05",
                "Asia/Shanghai",
                20,
                "0.2",
            ),
            (
                "outside",
                "2026-07-03T16:30:00Z",
                "2026-07-04",
                "Asia/Shanghai",
                99,
                "9.9",
            ),
            (
                "old-label",
                "2026-08-02T17:00:00Z",
                "2026-08-02",
                "UTC",
                88,
                "0.8",
            ),
        ] {
            connection.execute(
                "INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, api_key_id_snapshot, status, total_tokens, estimated_cost_usd, cost_calculable) VALUES (?1, ?1, ?2, ?3, ?4, 'responses', 'gpt-5.6-sol', 'key-stats', 'succeeded', ?5, ?6, 1)",
                params![id, started_at, local_date, timezone, tokens, cost],
            ).unwrap();
        }
        let timezone = AppTimeZone::Named("Asia/Shanghai".parse().unwrap());
        let today = key_usage(
            &connection,
            "key-stats",
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            &timezone,
        )
        .unwrap();
        assert_eq!((today.request_count, today.total_tokens), (2, 118));
        assert_eq!(today.estimated_cost_usd.as_deref(), Some("1.1"));
        let month = key_usage(
            &connection,
            "key-stats",
            NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            &timezone,
        )
        .unwrap();
        assert_eq!((month.request_count, month.total_tokens), (3, 138));
        assert_eq!(month.estimated_cost_usd.as_deref(), Some("1.3"));
        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn homepage_does_not_count_an_account_without_route_qualifications() {
        let path = std::env::temp_dir().join(format!(
            "onespace-homepage-routing-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = RootKey::try_from(vec![62; 32]).unwrap();
        let health = HealthTracker::default();
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, base_url, auth_method, upstream_protocol) VALUES ('unroutable', 'api_key', 'Unroutable', 'default', 'http://127.0.0.1:1/v1', 'bearer', 'responses')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id) VALUES ('unroutable', 'gpt-5.6-sol', 'upstream')",
                [],
            )
            .unwrap();

        let homepage = homepage(&connection, 7, None, Some(&root_key), &health).unwrap();
        assert_eq!(homepage.account_count, 1);
        assert_eq!(homepage.available_count, 0);

        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn homepage_available_count_matches_router_qualification_matrix() {
        let path = std::env::temp_dir().join(format!(
            "onespace-homepage-routing-matrix-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = RootKey::try_from(vec![63; 32]).unwrap();
        let health = HealthTracker::default();
        for (id, display_name, enabled) in [
            ("model-home", "Home Model", true),
            ("model-other", "Other Model", true),
            ("model-disabled", "Disabled Model", false),
        ] {
            connection
                .execute(
                    "INSERT INTO ai_gateway_models (id, display_name, enabled, source, capabilities_json) VALUES (?1, ?2, ?3, 'official', '{}')",
                    params![id, display_name, enabled],
                )
                .unwrap();
        }

        let insert_account = |id: &str, enabled: bool, health_status: &str, valid: bool| {
            connection
                .execute(
                    "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, enabled, health_status, base_url, auth_method, upstream_protocol) VALUES (?1, 'api_key', ?1, 'default', ?2, ?3, 'http://127.0.0.1:1/v1', 'bearer', 'responses')",
                    params![id, enabled, health_status],
                )
                .unwrap();
            let mut encrypted = super::super::security::encrypt_credential(
                &root_key,
                "third_party_api_key",
                id,
                b"SAFE_FIXTURE_API_KEY",
            )
            .unwrap();
            if !valid {
                encrypted.ciphertext[0] ^= 1;
            }
            connection
                .execute(
                    "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES (?1, 'third_party_api_key', ?2, ?3, ?4)",
                    params![id, encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
                )
                .unwrap();
        };

        insert_account("valid", true, "unknown", true);
        insert_account("stale", true, "unknown", true);
        insert_account("no-mapping", true, "unknown", true);
        insert_account("invalid-credential", true, "unknown", false);
        insert_account("disabled", false, "unknown", true);
        insert_account("persistent-unavailable", true, "unavailable", true);
        insert_account("exhausted", true, "unknown", true);
        insert_account("other-model", true, "unknown", true);
        insert_account("disabled-model", true, "unknown", true);

        for (account_id, model_id, enabled) in [
            ("valid", "model-home", true),
            ("stale", "model-home", true),
            ("invalid-credential", "model-home", true),
            ("disabled", "model-home", true),
            ("persistent-unavailable", "model-home", true),
            ("exhausted", "model-home", true),
            ("other-model", "model-other", true),
            ("disabled-model", "model-disabled", true),
        ] {
            connection
                .execute(
                    "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id, enabled) VALUES (?1, ?2, 'upstream', ?3)",
                    params![account_id, model_id, enabled],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO ai_gateway_quota_windows (id, account_id, name, scope_type, remaining_percent, is_stale) VALUES ('stale-window', 'stale', 'Stale', 'global', 0, 1), ('exhausted-window', 'exhausted', 'Exhausted', 'global', 0, 0)",
                [],
            )
            .unwrap();

        let all_models = homepage(&connection, 7, None, Some(&root_key), &health).unwrap();
        assert_eq!(all_models.account_count, 9);
        assert_eq!(all_models.available_count, 5);
        assert_eq!(all_models.unavailable_count, 4);
        assert_eq!(all_models.stale_count, 1);

        let home_model = homepage(
            &connection,
            7,
            Some(&HomepageFiltersInput {
                account_id: None,
                group_id: None,
                public_model_id: Some("model-home".to_owned()),
            }),
            Some(&root_key),
            &health,
        )
        .unwrap();
        assert_eq!(home_model.available_count, 5);

        let now = Instant::now();
        for _ in 0..3 {
            health.record_failure("valid", router::AttemptFailure::Network, now);
        }
        let health_blocked = homepage(
            &connection,
            7,
            Some(&HomepageFiltersInput {
                account_id: None,
                group_id: None,
                public_model_id: Some("model-home".to_owned()),
            }),
            Some(&root_key),
            &health,
        )
        .unwrap();
        assert_eq!(health_blocked.available_count, 4);

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
            "ai_routing_gateway_group_rename",
            "ai_routing_gateway_group_delete",
            "ai_routing_gateway_accounts_list",
            "ai_routing_gateway_account_create_api_key",
            "ai_routing_gateway_account_create_api_key_with_configuration",
            "ai_routing_gateway_account_update",
            "ai_routing_gateway_account_move",
            "ai_routing_gateway_account_delete_confirmation",
            "ai_routing_gateway_account_delete",
            "ai_routing_gateway_accounts_disable",
            "ai_routing_gateway_accounts_delete_confirmation",
            "ai_routing_gateway_accounts_delete",
            "ai_routing_gateway_quota_list",
            "ai_routing_gateway_quota_refresh",
            "ai_routing_gateway_models_list",
            "ai_routing_gateway_mapping_list",
            "ai_routing_gateway_mapping_save",
            "ai_routing_gateway_keys_list",
            "ai_routing_gateway_key_create",
            "ai_routing_gateway_key_regenerate",
            "ai_routing_gateway_key_copy",
            "ai_routing_gateway_key_groups_update",
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
    fn account_pool_inputs_deserialize_camel_case_contract() {
        let create: CreateAccountWithConfigurationInput =
            serde_json::from_value(serde_json::json!({
                "name": "Account",
                "baseUrl": "https://api.example.com/v1",
                "apiKey": "secret",
                "authMethod": "bearer",
                "upstreamProtocol": "responses",
                "groupId": "group-1",
                "tags": ["team"],
                "quotaThresholdOverridePercent": 80,
                "note": "note",
                "mappings": [],
                "prices": []
            }))
            .unwrap();
        assert_eq!(create.group_id.as_deref(), Some("group-1"));
        assert_eq!(create.tags, vec!["team"]);
        assert_eq!(create.quota_threshold_override_percent, Some(80));

        let rename: RenameGroupInput =
            serde_json::from_value(serde_json::json!({"groupId": "group-1", "name": "Renamed"}))
                .unwrap();
        assert_eq!(rename.group_id, "group-1");
        let disable: AccountIdsInput = serde_json::from_value(serde_json::json!({
            "accountIds": ["account-1", "account-2"]
        }))
        .unwrap();
        assert_eq!(disable.account_ids, vec!["account-1", "account-2"]);
        let batch: DeleteAccountsInput = serde_json::from_value(serde_json::json!({
            "accountIds": ["account-1", "account-2"],
            "confirmationToken": "token"
        }))
        .unwrap();
        assert_eq!(batch.account_ids, vec!["account-1", "account-2"]);
        assert_eq!(batch.confirmation_token, "token");
    }

    #[test]
    fn account_pool_error_categories_keep_the_public_command_contract() {
        for (category, expected) in [
            (GatewayErrorCategory::InvalidInput, "invalid_input:fixture"),
            (GatewayErrorCategory::NotFound, "not_found:fixture"),
            (GatewayErrorCategory::Conflict, "conflict:fixture"),
            (
                GatewayErrorCategory::ConfirmationRequired,
                "confirmation_required:fixture",
            ),
            (
                GatewayErrorCategory::StorageUnavailable,
                "storage_unavailable:fixture",
            ),
        ] {
            assert_eq!(
                error_code(GatewayError::new(category, Some("fixture"))),
                expected
            );
        }
    }

    #[test]
    fn account_pool_commands_use_real_storage_confirmation_and_account_events() {
        let _home = test_home();
        let connection = storage::open().unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_groups (id, name, sort_order, is_default) VALUES ('team', 'Team', 1, 0)",
                [],
            )
            .unwrap();
        for (id, name, sort_order) in [("account-a", "Account A", 0), ("account-b", "Account B", 1)]
        {
            connection
                .execute(
                    "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order) VALUES (?1, 'api_key', ?2, 'team', ?3)",
                    params![id, name, sort_order],
                )
                .unwrap();
        }
        drop(connection);

        let app = test_app();
        let events = Arc::new(TestMutex::new(Vec::<serde_json::Value>::new()));
        let event_sink = Arc::clone(&events);
        let listener = app.listen(ACCOUNT_EVENT, move |event| {
            event_sink
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(serde_json::from_str(event.payload()).unwrap());
        });

        let renamed = ai_routing_gateway_group_rename(RenameGroupInput {
            group_id: "team".to_owned(),
            name: "Platform".to_owned(),
        })
        .unwrap();
        assert_eq!(renamed.id, "team");
        assert_eq!(renamed.name, "Platform");
        assert!(!renamed.is_default);
        assert_eq!(
            ai_routing_gateway_group_rename(RenameGroupInput {
                group_id: "default".to_owned(),
                name: "Protected".to_owned(),
            })
            .unwrap_err(),
            "conflict:default"
        );

        let disabled = ai_routing_gateway_accounts_disable(
            app.handle().clone(),
            AccountIdsInput {
                account_ids: vec!["account-b".to_owned(), "account-a".to_owned()],
            },
        )
        .unwrap();
        assert_eq!(
            disabled
                .iter()
                .map(|account| account.id.as_str())
                .collect::<Vec<_>>(),
            vec!["account-a", "account-b"]
        );
        assert!(disabled.iter().all(|account| !account.enabled));
        assert_eq!(
            ai_routing_gateway_accounts_disable(
                app.handle().clone(),
                AccountIdsInput {
                    account_ids: vec!["missing".to_owned()],
                },
            )
            .unwrap_err(),
            "not_found:missing"
        );

        let confirmation = ai_routing_gateway_accounts_delete_confirmation(AccountIdsInput {
            account_ids: vec!["account-b".to_owned(), "account-a".to_owned()],
        })
        .unwrap();
        assert_eq!(
            ai_routing_gateway_accounts_delete(
                app.handle().clone(),
                DeleteAccountsInput {
                    account_ids: vec!["account-a".to_owned()],
                    confirmation_token: confirmation,
                },
            )
            .unwrap_err(),
            "confirmation_required:account-a"
        );
        let confirmation = ai_routing_gateway_accounts_delete_confirmation(AccountIdsInput {
            account_ids: vec!["account-a".to_owned(), "account-b".to_owned()],
        })
        .unwrap();
        ai_routing_gateway_accounts_delete(
            app.handle().clone(),
            DeleteAccountsInput {
                account_ids: vec!["account-b".to_owned(), "account-a".to_owned()],
                confirmation_token: confirmation,
            },
        )
        .unwrap();

        let captured = events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        assert_eq!(captured.len(), 4);
        assert_eq!(
            captured[..2]
                .iter()
                .map(|event| event["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["account-a", "account-b"]
        );
        assert!(captured[..2]
            .iter()
            .all(|event| event["enabled"].as_bool() == Some(false)));
        assert_eq!(
            captured[2..]
                .iter()
                .map(|event| event["accountId"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["account-b", "account-a"]
        );
        assert!(captured[2..]
            .iter()
            .all(|event| event["deleted"].as_bool() == Some(true)));

        let connection = storage::open().unwrap();
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_accounts WHERE id IN ('account-a', 'account-b')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
        app.unlisten(listener);
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
            "Bearer SAFE_FIXTURE_BEARER",
            "Cookie",
            "SAFE_FIXTURE_PROMPT_BODY",
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
    async fn runtime_diagnostic_preserves_the_attempted_port() {
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
                    port: 17_689,
                    code: "port_conflict",
                },
                None,
            )
            .await;

        let diagnostic = runtime_dto(&lifecycle, &settings).await;

        assert_eq!(diagnostic.port, 17_689);
        assert_eq!(diagnostic.error_code.as_deref(), Some("port_conflict"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn initialize_uses_the_real_managed_flow_and_fixed_dependency_order() {
        let _home = test_home();
        let port = free_loopback_port().await;
        write_settings(port, true);
        let app = test_app();
        let handle = app.handle().clone();

        initialize(handle.clone()).await;

        let lifecycle = handle.state::<GatewayLifecycle>();
        assert_eq!(
            lifecycle.startup_trace(),
            vec![
                "database_migrations",
                "local_root_key",
                "settings",
                "runtime",
                "scheduler"
            ]
        );
        assert_eq!(
            lifecycle.runtime.status(port).await,
            RuntimeStatus::Running { port }
        );
        assert!(lifecycle.schedulers.lock().await.is_some());

        shutdown(&handle).await;
        assert_eq!(
            lifecycle.runtime.status(port).await,
            RuntimeStatus::Stopped { port }
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn settings_save_uses_the_real_flow_for_conflict_rebind_and_stop() {
        let _home = test_home();
        let old_port = free_loopback_port().await;
        let new_port = loop {
            let port = free_loopback_port().await;
            if port != old_port {
                break port;
            }
        };
        write_settings(old_port, true);
        let occupied = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, new_port))
            .await
            .unwrap();
        let app = test_app();
        let handle = app.handle().clone();

        initialize(handle.clone()).await;
        let lifecycle = handle.state::<GatewayLifecycle>();
        assert_eq!(
            lifecycle.runtime.status(old_port).await,
            RuntimeStatus::Running { port: old_port }
        );
        assert!(lifecycle.schedulers.lock().await.is_some());

        let conflict = ai_routing_gateway_settings_save(
            handle.clone(),
            handle.state::<GatewayLifecycle>(),
            SettingsDto {
                port: new_port,
                global_quota_threshold_percent: 10,
                log_retention_days: Some(90),
                run_enabled: true,
            },
        )
        .await;
        assert!(matches!(conflict, Err(code) if code == "port_conflict"));
        assert!(
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, old_port))
                .await
                .is_ok()
        );
        assert!(lifecycle.schedulers.lock().await.is_none());
        let settings = read_settings(&storage::open().unwrap()).unwrap();
        let diagnostic = runtime_dto(&lifecycle, &settings).await;
        assert_eq!(diagnostic.state, "error");
        assert_eq!(diagnostic.port, new_port);
        assert_eq!(diagnostic.error_code.as_deref(), Some("port_conflict"));

        let account = ai_routing_gateway_account_create_api_key(
            handle.clone(),
            handle.state::<GatewayLifecycle>(),
            CreateAccountInput {
                name: "Post-conflict account".to_owned(),
                base_url: "https://api.example.com/v1".to_owned(),
                api_key: "SAFE_FIXTURE_POST_CONFLICT_API_KEY".to_owned(),
                auth_method: "bearer".to_owned(),
                upstream_protocol: UpstreamProtocol::Responses,
                note: "post-conflict fixture".to_owned(),
            },
        )
        .expect("verified root key remains available after port conflict");
        assert_eq!(account.name, "Post-conflict account");
        let gateway_key = ai_routing_gateway_key_create(
            handle.state::<GatewayLifecycle>(),
            CreateKeyInput {
                name: "Post-conflict gateway key".to_owned(),
                group_ids: vec!["default".to_owned()],
                model_ids: vec!["gpt-5.6-sol".to_owned()],
                expires_at: None,
            },
        )
        .expect("gateway API key command remains usable after port conflict");
        assert_eq!(
            ai_routing_gateway_key_copy(
                handle.state::<GatewayLifecycle>(),
                gateway_key.key.id.clone(),
            )
            .expect("decrypt gateway API key after port conflict"),
            gateway_key.plaintext
        );
        assert!(lifecycle.root_key().is_some());

        drop(occupied);
        let rebound = ai_routing_gateway_settings_save(
            handle.clone(),
            handle.state::<GatewayLifecycle>(),
            SettingsDto {
                port: new_port,
                global_quota_threshold_percent: 10,
                log_retention_days: Some(90),
                run_enabled: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(rebound.port, new_port);
        assert_eq!(
            lifecycle.runtime.status(new_port).await,
            RuntimeStatus::Running { port: new_port }
        );
        assert!(lifecycle.schedulers.lock().await.is_some());

        let stopped = ai_routing_gateway_settings_save(
            handle.clone(),
            handle.state::<GatewayLifecycle>(),
            SettingsDto {
                port: new_port,
                global_quota_threshold_percent: 10,
                log_retention_days: Some(90),
                run_enabled: false,
            },
        )
        .await
        .unwrap();
        assert!(!stopped.run_enabled);
        assert_eq!(
            lifecycle.runtime.status(new_port).await,
            RuntimeStatus::Stopped { port: new_port }
        );
        assert!(lifecycle.schedulers.lock().await.is_none());
        assert!(
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, new_port))
                .await
                .is_ok()
        );

        shutdown(&handle).await;
    }

    #[test]
    fn four_gateway_event_payloads_have_one_stable_typed_contract_each() {
        let runtime = RuntimeDto {
            state: "error".to_owned(),
            availability: GatewayAvailability::Error,
            port: 17_689,
            run_enabled: true,
            error_code: Some("port_conflict".to_owned()),
            lock_reason: None,
        };
        let account = AccountDto {
            id: "account-1".to_owned(),
            stable_external_id: None,
            account_type: super::super::types::AccountType::ApiKey,
            name: "Fixture Account".to_owned(),
            group_id: "default".to_owned(),
            sort_order: 0,
            note: "safe fixture".to_owned(),
            enabled: true,
            health_status: "healthy".to_owned(),
            quota_threshold_override_percent: None,
            base_url: Some("http://127.0.0.1:18000/v1".to_owned()),
            auth_method: Some("bearer".to_owned()),
            upstream_protocol: Some(super::super::types::UpstreamProtocol::Responses),
            tags: Vec::new(),
            model_mappings: Vec::new(),
        };
        let contracts = [
            serialize_event(GatewayEvent::Runtime(&runtime)).unwrap(),
            serialize_event(GatewayEvent::Account(AccountEventPayload::Updated(
                &account,
            )))
            .unwrap(),
            serialize_event(GatewayEvent::OAuth(OAuthEventPayload::State(
                OAuthStateEvent {
                    session_id: "fixture-session".to_owned(),
                    state: "completed".to_owned(),
                },
            )))
            .unwrap(),
            serialize_event(GatewayEvent::Maintenance(MaintenanceEvent {
                operation: "cleanup".to_owned(),
                state: "completed".to_owned(),
                affected_rows: Some(0),
            }))
            .unwrap(),
        ];
        assert_eq!(
            contracts.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            vec![RUNTIME_EVENT, ACCOUNT_EVENT, OAUTH_EVENT, MAINTENANCE_EVENT]
        );
        assert_eq!(contracts[0].1["run_enabled"], true);
        assert_eq!(contracts[1].1["account_type"], "api_key");
        assert_eq!(contracts[2].1["state"], "completed");
        assert_eq!(contracts[3].1["affectedRows"], 0);
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
}
#[test]
fn configured_api_key_input_deserializes_the_frozen_camel_case_contract() {
    let input: CreateAccountWithConfigurationInput = serde_json::from_value(serde_json::json!({
            "name": "Atomic",
            "baseUrl": "https://api.example.com/v1",
            "apiKey": "SAFE_FIXTURE_KEY",
            "authMethod": "bearer",
            "upstreamProtocol": "responses",
            "note": "fixture",
            "mappings": [{ "publicModelId": "gpt-test", "upstreamModelId": "vendor", "enabled": false }],
            "prices": [{
                "publicModelId": "gpt-test",
                "inputPerMillionUsd": "1",
                "outputPerMillionUsd": "2",
                "cacheReadPerMillionUsd": "0.1",
                "cacheWritePerMillionUsd": null
            }]
        }))
        .unwrap();
    assert_eq!(input.base_url, "https://api.example.com/v1");
    assert_eq!(input.mappings[0].upstream_model_id, "vendor");
    assert!(!input.mappings[0].enabled);
    assert_eq!(
        input.prices[0].cache_read_per_million_usd.as_deref(),
        Some("0.1")
    );
    assert_eq!(input.prices[0].cache_write_per_million_usd, None);
}
