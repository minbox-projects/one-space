use crate::{ai_env, ai_sessions, config, git, mcp_servers, secrets, storage};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 1;
const OUTBOX_DEDUP_WINDOW_SECS: u64 = 3;
const MANAGED_TOOLS: [&str; 3] = ["claude", "codex", "gemini"];
const LAUNCHER_EXPORT_VERSION: u32 = 1;
const LAUNCHER_TYPES: [&str; 5] = ["app", "script", "url", "folder", "internal"];

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub schema_version: u32,
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiErr {
    pub ok: bool,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

fn api_ok<T: Serialize>(data: T, meta: ApiMeta) -> Result<ApiOk<T>, ApiErr> {
    Ok(ApiOk { ok: true, data, meta })
}

fn api_error(code: &str, message: impl Into<String>) -> ApiErr {
    ApiErr {
        ok: false,
        code: code.to_string(),
        message: message.into(),
        details: None,
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchemaMeta {
    pub schema_version: u32,
    pub created_at: u64,
    pub last_migrated_at: u64,
    pub revision: u64,
}

impl Default for SchemaMeta {
    fn default() -> Self {
        let now = now_ts();
        Self {
            schema_version: SCHEMA_VERSION,
            created_at: now,
            last_migrated_at: now,
            revision: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderCore {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderRuntimePolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderHistoryEntry {
    pub ts: u64,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderRecord {
    pub core: ProviderCore,
    pub runtime_policy: ProviderRuntimePolicy,
    #[serde(default)]
    pub tool_config: Map<String, Value>,
    #[serde(default)]
    pub history: Vec<ProviderHistoryEntry>,
    #[serde(default)]
    pub extra: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProvidersState {
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub providers: Vec<ProviderRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub name: String,
    pub working_dir: String,
    pub tool: String,
    pub tool_session_id: String,
    pub created_at: u64,
    pub last_used_at: u64,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SessionsState {
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LauncherRecord {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub target: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pin_order: u32,
    #[serde(default)]
    pub launch_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_launched_at: Option<u64>,
    #[serde(default)]
    pub trusted: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LauncherState {
    #[serde(default)]
    pub items: Vec<LauncherRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EncryptedBlob {
    #[serde(default)]
    pub is_encrypted: bool,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutboxEvent {
    pub id: String,
    pub domain: String,
    pub reason: String,
    pub created_at: u64,
    pub attempts: u32,
    pub next_retry_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutboxState {
    #[serde(default)]
    pub events: Vec<OutboxEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<u64>,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub last_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl Default for OutboxState {
    fn default() -> Self {
        Self {
            events: vec![],
            last_run_at: None,
            running: false,
            last_status: "idle".to_string(),
            last_error: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSnapshot {
    pub providers: Value,
    pub sessions: Value,
    pub config: Value,
    pub schema: SchemaMeta,
    pub outbox: OutboxState,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigrationState {
    pub migrated: bool,
    pub schema_version: u32,
    pub last_migrated_at: Option<u64>,
    pub last_backup_id: Option<String>,
    pub in_progress: bool,
    pub last_error: Option<String>,
}

impl Default for MigrationState {
    fn default() -> Self {
        Self {
            migrated: false,
            schema_version: 0,
            last_migrated_at: None,
            last_backup_id: None,
            in_progress: false,
            last_error: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MigrationReport {
    pub started_at: u64,
    pub finished_at: u64,
    pub success: bool,
    pub backup_id: String,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderInput {
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub provider_key: Option<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionInput {
    pub id: Option<String>,
    pub name: String,
    pub working_dir: String,
    pub tool: String,
    pub tool_session_id: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LauncherItemInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub item_type: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub pin_order: Option<u32>,
    #[serde(default)]
    pub launch_count: Option<u64>,
    #[serde(default)]
    pub last_launched_at: Option<u64>,
    #[serde(default)]
    pub trusted: Option<bool>,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<u64>,
}

struct StorageEngine;

impl StorageEngine {
    fn base_dir() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        let target = root.join("data");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        Ok(target)
    }

    fn meta_dir() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("meta");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }

    fn providers_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("providers");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn sessions_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("sessions");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn launcher_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("launcher");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn secrets_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("secrets");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.enc.json"))
    }

    fn mcp_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("mcp");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn content_path(name: &str) -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("content");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join(format!("{}.enc.json", name)))
    }

    fn outbox_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("events");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("outbox.json"))
    }

    fn schema_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("schema.json"))
    }

    fn migration_state_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("migration_state.json"))
    }

    fn migration_report_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("migration_report.json"))
    }

    fn backup_root() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("backups");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }

    fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension("tmp");
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, String> {
        if !path.exists() {
            return Ok(T::default());
        }
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok(T::default());
        }
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
        let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        Self::atomic_write(path, &content)
    }

    fn load_schema() -> Result<SchemaMeta, String> {
        let path = Self::schema_path()?;
        if !path.exists() {
            let schema = SchemaMeta::default();
            Self::write_json(&path, &schema)?;
            return Ok(schema);
        }
        Self::read_json(&path)
    }

    fn bump_revision() -> Result<SchemaMeta, String> {
        let mut schema = Self::load_schema()?;
        schema.revision = schema.revision.saturating_add(1);
        schema.last_migrated_at = now_ts();
        Self::write_json(&Self::schema_path()?, &schema)?;
        Ok(schema)
    }
}

struct CryptoService;

impl CryptoService {
    fn encrypt(value: &str) -> Result<String, String> {
        let password = crate::crypto::get_or_init_master_password()?;
        crate::crypto::encrypt(value, &password)
    }

    fn decrypt(value: &str) -> Result<String, String> {
        let password = crate::crypto::get_or_init_master_password()?;
        crate::crypto::decrypt(value, &password)
    }

    fn encrypt_json(value: &Value) -> Result<EncryptedBlob, String> {
        Ok(EncryptedBlob {
            is_encrypted: true,
            data: Self::encrypt(&value.to_string())?,
        })
    }

    fn decrypt_json(blob: &EncryptedBlob) -> Result<Value, String> {
        if !blob.is_encrypted {
            return serde_json::from_str(&blob.data).map_err(|e| e.to_string());
        }
        let plain = Self::decrypt(&blob.data)?;
        serde_json::from_str(&plain).map_err(|e| e.to_string())
    }
}

fn provider_to_legacy(record: &ProviderRecord) -> Value {
    let mut map = Map::new();
    map.insert("id".to_string(), Value::String(record.core.id.clone()));
    map.insert("name".to_string(), Value::String(record.core.name.clone()));
    map.insert("tool".to_string(), Value::String(record.core.tool.clone()));
    map.insert("api_key".to_string(), Value::String(record.core.api_key.clone()));
    if let Some(v) = &record.core.base_url {
        map.insert("base_url".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = &record.core.model {
        map.insert("model".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = &record.is_enabled {
        map.insert("is_enabled".to_string(), Value::Bool(*v));
    }
    if let Some(v) = &record.provider_key {
        map.insert("provider_key".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = &record.runtime_policy.approval_policy {
        map.insert("approval_policy".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = &record.runtime_policy.sandbox_mode {
        map.insert("sandbox_mode".to_string(), Value::String(v.clone()));
    }
    if !record.history.is_empty() {
        let arr: Vec<Value> = record
            .history
            .iter()
            .map(|h| {
                json!({
                    "timestamp": h.ts.saturating_mul(1000),
                    "content": h.summary.clone().unwrap_or_default()
                })
            })
            .collect();
        map.insert("history".to_string(), Value::Array(arr));
    }
    for (k, v) in &record.tool_config {
        map.insert(k.clone(), v.clone());
    }
    for (k, v) in &record.extra {
        map.insert(k.clone(), v.clone());
    }
    Value::Object(map)
}

fn provider_from_input(input: ProviderInput, old: Option<&ProviderRecord>) -> ProviderRecord {
    let mut tool_config = old.map(|o| o.tool_config.clone()).unwrap_or_default();
    let mut extra = old.map(|o| o.extra.clone()).unwrap_or_default();

    for (k, v) in input.fields {
        match k.as_str() {
            "approval_policy" => {}
            "sandbox_mode" => {}
            _ => {
                tool_config.insert(k, v);
            }
        }
    }

    if let Some(o) = old {
        for (k, v) in &o.extra {
            extra.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    let mut history = old.map(|o| o.history.clone()).unwrap_or_default();
    history.insert(
        0,
        ProviderHistoryEntry {
            ts: now_ts(),
            action: if old.is_some() {
                "upsert".to_string()
            } else {
                "create".to_string()
            },
            summary: Some(format!("provider:{} tool:{}", input.id, input.tool)),
        },
    );
    history.truncate(50);

    ProviderRecord {
        core: ProviderCore {
            id: input.id,
            name: input.name,
            tool: input.tool,
            api_key: input.api_key,
            base_url: input.base_url,
            model: input.model,
        },
        runtime_policy: ProviderRuntimePolicy {
            approval_policy: tool_config
                .get("approval_policy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            sandbox_mode: tool_config
                .get("sandbox_mode")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
        tool_config,
        history,
        extra,
        is_enabled: input.is_enabled,
        provider_key: input.provider_key,
    }
}

fn load_providers_state() -> Result<ProvidersState, String> {
    let path = StorageEngine::providers_path()?;
    if !path.exists() {
        return Ok(ProvidersState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(ProvidersState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<ProvidersState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<ProvidersState>(&content).map_err(|e| e.to_string())
}

fn save_providers_state(state: &ProvidersState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::providers_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn load_sessions_state() -> Result<SessionsState, String> {
    let path = StorageEngine::sessions_path()?;
    if !path.exists() {
        return Ok(SessionsState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(SessionsState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<SessionsState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<SessionsState>(&content).map_err(|e| e.to_string())
}

fn save_sessions_state(state: &SessionsState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::sessions_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn load_launcher_state() -> Result<LauncherState, String> {
    let path = StorageEngine::launcher_path()?;
    if !path.exists() {
        return Ok(LauncherState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(LauncherState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<LauncherState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<LauncherState>(&content).map_err(|e| e.to_string())
}

fn save_launcher_state(state: &LauncherState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::launcher_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn load_outbox_state() -> Result<OutboxState, String> {
    let path = StorageEngine::outbox_path()?;
    if !path.exists() {
        return Ok(OutboxState::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(OutboxState::default());
    }

    match serde_json::from_str::<OutboxState>(&content) {
        Ok(state) => Ok(state),
        Err(strict_err) => {
            if let Some(recovered) = parse_first_json_value::<OutboxState>(&content) {
                // Self-heal corrupted trailing bytes and continue.
                let _ = StorageEngine::write_json(&path, &recovered);
                Ok(recovered)
            } else {
                Err(strict_err.to_string())
            }
        }
    }
}

fn save_outbox_state(state: &OutboxState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::outbox_path()?, state)
}

fn load_migration_state() -> Result<MigrationState, String> {
    StorageEngine::read_json(&StorageEngine::migration_state_path()?)
}

fn save_migration_state(state: &MigrationState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_state_path()?, state)
}

fn get_meta() -> Result<ApiMeta, String> {
    let schema = StorageEngine::load_schema()?;
    Ok(ApiMeta {
        schema_version: schema.schema_version,
        revision: schema.revision,
    })
}

fn extract_fields(value: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            match k.as_str() {
                "id" | "name" | "tool" | "api_key" | "base_url" | "model" | "is_enabled" | "provider_key" => {}
                _ => {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LegacyProvidersView {
    active_claude: Option<String>,
    active_codex: Option<String>,
    active_gemini: Option<String>,
    active_opencode: Option<String>,
    providers: Vec<Value>,
}

fn providers_to_legacy_view(state: &ProvidersState) -> LegacyProvidersView {
    let get = |k: &str| state.active.get(k).cloned();
    LegacyProvidersView {
        active_claude: get("claude"),
        active_codex: get("codex"),
        active_gemini: get("gemini"),
        active_opencode: get("opencode"),
        providers: state.providers.iter().map(provider_to_legacy).collect(),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliInstallCommand {
    pub label: String,
    pub command: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliInstallGuide {
    pub docs_url: String,
    pub commands: Vec<CliInstallCommand>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliEnvProbeResult {
    pub tool: String,
    pub installed: bool,
    pub version: String,
    pub configured: bool,
    pub importable: bool,
    pub install_guide: CliInstallGuide,
}

fn session_to_legacy(record: &SessionRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "working_dir": record.working_dir,
        "model_type": record.tool,
        "tool_session_id": record.tool_session_id,
        "created_at": record.created_at,
        "last_used_at": record.last_used_at,
        "status": record.status,
    })
}

fn launcher_to_legacy(record: &LauncherRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "type": record.item_type,
        "target": record.target,
        "pinned": record.pinned,
        "pin_order": record.pin_order,
        "launch_count": record.launch_count,
        "last_launched_at": record.last_launched_at,
        "trusted": record.trusted,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

fn is_valid_launcher_type(item_type: &str) -> bool {
    LAUNCHER_TYPES.contains(&item_type)
}

fn sanitize_launcher_record(record: &mut LauncherRecord) -> Result<(), String> {
    record.name = record.name.trim().to_string();
    record.target = record.target.trim().to_string();
    record.item_type = record.item_type.trim().to_lowercase();
    if record.id.trim().is_empty() {
        record.id = uuid::Uuid::new_v4().to_string();
    }
    if record.name.is_empty() {
        return Err("launcher name required".to_string());
    }
    if record.target.is_empty() {
        return Err("launcher target required".to_string());
    }
    if !is_valid_launcher_type(&record.item_type) {
        return Err(format!("invalid launcher type: {}", record.item_type));
    }
    if record.item_type == "app" {
        record.target = normalize_app_target(&record.target)?;
    }
    if record.item_type != "script" {
        record.trusted = true;
    }
    if !record.pinned {
        record.pin_order = 0;
    }
    Ok(())
}

fn normalize_app_target(raw: &str) -> Result<String, String> {
    let mut target = raw.trim().to_string();
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("open -a ") {
        target = target[8..].trim().to_string();
    } else if lower.starts_with("open -a") {
        target = target[7..].trim().to_string();
    }
    target = target.trim().trim_matches(is_wrapped_quote_char).trim().to_string();
    if target.is_empty() {
        return Err("app target required".to_string());
    }
    Ok(target)
}

fn is_wrapped_quote_char(c: char) -> bool {
    matches!(c, '"' | '\'' | '`' | '“' | '”' | '‘' | '’')
}

fn try_open_application(app_name: &str) -> Result<(), String> {
    if Command::new("open")
        .arg("-a")
        .arg(app_name)
        .spawn()
        .is_ok()
    {
        return Ok(());
    }

    let normalized = app_name.trim_end_matches(".app").to_lowercase();
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }

    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or_default();
            if !ext.eq_ignore_ascii_case("app") {
                continue;
            }
            let file_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if file_name.contains(&normalized) || normalized.contains(&file_name) {
                Command::new("open")
                    .arg(&path)
                    .spawn()
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
        }
    }

    Err(format!("Unable to find application named '{}'", app_name))
}

fn normalize_launcher_pin_order(items: &mut [LauncherRecord]) {
    let mut pinned_idx: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| if item.pinned { Some(idx) } else { None })
        .collect();
    pinned_idx.sort_by_key(|idx| items[*idx].pin_order);
    for (order, idx) in pinned_idx.into_iter().enumerate() {
        items[idx].pin_order = order as u32;
    }
}

fn sort_launcher_items(items: &mut [LauncherRecord]) {
    normalize_launcher_pin_order(items);
    items.sort_by(|a, b| {
        if a.pinned != b.pinned {
            return if a.pinned {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if a.pinned && b.pinned {
            return a.pin_order.cmp(&b.pin_order);
        }
        b.last_launched_at
            .unwrap_or(0)
            .cmp(&a.last_launched_at.unwrap_or(0))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

fn next_launcher_pin_order(items: &[LauncherRecord]) -> u32 {
    items
        .iter()
        .filter(|item| item.pinned)
        .map(|item| item.pin_order)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

fn merge_launcher_items(existing: &mut Vec<LauncherRecord>, imported: Vec<LauncherRecord>) {
    for incoming in imported {
        if let Some(idx) = existing.iter().position(|it| it.id == incoming.id) {
            existing[idx] = incoming;
        } else {
            existing.push(incoming);
        }
    }
}

fn launcher_record_from_import_input(
    input: LauncherItemInput,
    now: u64,
) -> Result<LauncherRecord, String> {
    let mut record = LauncherRecord {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name,
        item_type: input.item_type,
        target: input.target,
        pinned: input.pinned.unwrap_or(false),
        pin_order: input.pin_order.unwrap_or(0),
        launch_count: input.launch_count.unwrap_or(0),
        last_launched_at: input.last_launched_at,
        trusted: input.trusted.unwrap_or(false),
        created_at: input.created_at.unwrap_or(now),
        updated_at: input.updated_at.unwrap_or(now),
    };
    sanitize_launcher_record(&mut record)?;
    Ok(record)
}

fn is_managed_tool(tool: &str) -> bool {
    MANAGED_TOOLS.contains(&tool)
}

fn provider_env_managed(provider: &ProviderRecord) -> bool {
    if !is_managed_tool(&provider.core.tool) {
        return true;
    }
    provider
        .tool_config
        .get("env_managed")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

fn cli_cmd_name(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

fn detect_cli_installation(tool: &str) -> (bool, String) {
    let Some(cmd_name) = cli_cmd_name(tool) else {
        return (false, String::new());
    };

    match Command::new(cmd_name).arg("--version").output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let version = if !stdout.is_empty() { stdout } else { stderr };
            (output.status.success(), version)
        }
        Err(_) => (false, String::new()),
    }
}

fn read_json_object(path: &Path) -> Option<Map<String, Value>> {
    let content = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(&content).ok()?;
    value.as_object().cloned()
}

fn parse_first_json_value<T: DeserializeOwned>(content: &str) -> Option<T> {
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Value>();
    let first = stream.next()?.ok()?;
    serde_json::from_value::<T>(first).ok()
}

fn cli_has_system_config(tool: &str) -> bool {
    let Some(home_dir) = dirs::home_dir() else {
        return false;
    };

    match tool {
        "claude" => {
            let path = home_dir.join(".claude").join("settings.json");
            let Some(settings) = read_json_object(&path) else {
                return false;
            };
            if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
                return env.contains_key("ANTHROPIC_API_KEY")
                    || env.contains_key("ANTHROPIC_AUTH_TOKEN")
                    || env.contains_key("ANTHROPIC_BASE_URL")
                    || env.contains_key("ANTHROPIC_MODEL");
            }
            false
        }
        "codex" => {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if let Some(auth) = read_json_object(&auth_path) {
                if auth
                    .get("OPENAI_API_KEY")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            let cfg_path = home_dir.join(".codex").join("config.toml");
            if let Ok(content) = fs::read_to_string(cfg_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    return doc.get("base_url").is_some()
                        || doc.get("model").is_some()
                        || doc.get("approval_policy").is_some()
                        || doc.get("sandbox_mode").is_some();
                }
            }
            false
        }
        "gemini" => {
            let env_path = home_dir.join(".gemini").join(".env");
            if let Ok(content) = fs::read_to_string(env_path) {
                let has_key = content.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("GEMINI_API_KEY=")
                        || line.starts_with("GOOGLE_GEMINI_BASE_URL=")
                        || line.starts_with("GEMINI_MODEL=")
                });
                if has_key {
                    return true;
                }
            }
            let settings_path = home_dir.join(".gemini").join("settings.json");
            if let Some(settings) = read_json_object(&settings_path) {
                return settings.get("security").is_some() || settings.get("general").is_some();
            }
            false
        }
        "opencode" => {
            let path = home_dir.join(".config").join("opencode").join("opencode.json");
            if let Some(settings) = read_json_object(&path) {
                return settings
                    .get("provider")
                    .and_then(|v| v.as_object())
                    .map(|m| !m.is_empty())
                    .unwrap_or(false);
            }
            false
        }
        _ => false,
    }
}

fn install_guide_for(tool: &str) -> CliInstallGuide {
    match tool {
        "claude" => CliInstallGuide {
            docs_url: "https://docs.anthropic.com/en/docs/claude-code".to_string(),
            commands: vec![
                CliInstallCommand {
                    label: "Recommended".to_string(),
                    command: "npm install -g @anthropic-ai/claude-code".to_string(),
                },
                CliInstallCommand {
                    label: "Alternative".to_string(),
                    command: "brew install anthropic-ai/tap/claude-code".to_string(),
                },
            ],
        },
        "codex" => CliInstallGuide {
            docs_url: "https://github.com/openai/codex".to_string(),
            commands: vec![
                CliInstallCommand {
                    label: "Recommended".to_string(),
                    command: "npm install -g @openai/codex".to_string(),
                },
                CliInstallCommand {
                    label: "Alternative".to_string(),
                    command: "brew install codex".to_string(),
                },
            ],
        },
        "gemini" => CliInstallGuide {
            docs_url: "https://github.com/google-gemini/gemini-cli".to_string(),
            commands: vec![
                CliInstallCommand {
                    label: "Recommended".to_string(),
                    command: "npm install -g @google/gemini-cli".to_string(),
                },
                CliInstallCommand {
                    label: "Alternative".to_string(),
                    command: "brew install gemini-cli".to_string(),
                },
            ],
        },
        "opencode" => CliInstallGuide {
            docs_url: "https://opencode.ai/docs".to_string(),
            commands: vec![
                CliInstallCommand {
                    label: "Recommended".to_string(),
                    command: "npm install -g opencode-ai".to_string(),
                },
                CliInstallCommand {
                    label: "Alternative".to_string(),
                    command: "brew install opencode".to_string(),
                },
            ],
        },
        _ => CliInstallGuide {
            docs_url: String::new(),
            commands: vec![],
        },
    }
}

fn read_system_provider(tool: &str) -> Option<ProviderRecord> {
    if !is_managed_tool(tool) {
        return None;
    }
    let home_dir = dirs::home_dir()?;

    let mut provider = ProviderRecord::default();
    provider.core.id = format!("default-{}", tool);
    provider.core.tool = tool.to_string();
    provider.core.name = match tool {
        "claude" => "Imported Claude Config".to_string(),
        "codex" => "Imported Codex Config".to_string(),
        "gemini" => "Imported Gemini Config".to_string(),
        _ => "Imported Config".to_string(),
    };
    provider
        .tool_config
        .insert("env_managed".to_string(), Value::Bool(true));

    match tool {
        "claude" => {
            let path = home_dir.join(".claude").join("settings.json");
            let settings = read_json_object(&path)?;
            if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
                if let Some(key) = env
                    .get("ANTHROPIC_API_KEY")
                    .and_then(|v| v.as_str())
                    .or_else(|| env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()))
                {
                    provider.core.api_key = key.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
                    provider.core.base_url = Some(v.to_string());
                }
                for (src, dst) in [
                    ("ANTHROPIC_MODEL", "claude_default_model"),
                    ("ANTHROPIC_REASONING_MODEL", "claude_reasoning_model"),
                    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "claude_haiku_model"),
                    ("ANTHROPIC_DEFAULT_SONNET_MODEL", "claude_sonnet_model"),
                    ("ANTHROPIC_DEFAULT_OPUS_MODEL", "claude_opus_model"),
                ] {
                    if let Some(v) = env.get(src).and_then(|v| v.as_str()) {
                        provider
                            .tool_config
                            .insert(dst.to_string(), Value::String(v.to_string()));
                    }
                }
            }
            for (src, dst) in [
                ("dangerouslySkipPermissions", "dangerously_skip_permissions"),
                ("enableAllMemoryFeatures", "enable_all_memory_features"),
                ("enableMcp", "enable_mcp"),
            ] {
                if let Some(v) = settings.get(src).and_then(|v| v.as_bool()) {
                    provider.tool_config.insert(dst.to_string(), Value::Bool(v));
                }
            }
            for (src, dst) in [("allowedTools", "allowed_tools"), ("blockedTools", "blocked_tools")] {
                if let Some(v) = settings.get(src) {
                    provider.tool_config.insert(dst.to_string(), v.clone());
                }
            }
            if let Some(v) = settings.get("maxSessionTurns").and_then(|v| v.as_u64()) {
                provider
                    .tool_config
                    .insert("max_session_turns".to_string(), Value::Number(v.into()));
            }
        }
        "codex" => {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if let Some(auth) = read_json_object(&auth_path) {
                if let Some(v) = auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
                    provider.core.api_key = v.to_string();
                }
            }
            let config_path = home_dir.join(".codex").join("config.toml");
            if let Ok(content) = fs::read_to_string(config_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    if let Some(v) = doc.get("base_url").and_then(|v| v.as_str()) {
                        provider.core.base_url = Some(v.to_string());
                    }
                    if let Some(v) = doc.get("model").and_then(|v| v.as_str()) {
                        provider.core.model = Some(v.to_string());
                    }
                    for k in [
                        "disable_response_storage",
                        "personality",
                        "model_reasoning_effort",
                        "model_reasoning_summary",
                        "approval_policy",
                        "sandbox_mode",
                    ] {
                        if let Some(v) = doc.get(k) {
                            if let Some(b) = v.as_bool() {
                                provider.tool_config.insert(k.to_string(), Value::Bool(b));
                            } else if let Some(s) = v.as_str() {
                                provider
                                    .tool_config
                                    .insert(k.to_string(), Value::String(s.to_string()));
                            }
                        }
                    }
                    if let Some(mp) = doc.get("model_providers").and_then(|v| v.as_table()) {
                        if let Some(default) = mp.get("default") {
                            if let Some(wire_api) = default.get("wire_api").and_then(|v| v.as_str()) {
                                provider
                                    .tool_config
                                    .insert("wire_api".to_string(), Value::String(wire_api.to_string()));
                            }
                        }
                    }
                }
            }
        }
        "gemini" => {
            let env_path = home_dir.join(".gemini").join(".env");
            if let Ok(content) = fs::read_to_string(env_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        let key = k.trim();
                        let val = v.trim().to_string();
                        match key {
                            "GEMINI_API_KEY" => provider.core.api_key = val,
                            "GOOGLE_GEMINI_BASE_URL" => provider.core.base_url = Some(val),
                            "GEMINI_MODEL" => provider.core.model = Some(val),
                            _ => {}
                        }
                    }
                }
            }
            let settings_path = home_dir.join(".gemini").join("settings.json");
            if let Some(settings) = read_json_object(&settings_path) {
                if let Some(v) = settings.get("theme") {
                    provider.tool_config.insert("theme".to_string(), v.clone());
                }
                if let Some(general) = settings.get("general").and_then(|v| v.as_object()) {
                    if let Some(v) = general.get("vimMode").and_then(|v| v.as_bool()) {
                        provider.tool_config.insert("vim_mode".to_string(), Value::Bool(v));
                    }
                    if let Some(v) = general.get("defaultApprovalMode").and_then(|v| v.as_str()) {
                        provider.tool_config.insert(
                            "default_approval_mode".to_string(),
                            Value::String(v.to_string()),
                        );
                    }
                }
                if let Some(auth_type) = settings
                    .get("security")
                    .and_then(|v| v.as_object())
                    .and_then(|s| s.get("auth"))
                    .and_then(|v| v.as_object())
                    .and_then(|a| a.get("selectedType"))
                    .and_then(|v| v.as_str())
                {
                    provider.tool_config.insert(
                        "gemini_auth_type".to_string(),
                        Value::String(auth_type.to_string()),
                    );
                }
            }
        }
        _ => return None,
    }

    Some(provider)
}

fn render_claude(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let settings_path = home_dir.join(".claude").join("settings.json");
    let mut settings = Map::new();

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    let bool_fields = [
        ("dangerously_skip_permissions", "dangerouslySkipPermissions"),
        ("enable_all_memory_features", "enableAllMemoryFeatures"),
        ("enable_mcp", "enableMcp"),
    ];

    for (src, dst) in bool_fields {
        if let Some(v) = provider.tool_config.get(src).and_then(|v| v.as_bool()) {
            settings.insert(dst.to_string(), Value::Bool(v));
        } else {
            settings.remove(dst);
        }
    }

    for (src, dst) in [
        ("allowed_tools", "allowedTools"),
        ("blocked_tools", "blockedTools"),
    ] {
        if let Some(v) = provider.tool_config.get(src) {
            settings.insert(dst.to_string(), v.clone());
        } else {
            settings.remove(dst);
        }
    }

    if let Some(turns) = provider.tool_config.get("max_session_turns").and_then(|v| v.as_u64()) {
        settings.insert("maxSessionTurns".to_string(), Value::Number(turns.into()));
    }

    let mut env = settings
        .remove("env")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    env.insert(
        "ANTHROPIC_API_KEY".to_string(),
        Value::String(provider.core.api_key.clone()),
    );
    env.remove("ANTHROPIC_AUTH_TOKEN");

    if let Some(base_url) = &provider.core.base_url {
        if !base_url.is_empty() {
            env.insert("ANTHROPIC_BASE_URL".to_string(), Value::String(base_url.clone()));
        }
    } else {
        env.remove("ANTHROPIC_BASE_URL");
    }

    for (src, dst) in [
        ("claude_default_model", "ANTHROPIC_MODEL"),
        ("claude_reasoning_model", "ANTHROPIC_REASONING_MODEL"),
        ("claude_haiku_model", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        ("claude_sonnet_model", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
        ("claude_opus_model", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    ] {
        if let Some(v) = provider.tool_config.get(src).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                env.insert(dst.to_string(), Value::String(v.to_string()));
            }
        } else {
            env.remove(dst);
        }
    }

    settings.insert("env".to_string(), Value::Object(env));

    let content = serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    Ok(vec![(settings_path, content)])
}

fn render_claude_reset_to_unmanaged() -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let settings_path = home_dir.join(".claude").join("settings.json");
    let mut settings = Map::new();

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    for key in [
        "dangerouslySkipPermissions",
        "enableAllMemoryFeatures",
        "enableMcp",
        "allowedTools",
        "blockedTools",
        "maxSessionTurns",
    ] {
        settings.remove(key);
    }

    if let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_REASONING_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
        ] {
            env.remove(key);
        }
        if env.is_empty() {
            settings.remove("env");
        }
    }

    let content = serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    Ok(vec![(settings_path, content)])
}

fn render_codex(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let codex_dir = home_dir.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");

    let auth = json!({
        "OPENAI_API_KEY": provider.core.api_key,
    });

    let mut toml_str = String::new();
    if config_path.exists() {
        toml_str = fs::read_to_string(&config_path).unwrap_or_default();
    }
    let mut doc = toml_str
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());

    if let Some(v) = &provider.core.base_url {
        doc["base_url"] = toml_edit::value(v.clone());
    } else {
        doc.remove("base_url");
    }
    if let Some(v) = &provider.core.model {
        doc["model"] = toml_edit::value(v.clone());
    } else {
        doc.remove("model");
    }

    for (k, toml_key) in [
        ("disable_response_storage", "disable_response_storage"),
        ("personality", "personality"),
        ("model_reasoning_effort", "model_reasoning_effort"),
        ("model_reasoning_summary", "model_reasoning_summary"),
        ("approval_policy", "approval_policy"),
        ("sandbox_mode", "sandbox_mode"),
    ] {
        if let Some(value) = provider.tool_config.get(k) {
            match value {
                Value::Bool(b) => doc[toml_key] = toml_edit::value(*b),
                Value::String(s) => doc[toml_key] = toml_edit::value(s.clone()),
                _ => {}
            }
        }
    }

    if let Some(Value::String(wire_api)) = provider.tool_config.get("wire_api") {
        if !doc.contains_key("model_providers") {
            doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(providers) = doc["model_providers"].as_table_mut() {
            if !providers.contains_key("default") {
                providers["default"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            if let Some(provider_table) = providers["default"].as_table_mut() {
                provider_table.insert("wire_api", toml_edit::value(wire_api.clone()));
            }
        }
    }

    Ok(vec![
        (
            auth_path,
            serde_json::to_string_pretty(&auth).map_err(|e| e.to_string())?,
        ),
        (config_path, doc.to_string()),
    ])
}

fn render_codex_reset_to_unmanaged() -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let codex_dir = home_dir.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");
    let mut outputs = Vec::new();

    if auth_path.exists() {
        let mut auth = read_json_object(&auth_path).unwrap_or_default();
        auth.remove("OPENAI_API_KEY");
        outputs.push((
            auth_path,
            serde_json::to_string_pretty(&Value::Object(auth)).map_err(|e| e.to_string())?,
        ));
    }

    if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        let mut doc = content
            .parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|_| toml_edit::DocumentMut::new());

        for key in [
            "base_url",
            "model",
            "disable_response_storage",
            "personality",
            "model_reasoning_effort",
            "model_reasoning_summary",
            "approval_policy",
            "sandbox_mode",
        ] {
            doc.remove(key);
        }

        if let Some(providers) = doc.get_mut("model_providers").and_then(|v| v.as_table_mut()) {
            if let Some(default) = providers.get_mut("default").and_then(|v| v.as_table_mut()) {
                default.remove("wire_api");
            }
        }

        outputs.push((config_path, doc.to_string()));
    }

    Ok(outputs)
}

fn render_gemini(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let gemini_dir = home_dir.join(".gemini");
    let env_path = gemini_dir.join(".env");
    let settings_path = gemini_dir.join("settings.json");

    let mut env_map = std::collections::BTreeMap::new();
    env_map.insert("GEMINI_API_KEY".to_string(), provider.core.api_key.clone());
    if let Some(v) = &provider.core.base_url {
        env_map.insert("GOOGLE_GEMINI_BASE_URL".to_string(), v.clone());
    }
    if let Some(v) = &provider.core.model {
        env_map.insert("GEMINI_MODEL".to_string(), v.clone());
    }

    let mut env_content = String::new();
    for (k, v) in env_map {
        env_content.push_str(&format!("{}={}\n", k, v));
    }

    let mut settings = Map::new();
    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    for field in ["theme"] {
        if let Some(v) = provider.tool_config.get(field) {
            settings.insert(field.to_string(), v.clone());
        }
    }

    if let Some(v) = provider.tool_config.get("vim_mode").and_then(|v| v.as_bool()) {
        let mut general = settings
            .remove("general")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        general.insert("vimMode".to_string(), Value::Bool(v));
        if let Some(mode) = provider
            .tool_config
            .get("default_approval_mode")
            .and_then(|v| v.as_str())
        {
            general.insert("defaultApprovalMode".to_string(), Value::String(mode.to_string()));
        }
        settings.insert("general".to_string(), Value::Object(general));
    }

    if let Some(auth_type) = provider
        .tool_config
        .get("gemini_auth_type")
        .and_then(|v| v.as_str())
    {
        let mut security = settings
            .remove("security")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let mut auth = security
            .remove("auth")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        auth.insert(
            "selectedType".to_string(),
            Value::String(auth_type.to_string()),
        );
        security.insert("auth".to_string(), Value::Object(auth));
        settings.insert("security".to_string(), Value::Object(security));
    }

    Ok(vec![
        (env_path, env_content),
        (
            settings_path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ),
    ])
}

fn render_gemini_reset_to_unmanaged() -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let gemini_dir = home_dir.join(".gemini");
    let env_path = gemini_dir.join(".env");
    let settings_path = gemini_dir.join("settings.json");
    let mut outputs = Vec::new();

    if env_path.exists() {
        let content = fs::read_to_string(&env_path).unwrap_or_default();
        let mut env_map = std::collections::BTreeMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                if key == "GEMINI_API_KEY" || key == "GOOGLE_GEMINI_BASE_URL" || key == "GEMINI_MODEL" {
                    continue;
                }
                env_map.insert(key.to_string(), v.trim().to_string());
            }
        }
        let mut new_content = String::new();
        for (k, v) in env_map {
            new_content.push_str(&format!("{}={}\n", k, v));
        }
        outputs.push((env_path, new_content));
    }

    if settings_path.exists() {
        let mut settings = read_json_object(&settings_path).unwrap_or_default();
        settings.remove("theme");

        if let Some(general) = settings.get_mut("general").and_then(|v| v.as_object_mut()) {
            general.remove("vimMode");
            general.remove("defaultApprovalMode");
            if general.is_empty() {
                settings.remove("general");
            }
        }

        if let Some(security) = settings.get_mut("security").and_then(|v| v.as_object_mut()) {
            if let Some(auth) = security.get_mut("auth").and_then(|v| v.as_object_mut()) {
                auth.remove("selectedType");
                if auth.is_empty() {
                    security.remove("auth");
                }
            }
            if security.is_empty() {
                settings.remove("security");
            }
        }

        outputs.push((
            settings_path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ));
    }

    Ok(outputs)
}

fn render_opencode(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let path = home_dir.join(".config").join("opencode").join("opencode.json");

    let mut settings = Map::new();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    settings
        .entry("$schema".to_string())
        .or_insert(Value::String("https://opencode.ai/config.json".to_string()));

    if let Some(v) = provider.tool_config.get("opencode_default_model").and_then(|v| v.as_str()) {
        settings.insert("model".to_string(), Value::String(v.to_string()));
    }

    if let Some(v) = provider.tool_config.get("opencode_default_agent").and_then(|v| v.as_str()) {
        let mut agent = settings
            .remove("agent")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        agent.insert("default".to_string(), Value::String(v.to_string()));
        settings.insert("agent".to_string(), Value::Object(agent));
    }

    if let Some(v) = provider
        .tool_config
        .get("opencode_sessions_dir")
        .and_then(|v| v.as_str())
    {
        let mut sessions = settings
            .remove("sessions")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        sessions.insert("dir".to_string(), Value::String(v.to_string()));
        settings.insert("sessions".to_string(), Value::Object(sessions));
    }

    let mut providers = settings
        .remove("provider")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let provider_key = provider
        .provider_key
        .clone()
        .or_else(|| {
            if provider.core.id == "default-opencode" {
                Some("onespace_provider".to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| provider.core.id.clone());

    let mut provider_obj = provider.tool_config.clone();
    provider_obj.insert("name".to_string(), Value::String(provider.core.name.clone()));
    providers.insert(provider_key, Value::Object(provider_obj));

    settings.insert("provider".to_string(), Value::Object(providers));

    Ok(vec![
        (
            path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ),
    ])
}

fn render_projection(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    if !provider_env_managed(provider) {
        return match provider.core.tool.as_str() {
            "claude" => render_claude_reset_to_unmanaged(),
            "codex" => render_codex_reset_to_unmanaged(),
            "gemini" => render_gemini_reset_to_unmanaged(),
            _ => Err(format!("Unsupported tool for unmanaged reset: {}", provider.core.tool)),
        };
    }

    match provider.core.tool.as_str() {
        "claude" => render_claude(provider),
        "codex" => render_codex(provider),
        "gemini" => render_gemini(provider),
        "opencode" => render_opencode(provider),
        other => Err(format!("Unsupported tool: {}", other)),
    }
}

fn apply_projection(provider: &ProviderRecord) -> Result<(), String> {
    let renders = render_projection(provider)?;
    for (path, content) in renders {
        StorageEngine::atomic_write(&path, &content)?;
    }
    Ok(())
}

fn build_projection_diff(provider: &ProviderRecord) -> Result<Vec<Value>, String> {
    let renders = render_projection(provider)?;
    let mut diffs = Vec::new();

    for (path, desired) in renders {
        let current = if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };
        if current != desired {
            diffs.push(json!({
                "path": path.to_string_lossy(),
                "current": current,
                "desired": desired
            }));
        }
    }

    Ok(diffs)
}

static SYNC_RUNNING: AtomicBool = AtomicBool::new(false);

struct SyncRunningGuard;

impl Drop for SyncRunningGuard {
    fn drop(&mut self) {
        SYNC_RUNNING.store(false, Ordering::SeqCst);
    }
}

async fn process_sync_queue(app: tauri::AppHandle) -> Result<(), String> {
    if SYNC_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    let _guard = SyncRunningGuard;
    let mut outbox = load_outbox_state()?;
    outbox.running = true;
    outbox.last_status = "running".to_string();
    save_outbox_state(&outbox)?;

    let now = now_ts();
    let mut due = Vec::new();
    let mut keep = Vec::new();
    for ev in outbox.events.into_iter() {
        if ev.next_retry_at <= now {
            due.push(ev);
        } else {
            keep.push(ev);
        }
    }

    let mut last_error = None;
    if !due.is_empty() {
        let cfg = config::get_config()?;
        for mut ev in due {
            let run_res = if cfg.storage_type == "git" {
                git::sync_git(app.clone()).await
            } else {
                Ok(())
            };
            match run_res {
                Ok(_) => {}
                Err(err) => {
                    ev.attempts = ev.attempts.saturating_add(1);
                    let backoff = 2u64.saturating_pow(ev.attempts.min(8));
                    ev.next_retry_at = now_ts().saturating_add(backoff);
                    ev.last_error = Some(err.clone());
                    last_error = Some(err);
                    keep.push(ev);
                }
            }
        }
    }

    outbox.events = keep;
    outbox.last_run_at = Some(now_ts());
    outbox.running = false;
    if let Some(err) = last_error {
        outbox.last_status = "error".to_string();
        outbox.last_error = Some(err);
    } else {
        outbox.last_status = "success".to_string();
        outbox.last_error = None;
    }
    save_outbox_state(&outbox)?;
    Ok(())
}

fn enqueue_sync_event(domain: &str, reason: &str) -> Result<(), String> {
    let mut outbox = load_outbox_state()?;
    let now = now_ts();

    let is_dup = outbox.events.iter().any(|e| {
        e.domain == domain
            && now.saturating_sub(e.created_at) <= OUTBOX_DEDUP_WINDOW_SECS
            && e.last_error.is_none()
    });

    if !is_dup {
        outbox.events.push(OutboxEvent {
            id: format!("evt-{}", uuid::Uuid::new_v4()),
            domain: domain.to_string(),
            reason: reason.to_string(),
            created_at: now,
            attempts: 0,
            next_retry_at: now,
            last_error: None,
        });
    }
    save_outbox_state(&outbox)
}

fn copy_if_exists(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

fn backup_legacy_files(backup_id: &str) -> Result<PathBuf, String> {
    let backup_root = StorageEngine::backup_root()?.join(backup_id);
    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;

    let data_dir = crate::get_data_dir()?;
    let app_dir = config::get_app_dir()?;

    let files = vec![
        data_dir.join("ai_providers.json"),
        data_dir.join("ai_sessions.json"),
        data_dir.join("secrets.json"),
        data_dir.join("snippets.json"),
        data_dir.join("bookmarks.json"),
        data_dir.join("notes.json"),
        data_dir.join("mcp_servers.json"),
        app_dir.join("config.json"),
    ];

    for file in files {
        if file.exists() {
            let rel = file
                .file_name()
                .ok_or("invalid file")?
                .to_string_lossy()
                .to_string();
            copy_if_exists(&file, &backup_root.join(rel))?;
        }
    }

    Ok(backup_root)
}

fn build_new_providers_from_legacy() -> Result<ProvidersState, String> {
    let legacy = ai_env::get_ai_providers()?;
    let mut active = HashMap::new();
    if let Some(v) = legacy.active_claude { active.insert("claude".to_string(), v); }
    if let Some(v) = legacy.active_codex { active.insert("codex".to_string(), v); }
    if let Some(v) = legacy.active_gemini { active.insert("gemini".to_string(), v); }
    if let Some(v) = legacy.active_opencode { active.insert("opencode".to_string(), v); }

    let mut providers = Vec::new();
    for p in legacy.providers {
        let value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
        let obj = value.as_object().cloned().unwrap_or_default();

        let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let tool = obj.get("tool").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let api_key = obj.get("api_key").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let base_url = obj.get("base_url").and_then(|v| v.as_str()).map(|s| s.to_string());
        let model = obj.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
        let is_enabled = obj.get("is_enabled").and_then(|v| v.as_bool());
        let provider_key = obj.get("provider_key").and_then(|v| v.as_str()).map(|s| s.to_string());

        let mut tool_config = Map::new();
        for (k, v) in &obj {
            match k.as_str() {
                "id" | "name" | "tool" | "api_key" | "base_url" | "model" | "is_enabled" | "provider_key" | "history" => {}
                _ => { tool_config.insert(k.clone(), v.clone()); }
            }
        }

        let mut history = Vec::new();
        if let Some(Value::Array(arr)) = obj.get("history") {
            for item in arr {
                if let Some(ts) = item.get("timestamp").and_then(|v| v.as_u64()) {
                    let summary = item.get("content").and_then(|v| v.as_str()).map(|s| s.to_string());
                    history.push(ProviderHistoryEntry {
                        ts: ts / 1000,
                        action: "legacy-import".to_string(),
                        summary,
                    });
                }
            }
        }

        providers.push(ProviderRecord {
            core: ProviderCore {
                id,
                name,
                tool,
                api_key,
                base_url,
                model,
            },
            runtime_policy: ProviderRuntimePolicy {
                approval_policy: tool_config
                    .get("approval_policy")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                sandbox_mode: tool_config
                    .get("sandbox_mode")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            },
            tool_config,
            history,
            extra: Map::new(),
            is_enabled,
            provider_key,
        });
    }

    Ok(ProvidersState { active, providers })
}

fn build_new_sessions_from_legacy() -> Result<SessionsState, String> {
    let legacy = ai_sessions::get_ai_sessions()?;
    let sessions = legacy
        .into_iter()
        .map(|s| SessionRecord {
            id: s.id,
            name: s.name,
            working_dir: s.working_dir,
            tool: s.model_type,
            tool_session_id: s.tool_session_id,
            created_at: s.created_at,
            last_used_at: s.created_at,
            status: "active".to_string(),
        })
        .collect();
    Ok(SessionsState { sessions })
}

fn migrate_content_file(read: fn() -> Result<String, String>, name: &str) -> Result<(), String> {
    let content = read()?;
    let parsed: Value = serde_json::from_str(&content).unwrap_or_else(|_| Value::Array(vec![]));
    let encrypted = CryptoService::encrypt_json(&parsed)?;
    StorageEngine::write_json(&StorageEngine::content_path(name)?, &encrypted)
}

fn migrate_secrets() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    let legacy_path = data_dir.join("secrets.json");
    if !legacy_path.exists() {
        let empty = CryptoService::encrypt_json(&json!({}))?;
        return StorageEngine::write_json(&StorageEngine::secrets_path()?, &empty);
    }

    let content = fs::read_to_string(&legacy_path).map_err(|e| e.to_string())?;
    let mut legacy: secrets::Secrets = serde_json::from_str(&content).unwrap_or_default();

    let mut map = Map::new();
    if legacy.is_encrypted {
        for (k, v) in legacy.values.drain() {
            let dec = CryptoService::decrypt(&v).unwrap_or(v);
            map.insert(k, Value::String(dec));
        }
    } else {
        for (k, v) in legacy.values.drain() {
            map.insert(k, Value::String(v));
        }
    }

    let encrypted = CryptoService::encrypt_json(&Value::Object(map))?;
    StorageEngine::write_json(&StorageEngine::secrets_path()?, &encrypted)
}

fn migrate_mcp() -> Result<(), String> {
    let mut state = mcp_servers::get_mcp_servers().unwrap_or_default();
    state.is_encrypted = true;
    for server in state.servers.iter_mut() {
        let _ = mcp_servers::encrypt_sensitive_data(server);
    }
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    StorageEngine::write_json(&StorageEngine::mcp_path()?, &value)
}

fn migrate_config_shadow() -> Result<(), String> {
    let mut cfg = config::get_config()?;
    cfg.http_token = None;
    if let Some(ref mut proxy) = cfg.proxy {
        proxy.proxy_password = None;
    }
    let value = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    let path = StorageEngine::meta_dir()?.join("config_shadow.json");
    StorageEngine::write_json(&path, &value)
}

fn write_migration_report(report: &MigrationReport) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_report_path()?, report)
}

fn run_migration_impl() -> Result<MigrationState, String> {
    let mut state = load_migration_state().unwrap_or_default();
    let schema = StorageEngine::load_schema().unwrap_or_default();
    if state.migrated && schema.schema_version == SCHEMA_VERSION {
        return Ok(state);
    }

    state.in_progress = true;
    state.last_error = None;
    save_migration_state(&state)?;

    let started = now_ts();
    let backup_id = format!("backup-{}", started);
    let mut steps = Vec::new();

    let result = (|| -> Result<(), String> {
        let _backup_dir = backup_legacy_files(&backup_id)?;
        steps.push("backup".to_string());

        migrate_config_shadow()?;
        steps.push("config".to_string());

        let providers = build_new_providers_from_legacy()?;
        let providers_blob = CryptoService::encrypt_json(
            &serde_json::to_value(&providers).map_err(|e| e.to_string())?,
        )?;
        StorageEngine::write_json(&StorageEngine::providers_path()?, &providers_blob)?;
        steps.push("providers".to_string());

        let sessions = build_new_sessions_from_legacy()?;
        let sessions_blob = CryptoService::encrypt_json(
            &serde_json::to_value(&sessions).map_err(|e| e.to_string())?,
        )?;
        StorageEngine::write_json(&StorageEngine::sessions_path()?, &sessions_blob)?;
        steps.push("sessions".to_string());

        migrate_secrets()?;
        steps.push("secrets".to_string());

        migrate_mcp()?;
        steps.push("mcp".to_string());

        migrate_content_file(storage::read_snippets, "snippets")?;
        migrate_content_file(storage::read_bookmarks, "bookmarks")?;
        migrate_content_file(storage::read_notes, "notes")?;
        steps.push("content".to_string());

        let mut schema = StorageEngine::load_schema()?;
        schema.schema_version = SCHEMA_VERSION;
        schema.last_migrated_at = now_ts();
        schema.revision = schema.revision.saturating_add(1);
        StorageEngine::write_json(&StorageEngine::schema_path()?, &schema)?;

        let outbox = OutboxState::default();
        save_outbox_state(&outbox)?;

        Ok(())
    })();

    let finished = now_ts();

    match result {
        Ok(_) => {
            state.migrated = true;
            state.schema_version = SCHEMA_VERSION;
            state.last_migrated_at = Some(finished);
            state.last_backup_id = Some(backup_id.clone());
            state.in_progress = false;
            state.last_error = None;
            save_migration_state(&state)?;

            write_migration_report(&MigrationReport {
                started_at: started,
                finished_at: finished,
                success: true,
                backup_id,
                steps,
                error: None,
            })?;
            Ok(state)
        }
        Err(err) => {
            state.in_progress = false;
            state.last_error = Some(err.clone());
            save_migration_state(&state)?;

            let _ = write_migration_report(&MigrationReport {
                started_at: started,
                finished_at: finished,
                success: false,
                backup_id,
                steps,
                error: Some(err.clone()),
            });
            Err(err)
        }
    }
}

fn rollback_from_backup(backup_id: &str) -> Result<(), String> {
    let backup_dir = StorageEngine::backup_root()?.join(backup_id);
    if !backup_dir.exists() {
        return Err("Backup not found".to_string());
    }

    let data_dir = crate::get_data_dir()?;
    let app_dir = config::get_app_dir()?;

    for file in [
        "ai_providers.json",
        "ai_sessions.json",
        "secrets.json",
        "snippets.json",
        "bookmarks.json",
        "notes.json",
        "mcp_servers.json",
    ] {
        let src = backup_dir.join(file);
        let dst = data_dir.join(file);
        if src.exists() {
            copy_if_exists(&src, &dst)?;
        }
    }

    let cfg = backup_dir.join("config.json");
    if cfg.exists() {
        copy_if_exists(&cfg, &app_dir.join("config.json"))?;
    }

    Ok(())
}

fn cleanup_legacy_root_files() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    let checks = vec![
        (data_dir.join("ai_providers.json"), StorageEngine::providers_path()?),
        (data_dir.join("ai_sessions.json"), StorageEngine::sessions_path()?),
        (data_dir.join("secrets.json"), StorageEngine::secrets_path()?),
        (data_dir.join("snippets.json"), StorageEngine::content_path("snippets")?),
        (data_dir.join("bookmarks.json"), StorageEngine::content_path("bookmarks")?),
        (data_dir.join("notes.json"), StorageEngine::content_path("notes")?),
        (data_dir.join("mcp_servers.json"), StorageEngine::mcp_path()?),
    ];

    for (legacy, new_path) in checks {
        if legacy.exists() && new_path.exists() {
            let _ = fs::remove_file(legacy);
        }
    }
    Ok(())
}

fn rotate_encrypted_blob_file(path: &Path, old_pass: &str, new_pass: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(());
    }

    let plain_json = if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if blob.is_encrypted {
            match crate::crypto::decrypt(&blob.data, old_pass) {
                Ok(plain) => plain,
                Err(err) => {
                    // Do not fail the whole password-change flow for one incompatible/corrupted file.
                    // If payload itself looks like JSON, treat it as legacy plain content; otherwise skip.
                    if serde_json::from_str::<Value>(&blob.data).is_ok() {
                        eprintln!(
                            "rotate_encrypted_blob_file: decrypt failed but blob data is plain JSON, path={}, err={}",
                            path.display(),
                            err
                        );
                        blob.data
                    } else {
                        eprintln!(
                            "rotate_encrypted_blob_file: skip file due to decrypt failure, path={}, err={}",
                            path.display(),
                            err
                        );
                        return Ok(());
                    }
                }
            }
        } else {
            blob.data
        }
    } else {
        content
    };

    let parsed: Value = match serde_json::from_str(&plain_json) {
        Ok(v) => v,
        Err(err) => {
            eprintln!(
                "rotate_encrypted_blob_file: skip file due to invalid json, path={}, err={}",
                path.display(),
                err
            );
            return Ok(());
        }
    };
    let encrypted = crate::crypto::encrypt(&parsed.to_string(), new_pass)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    StorageEngine::write_json(path, &blob)
}

fn rotate_mcp_state_password(old_pass: &str, new_pass: &str) -> Result<(), String> {
    let path = StorageEngine::mcp_path()?;
    if !path.exists() {
        return Ok(());
    }
    let mut state: mcp_servers::MCPServersState = StorageEngine::read_json(&path)?;
    if state.servers.is_empty() {
        return Ok(());
    }

    for server in state.servers.iter_mut() {
        if let Some(ref mut env) = server.env {
            for (_, value) in env.iter_mut() {
                if value.is_empty() || value.starts_with('$') || value.starts_with("${") {
                    continue;
                }
                let plain = crate::crypto::decrypt(value, old_pass).unwrap_or_else(|_| value.clone());
                *value = crate::crypto::encrypt(&plain, new_pass)?;
            }
        }

        if let Some(ref mut headers) = server.headers {
            for (key, value) in headers.iter_mut() {
                let k = key.to_lowercase();
                if !(k.contains("auth")
                    || k.contains("key")
                    || k.contains("token")
                    || k.contains("secret"))
                {
                    continue;
                }
                if value.is_empty() || value.starts_with('$') || value.starts_with("${") {
                    continue;
                }
                let plain = crate::crypto::decrypt(value, old_pass).unwrap_or_else(|_| value.clone());
                *value = crate::crypto::encrypt(&plain, new_pass)?;
            }
        }
    }
    state.is_encrypted = true;
    StorageEngine::write_json(&path, &state)
}

pub fn rotate_master_password_data(old_pass: &str, new_pass: &str) -> Result<(), String> {
    rotate_encrypted_blob_file(&StorageEngine::providers_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::sessions_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::launcher_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::secrets_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::content_path("snippets")?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::content_path("bookmarks")?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::content_path("notes")?, old_pass, new_pass)?;
    rotate_mcp_state_password(old_pass, new_pass)?;
    Ok(())
}

pub fn ensure_migrated_on_startup() -> Result<(), String> {
    run_migration_impl().map(|_| ())?;
    let pass = crate::crypto::get_or_init_master_password()?;
    rotate_master_password_data(&pass, &pass)?;
    cleanup_legacy_root_files()?;
    Ok(())
}

fn try_auto_import_tool(state: &mut ProvidersState, tool: &str) -> Option<String> {
    if !is_managed_tool(tool) {
        return None;
    }
    if state.active.get(tool).is_some() {
        return None;
    }
    let default_id = format!("default-{}", tool);
    if state
        .providers
        .iter()
        .any(|p| p.core.id == default_id)
    {
        return None;
    }
    let (installed, _) = detect_cli_installation(tool);
    if !installed || !cli_has_system_config(tool) {
        return None;
    }
    let provider = read_system_provider(tool)?;
    let provider_id = provider.core.id.clone();
    state.providers.push(provider);
    state.active.insert(tool.to_string(), provider_id.clone());
    Some(provider_id)
}

fn reconcile_auto_import_on_list() -> Result<(), String> {
    let mut state = load_providers_state()?;
    let mut changed = false;
    for tool in MANAGED_TOOLS {
        if try_auto_import_tool(&mut state, tool).is_some() {
            changed = true;
        }
    }
    if changed {
        let _ = save_providers_state(&state)?;
        let _ = enqueue_sync_event("providers", "providers_auto_import_on_list");
    }
    Ok(())
}

#[tauri::command]
pub fn storage_get_snapshot() -> Result<ApiOk<AppSnapshot>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let providers = providers_to_legacy_view(&load_providers_state().map_err(|e| api_error("io_error", e))?);
    let sessions = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let cfg = config::get_storage_config().map_err(|e| api_error("config_error", e))?;
    let schema = StorageEngine::load_schema().map_err(|e| api_error("io_error", e))?;
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;

    api_ok(
        AppSnapshot {
            providers: serde_json::to_value(providers).map_err(|e| api_error("serialize_error", e.to_string()))?,
            sessions: Value::Array(sessions.sessions.iter().map(session_to_legacy).collect()),
            config: serde_json::to_value(cfg).map_err(|e| api_error("serialize_error", e.to_string()))?,
            schema,
            outbox,
        },
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn providers_list() -> Result<ApiOk<LegacyProvidersView>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    if let Err(e) = reconcile_auto_import_on_list() {
        return Err(api_error("auto_import_failed", e));
    }
    let state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        providers_to_legacy_view(&state),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn cli_env_probe(tool: String) -> Result<ApiOk<CliEnvProbeResult>, ApiErr> {
    let probe_tool = tool.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (installed, version) = detect_cli_installation(&probe_tool);
        let configured = cli_has_system_config(&probe_tool);
        CliEnvProbeResult {
            tool: probe_tool.clone(),
            installed,
            version,
            configured,
            importable: is_managed_tool(&probe_tool) && installed && configured,
            install_guide: install_guide_for(&probe_tool),
        }
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?;

    api_ok(result, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn providers_auto_import_from_system(
    app: tauri::AppHandle,
    tool: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    if !is_managed_tool(&tool) {
        return Err(api_error("invalid_tool", "tool does not support env managed import"));
    }

    let (installed, _) = detect_cli_installation(&tool);
    if !installed {
        return api_ok(
            json!({ "imported": false, "reason": "not_installed" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }
    if !cli_has_system_config(&tool) {
        return api_ok(
            json!({ "imported": false, "reason": "not_configured" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let mut state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    let default_id = format!("default-{}", tool);
    if state.active.get(&tool).is_some() {
        return api_ok(
            json!({ "imported": false, "reason": "active_exists" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }
    if state
        .providers
        .iter()
        .any(|p| p.core.id == default_id)
    {
        return api_ok(
            json!({ "imported": false, "reason": "provider_exists" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let provider = read_system_provider(&tool).ok_or_else(|| {
        api_error("import_failed", "failed to parse system config for selected tool")
    })?;
    let provider_id = provider.core.id.clone();
    state.providers.push(provider);
    state.active.insert(tool.clone(), provider_id.clone());
    let schema = save_providers_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("providers", "auto_import_system_config")
        .map_err(|e| api_error("sync_error", e))?;

    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "imported": true, "provider_id": provider_id, "tool": tool }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn providers_set_env_managed(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
    enabled: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    if !is_managed_tool(&tool) {
        return Err(api_error("invalid_tool", "tool does not support env managed switch"));
    }

    let mut state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    let Some(pos) = state
        .providers
        .iter()
        .position(|p| p.core.id == provider_id && p.core.tool == tool) else {
        return Err(api_error("not_found", "provider not found"));
    };

    state.providers[pos]
        .tool_config
        .insert("env_managed".to_string(), Value::Bool(enabled));
    let updated = state.providers[pos].clone();

    if !enabled {
        apply_projection(&updated).map_err(|e| api_error("projection_failed", e))?;
    }

    let schema = save_providers_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("providers", "providers_set_env_managed")
        .map_err(|e| api_error("sync_error", e))?;

    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "tool": tool,
            "provider_id": provider_id,
            "env_managed": enabled
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn providers_upsert(
    app: tauri::AppHandle,
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = provider
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "provider must be object"))?;

    let input = ProviderInput {
        id: obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| "")
            .to_string(),
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tool: obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        api_key: obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_url: obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_enabled: obj.get("is_enabled").and_then(|v| v.as_bool()),
        provider_key: obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        fields: extract_fields(&Value::Object(obj.clone())),
    };

    if input.id.is_empty() || input.tool.is_empty() {
        return Err(api_error("invalid_payload", "provider id/tool required"));
    }

    let mut state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    let old = state.providers.iter().find(|p| p.core.id == input.id).cloned();
    let record = provider_from_input(input, old.as_ref());

    if let Some(pos) = state.providers.iter().position(|p| p.core.id == record.core.id) {
        state.providers[pos] = record.clone();
    } else {
        state.providers.push(record.clone());
    }

    let schema = save_providers_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("providers", "providers_upsert").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        provider_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn providers_delete(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let mut state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    state.providers.retain(|p| p.core.id != provider_id);
    state.active.retain(|_, v| v != &provider_id);
    let schema = save_providers_state(&state).map_err(|e| api_error("io_error", e))?;

    enqueue_sync_event("providers", "providers_delete").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn providers_set_active(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let mut state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    state.active.insert(tool.clone(), provider_id.clone());
    let schema = save_providers_state(&state).map_err(|e| api_error("io_error", e))?;

    enqueue_sync_event("providers", "providers_set_active").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "tool": tool, "provider_id": provider_id }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_list() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    sort_launcher_items(&mut state.items);
    api_ok(
        state.items.iter().map(launcher_to_legacy).collect(),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn launcher_upsert(
    app: tauri::AppHandle,
    item: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = item
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "launcher item must be object"))?;

    let req_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req_name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_type").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let req_target = obj
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("command").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let req_pinned = obj.get("pinned").and_then(|v| v.as_bool());
    let req_pin_order = obj
        .get("pin_order")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let req_trusted = obj.get("trusted").and_then(|v| v.as_bool());

    let now = now_ts();
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let item_id = req_id
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing = state.items.iter().find(|it| it.id == item_id).cloned();

    let mut record = LauncherRecord {
        id: item_id,
        name: req_name
            .or_else(|| existing.as_ref().map(|it| it.name.clone()))
            .unwrap_or_default(),
        item_type: req_type
            .or_else(|| existing.as_ref().map(|it| it.item_type.clone()))
            .unwrap_or_default(),
        target: req_target
            .or_else(|| existing.as_ref().map(|it| it.target.clone()))
            .unwrap_or_default(),
        pinned: req_pinned
            .unwrap_or_else(|| existing.as_ref().map(|it| it.pinned).unwrap_or(false)),
        pin_order: req_pin_order
            .unwrap_or_else(|| existing.as_ref().map(|it| it.pin_order).unwrap_or(0)),
        launch_count: existing.as_ref().map(|it| it.launch_count).unwrap_or(0),
        last_launched_at: existing.as_ref().and_then(|it| it.last_launched_at),
        trusted: req_trusted
            .unwrap_or_else(|| existing.as_ref().map(|it| it.trusted).unwrap_or(false)),
        created_at: existing.as_ref().map(|it| it.created_at).unwrap_or(now),
        updated_at: now,
    };
    if let Err(err) = sanitize_launcher_record(&mut record) {
        if let Some(old) = &existing {
            if record.name.trim().is_empty() {
                record.name = old.name.clone();
            }
            if record.target.trim().is_empty() {
                record.target = old.target.clone();
            }
            if !is_valid_launcher_type(&record.item_type) {
                record.item_type = old.item_type.clone();
            }
            sanitize_launcher_record(&mut record).map_err(|e| api_error("invalid_payload", e))?;
        } else {
            return Err(api_error("invalid_payload", err));
        }
    }

    if record.pinned {
        let was_pinned = existing.as_ref().map(|it| it.pinned).unwrap_or(false);
        if !was_pinned && req_pin_order.is_none() {
            record.pin_order = next_launcher_pin_order(&state.items);
        }
    } else {
        record.pin_order = 0;
    }

    if let Some(pos) = state.items.iter().position(|it| it.id == record.id) {
        state.items[pos] = record.clone();
    } else {
        state.items.push(record.clone());
    }

    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("launcher", "launcher_upsert").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        launcher_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_delete(
    app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    state.items.retain(|it| it.id != item_id);
    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    enqueue_sync_event("launcher", "launcher_delete").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_reorder(
    app: tauri::AppHandle,
    ids: Vec<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;

    let mut ordered_ids: Vec<String> = ids
        .into_iter()
        .filter(|id| state.items.iter().any(|it| it.id == *id && it.pinned))
        .collect();
    let current_pinned: Vec<String> = state
        .items
        .iter()
        .filter(|it| it.pinned)
        .map(|it| it.id.clone())
        .collect();
    for id in current_pinned {
        if !ordered_ids.iter().any(|x| x == &id) {
            ordered_ids.push(id);
        }
    }

    for item in state.items.iter_mut() {
        if !item.pinned {
            continue;
        }
        if let Some(pos) = ordered_ids.iter().position(|id| id == &item.id) {
            item.pin_order = pos as u32;
            item.updated_at = now_ts();
        }
    }

    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("launcher", "launcher_reorder").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "reordered": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_mark_launched(
    app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let now = now_ts();
    let mut found = false;
    for item in state.items.iter_mut() {
        if item.id == item_id {
            item.launch_count = item.launch_count.saturating_add(1);
            item.last_launched_at = Some(now);
            item.updated_at = now;
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "launcher item not found"));
    }

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("launcher", "launcher_mark_launched")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "launched": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_set_trust(
    app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let trusted = obj
        .get("trusted")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| api_error("invalid_payload", "trusted bool required"))?;
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let mut found = false;
    for item in state.items.iter_mut() {
        if item.id == item_id {
            if item.item_type != "script" {
                return Err(api_error("invalid_payload", "only script item supports trust switch"));
            }
            item.trusted = trusted;
            item.updated_at = now_ts();
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "launcher item not found"));
    }

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("launcher", "launcher_set_trust")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "trusted": trusted }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_export(
    output_path: String,
    item_ids: Option<Vec<String>>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let selected_ids = item_ids.unwrap_or_default();
    let mut exported: Vec<LauncherRecord> = state
        .items
        .iter()
        .filter(|item| selected_ids.is_empty() || selected_ids.iter().any(|id| id == &item.id))
        .cloned()
        .collect();
    sort_launcher_items(&mut exported);

    let payload = json!({
        "version": LAUNCHER_EXPORT_VERSION,
        "exported_at": now_ts(),
        "items": exported,
    });

    let content =
        serde_json::to_string_pretty(&payload).map_err(|e| api_error("serialize_error", e.to_string()))?;
    StorageEngine::atomic_write(Path::new(&output_path), &content)
        .map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({
            "path": output_path,
            "count": payload
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn launcher_import(
    app: tauri::AppHandle,
    import_path: String,
    mode: Option<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let raw = fs::read_to_string(&import_path).map_err(|e| api_error("io_error", e.to_string()))?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| api_error("invalid_payload", e.to_string()))?;
    let items_val = parsed
        .get("items")
        .and_then(|v| v.as_array().cloned())
        .or_else(|| parsed.as_array().cloned())
        .ok_or_else(|| api_error("invalid_payload", "import payload must contain items array"))?;

    let now = now_ts();
    let mut imported_records: Vec<LauncherRecord> = Vec::new();
    for item in items_val {
        let input: LauncherItemInput = serde_json::from_value(item)
            .map_err(|e| api_error("invalid_payload", format!("invalid launcher item: {}", e)))?;
        let mut record =
            launcher_record_from_import_input(input, now).map_err(|e| api_error("invalid_payload", e))?;
        record.updated_at = now;
        imported_records.push(record);
    }
    let imported_count = imported_records.len();

    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let mode = mode.unwrap_or_else(|| "merge".to_string()).to_lowercase();
    if mode == "replace" {
        state.items = imported_records;
    } else {
        merge_launcher_items(&mut state.items, imported_records);
    }
    normalize_launcher_pin_order(&mut state.items);

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("launcher", "launcher_import").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": true,
            "mode": mode,
            "count": imported_count,
            "total": state.items.len()
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_execute(payload: Value) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_type").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let target = obj
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("command").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();

    if target.is_empty() {
        return Err(api_error("invalid_payload", "launcher target required"));
    }
    if !is_valid_launcher_type(&item_type) || item_type == "internal" {
        return Err(api_error("invalid_payload", "unsupported launcher type for execute"));
    }

    let run_result: Result<(), String> = match item_type.as_str() {
        "url" | "folder" => Command::new("open")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "app" => match normalize_app_target(&target) {
            Ok(app_name) => try_open_application(&app_name),
            Err(e) => Err(e),
        },
        "script" => Command::new("sh")
            .arg("-c")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err("unsupported launcher type".to_string()),
    };

    run_result.map_err(|e| api_error("launch_failed", e))?;
    api_ok(
        json!({ "launched": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn sessions_list() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    state.sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    api_ok(
        state.sessions.iter().map(session_to_legacy).collect(),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sessions_create(
    app: tauri::AppHandle,
    session: SessionInput,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let now = now_ts();
    let id = session
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let record = SessionRecord {
        id,
        name: session.name,
        working_dir: session.working_dir.clone(),
        tool: session.tool.clone(),
        tool_session_id: session.tool_session_id.clone(),
        created_at: now,
        last_used_at: now,
        status: session.status.unwrap_or_else(|| "active".to_string()),
    };

    state.sessions.push(record.clone());
    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;

    if let Err(e) = ai_sessions::launch_native_session_for_create(
        &session.working_dir,
        &session.tool,
        &session.tool_session_id,
    ) {
        return Err(api_error("launch_failed", e));
    }

    enqueue_sync_event("sessions", "sessions_create").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        session_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn sessions_update(
    app: tauri::AppHandle,
    session: SessionInput,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let id = session
        .id
        .clone()
        .ok_or_else(|| api_error("invalid_payload", "session.id required"))?;

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let mut found = false;
    let now = now_ts();

    for s in state.sessions.iter_mut() {
        if s.id == id {
            s.name = session.name.clone();
            s.working_dir = session.working_dir.clone();
            s.tool = session.tool.clone();
            s.tool_session_id = session.tool_session_id.clone();
            s.last_used_at = now;
            if let Some(status) = &session.status {
                s.status = status.clone();
            }
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "session not found"));
    }

    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;

    enqueue_sync_event("sessions", "sessions_update").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    let updated = state
        .sessions
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "session not found"))?;

    api_ok(
        session_to_legacy(&updated),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn sessions_delete(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    state.sessions.retain(|s| s.id != session_id);
    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;

    enqueue_sync_event("sessions", "sessions_delete").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn sessions_launch(
    session_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let now = now_ts();
    let mut target: Option<SessionRecord> = None;

    for s in state.sessions.iter_mut() {
        if s.id == session_id {
            s.last_used_at = now;
            target = Some(s.clone());
            break;
        }
    }

    let target = target.ok_or_else(|| api_error("not_found", "session not found"))?;

    crate::skills::skills_reconcile_for_tool(&target.tool)
        .map_err(|e| api_error("skills_preflight_failed", e))?;

    ai_sessions::launch_native_session(&target.working_dir, &target.tool, &target.tool_session_id)
        .map_err(|e| api_error("launch_failed", e))?;

    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        session_to_legacy(&target),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn projection_dry_run(tool: String, provider_id: String) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.core.id == provider_id && p.core.tool == tool)
        .ok_or_else(|| api_error("not_found", "provider not found"))?;

    let diffs = build_projection_diff(provider).map_err(|e| api_error("projection_failed", e))?;
    api_ok(
        json!({ "changes": diffs }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn projection_apply(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.core.id == provider_id && p.core.tool == tool)
        .cloned()
        .ok_or_else(|| api_error("not_found", "provider not found"))?;

    apply_projection(&provider).map_err(|e| api_error("projection_failed", e))?;

    enqueue_sync_event("projection", "projection_apply").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "applied": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sync_enqueue(
    app: tauri::AppHandle,
    reason: String,
) -> Result<ApiOk<Value>, ApiErr> {
    enqueue_sync_event("manual", &reason).map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "queued": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sync_run_now(app: tauri::AppHandle) -> Result<ApiOk<Value>, ApiErr> {
    process_sync_queue(app)
        .await
        .map_err(|e| api_error("sync_error", e))?;
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        serde_json::to_value(outbox).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn sync_status() -> Result<ApiOk<OutboxState>, ApiErr> {
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        outbox,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn migration_status() -> Result<ApiOk<MigrationState>, ApiErr> {
    let state = load_migration_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        state,
        get_meta().unwrap_or(ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: 0,
        }),
    )
}

#[tauri::command]
pub fn migration_run() -> Result<ApiOk<MigrationState>, ApiErr> {
    let state = run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    api_ok(
        state,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn migration_rollback(backup_id: String) -> Result<ApiOk<Value>, ApiErr> {
    rollback_from_backup(&backup_id).map_err(|e| api_error("rollback_failed", e))?;
    let mut state = load_migration_state().map_err(|e| api_error("io_error", e))?;
    state.migrated = false;
    state.last_error = None;
    save_migration_state(&state).map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({ "rolled_back": true, "backup_id": backup_id }),
        get_meta().unwrap_or(ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: 0,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher_item(
        id: &str,
        pinned: bool,
        pin_order: u32,
        last_launched_at: Option<u64>,
    ) -> LauncherRecord {
        LauncherRecord {
            id: id.to_string(),
            name: format!("item-{}", id),
            item_type: "script".to_string(),
            target: "echo hello".to_string(),
            pinned,
            pin_order,
            launch_count: 0,
            last_launched_at,
            trusted: false,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn launcher_sort_prefers_pinned_then_recent() {
        let mut items = vec![
            launcher_item("a", false, 0, Some(100)),
            launcher_item("b", true, 1, Some(1)),
            launcher_item("c", true, 0, Some(50)),
            launcher_item("d", false, 0, Some(200)),
        ];
        sort_launcher_items(&mut items);
        let ids: Vec<String> = items.into_iter().map(|it| it.id).collect();
        assert_eq!(ids, vec!["c", "b", "d", "a"]);
    }

    #[test]
    fn launcher_merge_overwrites_same_id() {
        let mut existing = vec![
            launcher_item("a", false, 0, Some(10)),
            launcher_item("b", false, 0, Some(20)),
        ];
        let mut updated_a = launcher_item("a", true, 0, Some(30));
        updated_a.name = "updated".to_string();
        let new_c = launcher_item("c", false, 0, Some(40));
        merge_launcher_items(&mut existing, vec![updated_a.clone(), new_c.clone()]);
        assert_eq!(existing.len(), 3);
        assert!(existing.iter().any(|it| it.id == "c"));
        let a = existing.iter().find(|it| it.id == "a").expect("a should exist");
        assert_eq!(a.name, "updated");
        assert!(a.pinned);
    }

    #[test]
    fn launcher_import_input_defaults() {
        let now = 1000;
        let input = LauncherItemInput {
            id: None,
            name: "docs".to_string(),
            item_type: "url".to_string(),
            target: "https://example.com".to_string(),
            ..LauncherItemInput::default()
        };
        let parsed =
            launcher_record_from_import_input(input, now).expect("parse launcher input should work");
        assert!(!parsed.id.is_empty());
        assert_eq!(parsed.item_type, "url");
        assert_eq!(parsed.created_at, now);
        assert_eq!(parsed.updated_at, now);
        assert!(parsed.trusted);
    }

    #[test]
    fn normalize_app_target_accepts_open_command() {
        let parsed = normalize_app_target("open -a \"Visual Studio Code\"")
            .expect("should parse open -a form");
        assert_eq!(parsed, "Visual Studio Code");
    }

    #[test]
    fn normalize_app_target_strips_smart_quotes() {
        let parsed = normalize_app_target("open -a “WPS”").expect("should strip smart quotes");
        assert_eq!(parsed, "WPS");
        let parsed2 = normalize_app_target("“微信").expect("should strip leading smart quote");
        assert_eq!(parsed2, "微信");
    }
}
