use std::collections::HashMap;

use super::types::DeviceConfig;

pub(super) fn default_ai_model_permission_modes() -> HashMap<String, String> {
    HashMap::from([
        ("claude".to_string(), "default".to_string()),
        ("antigravity".to_string(), "default".to_string()),
        ("codex".to_string(), "default".to_string()),
        ("opencode".to_string(), "default".to_string()),
    ])
}

/// Normalize permission modes: ensure all four tools exist and values are 'default' or 'full_access'.
pub(super) fn normalize_ai_model_permission_modes(
    input: Option<HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut result = default_ai_model_permission_modes();
    if let Some(map) = input {
        for key in &["claude", "antigravity", "codex", "opencode"] {
            if let Some(value) = map.get(*key) {
                let normalized = if value == "full_access" {
                    "full_access".to_string()
                } else {
                    "default".to_string()
                };
                result.insert(key.to_string(), normalized);
            }
        }
    }
    result
}

pub(super) fn default_ai_model_launch_commands() -> HashMap<String, String> {
    HashMap::from([
        (
            "claude".to_string(),
            "claude --session-id {session_id}".to_string(),
        ),
        ("antigravity".to_string(), "agy".to_string()),
        ("codex".to_string(), "codex".to_string()),
        ("opencode".to_string(), "opencode".to_string()),
    ])
}

/// True when a launch command still invokes the retired `gemini` binary. The
/// terminal tool was renamed to Antigravity (`agy`), so a persisted `gemini ...`
/// command would launch the wrong program.
fn launch_command_targets_legacy_gemini(command: &str) -> bool {
    let Some(first) = command.split_whitespace().next() else {
        return false;
    };
    let executable = first.rsplit('/').next().unwrap_or(first);
    executable.eq_ignore_ascii_case("gemini")
}

fn is_usable_launch_command(tool: &str, command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    !(tool == "antigravity" && launch_command_targets_legacy_gemini(trimmed))
}

/// Normalize launch commands: ensure the four tools exist, drop empty entries,
/// and replace any Antigravity command that still targets the retired `gemini`
/// binary with the `agy` default.
pub(super) fn normalize_ai_model_launch_commands(
    input: Option<HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut result = input.unwrap_or_default();
    for (tool, default_command) in default_ai_model_launch_commands() {
        let usable = result
            .get(&tool)
            .map(|command| is_usable_launch_command(&tool, command))
            .unwrap_or(false);
        if !usable {
            result.insert(tool, default_command);
        }
    }
    result
}

/// Persist a normalized launch-command map when it changed. Returns whether the
/// device config was modified.
pub(super) fn normalize_device_launch_commands(device: &mut DeviceConfig) -> bool {
    let Some(current) = device.ai_model_launch_commands.clone() else {
        return false;
    };
    let normalized = normalize_ai_model_launch_commands(Some(current.clone()));
    if current == normalized {
        return false;
    }
    device.ai_model_launch_commands = Some(normalized);
    true
}

pub(super) fn normalize_ai_news_keywords(input: Option<String>) -> Option<String> {
    let raw = input.unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut normalized = trimmed.to_string();
    if normalized.chars().count() > 1000 {
        normalized = normalized.chars().take(1000).collect();
    }
    Some(normalized)
}
