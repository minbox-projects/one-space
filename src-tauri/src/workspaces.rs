use crate::app_store::{ApiErr, ApiMeta, ApiOk, SessionInput};
use crate::{ai_sessions, app_store, mcp_servers, skills, subagents};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const SCHEMA_VERSION: u32 = 1;
const SOURCE_MANUAL: &str = "manual";
const SOURCE_SESSION_AUTO: &str = "session_auto";
const SOURCE_COPY_TARGET: &str = "copy_target";
const SUPPORTED_MODELS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
static WORKSPACE_SYNC_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    pub root_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_activity_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkspaceMcpBinding {
    pub workspace_id: String,
    pub server_id: String,
    #[serde(default)]
    pub enabled_models: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct WorkspacesState {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceRecord>,
    #[serde(default)]
    pub mcp_bindings: Vec<WorkspaceMcpBinding>,
    #[serde(default)]
    pub deleted_roots: Vec<String>,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceView {
    #[serde(flatten)]
    pub workspace: WorkspaceRecord,
    #[serde(default)]
    pub session_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceDetail {
    pub workspace: WorkspaceView,
    #[serde(default)]
    pub mcp_bindings: Vec<WorkspaceMcpBinding>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceCreateInput {
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceUpdateMetaInput {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub root_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceMcpBindingInput {
    pub workspace_id: String,
    pub server_id: String,
    #[serde(default)]
    pub enabled_models: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceCopySkillRef {
    pub model: String,
    pub source_id: String,
    pub source_rel_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceCopySubagentRef {
    pub model: String,
    pub source_id: String,
    pub source_rel_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceCopyInput {
    pub source_workspace_id: String,
    pub target_root_path: String,
    #[serde(default)]
    pub selected_mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub selected_skills: Vec<WorkspaceCopySkillRef>,
    #[serde(default)]
    pub selected_subagents: Vec<WorkspaceCopySubagentRef>,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn api_ok<T: Serialize>(data: T, revision: u64) -> Result<ApiOk<T>, ApiErr> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision,
        },
    })
}

fn api_error(code: &str, message: impl Into<String>) -> ApiErr {
    ApiErr {
        ok: false,
        code: code.to_string(),
        message: message.into(),
        details: None,
    }
}

fn state_path() -> Result<PathBuf, String> {
    let root = crate::get_data_dir()?;
    let dir = root.join("data").join("workspaces");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.json"))
}

fn normalize_root_path(value: &str) -> String {
    ai_sessions::normalize_working_dir_for_terminal(value)
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn normalize_models(models: &[String]) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    for model in models {
        let normalized = model.trim().to_lowercase();
        if SUPPORTED_MODELS.contains(&normalized.as_str()) && seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

fn default_workspace_name(root_path: &str) -> String {
    PathBuf::from(root_path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| root_path.to_string())
}

fn load_state() -> Result<WorkspacesState, String> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(WorkspacesState::default());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(WorkspacesState::default());
    }
    serde_json::from_str::<WorkspacesState>(&content).map_err(|e| e.to_string())
}

pub(crate) fn workspace_roots() -> Result<Vec<String>, String> {
    let state = load_state()?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for workspace in state.workspaces {
        let path = PathBuf::from(workspace.root_path.trim());
        if !path.exists() || !path.is_dir() {
            continue;
        }
        let canonical = fs::canonicalize(&path).unwrap_or(path);
        let root = canonical.to_string_lossy().to_string();
        if seen.insert(root.clone()) {
            out.push(root);
        }
    }
    Ok(out)
}

fn save_state(mut state: WorkspacesState) -> Result<WorkspacesState, String> {
    state.revision = state.revision.saturating_add(1);
    let content = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    fs::write(state_path()?, content).map_err(|e| e.to_string())?;
    Ok(state)
}

fn workspace_matches_tags(record: &WorkspaceRecord, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let tags = record
        .tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .collect::<HashSet<_>>();
    filters
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .any(|tag| tags.contains(&tag))
}

fn sort_workspaces(workspaces: &mut [WorkspaceRecord]) {
    workspaces.sort_by(|a, b| {
        b.last_activity_at
            .cmp(&a.last_activity_at)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

fn find_workspace_index_by_root(state: &WorkspacesState, root_path: &str) -> Option<usize> {
    state
        .workspaces
        .iter()
        .position(|workspace| workspace.root_path == root_path)
}

fn find_workspace_index_by_id(state: &WorkspacesState, workspace_id: &str) -> Option<usize> {
    state
        .workspaces
        .iter()
        .position(|workspace| workspace.id == workspace_id)
}

fn ensure_workspace_with_root(
    state: &mut WorkspacesState,
    root_path: &str,
    source: &str,
    overwrite_metadata: Option<(String, Option<String>, Vec<String>)>,
) -> Result<(WorkspaceRecord, bool), String> {
    let normalized_root = normalize_root_path(root_path);
    if normalized_root.trim().is_empty() {
        return Err("workspace root path is required".to_string());
    }
    let now = now_ts();
    if let Some(idx) = find_workspace_index_by_root(state, &normalized_root) {
        let mut changed = false;
        if let Some((name, description, tags)) = overwrite_metadata {
            let next_name = name.trim().to_string();
            if !next_name.is_empty() && state.workspaces[idx].name != next_name {
                state.workspaces[idx].name = next_name;
                changed = true;
            }
            let next_description = description
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            if state.workspaces[idx].description != next_description {
                state.workspaces[idx].description = next_description;
                changed = true;
            }
            let next_tags = normalize_tags(&tags);
            if state.workspaces[idx].tags != next_tags {
                state.workspaces[idx].tags = next_tags;
                changed = true;
            }
        }
        if state.workspaces[idx].updated_at < now {
            state.workspaces[idx].updated_at = now;
            changed = true;
        }
        return Ok((state.workspaces[idx].clone(), changed));
    }

    state.deleted_roots.retain(|root| root != &normalized_root);
    let name = overwrite_metadata
        .as_ref()
        .map(|(value, _, _)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_workspace_name(&normalized_root));
    let description = overwrite_metadata
        .as_ref()
        .and_then(|(_, value, _)| value.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let tags = overwrite_metadata
        .as_ref()
        .map(|(_, _, tags)| normalize_tags(tags))
        .unwrap_or_default();
    let record = WorkspaceRecord {
        id: format!("workspace-{}", uuid::Uuid::new_v4()),
        name,
        root_path: normalized_root,
        description,
        tags,
        source: source.to_string(),
        created_at: now,
        updated_at: now,
        last_activity_at: now,
    };
    state.workspaces.push(record.clone());
    Ok((record, true))
}

fn sync_state_with_sessions(
    state: &mut WorkspacesState,
    sessions: &[app_store::SessionRecord],
) -> Result<bool, String> {
    let deleted_roots = state.deleted_roots.iter().cloned().collect::<HashSet<_>>();
    let mut latest_by_root = HashMap::<String, u64>::new();
    for session in sessions {
        let normalized_root = normalize_root_path(&session.working_dir);
        if normalized_root.trim().is_empty() || deleted_roots.contains(&normalized_root) {
            continue;
        }
        let ts = session.last_used_at.max(session.created_at);
        latest_by_root
            .entry(normalized_root)
            .and_modify(|current| *current = (*current).max(ts))
            .or_insert(ts);
    }

    let mut changed = false;
    for (root_path, last_activity_at) in latest_by_root {
        match find_workspace_index_by_root(state, &root_path) {
            Some(idx) => {
                if state.workspaces[idx].last_activity_at != last_activity_at {
                    state.workspaces[idx].last_activity_at = last_activity_at;
                    state.workspaces[idx].updated_at = now_ts();
                    changed = true;
                }
            }
            None => {
                let (mut record, created) =
                    ensure_workspace_with_root(state, &root_path, SOURCE_SESSION_AUTO, None)?;
                record.last_activity_at = last_activity_at;
                if let Some(idx) = find_workspace_index_by_id(state, &record.id) {
                    state.workspaces[idx].last_activity_at = last_activity_at;
                }
                changed = changed || created;
            }
        }
    }
    Ok(changed)
}

fn build_workspace_view_with_counts(
    record: &WorkspaceRecord,
    session_counts: &HashMap<String, usize>,
) -> WorkspaceView {
    WorkspaceView {
        workspace: record.clone(),
        session_count: *session_counts.get(&record.root_path).unwrap_or(&0),
    }
}

fn build_workspace_view(record: &WorkspaceRecord) -> Result<WorkspaceView, String> {
    let session_counts = app_store::workspace_session_counts_by_root()?;
    Ok(build_workspace_view_with_counts(record, &session_counts))
}

fn workspace_detail_from_state(
    state: &WorkspacesState,
    workspace_id: &str,
) -> Result<WorkspaceDetail, ApiErr> {
    let record = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;
    let mut bindings = state
        .mcp_bindings
        .iter()
        .filter(|binding| binding.workspace_id == workspace_id)
        .cloned()
        .collect::<Vec<_>>();
    bindings.sort_by(|a, b| a.server_id.cmp(&b.server_id));
    Ok(WorkspaceDetail {
        workspace: build_workspace_view(&record).map_err(|e| api_error("io_error", e))?,
        mcp_bindings: bindings,
    })
}

fn apply_workspace_mcp_for_workspace_record(
    state: &WorkspacesState,
    workspace: &WorkspaceRecord,
    target_model: Option<&str>,
) -> Result<(), String> {
    let all_servers = mcp_servers::get_mcp_servers()?;
    let server_map = all_servers
        .servers
        .into_iter()
        .map(|server| (server.id.clone(), server))
        .collect::<HashMap<_, _>>();

    let models = target_model
        .map(|model| vec![model.trim().to_lowercase()])
        .unwrap_or_else(|| {
            SUPPORTED_MODELS
                .iter()
                .map(|value| value.to_string())
                .collect()
        });

    for model in models {
        if !SUPPORTED_MODELS.contains(&model.as_str()) {
            continue;
        }
        let selected_servers = state
            .mcp_bindings
            .iter()
            .filter(|binding| binding.workspace_id == workspace.id)
            .filter(|binding| binding.enabled_models.iter().any(|item| item == &model))
            .filter_map(|binding| server_map.get(&binding.server_id).cloned())
            .collect::<Vec<_>>();
        mcp_servers::apply_project_workspace_servers(
            &workspace.root_path,
            &model,
            &selected_servers,
        )?;
    }
    Ok(())
}

fn sync_from_sessions_impl() -> Result<bool, String> {
    let sessions = app_store::sessions_snapshot_all()?;
    let mut state = load_state()?;
    if sync_state_with_sessions(&mut state, &sessions)? {
        let _ = save_state(state)?;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn sync_from_sessions() -> Result<(), String> {
    let _ = sync_from_sessions_impl()?;
    Ok(())
}

pub(crate) fn schedule_sync_from_sessions(app: tauri::AppHandle) {
    if WORKSPACE_SYNC_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking(sync_from_sessions_impl).await;
        WORKSPACE_SYNC_RUNNING.store(false, Ordering::SeqCst);
        match result {
            Ok(Ok(true)) => {
                let _ = app.emit("workspaces-updated", ());
                let _ = app.emit("refresh-counts", ());
            }
            Ok(Ok(false)) => {}
            Ok(Err(err)) => {
                log::warn!("workspace sync skipped due to error: {}", err);
            }
            Err(err) => {
                log::warn!("workspace sync worker join failed: {}", err);
            }
        }
    });
}

pub(crate) fn workspace_count_fast() -> Result<usize, String> {
    let state = load_state()?;
    Ok(state.workspaces.len())
}

pub(crate) fn apply_workspace_mcp_for_session(root_path: &str, model: &str) -> Result<(), String> {
    let normalized_root = normalize_root_path(root_path);
    if normalized_root.trim().is_empty() {
        return Ok(());
    }
    let state = load_state()?;
    let Some(workspace) = state
        .workspaces
        .iter()
        .find(|workspace| workspace.root_path == normalized_root)
    else {
        return Ok(());
    };
    apply_workspace_mcp_for_workspace_record(&state, workspace, Some(model))
}

fn workspaces_list_impl(
    tag_filters: Option<Vec<String>>,
) -> Result<ApiOk<Vec<WorkspaceView>>, ApiErr> {
    let state = load_state().map_err(|e| api_error("io_error", e))?;
    let sessions = app_store::sessions_snapshot_all().map_err(|e| api_error("io_error", e))?;
    let session_counts = app_store::workspace_session_counts_by_root_from_sessions(&sessions);
    let filters = normalize_tags(&tag_filters.unwrap_or_default());
    let mut records = state
        .workspaces
        .iter()
        .filter(|workspace| workspace_matches_tags(workspace, &filters))
        .cloned()
        .collect::<Vec<_>>();
    sort_workspaces(&mut records);
    let mut views = Vec::with_capacity(records.len());
    for record in records {
        views.push(build_workspace_view_with_counts(&record, &session_counts));
    }
    api_ok(views, state.revision)
}

#[tauri::command]
pub async fn workspaces_list(
    app: tauri::AppHandle,
    tag_filters: Option<Vec<String>>,
) -> Result<ApiOk<Vec<WorkspaceView>>, ApiErr> {
    schedule_sync_from_sessions(app);
    tauri::async_runtime::spawn_blocking(move || workspaces_list_impl(tag_filters))
        .await
        .map_err(|e| api_error("task_join_error", e.to_string()))?
}

fn workspace_get_impl(workspace_id: String) -> Result<ApiOk<WorkspaceDetail>, ApiErr> {
    let state = load_state().map_err(|e| api_error("io_error", e))?;
    let detail = workspace_detail_from_state(&state, &workspace_id)?;
    api_ok(detail, state.revision)
}

#[tauri::command]
pub async fn workspace_get(
    app: tauri::AppHandle,
    workspace_id: String,
) -> Result<ApiOk<WorkspaceDetail>, ApiErr> {
    schedule_sync_from_sessions(app);
    tauri::async_runtime::spawn_blocking(move || workspace_get_impl(workspace_id))
        .await
        .map_err(|e| api_error("task_join_error", e.to_string()))?
}

#[tauri::command]
pub fn workspace_create(input: WorkspaceCreateInput) -> Result<ApiOk<WorkspaceDetail>, ApiErr> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(api_error("invalid_payload", "workspace name is required"));
    }
    let normalized_root = normalize_root_path(&input.root_path);
    if normalized_root.trim().is_empty() {
        return Err(api_error(
            "invalid_payload",
            "workspace root path is required",
        ));
    }

    let mut state = load_state().map_err(|e| api_error("io_error", e))?;
    if find_workspace_index_by_root(&state, &normalized_root).is_some() {
        return Err(api_error(
            "already_exists",
            "workspace root path already exists",
        ));
    }

    let (_, created) = ensure_workspace_with_root(
        &mut state,
        &normalized_root,
        SOURCE_MANUAL,
        Some((name, input.description, input.tags)),
    )
    .map_err(|e| api_error("io_error", e))?;
    if !created {
        return Err(api_error(
            "already_exists",
            "workspace root path already exists",
        ));
    }
    let state = save_state(state).map_err(|e| api_error("io_error", e))?;
    let detail = workspace_detail_from_state(
        &state,
        &state
            .workspaces
            .last()
            .map(|item| item.id.clone())
            .unwrap_or_default(),
    )?;
    api_ok(detail, state.revision)
}

#[tauri::command]
pub fn workspace_update_meta(
    input: WorkspaceUpdateMetaInput,
) -> Result<ApiOk<WorkspaceDetail>, ApiErr> {
    let mut state = load_state().map_err(|e| api_error("io_error", e))?;
    let idx = find_workspace_index_by_id(&state, &input.id)
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;

    if let Some(root_path) = input.root_path.as_ref() {
        let normalized = normalize_root_path(root_path);
        if !normalized.is_empty() && normalized != state.workspaces[idx].root_path {
            return Err(api_error(
                "IMMUTABLE_FIELD",
                "workspace root path is immutable",
            ));
        }
    }

    let next_name = input.name.trim().to_string();
    if next_name.is_empty() {
        return Err(api_error("invalid_payload", "workspace name is required"));
    }

    let next_description = input
        .description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let next_tags = normalize_tags(&input.tags);
    let now = now_ts();
    state.workspaces[idx].name = next_name;
    state.workspaces[idx].description = next_description;
    state.workspaces[idx].tags = next_tags;
    state.workspaces[idx].updated_at = now;
    let workspace_id = state.workspaces[idx].id.clone();
    let state = save_state(state).map_err(|e| api_error("io_error", e))?;
    let detail = workspace_detail_from_state(&state, &workspace_id)?;
    api_ok(detail, state.revision)
}

#[tauri::command]
pub fn workspace_delete(workspace_id: String) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_state().map_err(|e| api_error("io_error", e))?;
    let idx = find_workspace_index_by_id(&state, &workspace_id)
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;
    let workspace = state.workspaces[idx].clone();
    apply_workspace_mcp_for_workspace_record(&state, &workspace, None)
        .and_then(|_| {
            for model in SUPPORTED_MODELS {
                mcp_servers::apply_project_workspace_servers(&workspace.root_path, model, &[])?;
            }
            Ok(())
        })
        .map_err(|e| api_error("mcp_apply_failed", e))?;

    if !state
        .deleted_roots
        .iter()
        .any(|root| root == &workspace.root_path)
    {
        state.deleted_roots.push(workspace.root_path.clone());
    }
    state.workspaces.retain(|item| item.id != workspace_id);
    state
        .mcp_bindings
        .retain(|binding| binding.workspace_id != workspace_id);
    let state = save_state(state).map_err(|e| api_error("io_error", e))?;
    api_ok(json!({ "deleted": true }), state.revision)
}

fn workspace_sessions_list_impl(
    workspace_id: String,
    tool: Option<String>,
    model_name: Option<String>,
    query: Option<String>,
) -> Result<ApiOk<app_store::WorkspaceSessionsQueryResult>, ApiErr> {
    let state = load_state().map_err(|e| api_error("io_error", e))?;
    let workspace = state
        .workspaces
        .iter()
        .find(|item| item.id == workspace_id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;
    let sessions = app_store::workspace_sessions_query_by_root(
        &workspace.root_path,
        tool.as_deref(),
        model_name.as_deref(),
        query.as_deref(),
    )
    .map_err(|e| api_error("io_error", e))?;
    api_ok(sessions, state.revision)
}

#[tauri::command]
pub async fn workspace_sessions_list(
    app: tauri::AppHandle,
    workspace_id: String,
    tool: Option<String>,
    model_name: Option<String>,
    query: Option<String>,
) -> Result<ApiOk<app_store::WorkspaceSessionsQueryResult>, ApiErr> {
    schedule_sync_from_sessions(app);
    tauri::async_runtime::spawn_blocking(move || {
        workspace_sessions_list_impl(workspace_id, tool, model_name, query)
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

#[tauri::command]
pub fn workspace_mcp_binding_upsert(
    input: WorkspaceMcpBindingInput,
) -> Result<ApiOk<WorkspaceDetail>, ApiErr> {
    let mut state = load_state().map_err(|e| api_error("io_error", e))?;
    let workspace = state
        .workspaces
        .iter()
        .find(|item| item.id == input.workspace_id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;

    let enabled_models = normalize_models(&input.enabled_models);
    let now = now_ts();
    match state.mcp_bindings.iter().position(|binding| {
        binding.workspace_id == input.workspace_id && binding.server_id == input.server_id
    }) {
        Some(idx) if enabled_models.is_empty() => {
            state.mcp_bindings.remove(idx);
        }
        Some(idx) => {
            state.mcp_bindings[idx].enabled_models = enabled_models;
            state.mcp_bindings[idx].updated_at = now;
        }
        None if !enabled_models.is_empty() => {
            state.mcp_bindings.push(WorkspaceMcpBinding {
                workspace_id: input.workspace_id.clone(),
                server_id: input.server_id.clone(),
                enabled_models,
                created_at: now,
                updated_at: now,
            });
        }
        None => {}
    }

    apply_workspace_mcp_for_workspace_record(&state, &workspace, None)
        .map_err(|e| api_error("mcp_apply_failed", e))?;
    let workspace_id = workspace.id.clone();
    let state = save_state(state).map_err(|e| api_error("io_error", e))?;
    let detail = workspace_detail_from_state(&state, &workspace_id)?;
    api_ok(detail, state.revision)
}

#[tauri::command]
pub async fn workspace_launch_session(
    app: tauri::AppHandle,
    workspace_id: String,
    tool: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_state().map_err(|e| api_error("io_error", e))?;
    let workspace = state
        .workspaces
        .iter()
        .find(|item| item.id == workspace_id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;
    crate::app_store::sessions_create(
        app,
        SessionInput {
            id: None,
            name: String::new(),
            working_dir: workspace.root_path,
            tool,
            tool_session_id: None,
            runtime_mode: None,
            runtime_profile_id: None,
            preset_id: None,
            status: Some("active".to_string()),
            provider_id: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn workspace_copy(
    app: tauri::AppHandle,
    input: WorkspaceCopyInput,
) -> Result<ApiOk<WorkspaceDetail>, ApiErr> {
    let source_state = load_state().map_err(|e| api_error("io_error", e))?;
    let source_workspace = source_state
        .workspaces
        .iter()
        .find(|item| item.id == input.source_workspace_id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "workspace not found"))?;
    let target_root = normalize_root_path(&input.target_root_path);
    if target_root.trim().is_empty() {
        return Err(api_error("invalid_payload", "target root path is required"));
    }
    if target_root == source_workspace.root_path {
        return Err(api_error(
            "invalid_payload",
            "target root path must differ from source",
        ));
    }

    let mut state = source_state.clone();
    let (target_workspace, _) = ensure_workspace_with_root(
        &mut state,
        &target_root,
        SOURCE_COPY_TARGET,
        Some((default_workspace_name(&target_root), None, Vec::new())),
    )
    .map_err(|e| api_error("io_error", e))?;

    let selected_mcp_ids = input
        .selected_mcp_server_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();

    let now = now_ts();
    for server_id in selected_mcp_ids {
        let Some(source_binding) = source_state
            .mcp_bindings
            .iter()
            .find(|binding| {
                binding.workspace_id == source_workspace.id && binding.server_id == server_id
            })
            .cloned()
        else {
            continue;
        };
        match state.mcp_bindings.iter().position(|binding| {
            binding.workspace_id == target_workspace.id && binding.server_id == server_id
        }) {
            Some(idx) => {
                state.mcp_bindings[idx].enabled_models = source_binding.enabled_models.clone();
                state.mcp_bindings[idx].updated_at = now;
            }
            None => state.mcp_bindings.push(WorkspaceMcpBinding {
                workspace_id: target_workspace.id.clone(),
                server_id: server_id.clone(),
                enabled_models: source_binding.enabled_models.clone(),
                created_at: now,
                updated_at: now,
            }),
        }
    }

    let state = save_state(state).map_err(|e| api_error("io_error", e))?;
    apply_workspace_mcp_for_workspace_record(&state, &target_workspace, None)
        .map_err(|e| api_error("mcp_apply_failed", e))?;

    for skill in input.selected_skills {
        let model = skill.model.trim().to_lowercase();
        if !SUPPORTED_MODELS.contains(&model.as_str()) {
            continue;
        }
        skills::skills_install(
            app.clone(),
            skills::InstallInput {
                source_id: skill.source_id,
                skill_ref: skill.source_rel_path,
                model,
                scope: Some("project".to_string()),
                project_root: Some(target_root.clone()),
            },
        )
        .await
        .map_err(|e| api_error("skills_copy_failed", e))?;
    }

    for subagent in input.selected_subagents {
        let model = subagent.model.trim().to_lowercase();
        if !SUPPORTED_MODELS.contains(&model.as_str()) {
            continue;
        }
        subagents::subagents_install(
            app.clone(),
            subagents::InstallInput {
                source_id: subagent.source_id,
                subagent_ref: subagent.source_rel_path,
                model,
                scope: Some("project".to_string()),
                project_root: Some(target_root.clone()),
            },
        )
        .await
        .map_err(|e| api_error("subagents_copy_failed", e))?;
    }
    let latest_state = load_state().map_err(|e| api_error("io_error", e))?;
    let detail = workspace_detail_from_state(&latest_state, &target_workspace.id)?;
    api_ok(detail, latest_state.revision)
}
