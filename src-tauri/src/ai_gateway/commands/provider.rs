use super::{captured_gateway_app_handle, TerminalSyncProfile};
use super::terminal::{apply_terminal_sync, previously_synced_terminal_tools};
use crate::ai_gateway::model_pools::reserved_aliases;
use crate::ai_gateway::selection::{
    clear_key_runtime_state, clear_mapping_runtime_state, manual_reenable, mapping_matches_key,
    set_user_enabled,
};
use crate::ai_gateway::storage::{
    find_provider_mut, modify_config, new_key_id, new_provider_id, read_config,
};
use crate::ai_gateway::templates::effective_template;
#[cfg(test)]
use crate::ai_gateway::TerminalSyncRecord;
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, ModelPrice, MAX_PROVIDER_WEIGHT, MIN_PROVIDER_WEIGHT,
};

/// Validate and normalize a brand-new provider's pool: every key gets a stable
/// id when missing, every key must carry a value, and every key starts healthy.
fn prepare_new_provider_keys(provider: &mut GatewayUpstreamProvider) -> Result<(), String> {
    for key in &mut provider.keys {
        if key.id.trim().is_empty() {
            key.id = new_key_id();
        }
        if key.value.trim().is_empty() {
            return Err(format!(
                "a new upstream key '{}' requires a value",
                key.name.trim()
            ));
        }
        clear_key_runtime_state(key);
    }
    Ok(())
}

/// Merge an incoming pool into the stored one by key id. A blank value keeps
/// the stored value and runtime state, an unchanged value preserves the stored
/// runtime state, a changed value clears it (the user revalued the key), and an
/// unknown id starts a healthy new key. Deleting is by omission: a stored id
/// absent from the incoming list is dropped with its runtime state.
fn merge_provider_keys(
    existing: &GatewayUpstreamProvider,
    provider: &mut GatewayUpstreamProvider,
) -> Result<(), String> {
    for key in &mut provider.keys {
        let stored = if key.id.trim().is_empty() {
            None
        } else {
            existing.keys.iter().find(|stored| stored.id == key.id)
        };
        match stored {
            None => {
                if key.id.trim().is_empty() {
                    key.id = new_key_id();
                }
                if key.value.trim().is_empty() {
                    return Err(format!(
                        "a new upstream key '{}' requires a value",
                        key.name.trim()
                    ));
                }
                clear_key_runtime_state(key);
            }
            Some(stored) => {
                if key.value.trim().is_empty() || key.value == stored.value {
                    key.value = stored.value.clone();
                    key.auto_marked = stored.auto_marked;
                    key.failure_kind = stored.failure_kind;
                    key.marked_at = stored.marked_at;
                    key.reason = stored.reason.clone();
                } else {
                    clear_key_runtime_state(key);
                }
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub fn ai_gateway_upsert_provider(
    provider: GatewayUpstreamProvider,
    prices: Option<Vec<ModelPrice>>,
) -> Result<GatewayConfig, String> {
    let (saved, context_changed) = save_provider_persist(provider, prices)?;
    if context_changed {
        refresh_previously_synced_tools_after_save(&saved);
    }
    Ok(saved)
}

/// Persist one provider save through the same validation and merge path every
/// caller uses, returning whether any mapping's effective context changed.
fn save_provider_persist(
    mut provider: GatewayUpstreamProvider,
    prices: Option<Vec<ModelPrice>>,
) -> Result<(GatewayConfig, bool), String> {
    let context_changed = modify_config(|config| {
        let previous_context = config
            .providers
            .iter()
            .find(|stored| stored.id == provider.id.trim())
            .map(provider_context_rows);
        if provider.weight < MIN_PROVIDER_WEIGHT || provider.weight > MAX_PROVIDER_WEIGHT {
            return Err(format!(
                "provider weight must be between {MIN_PROVIDER_WEIGHT} and {MAX_PROVIDER_WEIGHT}"
            ));
        }
        // A submitted template binding must resolve against a known, non-deleted
        // template before anything is staged; `None` (or a missing field)
        // unbinds. An unknown or deleted id is rejected without a write, so a
        // broken binding can never be persisted through the upsert.
        let bound_template = match provider.template_id.as_deref() {
            Some(template_id) => {
                let template_id = template_id.trim();
                if template_id.is_empty() {
                    return Err("provider template id must not be blank".to_string());
                }
                Some(effective_template(config, template_id)?)
            }
            None => None,
        };
        // A key name is required and may repeat; reject before anything is staged.
        for key in &provider.keys {
            if key.name.trim().is_empty() {
                return Err("an upstream key requires a non-blank name".to_string());
            }
        }
        // A declared context window must be a positive token count; an invalid
        // submitted value is rejected before any write.
        for mapping in &provider.mappings {
            if mapping.context_window == Some(0) {
                return Err(format!(
                    "mapping '{} -> {}' declares an invalid context window: expected a positive integer",
                    mapping.local_model, mapping.upstream_model
                ));
            }
        }
        // A bound provider is read-only for this field: the submitted rows are
        // canonicalized against the latest effective template, so a stale dialog
        // can never establish an override. Unbinding keeps the submitted value.
        if let Some(template) = &bound_template {
            for mapping in &mut provider.mappings {
                mapping.context_window = template
                    .models
                    .iter()
                    .find(|model| model.upstream_model.trim() == mapping.upstream_model.trim())
                    .and_then(|model| model.context_window);
            }
        }
        if provider.id.trim().is_empty() {
            provider.id = new_provider_id();
        }
        let provider_id = provider.id.clone();
        // An ordinary provider mapping must not acquire a current or retired
        // model-pool alias. Retaining an already-stored row is
        // allowed so resubmitting an unchanged provider never fails.
        let pool_aliases = reserved_aliases(config);
        if !pool_aliases.is_empty() {
            let stored_mappings: Vec<String> = config
                .providers
                .iter()
                .find(|stored| stored.id == provider_id)
                .map(|stored| {
                    stored
                        .mappings
                        .iter()
                        .map(|mapping| mapping.local_model.trim().to_string())
                        .collect()
                })
                .unwrap_or_default();
            for mapping in &provider.mappings {
                let local = mapping.local_model.trim();
                if local.is_empty() {
                    continue;
                }
                if pool_aliases.contains(local) && !stored_mappings.iter().any(|stored| stored == local)
                {
                    return Err(format!(
                        "mapping local model '{local}' is reserved by a model pool"
                    ));
                }
            }
        }
        if let Some(existing) = find_provider_mut(config, &provider_id) {
            merge_provider_keys(existing, &mut provider)?;
            // Runtime health belongs to a trimmed `(local_model, upstream_model)`
            // key: an unchanged key keeps the stored state, a new or changed key
            // starts healthy, and a deleted row drops its state with the replacement.
            for mapping in &mut provider.mappings {
                match existing.mappings.iter().find(|stored| {
                    mapping_matches_key(stored, &mapping.local_model, &mapping.upstream_model)
                }) {
                    Some(stored) => {
                        mapping.auto_disabled = stored.auto_disabled;
                        mapping.disabled_reason = stored.disabled_reason.clone();
                        mapping.disabled_at = stored.disabled_at;
                        mapping.consecutive_failures = stored.consecutive_failures;
                        mapping.last_error_at = stored.last_error_at;
                    }
                    None => clear_mapping_runtime_state(mapping),
                }
            }
            *existing = provider;
        } else {
            // A brand-new provider has no stored keys or rows, so every key and row
            // starts healthy; a new key still needs a value.
            prepare_new_provider_keys(&mut provider)?;
            for mapping in &mut provider.mappings {
                clear_mapping_runtime_state(mapping);
            }
            config.providers.push(provider);
        }
        if let Some(prices) = prices {
            let mut normalized: Vec<ModelPrice> = Vec::with_capacity(prices.len());
            for mut row in prices {
                let upstream_model = row.upstream_model.trim().to_string();
                if upstream_model.is_empty() {
                    continue;
                }
                if normalized
                    .iter()
                    .any(|existing| existing.upstream_model == upstream_model)
                {
                    continue;
                }
                row.upstream_model = upstream_model;
                row.provider_id = Some(provider_id.clone());
                normalized.push(row);
            }
            config
                .model_prices
                .retain(|row| row.provider_id.as_deref() != Some(provider_id.as_str()));
            config.model_prices.extend(normalized);
        }
        let next_context = config
            .providers
            .iter()
            .find(|stored| stored.id == provider_id)
            .map(provider_context_rows);
        let context_changed = previous_context != next_context;
        Ok((true, context_changed))
    })?;
    // The write path publishes a normalized, provider-scoped copy, so hand back
    // that persisted value instead of the pre-normalization mutation clone.
    let saved = read_config()?;
    Ok((saved, context_changed))
}

/// Stable comparison rows for one provider's declared context values.
fn provider_context_rows(provider: &GatewayUpstreamProvider) -> Vec<(String, String, Option<u32>)> {
    let mut rows: Vec<(String, String, Option<u32>)> = provider
        .mappings
        .iter()
        .map(|mapping| {
            (
                mapping.local_model.trim().to_string(),
                mapping.upstream_model.trim().to_string(),
                mapping.context_window,
            )
        })
        .collect();
    rows.sort();
    rows
}

/// Stable snapshot of every provider's effective mapping contexts, used to
/// decide whether a template save actually changed inherited values.
pub(super) fn provider_context_snapshot(
    config: &GatewayConfig,
) -> Vec<(String, Vec<(String, String, Option<u32>)>)> {
    let mut snapshot: Vec<(String, Vec<(String, String, Option<u32>)>)> = config
        .providers
        .iter()
        .map(|provider| (provider.id.clone(), provider_context_rows(provider)))
        .collect();
    snapshot.sort_by(|left, right| left.0.cmp(&right.0));
    snapshot
}

/// Best-effort post-save refresh for tools that already carry the managed
/// marker. The save already persisted, so a refresh failure never changes the
/// caller's result.
pub(super) fn refresh_previously_synced_tools_after_save(config: &GatewayConfig) {
    let Some(app) = captured_gateway_app_handle() else {
        return;
    };
    let Ok(payload) = crate::app_store::service_providers_list() else {
        return;
    };
    let tools = previously_synced_terminal_tools(
        config,
        &payload.data,
        TerminalSyncProfile::current(),
    );
    if tools.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let _ = apply_terminal_sync(app, tools, None).await;
    });
}

/// Injectable provider-save composition: persist through the production save
/// path, then refresh the previously synchronized tools at most once.
#[cfg(test)]
pub(in crate::ai_gateway) async fn apply_provider_save_with_terminal_refresh<F, Fut>(
    provider: GatewayUpstreamProvider,
    prices: Option<Vec<ModelPrice>>,
    providers_data: &serde_json::Value,
    terminal_sync: F,
) -> Result<GatewayConfig, String>
where
    F: FnOnce(Vec<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Vec<TerminalSyncRecord>, String>> + Send,
{
    let (saved, context_changed) = save_provider_persist(provider, prices)?;
    if context_changed {
        let tools = previously_synced_terminal_tools(
            &saved,
            providers_data,
            TerminalSyncProfile::current(),
        );
        if !tools.is_empty() {
            let _ = terminal_sync(tools).await;
        }
    }
    Ok(saved)
}
#[tauri::command]
pub fn ai_gateway_delete_provider(provider_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        config.providers.retain(|provider| provider.id != provider_id);
        config
            .model_prices
            .retain(|row| row.provider_id.as_deref() != Some(provider_id.as_str()));
        config
            .terminal_syncs
            .retain(|record| record.provider_id != provider_id);
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_set_provider_enabled(
    provider_id: String,
    enabled: bool,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        set_user_enabled(provider, enabled);
        Ok((true, ()))
    })?;
    read_config()
}
/// Clear the runtime health state of the mapping row identified by the trimmed
/// `(local_model, upstream_model)` key. Duplicate rows that share the trimmed
/// key are cleared together, and the user's `enabled` intent is never touched.
/// An unknown provider, or a key that matches no row (including a blank local or
/// upstream model), is an actionable error that writes nothing.
#[tauri::command]
pub fn ai_gateway_reenable_provider_model(
    provider_id: String,
    local_model: String,
    upstream_model: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        let mut matched = false;
        for mapping in provider.mappings.iter_mut().filter(|mapping| {
            mapping_matches_key(mapping, &local_model, &upstream_model)
        }) {
            clear_mapping_runtime_state(mapping);
            matched = true;
        }
        if !matched {
            return Err(format!(
                "no mapping row matches '{local_model}' -> '{upstream_model}' for provider '{provider_id}'"
            ));
        }
        Ok((true, ()))
    })?;
    read_config()
}

/// Clear the runtime health state of every auto-disabled mapping row of one
/// provider. Each row keeps its `enabled` intent (a user-disabled row stays
/// disabled), rows that are not auto-disabled keep their counter, and other
/// providers are untouched. An unknown provider is an actionable error that
/// writes nothing.
#[tauri::command]
pub fn ai_gateway_reenable_provider_models(provider_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        manual_reenable(provider);
        Ok((true, ()))
    })?;
    read_config()
}
