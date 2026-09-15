use super::runtime_http::{autostart, server_status, start_server, stop_server};
use super::selection::{manual_reenable, set_user_enabled};
use super::storage::{
    effective_default_key, find_provider_mut, local_base_url, new_key_id, new_provider_id,
    read_config, touch_key_created_at, write_config,
};
use super::{now_ts, FusionConfig, FusionKey, FusionStatus, FusionUpstreamProvider, TerminalSyncRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Terminal tools that API Fusion is allowed to write to.
pub(in crate::api_fusion) const SUPPORTED_TERMINAL_TOOLS: [&str; 2] = ["opencode", "codex"];

/// A terminal service provider record that API Fusion can configure or sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalTarget {
    pub provider_id: String,
    pub tool: String,
    pub name: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: String,
    pub synced: bool,
    pub pending_sync: bool,
    #[serde(default)]
    pub synced_key_id: Option<String>,
    #[serde(default)]
    pub synced_at: Option<u64>,
}

/// Plan describing the merged record to submit for one terminal target.
#[derive(Debug)]
pub(in crate::api_fusion) struct TerminalSyncPlan {
    pub(in crate::api_fusion) provider_id: String,
    pub(in crate::api_fusion) tool: String,
    pub(in crate::api_fusion) merged: Value,
}

pub(in crate::api_fusion) fn is_supported_terminal_tool(tool: &str) -> bool {
    SUPPORTED_TERMINAL_TOOLS
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(tool.trim()))
}

/// Merge only `base_url` and `api_key` into an existing provider record so every
/// other field (name, model, icon, enabled state, tool config, ...) is preserved.
pub(in crate::api_fusion) fn merge_terminal_provider(
    existing: &Value,
    base_url: &str,
    api_key: &str,
) -> Result<Value, String> {
    let mut object = existing
        .as_object()
        .cloned()
        .ok_or_else(|| "terminal provider record must be a JSON object".to_string())?;
    object.insert("base_url".to_string(), Value::String(base_url.to_string()));
    object.insert("api_key".to_string(), Value::String(api_key.to_string()));
    Ok(Value::Object(object))
}

pub(in crate::api_fusion) fn plan_terminal_sync(
    providers_payload: &Value,
    target_ids: &[String],
    base_url: &str,
    api_key: &str,
) -> Result<Vec<TerminalSyncPlan>, String> {
    let providers = providers_payload
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| "service providers payload is missing 'providers'".to_string())?;
    let mut plans = Vec::new();
    for target_id in target_ids {
        let existing = providers
            .iter()
            .find(|provider| provider.get("id").and_then(Value::as_str) == Some(target_id.as_str()))
            .ok_or_else(|| format!("terminal provider not found: {target_id}"))?;
        let tool = existing
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !is_supported_terminal_tool(&tool) {
            return Err(format!(
                "unsupported terminal tool '{tool}': only opencode and codex are supported"
            ));
        }
        let merged = merge_terminal_provider(existing, base_url, api_key)?;
        plans.push(TerminalSyncPlan {
            provider_id: target_id.clone(),
            tool,
            merged,
        });
    }
    Ok(plans)
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
    match effective_default_key(config) {
        Some(key) => Ok((key.id.clone(), key.value.clone())),
        None => Err(
            "no enabled local API key: add and enable a local key before configuring terminals"
                .to_string(),
        ),
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

#[tauri::command]
pub fn api_fusion_terminal_targets() -> Result<Vec<TerminalTarget>, String> {
    let config = read_config()?;
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    let providers = payload
        .data
        .get("providers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let base_url = local_base_url(config.port);
    let mut targets = Vec::new();
    for provider in providers {
        let tool = provider
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !is_supported_terminal_tool(&tool) {
            continue;
        }
        let provider_id = provider
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ledger = config
            .terminal_syncs
            .iter()
            .find(|record| record.provider_id == provider_id);
        let pending_sync = match ledger {
            Some(record) => {
                terminal_sync_pending(record, config.default_key_id.as_deref(), &base_url)
            }
            None => true,
        };
        targets.push(TerminalTarget {
            provider_id,
            tool,
            name: provider
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            base_url: provider
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::to_string),
            api_key: provider
                .get("api_key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            synced: ledger.is_some(),
            pending_sync,
            synced_key_id: ledger.map(|record| record.synced_key_id.clone()),
            synced_at: ledger.map(|record| record.synced_at),
        });
    }
    Ok(targets)
}

/// Boxed future returned by an injected terminal upsert, kept `Send` so the
/// pipeline can run on the Tauri async runtime.
pub(in crate::api_fusion) type UpsertFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Terminal sync pipeline with an injectable upsert seam: plan the merged
/// provider records, upsert each one, then refresh the ledger. Any upsert error
/// aborts before the ledger is written, so the ledger never claims a sync that
/// did not happen.
pub(in crate::api_fusion) async fn apply_terminal_sync_with<F>(
    providers_data: &serde_json::Value,
    mut upsert: F,
    target_ids: Vec<String>,
) -> Result<Vec<TerminalSyncRecord>, String>
where
    F: FnMut(serde_json::Value) -> UpsertFuture,
{
    let mut config = read_config()?;
    let (key_id, key_value) = default_key_for_sync(&config)?;
    if target_ids.is_empty() {
        return Err("no terminal targets selected".to_string());
    }
    let base_url = local_base_url(config.port);
    let plans = plan_terminal_sync(providers_data, &target_ids, &base_url, &key_value)?;

    let mut synced = Vec::new();
    for plan in plans {
        upsert(plan.merged).await?;
        let record = TerminalSyncRecord {
            provider_id: plan.provider_id.clone(),
            tool: plan.tool.clone(),
            synced_key_id: key_id.clone(),
            synced_base_url: base_url.clone(),
            synced_at: now_ts(),
        };
        config
            .terminal_syncs
            .retain(|existing| existing.provider_id != record.provider_id);
        config.terminal_syncs.push(record.clone());
        synced.push(record);
    }
    write_config(&config)?;
    Ok(synced)
}

async fn apply_terminal_sync(
    app: tauri::AppHandle,
    target_ids: Vec<String>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    apply_terminal_sync_with(
        &payload.data,
        move |value| -> UpsertFuture {
            let app = app.clone();
            Box::pin(async move {
                crate::app_store::service_providers_upsert(app, value)
                    .await
                    .map(|_| ())
                    .map_err(api_err_to_string)
            })
        },
        target_ids,
    )
    .await
}

#[tauri::command]
pub async fn api_fusion_configure_terminal(
    app: tauri::AppHandle,
    target_ids: Vec<String>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    apply_terminal_sync(app, target_ids).await
}

#[tauri::command]
pub async fn api_fusion_sync_terminal(
    app: tauri::AppHandle,
    target_ids: Option<Vec<String>>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let config = read_config()?;
    let targets = match target_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => config
            .terminal_syncs
            .iter()
            .map(|record| record.provider_id.clone())
            .collect(),
    };
    apply_terminal_sync(app, targets).await
}
