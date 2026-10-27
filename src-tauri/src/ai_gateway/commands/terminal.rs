mod provider;
pub(in crate::ai_gateway) mod sync;

#[cfg(test)]
pub(in crate::ai_gateway) use provider::build_gateway_provider;
pub(in crate::ai_gateway) use sync::{
    apply_terminal_sync, previously_synced_terminal_tools, terminal_targets_from,
};

use super::{api_err_to_string, TerminalSyncProfile, SUPPORTED_TERMINAL_TOOLS};
use crate::ai_gateway::selection::{resolve_model_for_protocol, ModelResolution};
use crate::ai_gateway::storage::read_config;
#[cfg(test)]
use crate::ai_gateway::templates::{apply_template_sync_with, ProviderTemplateView};
#[cfg(test)]
use crate::ai_gateway::ProviderTemplate;
#[cfg(test)]
use crate::ai_gateway::GatewayConfig;
use crate::ai_gateway::{
    ClaudeModelSelection, GatewayUpstreamProvider, TerminalSyncRecord, UpstreamProtocol,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A terminal service provider record that AI Gateway can configure or sync.
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
    /// Model-only Claude selection snapshot (the four local mapping ids) for the
    /// `claude` target; absent for the OpenAI tools. It never carries a
    /// credential value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_models: Option<ClaudeModelSelection>,
    /// Expected native context configuration for this target. It never carries a
    /// credential and never claims that a running CLI consumed the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<TerminalContextSummary>,
}

/// How many applicable public models or deduplicated Claude choices have a
/// known declared context window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalContextCoverage {
    pub total: usize,
    pub known: usize,
    pub unknown: usize,
}

/// Per-target expected native context configuration.
///
/// `mode` is `per_model`, `selected_model` or `shared_compaction`; `scope` is
/// `generated_models`, `activation` or `global_and_isolated_settings`. Reasons
/// are stable codes (`unknown_context`, `below_minimum`, `capped_at_1000000`,
/// `incomplete_selection`) that explain an unavailable or adjusted value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalContextSummary {
    pub mode: String,
    pub scope: String,
    pub coverage: TerminalContextCoverage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_context: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_context: Option<u32>,
    pub reasons: Vec<String>,
}
pub(in crate::ai_gateway) fn is_supported_terminal_tool(tool: &str) -> bool {
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
fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Whether any enabled gateway can serve `local_model` under `protocol`,
/// resolved through the same authoritative model resolution the request path
/// uses. Disabled gateways never count.
fn gateway_protocol_servable(
    gateways: &[GatewayUpstreamProvider],
    local_model: &str,
    protocol: UpstreamProtocol,
) -> bool {
    gateways.iter().filter(|gateway| gateway.enabled).any(|gateway| {
        matches!(
            resolve_model_for_protocol(gateway, Some(local_model), protocol),
            ModelResolution::Serve(_)
        )
    })
}
#[tauri::command]
pub fn ai_gateway_terminal_targets() -> Result<Vec<TerminalTarget>, String> {
    let config = read_config()?;
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    Ok(terminal_targets_from(
        &config,
        &payload.data,
        TerminalSyncProfile::current(),
    ))
}
/// Apply one template sync, then best-effort refresh every previously synced
/// terminal tool through the injected `terminal_sync` port.
///
/// The terminal refresh runs only after the template sync succeeded, only when
/// at least one upstream provider is bound to that template, and only when at
/// least one supported tool already holds a managed gateway record. It is
/// awaited at most once and its error is swallowed, so the template sync result
/// (and its `synced_at` update) always stands.
///
/// This is the injectable composition seam the template/terminal resync tests
/// drive; the production `ai_gateway_sync_provider_template` command splits the
/// same steps around the serialized configuration mutation so the write lock is
/// never held across an upstream wait.
#[cfg(test)]
pub(in crate::ai_gateway) async fn apply_template_sync_with_terminal_refresh<F, Fut>(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
    providers_data: &serde_json::Value,
    terminal_sync: F,
) -> Result<ProviderTemplateView, String>
where
    F: FnOnce(Vec<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Vec<TerminalSyncRecord>, String>> + Send,
{
    let view = apply_template_sync_with(config, template_id, fetch, persist)?;
    let bound = config
        .providers
        .iter()
        .any(|provider| provider.template_id.as_deref() == Some(template_id));
    if bound {
        let tools = previously_synced_terminal_tools(config, providers_data, TerminalSyncProfile::current());
        if !tools.is_empty() {
            let _ = terminal_sync(tools).await;
        }
    }
    Ok(view)
}
#[tauri::command]
pub async fn ai_gateway_configure_terminal(
    app: tauri::AppHandle,
    target_tools: Vec<String>,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    apply_terminal_sync(app, target_tools, claude_models).await
}

#[tauri::command]
pub async fn ai_gateway_sync_terminal(
    app: tauri::AppHandle,
    target_tools: Option<Vec<String>>,
    claude_models: Option<ClaudeModelSelection>,
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
    apply_terminal_sync(app, targets, claude_models).await
}
