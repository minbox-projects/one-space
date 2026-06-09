use super::{
    normalize_launch_scope, WorkflowPreset, WorkflowRun, DEP_MODE_SHARED_GLOBAL,
    DEP_MODE_STRICT_LOCAL, LAUNCH_SCOPE_SHARED, LAUNCH_SCOPE_STRICT, PROMPT_STATUS_APPLIED,
    PROMPT_STATUS_MANUAL,
};
use crate::{app_store, atomic_write_string, get_data_dir};
use std::fs;
use std::path::PathBuf;

pub(in crate::workflows) fn presets_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join("workflow_presets.json"))
}

pub(in crate::workflows) fn runs_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join("workflow_runs.json"))
}

pub(in crate::workflows) fn load_presets() -> Result<Vec<WorkflowPreset>, String> {
    let path = presets_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut presets =
        serde_json::from_str::<Vec<WorkflowPreset>>(&content).map_err(|e| e.to_string())?;
    for preset in &mut presets {
        preset.launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    }
    Ok(presets)
}

pub(in crate::workflows) fn save_presets(presets: &[WorkflowPreset]) -> Result<(), String> {
    let path = presets_path()?;
    let content = serde_json::to_string_pretty(presets).map_err(|e| e.to_string())?;
    atomic_write_string(&path, &content)
}

pub(in crate::workflows) fn load_runs() -> Result<Vec<WorkflowRun>, String> {
    let path = runs_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut runs = serde_json::from_str::<Vec<WorkflowRun>>(&content).map_err(|e| e.to_string())?;
    for run in &mut runs {
        run.launch_scope = normalize_launch_scope(Some(&run.launch_scope));
        if run.runtime_mode.trim().is_empty() {
            run.runtime_mode = if run.launch_scope == LAUNCH_SCOPE_STRICT {
                LAUNCH_SCOPE_STRICT.to_string()
            } else {
                LAUNCH_SCOPE_SHARED.to_string()
            };
        }
        if run.prompt_apply_status.trim().is_empty() {
            run.prompt_apply_status = if run
                .launch_prompt
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                PROMPT_STATUS_MANUAL.to_string()
            } else {
                PROMPT_STATUS_APPLIED.to_string()
            };
        }
        if run.dependency_apply_mode.trim().is_empty() {
            run.dependency_apply_mode = if run.launch_scope == LAUNCH_SCOPE_STRICT {
                DEP_MODE_STRICT_LOCAL.to_string()
            } else {
                DEP_MODE_SHARED_GLOBAL.to_string()
            };
        }
    }
    Ok(runs)
}

pub(in crate::workflows) fn save_runs(runs: &[WorkflowRun]) -> Result<(), String> {
    let path = runs_path()?;
    let content = serde_json::to_string_pretty(runs).map_err(|e| e.to_string())?;
    atomic_write_string(&path, &content)
}

pub(in crate::workflows) fn active_provider_id_for_tool(tool: &str) -> Option<String> {
    let resp = app_store::providers_list().ok()?;
    let view = serde_json::to_value(resp.data).ok()?;
    let key = format!("active_{}", tool);
    view.get(key.as_str())
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

#[derive(Debug, Clone)]
pub(in crate::workflows) struct ProviderLite {
    pub(in crate::workflows) id: String,
    pub(in crate::workflows) tool: String,
    pub(in crate::workflows) name: String,
    pub(in crate::workflows) env_managed: bool,
}

pub(in crate::workflows) fn providers_for_tool(tool: &str) -> Vec<ProviderLite> {
    let resp = match app_store::providers_list() {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let value = match serde_json::to_value(resp.data) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let list = value
        .get("providers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    list.into_iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(|v| v.as_str())?.trim().to_string();
            let item_tool = item
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if id.is_empty() || item_tool != tool {
                return None;
            }
            Some(ProviderLite {
                id,
                tool: item_tool,
                name: item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                env_managed: item
                    .get("env_managed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            })
        })
        .collect::<Vec<_>>()
}

pub(in crate::workflows) fn provider_by_id_for_tool(
    tool: &str,
    provider_id: &str,
) -> Option<ProviderLite> {
    providers_for_tool(tool)
        .into_iter()
        .find(|item| item.id == provider_id && item.tool == tool)
}

pub(in crate::workflows) fn active_provider_name_for_tool(tool: &str) -> Option<String> {
    let active_id = active_provider_id_for_tool(tool)?;
    provider_by_id_for_tool(tool, &active_id).map(|p| p.name)
}
