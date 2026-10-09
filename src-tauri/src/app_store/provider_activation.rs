use super::{
    api_error, api_ok, load_service_providers_state, lock_service_provider_operation,
    save_service_providers_internal, validate_service_provider_reference, ApiErr, ApiMeta, ApiOk,
};
use serde_json::{json, Value};

/// AppHandle-free canonical activation shared by GUI and CLI adapters. Projection and
/// synchronization remain the caller's responsibility.
pub fn activate_provider(tool: &str, provider_id: &str) -> Result<ApiOk<Value>, ApiErr> {
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    validate_service_provider_reference(tool, provider_id)
        .map_err(|e| api_error("invalid_payload", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    if tool == "opencode" {
        if !state.active_opencode.iter().any(|id| id == provider_id) {
            state.active_opencode.push(provider_id.to_string());
        }
    } else {
        state
            .active
            .insert(tool.to_string(), provider_id.to_string());
    }
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({ "tool": tool, "provider_id": provider_id }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

/// Resolve the public `env use` tool/name-or-ID arguments and activate through
/// the shared canonical writer, without projecting tool-side configuration.
pub fn use_cli_environment(tool: &str, target: &str) -> Result<String, String> {
    if tool.trim().is_empty() || target.trim().is_empty() {
        return Err("Usage: onespace env use <tool> <provider_name_or_id>".to_string());
    }
    let _operation = lock_service_provider_operation()?;
    let state =
        load_service_providers_state().map_err(|e| format!("Failed to load providers: {e}"))?;
    let provider_id = state
        .providers
        .iter()
        .find(|provider| {
            provider.tool == tool && (provider.id == target || provider.name == target)
        })
        .map(|provider| provider.id.clone())
        .ok_or_else(|| format!("Provider not found: {target}"))?;
    activate_provider(tool, &provider_id).map_err(|e| e.message)?;
    Ok(provider_id)
}

/// Render the public `env list` output from canonical provider state. OpenCode
/// uses only its multi-active set, including an empty set, never the legacy slot.
pub fn cli_environment_listing() -> Result<String, String> {
    let state =
        load_service_providers_state().map_err(|e| format!("Failed to load providers: {e}"))?;
    let mut lines = vec![
        "Available Environments (Providers):".to_string(),
        "----------------------------------".to_string(),
    ];
    for provider in &state.providers {
        lines.push(format!("{} -> {}", provider.tool, provider.name));
    }
    lines.push(String::new());
    lines.push("Current Active:".to_string());
    let active = state
        .active
        .iter()
        .filter(|(tool, _)| tool.as_str() != "opencode")
        .map(|(tool, id)| (tool.as_str(), id))
        .chain(state.active_opencode.iter().map(|id| ("opencode", id)));
    for (tool, provider_id) in active {
        let name = state
            .providers
            .iter()
            .find(|provider| provider.id == *provider_id)
            .map(|provider| provider.name.as_str())
            .unwrap_or(provider_id.as_str());
        lines.push(format!("{} -> {}", tool, name));
    }
    Ok(lines.join("\n"))
}
