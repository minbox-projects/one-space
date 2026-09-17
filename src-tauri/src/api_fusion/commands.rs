use super::runtime_http::{autostart, server_status, start_server, stop_server};
use super::selection::{manual_reenable, set_user_enabled};
use super::storage::{
    effective_default_key, find_provider_mut, local_base_url, new_key_id, new_key_value,
    new_provider_id, read_config, resolve_default_key_id, touch_key_created_at, write_config,
};
use super::usage_log::{
    normalize_retention_days, now_millis, resolve_range, validate_retention_days, LogFilter,
    UsageLogStore, UsageLogsPage, UsageStats, USAGE_LOG_PAGE_SIZE,
};
use super::{
    now_ts, FusionConfig, FusionKey, FusionStatus, FusionUpstreamProvider, ModelPrice,
    TerminalSyncRecord, UsageResult,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Terminal tools that API Fusion is allowed to write to.
pub(in crate::api_fusion) const SUPPORTED_TERMINAL_TOOLS: [&str; 2] = ["opencode", "codex"];

/// Display name and provider key of the managed API Fusion gateway record.
const GATEWAY_PROVIDER_NAME: &str = "API Gateway";
const GATEWAY_PROVIDER_KEY: &str = "apigateway";
/// Stable marker identifying a provider record written by API Fusion.
const GATEWAY_MARKER_KEY: &str = "api_fusion_gateway";

/// A terminal service provider record that API Fusion can configure or sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalTarget {
    pub tool: String,
    pub name: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub synced: bool,
    pub pending_sync: bool,
    #[serde(default)]
    pub synced_key_id: Option<String>,
    #[serde(default)]
    pub synced_at: Option<u64>,
}

pub(in crate::api_fusion) fn is_supported_terminal_tool(tool: &str) -> bool {
    SUPPORTED_TERMINAL_TOOLS
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(tool.trim()))
}

fn provider_tool(provider: &Value) -> &str {
    provider
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// A provider carries the gateway marker either at the top level or under
/// `tool_config`; both shapes are recognized.
fn provider_has_gateway_marker(provider: &Value) -> bool {
    provider.get(GATEWAY_MARKER_KEY).and_then(Value::as_bool) == Some(true)
        || provider
            .get("tool_config")
            .and_then(|tool_config| tool_config.get(GATEWAY_MARKER_KEY))
            .and_then(Value::as_bool)
            == Some(true)
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A gateway contributes models only while it is active: user-enabled and not
/// auto-disabled, the same rule used by candidate selection and `/v1/models`.
fn gateway_is_active(gateway: &FusionUpstreamProvider) -> bool {
    gateway.enabled && !gateway.auto_disabled
}

/// Build the terminal provider record written by API Fusion for one tool.
///
/// The record is always marked as an API Fusion gateway, carries the resolved
/// default local key value as its `api_key` (top-level and, for opencode,
/// `tool_config.options.apiKey`), and never carries an `active`/`is_active`
/// flag. Opencode activation plus projection to opencode.json are applied
/// separately via the service-provider active list and projection after the
/// upsert succeeds.
pub(in crate::api_fusion) fn build_gateway_provider(
    provider_id: &str,
    tool: &str,
    base_url: &str,
    api_key: &str,
    gateways: &[FusionUpstreamProvider],
) -> Result<serde_json::Value, String> {
    let tool = tool.trim().to_ascii_lowercase();
    if !is_supported_terminal_tool(&tool) {
        return Err(format!(
            "unsupported terminal tool '{tool}': only opencode and codex are supported"
        ));
    }

    let mut tool_config = Map::new();
    tool_config.insert(GATEWAY_MARKER_KEY.to_string(), Value::Bool(true));

    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(provider_id.to_string()));
    object.insert(
        "name".to_string(),
        Value::String(GATEWAY_PROVIDER_NAME.to_string()),
    );
    object.insert("tool".to_string(), Value::String(tool.clone()));
    object.insert("base_url".to_string(), Value::String(base_url.to_string()));
    object.insert("api_key".to_string(), Value::String(api_key.to_string()));

    if tool == "opencode" {
        object.insert(
            "provider_key".to_string(),
            Value::String(GATEWAY_PROVIDER_KEY.to_string()),
        );
        tool_config.insert(
            "npm".to_string(),
            Value::String("@ai-sdk/openai-compatible".to_string()),
        );
        let mut options = Map::new();
        options.insert("baseURL".to_string(), Value::String(base_url.to_string()));
        options.insert("apiKey".to_string(), Value::String(api_key.to_string()));
        tool_config.insert("options".to_string(), Value::Object(options));

        let mut models = Map::new();
        for gateway in gateways.iter().filter(|gateway| gateway_is_active(gateway)) {
            for mapping in &gateway.mappings {
                let Some(local_model) = non_empty(Some(mapping.local_model.as_str())) else {
                    continue;
                };
                if models.contains_key(&local_model) {
                    continue;
                }
                let name = non_empty(mapping.display_name.as_deref())
                    .or_else(|| non_empty(Some(mapping.upstream_model.as_str())))
                    .unwrap_or_default();
                let mut model = Map::new();
                model.insert("name".to_string(), Value::String(name));
                models.insert(local_model, Value::Object(model));
            }
        }
        tool_config.insert("models".to_string(), Value::Object(models));
    } else {
        tool_config.insert("wire_api".to_string(), Value::String("chat".to_string()));
        let model = gateways
            .iter()
            .filter(|gateway| gateway_is_active(gateway))
            .find_map(|gateway| {
                gateway
                    .mappings
                    .iter()
                    .find_map(|mapping| non_empty(Some(mapping.local_model.as_str())))
            })
            .or_else(|| {
                gateways
                    .iter()
                    .filter(|gateway| gateway_is_active(gateway))
                    .find_map(|gateway| non_empty(gateway.default_model.as_deref()))
            });
        if let Some(model) = model {
            object.insert("model".to_string(), Value::String(model));
        }
    }

    object.insert("tool_config".to_string(), Value::Object(tool_config));
    Ok(Value::Object(object))
}

/// Pending-sync is derived from the persisted ledger, never from the redacted api_key.
pub(in crate::api_fusion) fn terminal_sync_pending(
    record: &TerminalSyncRecord,
    current_key_id: Option<&str>,
    current_base_url: &str,
) -> bool {
    match current_key_id {
        Some(key_id) => {
            record.synced_key_id != key_id || record.synced_base_url != current_base_url
        }
        None => true,
    }
}

pub(in crate::api_fusion) fn default_key_for_sync(
    config: &FusionConfig,
) -> Result<(String, String), String> {
    // Mirror the frontend `resolveDefaultKeyId` rule: the stored choice wins
    // while it points at an enabled key, otherwise fall through to the next
    // enabled key in list order (wrapping), so syncing still carries the
    // default API key the UI shows.
    let resolved = resolve_default_key_id(&config.keys, config.default_key_id.as_deref());
    match resolved.and_then(|id| {
        config
            .keys
            .iter()
            .find(|key| key.id == id && key.enabled)
    }) {
        Some(key) => Ok((key.id.clone(), key.value.clone())),
        None => match effective_default_key(config) {
            Some(key) => Ok((key.id.clone(), key.value.clone())),
            None => Err(
                "no enabled local API key: add and enable a local key before configuring terminals"
                    .to_string(),
            ),
        },
    }
}

fn api_err_to_string(error: crate::app_store::ApiErr) -> String {
    format!("{}: {}", error.code, error.message)
}

#[tauri::command]
pub fn api_fusion_get_config() -> Result<FusionConfig, String> {
    read_config()
}

#[tauri::command]
pub async fn api_fusion_save_config(config: FusionConfig) -> Result<FusionConfig, String> {
    let existing = read_config()?;
    let mut next = config;
    for provider in &mut next.providers {
        if provider.api_key.trim().is_empty() || provider.api_key == "********" {
            if let Some(previous) = existing
                .providers
                .iter()
                .find(|candidate| candidate.id == provider.id)
            {
                provider.api_key = previous.api_key.clone();
            }
        }
    }
    for key in &mut next.keys {
        if key.value.trim().is_empty() || key.value == "********" {
            if let Some(previous) = existing.keys.iter().find(|candidate| candidate.id == key.id) {
                key.value = previous.value.clone();
            }
        }
    }
    write_config(&next)?;
    if next.enabled {
        api_fusion_start().await?;
    } else {
        api_fusion_stop().await?;
    }
    read_config()
}

#[tauri::command]
pub fn api_fusion_upsert_provider(
    mut provider: FusionUpstreamProvider,
) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    if provider.id.trim().is_empty() {
        provider.id = new_provider_id();
    }
    if let Some(existing) = find_provider_mut(&mut config, &provider.id) {
        if provider.api_key.trim().is_empty() || provider.api_key == "********" {
            provider.api_key = existing.api_key.clone();
        }
        *existing = provider;
    } else {
        config.providers.push(provider);
    }
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub fn api_fusion_delete_provider(provider_id: String) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    config.providers.retain(|provider| provider.id != provider_id);
    config
        .terminal_syncs
        .retain(|record| record.provider_id != provider_id);
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub fn api_fusion_set_provider_enabled(
    provider_id: String,
    enabled: bool,
) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    let provider = find_provider_mut(&mut config, &provider_id)
        .ok_or_else(|| format!("provider not found: {provider_id}"))?;
    set_user_enabled(provider, enabled);
    write_config(&config)?;
    read_config()
}

/// Manual re-enable clears only the auto-disabled runtime state.
#[tauri::command]
pub fn api_fusion_reenable_provider(provider_id: String) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    let provider = find_provider_mut(&mut config, &provider_id)
        .ok_or_else(|| format!("provider not found: {provider_id}"))?;
    manual_reenable(provider);
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub fn api_fusion_upsert_key(mut key: FusionKey) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    touch_key_created_at(&mut key);
    if key.id.trim().is_empty() {
        key.id = new_key_id();
    }
    if let Some(existing) = config.keys.iter_mut().find(|candidate| candidate.id == key.id) {
        if key.value.trim().is_empty() || key.value == "********" {
            key.value = existing.value.clone();
        }
        *existing = key;
    } else {
        if key.value.trim().is_empty() {
            key.value = new_key_value();
        }
        config.keys.push(key);
    }
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub fn api_fusion_delete_key(key_id: String) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    config.keys.retain(|key| key.id != key_id);
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub fn api_fusion_set_default_key(key_id: String) -> Result<FusionConfig, String> {
    let mut config = read_config()?;
    let enabled = config
        .keys
        .iter()
        .any(|key| key.id == key_id && key.enabled);
    if !enabled {
        return Err(format!(
            "cannot switch default key: '{key_id}' is not an enabled local key"
        ));
    }
    config.default_key_id = Some(key_id);
    write_config(&config)?;
    read_config()
}

/// Persist the enable intent after a successful start/stop so a later autostart
/// restores the last state. Re-reads the config first so other fields (and any
/// concurrent edits) are preserved; the flag is only written once the listener
/// transition succeeded.
fn persist_enabled(enabled: bool) -> Result<(), String> {
    let mut config = read_config()?;
    if config.enabled != enabled {
        config.enabled = enabled;
        write_config(&config)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn api_fusion_start() -> Result<FusionStatus, String> {
    let status = start_server().await?;
    persist_enabled(true)?;
    Ok(status)
}

#[tauri::command]
pub async fn api_fusion_stop() -> Result<FusionStatus, String> {
    let status = stop_server().await?;
    persist_enabled(false)?;
    Ok(status)
}

#[tauri::command]
pub fn api_fusion_status() -> Result<FusionStatus, String> {
    server_status()
}

pub async fn api_fusion_autostart() -> Result<FusionStatus, String> {
    autostart().await
}

/// Find the managed gateway provider for a tool: the ledger record's provider id
/// when it still exists for the same tool AND still carries the gateway marker,
/// else the marked provider for the tool. A ledger id that now points at an
/// unmarked user provider is stale and must never be claimed.
fn find_managed_gateway_provider<'a>(
    tool: &str,
    providers: &'a [Value],
    ledger: Option<&TerminalSyncRecord>,
) -> Option<&'a Value> {
    if let Some(record) = ledger {
        if record.tool.eq_ignore_ascii_case(tool) {
            let found = providers.iter().find(|provider| {
                provider.get("id").and_then(Value::as_str) == Some(record.provider_id.as_str())
                    && provider_tool(provider).eq_ignore_ascii_case(tool)
                    && provider_has_gateway_marker(provider)
            });
            if found.is_some() {
                return found;
            }
        }
    }
    providers
        .iter()
        .find(|provider| provider_tool(provider).eq_ignore_ascii_case(tool) && provider_has_gateway_marker(provider))
}

/// Resolve the provider id a sync should write to for one tool: first the ledger
/// record that still matches a marked same-tool provider, then a marked gateway
/// provider, then a freshly generated id. A ledger id pointing at an unmarked
/// user provider is stale and is skipped.
fn resolve_gateway_provider_id(
    tool: &str,
    providers: &[Value],
    config: &FusionConfig,
) -> String {
    for record in &config.terminal_syncs {
        if !record.tool.eq_ignore_ascii_case(tool) {
            continue;
        }
        let existing = providers.iter().find(|provider| {
            provider.get("id").and_then(Value::as_str) == Some(record.provider_id.as_str())
                && provider_tool(provider).eq_ignore_ascii_case(tool)
                && provider_has_gateway_marker(provider)
        });
        if let Some(id) = existing.and_then(|provider| provider.get("id").and_then(Value::as_str)) {
            return id.to_string();
        }
    }
    if let Some(id) = providers
        .iter()
        .find(|provider| {
            provider_tool(provider).eq_ignore_ascii_case(tool)
                && provider_has_gateway_marker(provider)
        })
        .and_then(|provider| provider.get("id").and_then(Value::as_str))
    {
        return id.to_string();
    }
    uuid::Uuid::new_v4().to_string()
}

/// Build the per-tool terminal target projection from the persisted config and
/// the current terminal service provider list. Managed gateways are recognized
/// through the gateway marker only; a stale ledger id that points at an unmarked
/// user provider is never claimed.
pub(in crate::api_fusion) fn terminal_targets_from(
    config: &FusionConfig,
    providers_data: &serde_json::Value,
) -> Vec<TerminalTarget> {
    let providers = providers_data
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let base_url = local_base_url(config.port);
    let mut targets = Vec::new();
    for tool in SUPPORTED_TERMINAL_TOOLS {
        let ledger = config
            .terminal_syncs
            .iter()
            .find(|record| record.tool.eq_ignore_ascii_case(tool));
        let managed = find_managed_gateway_provider(tool, providers, ledger);
        let provider_id = managed
            .and_then(|provider| provider.get("id").and_then(Value::as_str))
            .map(str::to_string);
        let provider_base_url = managed
            .and_then(|provider| provider.get("base_url").and_then(Value::as_str))
            .map(str::to_string);
        let synced = provider_id.is_some();
        let pending_sync = match (synced, ledger) {
            (true, Some(record)) => {
                terminal_sync_pending(record, config.default_key_id.as_deref(), &base_url)
            }
            _ => true,
        };
        targets.push(TerminalTarget {
            tool: tool.to_string(),
            name: match tool {
                "opencode" => "OpenCode".to_string(),
                _ => "Codex".to_string(),
            },
            provider_id,
            base_url: provider_base_url,
            synced,
            pending_sync,
            synced_key_id: ledger.map(|record| record.synced_key_id.clone()),
            synced_at: ledger.map(|record| record.synced_at),
        });
    }
    targets
}

#[tauri::command]
pub fn api_fusion_terminal_targets() -> Result<Vec<TerminalTarget>, String> {
    let config = read_config()?;
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    Ok(terminal_targets_from(&config, &payload.data))
}

/// Boxed future returned by an injected terminal upsert, kept `Send` so the
/// pipeline can run on the Tauri async runtime.
pub(in crate::api_fusion) type UpsertFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Terminal sync pipeline with an injectable upsert seam: build one gateway
/// provider per requested tool, upsert each one (carrying the resolved default
/// local key value as its `api_key`), then refresh the ledger. Any
/// upsert error aborts before the ledger is written, so the ledger never claims
/// a sync that did not happen. The payload itself never carries an
/// `active`/`is_active` flag; opencode activation plus projection to
/// opencode.json are applied separately in `apply_terminal_sync` after the
/// ledger is persisted.
pub(in crate::api_fusion) async fn apply_terminal_sync_with<F>(
    providers_data: &serde_json::Value,
    mut upsert: F,
    target_tools: Vec<String>,
) -> Result<Vec<TerminalSyncRecord>, String>
where
    F: FnMut(serde_json::Value) -> UpsertFuture,
{
    let mut config = read_config()?;
    let (key_id, key_value) = default_key_for_sync(&config)?;
    if target_tools.is_empty() {
        return Err("no terminal targets selected".to_string());
    }

    let mut tools: Vec<String> = Vec::new();
    for tool in target_tools {
        let tool = tool.trim().to_string();
        if tools
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&tool))
        {
            continue;
        }
        tools.push(tool);
    }
    for tool in &tools {
        if !is_supported_terminal_tool(tool) {
            return Err(format!(
                "unsupported terminal tool '{tool}': only opencode and codex are supported"
            ));
        }
    }

    let base_url = local_base_url(config.port);
    let providers: &[Value] = providers_data
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut synced = Vec::new();
    for tool in &tools {
        let provider_id = resolve_gateway_provider_id(tool, providers, &config);
        let payload = build_gateway_provider(
            &provider_id,
            tool,
            &base_url,
            &key_value,
            &config.providers,
        )?;
        upsert(payload).await?;
        let record = TerminalSyncRecord {
            provider_id,
            tool: tool.clone(),
            synced_key_id: key_id.clone(),
            synced_base_url: base_url.clone(),
            synced_at: now_ts(),
        };
        config
            .terminal_syncs
            .retain(|existing| !existing.tool.eq_ignore_ascii_case(&record.tool));
        config.terminal_syncs.push(record.clone());
        synced.push(record);
    }
    write_config(&config)?;
    Ok(synced)
}

async fn apply_terminal_sync(
    app: tauri::AppHandle,
    target_tools: Vec<String>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    let upsert_app = app.clone();
    let records = apply_terminal_sync_with(
        &payload.data,
        move |value| -> UpsertFuture {
            let app = upsert_app.clone();
            Box::pin(async move {
                crate::app_store::service_providers_upsert(app, value)
                    .await
                    .map(|_| ())
                    .map_err(api_err_to_string)
            })
        },
        target_tools,
    )
    .await?;
    // Auto-activate the gateway provider under opencode so the synced endpoint
    // takes effect, then project it to ~/.config/opencode/opencode.json so the
    // entry (including options.apiKey) actually lands on disk; codex keeps its
    // manual activation. A stale ledger entry that resolved to an unmarked user
    // record never reaches here because the pipeline writes a fresh gateway
    // record instead.
    for record in &records {
        if record.tool.eq_ignore_ascii_case("opencode") {
            crate::app_store::service_providers_set_active(
                app.clone(),
                "opencode".to_string(),
                record.provider_id.clone(),
            )
            .await
            .map_err(api_err_to_string)?;
            crate::app_store::projection_apply(
                app.clone(),
                "opencode".to_string(),
                record.provider_id.clone(),
            )
            .await
            .map_err(api_err_to_string)?;
        }
    }
    Ok(records)
}

#[tauri::command]
pub async fn api_fusion_configure_terminal(
    app: tauri::AppHandle,
    target_tools: Vec<String>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    apply_terminal_sync(app, target_tools).await
}

#[tauri::command]
pub async fn api_fusion_sync_terminal(
    app: tauri::AppHandle,
    target_tools: Option<Vec<String>>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let config = read_config()?;
    let targets = match target_tools {
        Some(tools) if !tools.is_empty() => tools,
        _ => config
            .terminal_syncs
            .iter()
            .map(|record| record.tool.clone())
            .collect(),
    };
    apply_terminal_sync(app, targets).await
}

// ---------------------------------------------------------------------------
// Usage statistics, request logs, model prices and retention
// ---------------------------------------------------------------------------

/// Aggregated cards, UTC+8 buckets and per-model/provider detail for `days`.
/// `None` means all time, `Some(1)` means today; a single day buckets by hour.
#[tauri::command]
pub fn api_fusion_usage_stats(days: Option<i64>) -> Result<UsageStats, String> {
    let range = resolve_range(days, now_millis());
    let hour_buckets = days == Some(1);
    UsageLogStore::default_store()?.usage_stats(&range, hour_buckets)
}

/// One page (50 rows, newest first) of request logs, or grouped rows when
/// `group_by` is `"model"` or `"day"`. Range resolution, grouping, filtering and
/// pagination all happen here in the backend.
#[tauri::command]
pub fn api_fusion_request_logs(
    days: Option<i64>,
    group_by: Option<String>,
    status: Option<String>,
    model: Option<String>,
    page: Option<u32>,
) -> Result<UsageLogsPage, String> {
    let range = resolve_range(days, now_millis());
    let filter = LogFilter {
        status: status.as_deref().and_then(UsageResult::parse),
        model: model.filter(|model| !model.trim().is_empty()),
    };
    let store = UsageLogStore::default_store()?;
    let group = group_by.as_deref().unwrap_or("none");
    match group {
        "none" | "" => store.query_logs(&range, &filter, page.unwrap_or(1)),
        "model" | "day" => {
            let groups = store.group_logs(&range, &filter, group)?;
            Ok(UsageLogsPage {
                page: 1,
                page_size: USAGE_LOG_PAGE_SIZE,
                total: groups.len() as u32,
                total_pages: 1,
                group_by: Some(group.to_string()),
                records: Vec::new(),
                groups,
            })
        }
        other => Err(format!(
            "unsupported group_by '{other}': expected 'none', 'model' or 'day'"
        )),
    }
}

#[tauri::command]
pub fn api_fusion_model_prices_get() -> Result<Vec<ModelPrice>, String> {
    Ok(read_config()?.model_prices)
}

/// Replace only the price table, preserving providers, keys and terminal_syncs.
#[tauri::command]
pub fn api_fusion_model_prices_save(prices: Vec<ModelPrice>) -> Result<Vec<ModelPrice>, String> {
    let mut config = read_config()?;
    config.model_prices = prices;
    write_config(&config)?;
    Ok(read_config()?.model_prices)
}

#[tauri::command]
pub fn api_fusion_usage_retention_get() -> Result<u32, String> {
    Ok(normalize_retention_days(read_config()?.usage_retention_days))
}

/// Replace only the retention days; invalid values (not 1-365) are rejected
/// with an actionable error and are never persisted.
#[tauri::command]
pub fn api_fusion_usage_retention_save(days: i64) -> Result<u32, String> {
    let validated = validate_retention_days(days)?;
    let mut config = read_config()?;
    config.usage_retention_days = validated;
    write_config(&config)?;
    Ok(read_config()?.usage_retention_days)
}
