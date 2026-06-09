use super::{
    filter_sessions_by_history_window, load_sessions_state, normalize_runtime_mode, SessionRecord,
};
use crate::ai_sessions;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LegacyProvidersView {
    pub(in crate::app_store) active_claude: Option<String>,
    pub(in crate::app_store) active_codex: Option<String>,
    pub(in crate::app_store) active_gemini: Option<String>,
    pub(in crate::app_store) active_opencode: Option<String>,
    pub(in crate::app_store) providers: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderImportPreviewItem {
    pub import_key: String,
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub conflict: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProvidersImportPreview {
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub conflicts: usize,
    #[serde(default)]
    pub items: Vec<ProviderImportPreviewItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderImportDecision {
    pub import_key: String,
    pub action: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncedDeviceProviderLite {
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncedDeviceProvidersView {
    pub device_id: String,
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub providers: Vec<SyncedDeviceProviderLite>,
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkspaceSessionsQueryResult {
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub tool_options: Vec<String>,
    #[serde(default)]
    pub model_options: Vec<String>,
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

pub(crate) fn session_to_legacy(record: &SessionRecord) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), json!(record.id));
    map.insert("name".into(), json!(record.name));
    map.insert("working_dir".into(), json!(record.working_dir));
    map.insert("model_type".into(), json!(record.tool));
    map.insert("model_name".into(), json!(record.model_name));
    map.insert("tool_session_id".into(), json!(record.tool_session_id));
    map.insert(
        "runtime_mode".into(),
        json!(normalize_runtime_mode(Some(&record.runtime_mode))),
    );
    map.insert(
        "runtime_profile_id".into(),
        json!(record.runtime_profile_id),
    );
    map.insert("preset_id".into(), json!(record.preset_id));
    map.insert("created_at".into(), json!(record.created_at));
    map.insert("last_used_at".into(), json!(record.last_used_at));
    map.insert("status".into(), json!(record.status));
    if let Some(ts) = record.favorited_at {
        map.insert("favorited_at".into(), json!(ts));
    }
    map.insert("provider_id".into(), json!(record.provider_id));
    Value::Object(map)
}

pub(crate) fn sessions_snapshot_all() -> Result<Vec<SessionRecord>, String> {
    let state = load_sessions_state()?;
    Ok(state.sessions)
}

pub(crate) fn workspace_session_counts_by_root_from_sessions(
    sessions: &[SessionRecord],
) -> HashMap<String, usize> {
    let mut counts = HashMap::<String, usize>::new();
    for session in filter_sessions_by_history_window(sessions.iter()) {
        let normalized_root = ai_sessions::normalize_working_dir_for_terminal(&session.working_dir);
        if normalized_root.trim().is_empty() {
            continue;
        }
        *counts.entry(normalized_root).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn workspace_session_counts_by_root() -> Result<HashMap<String, usize>, String> {
    let sessions = sessions_snapshot_all()?;
    Ok(workspace_session_counts_by_root_from_sessions(&sessions))
}

pub(in crate::app_store) fn workspace_session_matches_query(
    record: &SessionRecord,
    query: &str,
) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }

    let haystacks = [
        record.name.trim().to_lowercase(),
        record.tool_session_id.trim().to_lowercase(),
        record
            .model_name
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        record.working_dir.trim().to_lowercase(),
    ];

    haystacks.iter().any(|value| value.contains(&needle))
}

pub(crate) fn workspace_sessions_query_by_root(
    root_path: &str,
    tool: Option<&str>,
    model_name: Option<&str>,
    query: Option<&str>,
) -> Result<WorkspaceSessionsQueryResult, String> {
    let normalized_root = ai_sessions::normalize_working_dir_for_terminal(root_path);
    if normalized_root.trim().is_empty() {
        return Ok(WorkspaceSessionsQueryResult::default());
    }

    let normalized_tool = tool
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty() && value != "all");
    let normalized_model = model_name
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty() && value != "all");
    let normalized_query = query
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    let state = load_sessions_state()?;
    let workspace_sessions =
        filter_sessions_by_history_window(state.sessions.iter().filter(|session| {
            ai_sessions::normalize_working_dir_for_terminal(&session.working_dir) == normalized_root
        }));

    let total = workspace_sessions.len();

    let mut tool_options = workspace_sessions
        .iter()
        .map(|session| session.tool.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    tool_options.sort();
    tool_options.dedup();

    let mut model_options = workspace_sessions
        .iter()
        .filter(|session| {
            normalized_tool.as_ref().map_or(true, |tool_value| {
                session.tool.trim().eq_ignore_ascii_case(tool_value)
            })
        })
        .filter_map(|session| {
            session
                .model_name
                .as_deref()
                .map(|value| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    model_options.sort();
    model_options.dedup();

    let items = workspace_sessions
        .into_iter()
        .filter(|session| {
            normalized_tool.as_ref().map_or(true, |tool_value| {
                session.tool.trim().eq_ignore_ascii_case(tool_value)
            })
        })
        .filter(|session| {
            normalized_model.as_ref().map_or(true, |model_value| {
                session
                    .model_name
                    .as_deref()
                    .map(|value| value.trim().eq_ignore_ascii_case(model_value))
                    .unwrap_or(false)
            })
        })
        .filter(|session| {
            normalized_query.as_ref().map_or(true, |query_value| {
                workspace_session_matches_query(session, query_value)
            })
        })
        .map(|session| session_to_legacy(&session))
        .collect();

    Ok(WorkspaceSessionsQueryResult {
        items,
        total,
        tool_options,
        model_options,
    })
}
