use crate::{app_store, get_data_dir, mcp_servers, runtime_profiles, skills};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 1;
const ALLOWED_TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
const LAUNCH_SCOPE_SHARED: &str = "shared";
const LAUNCH_SCOPE_STRICT: &str = "strict";
const PROMPT_STATUS_APPLIED: &str = "applied";
const PROMPT_STATUS_MANUAL: &str = "manual";
const DEP_MODE_SHARED_GLOBAL: &str = "shared-global";
const DEP_MODE_STRICT_LOCAL: &str = "strict-local";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub schema_version: u32,
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn api_ok<T: Serialize>(data: T) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: now_ts(),
        },
    })
}

fn normalize_tool(tool: &str) -> String {
    let t = tool.trim().to_lowercase();
    if ALLOWED_TOOLS.contains(&t.as_str()) {
        t
    } else {
        "claude".to_string()
    }
}

fn dedup_non_empty(items: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_string();
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}

fn normalize_launch_scope(scope: Option<&str>) -> String {
    let value = scope.unwrap_or("").trim().to_lowercase();
    if value == LAUNCH_SCOPE_STRICT {
        LAUNCH_SCOPE_STRICT.to_string()
    } else {
        LAUNCH_SCOPE_SHARED.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowPreset {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub working_dir: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub required_skill_ids: Vec<String>,
    #[serde(default)]
    pub launch_prompt: Option<String>,
    #[serde(default = "default_launch_scope")]
    pub launch_scope: String,
    pub created_at: u64,
    pub updated_at: u64,
}

fn default_launch_scope() -> String {
    LAUNCH_SCOPE_SHARED.to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowPresetInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub required_skill_ids: Vec<String>,
    #[serde(default)]
    pub launch_prompt: Option<String>,
    #[serde(default)]
    pub launch_scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRun {
    pub id: String,
    pub preset_id: String,
    pub preset_name: String,
    pub tool: String,
    pub working_dir: String,
    #[serde(default)]
    pub launch_prompt: Option<String>,
    #[serde(default)]
    pub launch_scope: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tool_session_id: Option<String>,
    #[serde(default)]
    pub runtime_mode: String,
    #[serde(default)]
    pub runtime_profile_id: Option<String>,
    #[serde(default)]
    pub prompt_apply_status: String,
    #[serde(default)]
    pub dependency_apply_mode: String,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    pub started_at: u64,
    #[serde(default)]
    pub ended_at: Option<u64>,
    #[serde(default)]
    pub replay_of_run_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRunUpdateInput {
    pub run_id: String,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRunDeleteInput {
    pub run_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowLaunchInput {
    pub preset_id: String,
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub override_working_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowReplayInput {
    pub run_id: String,
    #[serde(default)]
    pub session_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRunListInput {
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkflowDependencyState {
    #[serde(default)]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub active_provider_name: Option<String>,
    #[serde(default)]
    pub missing_mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub missing_mcp_names: Vec<String>,
    #[serde(default)]
    pub inactive_mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub inactive_mcp_names: Vec<String>,
    #[serde(default)]
    pub missing_skill_ids: Vec<String>,
    #[serde(default)]
    pub missing_skill_names: Vec<String>,
    #[serde(default)]
    pub installable_skill_ids: Vec<String>,
    #[serde(default)]
    pub unresolved_skill_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowDependencyApplyResult {
    pub preset_id: String,
    pub linked_mcp_count: usize,
    pub enabled_mcp_switch_count: usize,
    pub installed_skill_count: usize,
    pub failed_skill_installs: Vec<String>,
    pub dependencies_after: WorkflowDependencyState,
}

fn presets_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join("workflow_presets.json"))
}

fn runs_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join("workflow_runs.json"))
}

fn load_presets() -> Result<Vec<WorkflowPreset>, String> {
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

fn save_presets(presets: &[WorkflowPreset]) -> Result<(), String> {
    let path = presets_path()?;
    let content = serde_json::to_string_pretty(presets).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn load_runs() -> Result<Vec<WorkflowRun>, String> {
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

fn save_runs(runs: &[WorkflowRun]) -> Result<(), String> {
    let path = runs_path()?;
    let content = serde_json::to_string_pretty(runs).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn active_provider_id_for_tool(tool: &str) -> Option<String> {
    let resp = app_store::providers_list().ok()?;
    let view = serde_json::to_value(resp.data).ok()?;
    let key = format!("active_{}", tool);
    view.get(key.as_str())
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

#[derive(Debug, Clone)]
struct ProviderLite {
    id: String,
    tool: String,
    name: String,
    env_managed: bool,
}

fn providers_for_tool(tool: &str) -> Vec<ProviderLite> {
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

fn provider_by_id_for_tool(tool: &str, provider_id: &str) -> Option<ProviderLite> {
    providers_for_tool(tool)
        .into_iter()
        .find(|item| item.id == provider_id && item.tool == tool)
}

fn active_provider_name_for_tool(tool: &str) -> Option<String> {
    let active_id = active_provider_id_for_tool(tool)?;
    provider_by_id_for_tool(tool, &active_id).map(|p| p.name)
}

#[derive(Debug, Clone, Default)]
struct SkillIndexes {
    installed_ids: HashSet<String>,
    installed_source_rel: HashSet<(String, String)>,
    catalog_by_source_ref: HashMap<(String, String), skills::CatalogSkill>,
    catalog_by_id: HashMap<String, skills::CatalogSkill>,
    catalog_by_rel_path: HashMap<String, skills::CatalogSkill>,
    repo_by_key: HashMap<String, skills::RepositorySkillView>,
    repo_by_skill_id: HashMap<String, skills::RepositorySkillView>,
    repo_by_source_rel: HashMap<(String, String), skills::RepositorySkillView>,
}

#[derive(Debug, Clone)]
enum ResolvedSkillTarget {
    Catalog {
        source_id: String,
        skill_ref: String,
        skill_id: String,
        source_rel_path: String,
        skill_name: String,
    },
    Repo {
        repo_key: String,
        skill_id: String,
        source_id: String,
        source_rel_path: String,
        skill_name: String,
    },
}

fn make_repo_key(source_id: &str, source_rel_path: &str) -> String {
    format!("{}::{}", source_id, source_rel_path)
}

fn parse_catalog_selector(input: &str) -> Option<(String, String)> {
    let prefix = "catalog::";
    let value = input.trim();
    if !value.starts_with(prefix) {
        return None;
    }
    let payload = &value[prefix.len()..];
    let mut parts = payload.splitn(2, "::");
    let source_id = parts.next()?.trim();
    let skill_ref = parts.next()?.trim();
    if source_id.is_empty() || skill_ref.is_empty() {
        return None;
    }
    Some((source_id.to_string(), skill_ref.to_string()))
}

fn parse_repo_selector(input: &str) -> Option<String> {
    let prefix = "repo::";
    let value = input.trim();
    if !value.starts_with(prefix) {
        return None;
    }
    let payload = value[prefix.len()..].trim();
    if payload.is_empty() {
        return None;
    }
    Some(payload.to_string())
}

fn parse_legacy_skill_ref(input: &str) -> Option<(String, String)> {
    let value = input.trim();
    if value.starts_with("catalog::") || value.starts_with("repo::") {
        return None;
    }
    let mut parts = value.splitn(2, "::");
    let source = parts.next()?.trim();
    let skill_ref = parts.next()?.trim();
    if source.is_empty() || skill_ref.is_empty() {
        return None;
    }
    Some((source.to_string(), skill_ref.to_string()))
}

fn repo_installed_for_tool(repo: &skills::RepositorySkillView, tool: &str) -> bool {
    match tool {
        "claude" => repo.installed.claude,
        "codex" => repo.installed.codex,
        "gemini" => repo.installed.gemini,
        "opencode" => repo.installed.opencode,
        _ => false,
    }
}

fn canonicalize_working_dir(working_dir: &str) -> Option<String> {
    let raw = working_dir.trim();
    if raw.is_empty() {
        return None;
    }
    fs::canonicalize(PathBuf::from(raw))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| Some(raw.to_string()))
}

fn install_scope_and_project_root(
    launch_scope: &str,
    working_dir: &str,
) -> (String, Option<String>) {
    if launch_scope == LAUNCH_SCOPE_STRICT {
        ("project".to_string(), canonicalize_working_dir(working_dir))
    } else {
        ("global".to_string(), None)
    }
}

fn build_skill_indexes(tool: &str, scope: &str, project_root: Option<&str>) -> SkillIndexes {
    let installed_records = skills::skills_list_installed(
        None,
        Some(scope.to_string()),
        project_root.map(|v| v.to_string()),
    )
    .map(|resp| {
        resp.data
            .into_iter()
            .filter(|record| record.model == tool)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let catalog = skills::skills_list_catalog(Some(tool.to_string()))
        .map(|resp| resp.data)
        .unwrap_or_default();
    let repo_list = skills::skills_repo_list(
        Some(false),
        Some(scope.to_string()),
        project_root.map(|v| v.to_string()),
    )
    .map(|resp| resp.data)
    .unwrap_or_default();

    let mut indexes = SkillIndexes::default();

    for record in installed_records {
        indexes.installed_ids.insert(record.id.clone());
        indexes
            .installed_source_rel
            .insert((record.source_id.clone(), record.source_rel_path.clone()));
    }

    for item in catalog {
        indexes.catalog_by_source_ref.insert(
            (item.source_id.clone(), item.rel_path.clone()),
            item.clone(),
        );
        indexes
            .catalog_by_source_ref
            .insert((item.source_id.clone(), item.id.clone()), item.clone());
        indexes.catalog_by_id.insert(item.id.clone(), item.clone());
        indexes
            .catalog_by_rel_path
            .insert(item.rel_path.clone(), item.clone());
    }

    for repo in repo_list {
        indexes
            .repo_by_source_rel
            .entry((repo.source_id.clone(), repo.source_rel_path.clone()))
            .or_insert_with(|| repo.clone());
        indexes
            .repo_by_skill_id
            .entry(repo.skill_id.clone())
            .or_insert_with(|| repo.clone());
        indexes.repo_by_key.insert(repo.repo_key.clone(), repo);
    }

    indexes
}

fn resolve_catalog_target(
    source_id: &str,
    skill_ref: &str,
    indexes: &SkillIndexes,
) -> Option<ResolvedSkillTarget> {
    let item = indexes
        .catalog_by_source_ref
        .get(&(source_id.to_string(), skill_ref.to_string()))?;
    Some(ResolvedSkillTarget::Catalog {
        source_id: item.source_id.clone(),
        skill_ref: item.rel_path.clone(),
        skill_id: item.id.clone(),
        source_rel_path: item.rel_path.clone(),
        skill_name: item.name.clone(),
    })
}

fn resolve_skill_target(raw: &str, indexes: &SkillIndexes) -> Option<ResolvedSkillTarget> {
    if let Some(repo_key) = parse_repo_selector(raw) {
        let repo = indexes.repo_by_key.get(&repo_key)?;
        return Some(ResolvedSkillTarget::Repo {
            repo_key: repo.repo_key.clone(),
            skill_id: repo.skill_id.clone(),
            source_id: repo.source_id.clone(),
            source_rel_path: repo.source_rel_path.clone(),
            skill_name: repo.name.clone(),
        });
    }

    if let Some((source_id, skill_ref)) = parse_catalog_selector(raw) {
        return resolve_catalog_target(&source_id, &skill_ref, indexes);
    }

    if let Some((source_id, skill_ref)) = parse_legacy_skill_ref(raw) {
        if let Some(catalog_target) = resolve_catalog_target(&source_id, &skill_ref, indexes) {
            return Some(catalog_target);
        }
        if let Some(repo) = indexes
            .repo_by_source_rel
            .get(&(source_id.clone(), skill_ref.clone()))
        {
            return Some(ResolvedSkillTarget::Repo {
                repo_key: repo.repo_key.clone(),
                skill_id: repo.skill_id.clone(),
                source_id: repo.source_id.clone(),
                source_rel_path: repo.source_rel_path.clone(),
                skill_name: repo.name.clone(),
            });
        }
    }

    if let Some(item) = indexes.catalog_by_id.get(raw) {
        return Some(ResolvedSkillTarget::Catalog {
            source_id: item.source_id.clone(),
            skill_ref: item.rel_path.clone(),
            skill_id: item.id.clone(),
            source_rel_path: item.rel_path.clone(),
            skill_name: item.name.clone(),
        });
    }
    if let Some(item) = indexes.catalog_by_rel_path.get(raw) {
        return Some(ResolvedSkillTarget::Catalog {
            source_id: item.source_id.clone(),
            skill_ref: item.rel_path.clone(),
            skill_id: item.id.clone(),
            source_rel_path: item.rel_path.clone(),
            skill_name: item.name.clone(),
        });
    }
    if let Some(repo) = indexes.repo_by_skill_id.get(raw) {
        return Some(ResolvedSkillTarget::Repo {
            repo_key: repo.repo_key.clone(),
            skill_id: repo.skill_id.clone(),
            source_id: repo.source_id.clone(),
            source_rel_path: repo.source_rel_path.clone(),
            skill_name: repo.name.clone(),
        });
    }

    None
}

fn target_installed(
    target: &ResolvedSkillTarget,
    installed_ids: &HashSet<String>,
    installed_source_rel: &HashSet<(String, String)>,
    repo_installed_by_key: &HashMap<String, bool>,
) -> bool {
    match target {
        ResolvedSkillTarget::Catalog {
            source_id,
            source_rel_path,
            skill_id,
            ..
        } => {
            installed_ids.contains(skill_id)
                || installed_source_rel.contains(&(source_id.clone(), source_rel_path.clone()))
        }
        ResolvedSkillTarget::Repo {
            repo_key,
            source_id,
            source_rel_path,
            skill_id,
            ..
        } => {
            repo_installed_by_key
                .get(repo_key)
                .copied()
                .unwrap_or(false)
                || installed_ids.contains(skill_id)
                || installed_source_rel.contains(&(source_id.clone(), source_rel_path.clone()))
        }
    }
}

fn detect_dependencies_for_working_dir(
    preset: &WorkflowPreset,
    working_dir_override: Option<&str>,
) -> Result<WorkflowDependencyState, String> {
    let tool = normalize_tool(&preset.tool);
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let effective_working_dir = working_dir_override.unwrap_or(&preset.working_dir);
    let (install_scope, install_project_root) =
        install_scope_and_project_root(&launch_scope, effective_working_dir);
    let active_provider_id = preset
        .provider_id
        .clone()
        .or_else(|| active_provider_id_for_tool(&tool));
    let active_provider_name = if let Some(provider_id) = active_provider_id.as_ref() {
        provider_by_id_for_tool(&tool, provider_id).map(|p| p.name)
    } else {
        active_provider_name_for_tool(&tool)
    };

    let mcp_state = mcp_servers::get_mcp_servers()?;
    let mut mcp_by_id = HashMap::new();
    for server in mcp_state.servers {
        mcp_by_id.insert(server.id.clone(), server);
    }

    let mut missing_mcp_server_ids = Vec::new();
    let mut missing_mcp_names = Vec::new();
    let mut inactive_mcp_server_ids = Vec::new();
    let mut inactive_mcp_names = Vec::new();
    for id in &preset.mcp_server_ids {
        match mcp_by_id.get(id) {
            None => {
                missing_mcp_server_ids.push(id.clone());
                missing_mcp_names.push(id.clone());
            }
            Some(server) => {
                if launch_scope == LAUNCH_SCOPE_SHARED {
                    if let Some(provider_id) = active_provider_id.as_ref() {
                        if !server.linked_provider_ids.iter().any(|p| p == provider_id) {
                            inactive_mcp_server_ids.push(id.clone());
                            inactive_mcp_names.push(server.name.clone());
                        }
                    }
                }
            }
        }
    }

    let indexes = build_skill_indexes(&tool, &install_scope, install_project_root.as_deref());
    let repo_installed_by_key = indexes
        .repo_by_key
        .iter()
        .map(|(repo_key, repo)| (repo_key.clone(), repo_installed_for_tool(repo, &tool)))
        .collect::<HashMap<_, _>>();

    let mut missing_skill_ids = Vec::new();
    let mut missing_skill_names = Vec::new();
    let mut installable_skill_ids = Vec::new();
    let mut unresolved_skill_ids = Vec::new();
    for skill_id in &preset.required_skill_ids {
        let resolved = resolve_skill_target(skill_id, &indexes);
        let installed = if let Some(target) = resolved.as_ref() {
            target_installed(
                target,
                &indexes.installed_ids,
                &indexes.installed_source_rel,
                &repo_installed_by_key,
            )
        } else {
            indexes.installed_ids.contains(skill_id)
        };
        if installed {
            continue;
        }
        missing_skill_ids.push(skill_id.clone());
        missing_skill_names.push(match resolved.as_ref() {
            Some(ResolvedSkillTarget::Catalog { skill_name, .. }) => skill_name.clone(),
            Some(ResolvedSkillTarget::Repo { skill_name, .. }) => skill_name.clone(),
            None => skill_id.clone(),
        });
        if resolved.is_some() {
            installable_skill_ids.push(skill_id.clone());
        } else {
            unresolved_skill_ids.push(skill_id.clone());
        }
    }

    Ok(WorkflowDependencyState {
        active_provider_id,
        active_provider_name,
        missing_mcp_server_ids,
        missing_mcp_names,
        inactive_mcp_server_ids,
        inactive_mcp_names,
        missing_skill_ids,
        missing_skill_names,
        installable_skill_ids,
        unresolved_skill_ids,
    })
}

fn detect_dependencies(preset: &WorkflowPreset) -> Result<WorkflowDependencyState, String> {
    detect_dependencies_for_working_dir(preset, None)
}

fn allowed_run_status(status: &str) -> bool {
    matches!(status, "running" | "success" | "failed" | "interrupted")
}

fn make_run_for_launch(
    preset: &WorkflowPreset,
    working_dir: String,
    session_id: Option<String>,
    tool_session_id: Option<String>,
    runtime_mode: String,
    runtime_profile_id: Option<String>,
    prompt_apply_status: String,
    dependency_apply_mode: String,
    status: &str,
    error_message: Option<String>,
    replay_of_run_id: Option<String>,
) -> WorkflowRun {
    let started_at = now_ts();
    let ended_at = if status == "running" {
        None
    } else {
        Some(started_at)
    };
    WorkflowRun {
        id: uuid::Uuid::new_v4().to_string(),
        preset_id: preset.id.clone(),
        preset_name: preset.name.clone(),
        tool: normalize_tool(&preset.tool),
        working_dir,
        launch_prompt: preset.launch_prompt.clone(),
        launch_scope: normalize_launch_scope(Some(&preset.launch_scope)),
        session_id,
        tool_session_id,
        runtime_mode,
        runtime_profile_id,
        prompt_apply_status,
        dependency_apply_mode,
        status: status.to_string(),
        summary: None,
        error_message,
        started_at,
        ended_at,
        replay_of_run_id,
    }
}

async fn create_session_for_preset(
    app: tauri::AppHandle,
    preset: &WorkflowPreset,
    _session_name: Option<String>,
    override_working_dir: Option<String>,
    runtime_mode: String,
    runtime_profile_id: Option<String>,
) -> Result<(Value, String, Option<String>), String> {
    let working_dir = override_working_dir
        .unwrap_or_else(|| preset.working_dir.clone())
        .trim()
        .to_string();
    let normalized_working_dir = if working_dir.is_empty() {
        "./".to_string()
    } else {
        working_dir
    };
    let tool = normalize_tool(&preset.tool);
    let resp = app_store::sessions_create(
        app,
        app_store::SessionInput {
            id: None,
            name: String::new(),
            working_dir: normalized_working_dir.clone(),
            tool: tool.clone(),
            tool_session_id: None,
            runtime_mode: Some(runtime_mode),
            runtime_profile_id,
            preset_id: Some(preset.id.clone()),
            status: Some("active".to_string()),
            provider_id: None,
        },
    )
    .await
    .map_err(|e| e.message)?;

    let resolved_tool_session_id = resp
        .data
        .get("tool_session_id")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    Ok((resp.data, normalized_working_dir, resolved_tool_session_id))
}

fn prompt_apply_status_for_preset(preset: &WorkflowPreset) -> String {
    if preset
        .launch_prompt
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        PROMPT_STATUS_MANUAL.to_string()
    } else {
        PROMPT_STATUS_APPLIED.to_string()
    }
}

fn strict_provider_for_preset(preset: &WorkflowPreset) -> Option<ProviderLite> {
    let tool = normalize_tool(&preset.tool);
    let provider_id = preset
        .provider_id
        .clone()
        .or_else(|| active_provider_id_for_tool(&tool))?;
    provider_by_id_for_tool(&tool, &provider_id)
}

fn ensure_strict_provider_env_managed(preset: &WorkflowPreset) -> Result<(), String> {
    if normalize_launch_scope(Some(&preset.launch_scope)) != LAUNCH_SCOPE_STRICT {
        return Ok(());
    }
    let Some(provider) = strict_provider_for_preset(preset) else {
        return Ok(());
    };
    if !provider.env_managed {
        return Err(format!(
            "strict workflow launch requires env-managed provider: {}",
            provider.name
        ));
    }
    Ok(())
}

fn selected_mcp_servers_for_preset(
    preset: &WorkflowPreset,
) -> Result<Vec<mcp_servers::MCPServer>, String> {
    let mcp_state = mcp_servers::get_mcp_servers()?;
    let mut by_id = HashMap::new();
    for server in mcp_state.servers {
        by_id.insert(server.id.clone(), server);
    }

    let mut out = Vec::new();
    let mut missing = Vec::new();
    for server_id in &preset.mcp_server_ids {
        if let Some(server) = by_id.get(server_id) {
            out.push(server.clone());
        } else {
            missing.push(server_id.clone());
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "workflow required MCP servers are missing: {}",
            missing.join(", ")
        ));
    }
    Ok(out)
}

fn installed_skill_records_for_tool(
    tool: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Vec<skills::SkillRecord> {
    skills::skills_list_installed(
        Some(tool.to_string()),
        Some(scope.to_string()),
        project_root.map(|v| v.to_string()),
    )
    .map(|resp| resp.data)
    .unwrap_or_default()
}

fn resolve_skill_dir_names_for_preset(
    preset: &WorkflowPreset,
    working_dir: &str,
) -> Result<Vec<String>, String> {
    let tool = normalize_tool(&preset.tool);
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let (install_scope, install_project_root) =
        install_scope_and_project_root(&launch_scope, working_dir);
    let indexes = build_skill_indexes(&tool, &install_scope, install_project_root.as_deref());
    let installed =
        installed_skill_records_for_tool(&tool, &install_scope, install_project_root.as_deref());
    let mut by_id: HashMap<String, String> = HashMap::new();
    let mut by_source_rel: HashMap<(String, String), String> = HashMap::new();
    for item in installed {
        by_id.insert(item.id.clone(), item.dir_name.clone());
        by_source_rel.insert(
            (item.source_id.clone(), item.source_rel_path.clone()),
            item.dir_name,
        );
    }

    let mut out = Vec::new();
    let mut missing = Vec::new();
    for raw in &preset.required_skill_ids {
        let Some(target) = resolve_skill_target(raw, &indexes) else {
            missing.push(raw.clone());
            continue;
        };
        let dir_name = match &target {
            ResolvedSkillTarget::Catalog {
                source_id,
                source_rel_path,
                skill_id,
                ..
            } => by_id.get(skill_id).cloned().or_else(|| {
                by_source_rel
                    .get(&(source_id.clone(), source_rel_path.clone()))
                    .cloned()
            }),
            ResolvedSkillTarget::Repo {
                source_id,
                source_rel_path,
                skill_id,
                ..
            } => by_id.get(skill_id).cloned().or_else(|| {
                by_source_rel
                    .get(&(source_id.clone(), source_rel_path.clone()))
                    .cloned()
            }),
        };
        if let Some(name) = dir_name {
            if !name.trim().is_empty() && !out.contains(&name) {
                out.push(name);
            }
        } else {
            missing.push(raw.clone());
        }
    }

    if !missing.is_empty() {
        return Err(format!(
            "strict workflow required skills are not mirrored yet: {}",
            missing.join(", ")
        ));
    }
    Ok(out)
}

fn protected_runtime_profile_ids_from_sessions() -> HashSet<String> {
    let mut protected = HashSet::new();
    if let Ok(resp) = app_store::sessions_list() {
        for item in resp.data {
            if let Some(profile_id) = item.get("runtime_profile_id").and_then(|v| v.as_str()) {
                let trimmed = profile_id.trim();
                if !trimmed.is_empty() {
                    protected.insert(trimmed.to_string());
                }
            }
        }
    }
    protected
}

fn protected_runtime_profile_ids_from_runs(runs: &[WorkflowRun]) -> HashSet<String> {
    let mut protected = HashSet::new();
    for run in runs {
        if run.status != "running" {
            continue;
        }
        if let Some(profile_id) = run.runtime_profile_id.as_ref() {
            let trimmed = profile_id.trim();
            if !trimmed.is_empty() {
                protected.insert(trimmed.to_string());
            }
        }
    }
    protected
}

fn cleanup_runtime_profiles() -> Result<Vec<String>, String> {
    let runs = load_runs().unwrap_or_default();
    let mut protected = protected_runtime_profile_ids_from_sessions();
    for profile_id in protected_runtime_profile_ids_from_runs(&runs) {
        protected.insert(profile_id);
    }
    runtime_profiles::cleanup_stale_runtime_profiles(
        &protected,
        runtime_profiles::DEFAULT_PROFILE_TTL_SECS,
    )
}

pub fn workflows_cleanup_runtime_profiles_on_startup() -> Result<(), String> {
    let _ = cleanup_runtime_profiles()?;
    Ok(())
}

async fn apply_dependencies_for_preset(
    app: tauri::AppHandle,
    preset: &WorkflowPreset,
    launch_scope: &str,
    working_dir: &str,
) -> Result<WorkflowDependencyApplyResult, String> {
    let tool = normalize_tool(&preset.tool);
    let (install_scope, install_project_root) =
        install_scope_and_project_root(launch_scope, working_dir);
    let provider_id = preset
        .provider_id
        .clone()
        .or_else(|| active_provider_id_for_tool(&tool));
    let mut linked_mcp_count = 0usize;
    let mut enabled_mcp_switch_count = 0usize;
    let mut installed_skill_count = 0usize;
    let mut failed_skill_installs: Vec<String> = Vec::new();

    let mcp_state = mcp_servers::get_mcp_servers()?;
    let mut mcp_by_id = HashMap::new();
    for server in mcp_state.servers {
        mcp_by_id.insert(server.id.clone(), server);
    }

    if launch_scope == LAUNCH_SCOPE_SHARED {
        if let Some(provider) = provider_id {
            for server_id in &preset.mcp_server_ids {
                if let Some(server) = mcp_by_id.get(server_id) {
                    if !server.linked_provider_ids.iter().any(|id| id == &provider) {
                        let mut next_links = server.linked_provider_ids.clone();
                        next_links.push(provider.clone());
                        next_links = dedup_non_empty(&next_links);
                        mcp_servers::link_mcp_to_providers(
                            app.clone(),
                            server_id.clone(),
                            next_links,
                        )?;
                        linked_mcp_count += 1;
                    }
                    if mcp_servers::set_mcp_model_switch(server_id.clone(), tool.clone(), true)
                        .is_ok()
                    {
                        enabled_mcp_switch_count += 1;
                    }
                }
            }
        }
    }

    let indexes = build_skill_indexes(&tool, &install_scope, install_project_root.as_deref());
    let mut installed_ids = indexes.installed_ids.clone();
    let mut installed_source_rel = indexes.installed_source_rel.clone();
    let mut repo_installed_by_key = indexes
        .repo_by_key
        .iter()
        .map(|(repo_key, repo)| (repo_key.clone(), repo_installed_for_tool(repo, &tool)))
        .collect::<HashMap<_, _>>();

    for skill_id in &preset.required_skill_ids {
        let Some(target) = resolve_skill_target(skill_id, &indexes) else {
            failed_skill_installs.push(format!("{} (unresolved)", skill_id));
            continue;
        };
        if target_installed(
            &target,
            &installed_ids,
            &installed_source_rel,
            &repo_installed_by_key,
        ) {
            continue;
        }

        let install_result: Result<(), String> = match target {
            ResolvedSkillTarget::Catalog {
                source_id,
                skill_ref,
                ..
            } => match skills::skills_install(
                app.clone(),
                skills::InstallInput {
                    source_id,
                    skill_ref,
                    model: tool.clone(),
                    scope: Some(install_scope.clone()),
                    project_root: install_project_root.clone(),
                },
            )
            .await
            {
                Ok(result) => {
                    installed_ids.insert(result.data.id.clone());
                    installed_source_rel.insert((
                        result.data.source_id.clone(),
                        result.data.source_rel_path.clone(),
                    ));
                    repo_installed_by_key.insert(
                        make_repo_key(&result.data.source_id, &result.data.source_rel_path),
                        true,
                    );
                    Ok(())
                }
                Err(err) => Err(err),
            },
            ResolvedSkillTarget::Repo {
                repo_key,
                source_id,
                source_rel_path,
                ..
            } => match skills::skills_repo_set_model(
                app.clone(),
                skills::RepoSetModelInput {
                    repo_key: repo_key.clone(),
                    model: tool.clone(),
                    enabled: true,
                    scope: Some(install_scope.clone()),
                    project_root: install_project_root.clone(),
                },
            )
            .await
            {
                Ok(result) => {
                    repo_installed_by_key
                        .insert(repo_key, repo_installed_for_tool(&result.data, &tool));
                    installed_ids.insert(result.data.skill_id.clone());
                    installed_source_rel.insert((source_id, source_rel_path));
                    Ok(())
                }
                Err(err) => Err(err),
            },
        };

        match install_result {
            Ok(()) => {
                installed_skill_count += 1;
            }
            Err(err) => {
                failed_skill_installs.push(format!("{} ({})", skill_id, err));
            }
        }
    }

    let deps_after = detect_dependencies_for_working_dir(preset, Some(working_dir))?;
    Ok(WorkflowDependencyApplyResult {
        preset_id: preset.id.clone(),
        linked_mcp_count,
        enabled_mcp_switch_count,
        installed_skill_count,
        failed_skill_installs,
        dependencies_after: deps_after,
    })
}

fn build_missing_skill_error(deps: &WorkflowDependencyState) -> Option<String> {
    if deps.missing_skill_ids.is_empty() {
        return None;
    }
    let display = if deps.missing_skill_names.is_empty() {
        deps.missing_skill_ids.join(", ")
    } else {
        deps.missing_skill_names.join(", ")
    };
    Some(format!(
        "workflow required skills are not ready: {}",
        display
    ))
}

#[tauri::command]
pub fn workflows_presets_list() -> Result<ApiOk<Vec<WorkflowPreset>>, String> {
    let mut presets = load_presets()?;
    presets.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    api_ok(presets)
}

#[tauri::command]
pub fn workflows_preset_upsert(
    app: tauri::AppHandle,
    input: WorkflowPresetInput,
) -> Result<ApiOk<WorkflowPreset>, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("workflow preset name is required".to_string());
    }

    let now = now_ts();
    let mut presets = load_presets()?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| format!("wf-{}", uuid::Uuid::new_v4()));
    let tool = normalize_tool(input.tool.as_deref().unwrap_or("claude"));
    let working_dir = input.working_dir.unwrap_or_default().trim().to_string();
    let preset = WorkflowPreset {
        id: id.clone(),
        name: name.to_string(),
        tool,
        working_dir,
        provider_id: input.provider_id.and_then(|v| {
            if v.trim().is_empty() {
                None
            } else {
                Some(v.trim().to_string())
            }
        }),
        mcp_server_ids: dedup_non_empty(&input.mcp_server_ids),
        required_skill_ids: dedup_non_empty(&input.required_skill_ids),
        launch_prompt: input.launch_prompt.and_then(|v| {
            let s = v.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }),
        launch_scope: normalize_launch_scope(input.launch_scope.as_deref()),
        created_at: now,
        updated_at: now,
    };

    if let Some(pos) = presets.iter().position(|p| p.id == id) {
        let created_at = presets[pos].created_at;
        let mut updated = preset.clone();
        updated.created_at = created_at;
        presets[pos] = updated.clone();
        save_presets(&presets)?;
        tauri::async_runtime::spawn(async move {
            let _ = crate::app_store::sync_enqueue(app, "workflow_preset_upsert".to_string()).await;
        });
        return api_ok(updated);
    }

    presets.push(preset.clone());
    save_presets(&presets)?;
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, "workflow_preset_upsert".to_string()).await;
    });
    api_ok(preset)
}

#[tauri::command]
pub fn workflows_preset_delete(
    app: tauri::AppHandle,
    preset_id: String,
) -> Result<ApiOk<Value>, String> {
    let mut presets = load_presets()?;
    let before = presets.len();
    presets.retain(|p| p.id != preset_id);
    if before != presets.len() {
        save_presets(&presets)?;
        tauri::async_runtime::spawn(async move {
            let _ = crate::app_store::sync_enqueue(app, "workflow_preset_delete".to_string()).await;
        });
    }
    api_ok(json!({ "deleted": before != presets.len() }))
}

#[tauri::command]
pub fn workflows_check_dependencies(
    preset_id: String,
) -> Result<ApiOk<WorkflowDependencyState>, String> {
    let presets = load_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| "workflow preset not found".to_string())?;
    let deps = detect_dependencies(preset)?;
    api_ok(deps)
}

#[tauri::command]
pub async fn workflows_apply_dependencies(
    app: tauri::AppHandle,
    preset_id: String,
) -> Result<ApiOk<WorkflowDependencyApplyResult>, String> {
    let presets = load_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .cloned()
        .ok_or_else(|| "workflow preset not found".to_string())?;
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let result =
        apply_dependencies_for_preset(app, &preset, &launch_scope, &preset.working_dir).await?;
    api_ok(result)
}

#[tauri::command]
pub async fn workflows_launch_preset(
    app: tauri::AppHandle,
    input: WorkflowLaunchInput,
) -> Result<ApiOk<Value>, String> {
    let presets = load_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.id == input.preset_id)
        .cloned()
        .ok_or_else(|| "workflow preset not found".to_string())?;
    let mut runs = load_runs()?;
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let prompt_apply_status = prompt_apply_status_for_preset(&preset);
    let _ = cleanup_runtime_profiles();
    let default_working_dir = input
        .override_working_dir
        .clone()
        .unwrap_or_else(|| preset.working_dir.clone());

    if let Err(err) = ensure_strict_provider_env_managed(&preset) {
        let run = make_run_for_launch(
            &preset,
            default_working_dir.clone(),
            None,
            None,
            launch_scope.clone(),
            None,
            prompt_apply_status.clone(),
            if launch_scope == LAUNCH_SCOPE_STRICT {
                DEP_MODE_STRICT_LOCAL.to_string()
            } else {
                DEP_MODE_SHARED_GLOBAL.to_string()
            },
            "failed",
            Some(err.clone()),
            None,
        );
        runs.push(run);
        save_runs(&runs)?;
        return Err(err);
    }

    let dependency_result =
        apply_dependencies_for_preset(app.clone(), &preset, &launch_scope, &default_working_dir)
            .await;
    let dependency_apply_mode = if launch_scope == LAUNCH_SCOPE_STRICT {
        DEP_MODE_STRICT_LOCAL.to_string()
    } else {
        DEP_MODE_SHARED_GLOBAL.to_string()
    };
    match dependency_result {
        Ok(result) => {
            if let Some(dep_err) = build_missing_skill_error(&result.dependencies_after) {
                let run = make_run_for_launch(
                    &preset,
                    default_working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    None,
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(dep_err.clone()),
                    None,
                );
                runs.push(run);
                save_runs(&runs)?;
                return Err(dep_err);
            }
        }
        Err(err) => {
            let run = make_run_for_launch(
                &preset,
                default_working_dir.clone(),
                None,
                None,
                launch_scope.clone(),
                None,
                prompt_apply_status.clone(),
                dependency_apply_mode.clone(),
                "failed",
                Some(err.clone()),
                None,
            );
            runs.push(run);
            save_runs(&runs)?;
            return Err(err);
        }
    }

    let mut runtime_profile_id: Option<String> = None;
    if launch_scope == LAUNCH_SCOPE_STRICT {
        let selected_mcp = match selected_mcp_servers_for_preset(&preset) {
            Ok(v) => v,
            Err(err) => {
                let run = make_run_for_launch(
                    &preset,
                    default_working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    None,
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    None,
                );
                runs.push(run);
                save_runs(&runs)?;
                return Err(err);
            }
        };
        let skill_dir_names =
            match resolve_skill_dir_names_for_preset(&preset, &default_working_dir) {
                Ok(v) => v,
                Err(err) => {
                    let run = make_run_for_launch(
                        &preset,
                        default_working_dir.clone(),
                        None,
                        None,
                        launch_scope.clone(),
                        None,
                        prompt_apply_status.clone(),
                        dependency_apply_mode.clone(),
                        "failed",
                        Some(err.clone()),
                        None,
                    );
                    runs.push(run);
                    save_runs(&runs)?;
                    return Err(err);
                }
            };
        let profile_id = format!("rp-{}", uuid::Uuid::new_v4());
        match runtime_profiles::materialize_strict_profile(runtime_profiles::StrictProfileInput {
            profile_id: profile_id.clone(),
            tool: normalize_tool(&preset.tool),
            mcp_servers: selected_mcp,
            skill_dir_names,
            install_scope: Some("project".to_string()),
            project_root: canonicalize_working_dir(&default_working_dir),
            reuse_existing: false,
        }) {
            Ok(result) => {
                runtime_profile_id = Some(result.profile_id);
            }
            Err(err) => {
                let run = make_run_for_launch(
                    &preset,
                    default_working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    Some(profile_id),
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    None,
                );
                runs.push(run);
                save_runs(&runs)?;
                return Err(err);
            }
        }
    }

    let launch_result = create_session_for_preset(
        app,
        &preset,
        input.session_name.clone(),
        input.override_working_dir.clone(),
        launch_scope.clone(),
        runtime_profile_id.clone(),
    )
    .await;

    match launch_result {
        Ok((session_value, used_working_dir, tool_session_id)) => {
            let session_id = session_value
                .get("id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let run = make_run_for_launch(
                &preset,
                used_working_dir,
                session_id.clone(),
                tool_session_id,
                launch_scope.clone(),
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "running",
                None,
                None,
            );
            runs.push(run.clone());
            save_runs(&runs)?;
            api_ok(json!({
                "preset": preset,
                "session": session_value,
                "run": run
            }))
        }
        Err(err) => {
            let run = make_run_for_launch(
                &preset,
                input
                    .override_working_dir
                    .unwrap_or_else(|| preset.working_dir.clone()),
                None,
                None,
                launch_scope,
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "failed",
                Some(err.clone()),
                None,
            );
            runs.push(run);
            save_runs(&runs)?;
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn workflows_replay_run(
    app: tauri::AppHandle,
    input: WorkflowReplayInput,
) -> Result<ApiOk<Value>, String> {
    let presets = load_presets()?;
    let mut runs = load_runs()?;
    let idx = runs
        .iter()
        .position(|r| r.id == input.run_id)
        .ok_or_else(|| "workflow run not found".to_string())?;
    let base_run = runs[idx].clone();
    let preset = presets
        .iter()
        .find(|p| p.id == base_run.preset_id)
        .cloned()
        .ok_or_else(|| "workflow preset not found for replay".to_string())?;
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let prompt_apply_status = prompt_apply_status_for_preset(&preset);
    let _ = cleanup_runtime_profiles();

    if base_run.status == "running" {
        runs[idx].status = "interrupted".to_string();
        runs[idx].ended_at = Some(now_ts());
        if runs[idx].error_message.is_none() {
            runs[idx].error_message =
                Some("Replay requested while previous run still running".to_string());
        }
    }

    if let Err(err) = ensure_strict_provider_env_managed(&preset) {
        let failed_run = make_run_for_launch(
            &preset,
            base_run.working_dir.clone(),
            None,
            None,
            launch_scope.clone(),
            None,
            prompt_apply_status.clone(),
            if launch_scope == LAUNCH_SCOPE_STRICT {
                DEP_MODE_STRICT_LOCAL.to_string()
            } else {
                DEP_MODE_SHARED_GLOBAL.to_string()
            },
            "failed",
            Some(err.clone()),
            Some(base_run.id.clone()),
        );
        runs.push(failed_run);
        save_runs(&runs)?;
        return Err(err);
    }

    let dependency_result =
        apply_dependencies_for_preset(app.clone(), &preset, &launch_scope, &base_run.working_dir)
            .await;
    let dependency_apply_mode = if launch_scope == LAUNCH_SCOPE_STRICT {
        DEP_MODE_STRICT_LOCAL.to_string()
    } else {
        DEP_MODE_SHARED_GLOBAL.to_string()
    };
    match dependency_result {
        Ok(result) => {
            if let Some(dep_err) = build_missing_skill_error(&result.dependencies_after) {
                let failed_run = make_run_for_launch(
                    &preset,
                    base_run.working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    None,
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(dep_err.clone()),
                    Some(base_run.id.clone()),
                );
                runs.push(failed_run);
                save_runs(&runs)?;
                return Err(dep_err);
            }
        }
        Err(err) => {
            let failed_run = make_run_for_launch(
                &preset,
                base_run.working_dir.clone(),
                None,
                None,
                launch_scope.clone(),
                None,
                prompt_apply_status.clone(),
                dependency_apply_mode.clone(),
                "failed",
                Some(err.clone()),
                Some(base_run.id.clone()),
            );
            runs.push(failed_run);
            save_runs(&runs)?;
            return Err(err);
        }
    }

    let mut runtime_profile_id = if launch_scope == LAUNCH_SCOPE_STRICT {
        base_run.runtime_profile_id.clone()
    } else {
        None
    };
    if launch_scope == LAUNCH_SCOPE_STRICT {
        let selected_mcp = match selected_mcp_servers_for_preset(&preset) {
            Ok(v) => v,
            Err(err) => {
                let failed_run = make_run_for_launch(
                    &preset,
                    base_run.working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    runtime_profile_id.clone(),
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    Some(base_run.id.clone()),
                );
                runs.push(failed_run);
                save_runs(&runs)?;
                return Err(err);
            }
        };
        let skill_dir_names =
            match resolve_skill_dir_names_for_preset(&preset, &base_run.working_dir) {
                Ok(v) => v,
                Err(err) => {
                    let failed_run = make_run_for_launch(
                        &preset,
                        base_run.working_dir.clone(),
                        None,
                        None,
                        launch_scope.clone(),
                        runtime_profile_id.clone(),
                        prompt_apply_status.clone(),
                        dependency_apply_mode.clone(),
                        "failed",
                        Some(err.clone()),
                        Some(base_run.id.clone()),
                    );
                    runs.push(failed_run);
                    save_runs(&runs)?;
                    return Err(err);
                }
            };
        let desired_profile_id = if let Some(existing) = runtime_profile_id.clone() {
            if runtime_profiles::runtime_profile_exists(&existing).unwrap_or(false) {
                existing
            } else {
                format!("rp-{}", uuid::Uuid::new_v4())
            }
        } else {
            format!("rp-{}", uuid::Uuid::new_v4())
        };
        match runtime_profiles::materialize_strict_profile(runtime_profiles::StrictProfileInput {
            profile_id: desired_profile_id.clone(),
            tool: normalize_tool(&preset.tool),
            mcp_servers: selected_mcp,
            skill_dir_names,
            install_scope: Some("project".to_string()),
            project_root: canonicalize_working_dir(&base_run.working_dir),
            reuse_existing: true,
        }) {
            Ok(result) => {
                runtime_profile_id = Some(result.profile_id);
            }
            Err(err) => {
                let failed_run = make_run_for_launch(
                    &preset,
                    base_run.working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    Some(desired_profile_id),
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    Some(base_run.id.clone()),
                );
                runs.push(failed_run);
                save_runs(&runs)?;
                return Err(err);
            }
        }
    }

    let launch_result = create_session_for_preset(
        app,
        &preset,
        input.session_name.clone(),
        Some(base_run.working_dir.clone()),
        launch_scope.clone(),
        runtime_profile_id.clone(),
    )
    .await;

    match launch_result {
        Ok((session_value, used_working_dir, tool_session_id)) => {
            let session_id = session_value
                .get("id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let run = make_run_for_launch(
                &preset,
                used_working_dir,
                session_id,
                tool_session_id,
                launch_scope.clone(),
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "running",
                None,
                Some(base_run.id.clone()),
            );
            runs.push(run.clone());
            save_runs(&runs)?;
            api_ok(json!({
                "replay_of": base_run,
                "session": session_value,
                "run": run
            }))
        }
        Err(err) => {
            let failed_run = make_run_for_launch(
                &preset,
                base_run.working_dir,
                None,
                None,
                launch_scope,
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "failed",
                Some(err.clone()),
                Some(base_run.id.clone()),
            );
            runs.push(failed_run);
            save_runs(&runs)?;
            Err(err)
        }
    }
}

#[tauri::command]
pub fn workflows_runs_list(
    input: Option<WorkflowRunListInput>,
) -> Result<ApiOk<Vec<WorkflowRun>>, String> {
    let payload = input.unwrap_or(WorkflowRunListInput {
        preset_id: None,
        limit: Some(100),
    });
    let mut runs = load_runs()?;
    if let Some(preset_id) = payload.preset_id {
        runs.retain(|r| r.preset_id == preset_id);
    }
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    if let Some(limit) = payload.limit {
        runs.truncate(limit.max(1));
    }
    api_ok(runs)
}

#[tauri::command]
pub fn workflows_run_update(input: WorkflowRunUpdateInput) -> Result<ApiOk<WorkflowRun>, String> {
    let status = input.status.trim().to_lowercase();
    if !allowed_run_status(&status) {
        return Err("invalid run status".to_string());
    }
    let mut runs = load_runs()?;
    let idx = runs
        .iter()
        .position(|r| r.id == input.run_id)
        .ok_or_else(|| "workflow run not found".to_string())?;

    runs[idx].status = status.clone();
    runs[idx].summary = input.summary.and_then(|s| {
        let v = s.trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    });
    runs[idx].error_message = input.error_message.and_then(|s| {
        let v = s.trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    });
    if status == "running" {
        runs[idx].ended_at = None;
    } else if runs[idx].ended_at.is_none() {
        runs[idx].ended_at = Some(now_ts());
    }

    let updated = runs[idx].clone();
    save_runs(&runs)?;
    api_ok(updated)
}

#[tauri::command]
pub fn workflows_run_delete(input: WorkflowRunDeleteInput) -> Result<ApiOk<Value>, String> {
    let run_id = input.run_id.trim().to_string();
    if run_id.is_empty() {
        return Err("run id required".to_string());
    }

    let mut runs = load_runs()?;
    let before = runs.len();
    runs.retain(|r| r.id != run_id);
    if runs.len() == before {
        return Err("workflow run not found".to_string());
    }

    save_runs(&runs)?;
    api_ok(json!({ "deleted": true }))
}
