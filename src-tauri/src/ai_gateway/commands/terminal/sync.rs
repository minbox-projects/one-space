use super::provider::{
    build_claude_gateway_provider, build_gateway_provider_with_pools, claude_context_summary,
    claude_selection_from_provider, codex_context_summary, context_scalar, derived_claude_code,
    opencode_context_summary, validate_claude_selection,
};
use super::{is_supported_terminal_tool, provider_tool, TerminalTarget};
use super::super::{
    api_err_to_string, provider_has_gateway_marker, TerminalSyncProfile, SUPPORTED_TERMINAL_TOOLS,
};
use crate::ai_gateway::storage::{
    local_base_url, local_origin, modify_config, read_config, resolve_default_key_id,
};
use crate::ai_gateway::{now_ts, ClaudeModelSelection, GatewayConfig, TerminalSyncRecord};
use serde_json::Value;

/// Pending-sync is derived from the persisted ledger, never from the redacted api_key.
pub(in crate::ai_gateway) fn terminal_sync_pending(
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

/// Whether a fresh terminal sync would write a different model selection than
/// the stored gateway record. Key material is never part of the comparison, so
/// callers may build the fresh payload with a placeholder key.
fn terminal_model_selection_drifted(tool: &str, stored: &Value, generated: &Value) -> bool {
    match tool {
        "opencode" => {
            stored.pointer("/tool_config/models") != generated.pointer("/tool_config/models")
        }
        "codex" => {
            stored.get("model") != generated.get("model")
                || context_scalar(stored.pointer("/tool_config/model_context_window"))
                    != context_scalar(generated.pointer("/tool_config/model_context_window"))
        }
        _ => true,
    }
}

pub(in crate::ai_gateway) fn default_key_for_sync(
    config: &GatewayConfig,
) -> Result<(String, String), String> {
    // Mirror the frontend `resolveDefaultKeyId` rule: the stored choice wins
    // while it points at an enabled key, otherwise fall through to the next
    // enabled key in list order (wrapping), so syncing still carries the
    // default API key the UI shows. A `None` result means no enabled key exists.
    let resolved = resolve_default_key_id(&config.keys, config.default_key_id.as_deref());
    match resolved.and_then(|id| {
        config
            .keys
            .iter()
            .find(|key| key.id == id && key.enabled)
    }) {
        Some(key) => Ok((key.id.clone(), key.value.clone())),
        None => Err(
            "no enabled local API key: add and enable a local key before configuring terminals"
                .to_string(),
        ),
    }
}
/// Find the managed gateway provider for a tool: the ledger record's provider id
/// when it still exists for the same tool AND still carries the profile's
/// gateway marker, else the profile-marked provider for the tool. A ledger id
/// that now points at a provider without the profile marker (a user provider or
/// the other profile's record) is stale and must never be claimed.
fn find_managed_gateway_provider<'a>(
    tool: &str,
    providers: &'a [Value],
    ledger: Option<&TerminalSyncRecord>,
    profile: TerminalSyncProfile,
) -> Option<&'a Value> {
    if let Some(record) = ledger {
        if record.tool.eq_ignore_ascii_case(tool) {
            let found = providers.iter().find(|provider| {
                provider.get("id").and_then(Value::as_str) == Some(record.provider_id.as_str())
                    && provider_tool(provider).eq_ignore_ascii_case(tool)
                    && provider_has_gateway_marker(provider, profile)
            });
            if found.is_some() {
                return found;
            }
        }
    }
    providers.iter().find(|provider| {
        provider_tool(provider).eq_ignore_ascii_case(tool)
            && provider_has_gateway_marker(provider, profile)
    })
}

/// Resolve the provider id a sync should write to for one tool: first the ledger
/// record that still matches a profile-marked same-tool provider, then a
/// profile-marked gateway provider, then a freshly generated id. A ledger id
/// pointing at a provider without the profile marker is stale and is skipped, so
/// a foreign-marked record (including the other profile's) is treated as absent.
fn resolve_gateway_provider_id(
    tool: &str,
    providers: &[Value],
    config: &GatewayConfig,
    profile: TerminalSyncProfile,
) -> String {
    for record in &config.terminal_syncs {
        if !record.tool.eq_ignore_ascii_case(tool) {
            continue;
        }
        let existing = providers.iter().find(|provider| {
            provider.get("id").and_then(Value::as_str) == Some(record.provider_id.as_str())
                && provider_tool(provider).eq_ignore_ascii_case(tool)
                && provider_has_gateway_marker(provider, profile)
        });
        if let Some(id) = existing.and_then(|provider| provider.get("id").and_then(Value::as_str)) {
            return id.to_string();
        }
    }
    if let Some(id) = providers
        .iter()
        .find(|provider| {
            provider_tool(provider).eq_ignore_ascii_case(tool)
                && provider_has_gateway_marker(provider, profile)
        })
        .and_then(|provider| provider.get("id").and_then(Value::as_str))
    {
        return id.to_string();
    }
    uuid::Uuid::new_v4().to_string()
}

/// Build the per-tool terminal target projection from the persisted config and
/// the current terminal service provider list. Managed gateways are recognized
/// through the profile's gateway marker only; a stale ledger id that points at a
/// provider without that marker (a user provider or the other profile's record)
/// is never claimed.
pub(in crate::ai_gateway) fn terminal_targets_from(
    config: &GatewayConfig,
    providers_data: &serde_json::Value,
    profile: TerminalSyncProfile,
) -> Vec<TerminalTarget> {
    let providers = providers_data
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut targets = Vec::new();
    for tool in SUPPORTED_TERMINAL_TOOLS {
        let ledger = config
            .terminal_syncs
            .iter()
            .find(|record| record.tool.eq_ignore_ascii_case(tool));
        let managed = find_managed_gateway_provider(tool, providers, ledger, profile);
        let provider_id = managed
            .and_then(|provider| provider.get("id").and_then(Value::as_str))
            .map(str::to_string);
        let provider_base_url = managed
            .and_then(|provider| provider.get("base_url").and_then(Value::as_str))
            .map(str::to_string);
        let synced = provider_id.is_some();
        // Claude's native base URL is the bare origin; the OpenAI tools keep the
        // `/v1` loopback base URL.
        let current_base_url = if tool == "claude" {
            local_origin(config.port)
        } else {
            local_base_url(config.port)
        };
        // The Claude target exposes a model-only snapshot of the persisted
        // choices; an incomplete snapshot counts as drifted (pending) but stays
        // `synced` through the marker, so a manual sync can request replacement.
        let claude_models = if tool == "claude" {
            managed.and_then(claude_selection_from_provider)
        } else {
            None
        };
        let context = if tool == "opencode" {
            opencode_context_summary(config)
        } else if tool == "codex" {
            // Match the generated sync selection: the owned record's current
            // model can retain a previously selected enabled pool alias.
            let previous_model = managed
                .and_then(|provider| provider.get("model").and_then(Value::as_str));
            codex_context_summary(config, previous_model)
        } else {
            claude_context_summary(config, claude_models.as_ref())
        };
        let pending_sync = match (synced, ledger) {
            (true, Some(record)) => {
                let ledger_pending = terminal_sync_pending(
                    record,
                    config.default_key_id.as_deref(),
                    &current_base_url,
                );
                let model_drifted = if tool == "claude" {
                    let stored_context = managed.and_then(|stored| {
                        context_scalar(stored.pointer("/tool_config/claude_auto_compact_window"))
                    });
                    claude_models.is_none()
                        || stored_context != context.applied_context.map(u64::from)
                } else {
                    match (provider_id.as_deref(), managed) {
                        (Some(id), Some(stored)) => build_gateway_provider_with_pools(
                            id,
                            tool,
                            &current_base_url,
                            "",
                            &config.providers,
                            &config.model_pools,
                            stored.get("model").and_then(Value::as_str),
                            profile,
                        )
                        .map(|generated| terminal_model_selection_drifted(tool, stored, &generated))
                        .unwrap_or(true),
                        _ => true,
                    }
                };
                ledger_pending || model_drifted
            }
            _ => true,
        };
        targets.push(TerminalTarget {
            tool: tool.to_string(),
            name: match tool {
                "opencode" => "OpenCode".to_string(),
                "codex" => "Codex".to_string(),
                _ => "Claude".to_string(),
            },
            provider_id,
            base_url: provider_base_url,
            synced,
            pending_sync,
            synced_key_id: ledger.map(|record| record.synced_key_id.clone()),
            synced_at: ledger.map(|record| record.synced_at),
            claude_models,
            context: Some(context),
        });
    }
    targets
}

/// The supported terminal tools whose managed AI Gateway provider record for
/// `profile` already exists, in `SUPPORTED_TERMINAL_TOOLS` order. Reuses the same
/// `synced` predicate the terminal target list exposes, so a null or unreadable
/// payload yields no tools and the other profile's records are ignored. A marked
/// Claude record participates like the other tools; when its model selection is
/// unavailable the refresh performs no Claude write (the pipeline rejects the
/// missing selection before any write) and the template sync result still
/// stands.
pub(in crate::ai_gateway) fn previously_synced_terminal_tools(
    config: &GatewayConfig,
    providers_data: &serde_json::Value,
    profile: TerminalSyncProfile,
) -> Vec<String> {
    terminal_targets_from(config, providers_data, profile)
        .into_iter()
        .filter(|target| target.synced)
        .map(|target| target.tool)
        .collect()
}
/// Boxed future returned by an injected terminal upsert, kept `Send` so the
/// pipeline can run on the Tauri async runtime.
pub(in crate::ai_gateway) type UpsertFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Terminal sync pipeline with an injectable upsert seam: build one gateway
/// provider per requested tool for `profile`, upsert each one (carrying the
/// resolved default local key value as its `api_key`), then refresh the ledger.
/// Any upsert error aborts before the ledger is written, so the ledger never
/// claims a sync that did not happen. The payload itself never carries an
/// `active`/`is_active` flag; activation plus projection are applied separately
/// in `apply_terminal_sync` after the ledger is persisted.
///
/// This 4-argument entry point reads any Claude selection back from the owned
/// canonical provider; the command layer uses
/// [`apply_terminal_sync_with_selection`] to also accept an explicit operator
/// selection for a first sync.
#[cfg(test)]
pub(in crate::ai_gateway) async fn apply_terminal_sync_with<F>(
    providers_data: &serde_json::Value,
    upsert: F,
    target_tools: Vec<String>,
    profile: TerminalSyncProfile,
) -> Result<Vec<TerminalSyncRecord>, String>
where
    F: FnMut(serde_json::Value) -> UpsertFuture,
{
    apply_terminal_sync_with_selection(providers_data, upsert, target_tools, profile, None).await
}

/// Terminal sync pipeline with an injected upsert and an optional explicit
/// Claude model selection.
///
/// Every payload is built (and every Claude selection validated) before the
/// first upsert, so an invalid or missing Claude selection is rejected before
/// any Claude provider or ledger write. A `claude` target without an explicit
/// selection reads the four saved choices back from the owned canonical Claude
/// provider and resends them whole, so the existing replace-style upsert cannot
/// clear omitted choices.
pub(in crate::ai_gateway) async fn apply_terminal_sync_with_selection<F>(
    providers_data: &serde_json::Value,
    mut upsert: F,
    target_tools: Vec<String>,
    profile: TerminalSyncProfile,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String>
where
    F: FnMut(serde_json::Value) -> UpsertFuture,
{
    let config = read_config()?;
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
                "unsupported terminal tool '{tool}': only opencode, codex and claude are supported"
            ));
        }
    }

    let base_url = local_base_url(config.port);
    let origin = local_origin(config.port);
    let providers: &[Value] = providers_data
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    // Build every payload first so a rejected Claude selection never leaves a
    // partially upserted batch behind.
    struct PlannedTool {
        tool: String,
        provider_id: String,
        base_url: String,
        payload: Value,
    }
    let mut planned: Vec<PlannedTool> = Vec::new();
    for tool in &tools {
        if tool == "claude" {
            let ledger = config
                .terminal_syncs
                .iter()
                .find(|record| record.tool.eq_ignore_ascii_case("claude"));
            let managed = find_managed_gateway_provider("claude", providers, ledger, profile);
            let selection = match claude_models.as_ref() {
                Some(explicit) => {
                    let normalized = explicit.normalized().ok_or_else(|| {
                        "a Claude sync requires all four model choices to be non-blank".to_string()
                    })?;
                    validate_claude_selection(&config, &normalized)?;
                    normalized
                }
                None => {
                    // The read-back path validates exactly like an explicit
                    // submission. A multi-tool best-effort fan-out (the
                    // post-template refresh) requests Claude alongside other
                    // tools: an unavailable or unservable saved selection skips
                    // only Claude so the rest of the batch still syncs, while a
                    // Claude-only sync (manual) surfaces the rejection and
                    // writes nothing.
                    let fan_out = tools.len() > 1;
                    match managed.and_then(claude_selection_from_provider) {
                        Some(saved) => {
                            if let Err(error) = validate_claude_selection(&config, &saved) {
                                if fan_out {
                                    continue;
                                }
                                return Err(error);
                            }
                            saved
                        }
                        None => {
                            if fan_out {
                                continue;
                            }
                            return Err(
                                "a Claude sync requires a saved or submitted model selection; open the Claude model dialog first"
                                    .to_string(),
                            );
                        }
                    }
                }
            };
            let provider_id = managed
                .and_then(|provider| provider.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    resolve_gateway_provider_id("claude", providers, &config, profile)
                });
            let code = managed
                .and_then(|provider| provider.get("code").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| derived_claude_code(&provider_id));
            let auto_compact_window =
                claude_context_summary(&config, Some(&selection)).applied_context;
            let payload = build_claude_gateway_provider(
                &provider_id,
                &origin,
                &key_value,
                &selection,
                &code,
                auto_compact_window,
                profile,
            );
            planned.push(PlannedTool {
                tool: tool.clone(),
                provider_id,
                base_url: origin.clone(),
                payload,
            });
        } else {
            let provider_id = resolve_gateway_provider_id(tool, providers, &config, profile);
            // The owned record's current model lets a Codex resync retain an
            // already selected enabled pool alias instead of dropping it.
            let managed_ledger = config
                .terminal_syncs
                .iter()
                .find(|record| record.tool.eq_ignore_ascii_case(tool));
            let previous_model =
                find_managed_gateway_provider(tool, providers, managed_ledger, profile)
                    .and_then(|provider| provider.get("model").and_then(Value::as_str));
            let payload = build_gateway_provider_with_pools(
                &provider_id,
                tool,
                &base_url,
                &key_value,
                &config.providers,
                &config.model_pools,
                previous_model,
                profile,
            )?;
            planned.push(PlannedTool {
                tool: tool.clone(),
                provider_id,
                base_url: base_url.clone(),
                payload,
            });
        }
    }

    let mut synced = Vec::new();
    for plan in planned {
        upsert(plan.payload).await?;
        let record = TerminalSyncRecord {
            provider_id: plan.provider_id,
            tool: plan.tool,
            synced_key_id: key_id.clone(),
            synced_base_url: plan.base_url,
            synced_at: now_ts(),
        };
        synced.push(record);
    }
    // The upserts above are external network waits, so the ledger is persisted
    // afterwards as one serialized read-modify-write: the lock is never held
    // across an upsert, and a concurrent configuration write cannot be lost.
    modify_config(|latest| {
        for record in &synced {
            latest
                .terminal_syncs
                .retain(|existing| !existing.tool.eq_ignore_ascii_case(&record.tool));
            latest.terminal_syncs.push(record.clone());
        }
        Ok((true, ()))
    })?;
    Ok(synced)
}
pub(in crate::ai_gateway) async fn apply_terminal_sync(
    app: tauri::AppHandle,
    target_tools: Vec<String>,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    let upsert_app = app.clone();
    let records = apply_terminal_sync_with_selection(
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
        TerminalSyncProfile::current(),
        claude_models,
    )
    .await?;
    // Auto-activate the synced provider so the endpoint takes effect, then
    // project it to disk so the entry (including options.apiKey / the Claude
    // settings) actually lands; codex keeps its manual activation. A stale
    // ledger entry that resolved to an unmarked user record never reaches here
    // because the pipeline writes a fresh gateway record instead. Activation and
    // projection are not one transaction with the upsert, so a projection
    // failure is surfaced rather than reported as a successful application.
    for record in &records {
        if record.tool.eq_ignore_ascii_case("opencode")
            || record.tool.eq_ignore_ascii_case("claude")
        {
            crate::app_store::service_providers_set_active(
                app.clone(),
                record.tool.clone(),
                record.provider_id.clone(),
            )
            .await
            .map_err(api_err_to_string)?;
            crate::app_store::projection_apply(
                app.clone(),
                record.tool.clone(),
                record.provider_id.clone(),
            )
            .await
            .map_err(api_err_to_string)?;
        }
    }
    Ok(records)
}
