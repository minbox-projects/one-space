use crate::config::{self, SkillSourceConfig, StorageConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

const MODELS: [&str; 4] = ["claude", "gemini", "codex", "opencode"];
const IGNORE_NAMES: [&str; 5] = [".git", ".DS_Store", "node_modules", "dist", "target"];
const INSTALL_SCOPE_GLOBAL: &str = "global";
const INSTALL_SCOPE_PROJECT: &str = "project";

static JOB_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNNING_JOB_KEYS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn job_lock() -> &'static Mutex<()> {
    JOB_LOCK.get_or_init(|| Mutex::new(()))
}

fn running_job_keys() -> &'static Mutex<HashSet<String>> {
    RUNNING_JOB_KEYS.get_or_init(|| Mutex::new(HashSet::new()))
}

struct JobKeyGuard {
    key: String,
}

impl Drop for JobKeyGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = running_job_keys().lock() {
            running.remove(&self.key);
        }
    }
}

fn acquire_job_key(key: impl Into<String>) -> Result<Option<JobKeyGuard>, String> {
    let key = key.into();
    let mut running = running_job_keys().lock().map_err(|e| e.to_string())?;
    if running.contains(&key) {
        return Ok(None);
    }
    running.insert(key.clone());
    Ok(Some(JobKeyGuard { key }))
}

fn default_install_scope() -> String {
    INSTALL_SCOPE_GLOBAL.to_string()
}

fn normalize_install_scope(scope: Option<&str>) -> String {
    match scope.unwrap_or("").trim().to_lowercase().as_str() {
        INSTALL_SCOPE_PROJECT => INSTALL_SCOPE_PROJECT.to_string(),
        _ => INSTALL_SCOPE_GLOBAL.to_string(),
    }
}

fn normalize_project_root_for_scope(
    scope: &str,
    project_root: Option<&str>,
) -> Result<Option<String>, String> {
    if scope != INSTALL_SCOPE_PROJECT {
        return Ok(None);
    }
    let raw = project_root
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });
    if raw.trim().is_empty() {
        return Err("skills/project_root_required".to_string());
    }
    let path = PathBuf::from(raw.trim());
    if !path.exists() || !path.is_dir() {
        return Err("skills/project_root_invalid".to_string());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    Ok(Some(canonical.to_string_lossy().to_string()))
}

fn record_scope(record: &SkillRecord) -> String {
    normalize_install_scope(Some(&record.scope))
}

fn normalized_project_root_value(project_root: Option<&str>) -> Option<String> {
    project_root
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn scope_project_match(record: &SkillRecord, scope: &str, project_root: Option<&str>) -> bool {
    if record_scope(record) != scope {
        return false;
    }
    record_project_root(record) == normalized_project_root_value(project_root)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillRecord {
    pub id: String,
    #[serde(default)]
    pub dir_name: String,
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub name: String,
    pub description: String,
    pub source_id: String,
    pub source_rel_path: String,
    pub installed_at: u64,
    pub updated_at: Option<u64>,
    pub last_synced_at: Option<u64>,
    pub local_hash: String,
    pub remote_hash: Option<String>,
    pub has_update: bool,
    pub icon_seed: String,
    #[serde(default = "default_install_scope")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoModelInstallState {
    pub claude: bool,
    pub gemini: bool,
    pub codex: bool,
    pub opencode: bool,
}

impl Default for RepoModelInstallState {
    fn default() -> Self {
        Self {
            claude: false,
            gemini: false,
            codex: false,
            opencode: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepositoryRecord {
    pub repo_key: String,
    pub skill_id: String,
    #[serde(default)]
    pub dir_name: String,
    pub source_id: String,
    pub source_rel_path: String,
    pub source_type: String,
    pub source_path: Option<String>,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub icon_seed: String,
    pub hash: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    pub updated_at: Option<u64>,
    #[serde(default)]
    pub ever_installed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepositorySkillView {
    pub repo_key: String,
    pub skill_id: String,
    #[serde(default)]
    pub dir_name: String,
    pub source_id: String,
    pub source_rel_path: String,
    pub source_type: String,
    pub source_path: Option<String>,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub icon_seed: String,
    pub hash: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    pub updated_at: Option<u64>,
    pub has_update: bool,
    pub installed: RepoModelInstallState,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillsState {
    // Legacy field from old versions. Installed skills are now stored in local state.
    #[serde(default, skip_serializing)]
    pub skills: Vec<SkillRecord>,
    #[serde(default)]
    pub repositories: Vec<RepositoryRecord>,
    pub revision: u64,
    pub last_rescan_at: Option<u64>,
    pub last_sync_at: Option<u64>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillsLocalState {
    #[serde(default)]
    pub skills: Vec<SkillRecord>,
    pub revision: u64,
    pub last_rescan_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SourceSyncState {
    pub source_id: String,
    pub last_synced_at: Option<u64>,
    pub last_commit_sha: Option<String>,
    pub last_status: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CatalogSkill {
    pub source_id: String,
    pub id: String,
    pub rel_path: String,
    #[serde(default)]
    pub dir_name: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub remote_hash: String,
    pub icon_seed: String,
    #[serde(default)]
    pub first_seen_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillsSyncState {
    pub status: String,
    pub last_error: Option<String>,
    pub last_sync_at: Option<u64>,
    #[serde(default)]
    pub sources: Vec<SourceSyncState>,
    #[serde(default)]
    pub catalog: Vec<CatalogSkill>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub revision: u64,
    pub ts: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillsConfigPayload {
    pub skills_sync_enabled: bool,
    pub skills_sync_interval_minutes: u64,
    pub skills_new_badge_hours: u64,
    #[serde(default)]
    pub skills_sources: Vec<SkillSourceConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillsSourcesExportPayload {
    pub version: u32,
    pub exported_at: String,
    #[serde(default)]
    pub skills_sources: Vec<SkillSourceConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallInput {
    pub source_id: String,
    pub skill_ref: String,
    pub model: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub project_root: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalScanInput {
    pub root_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalSkillCandidate {
    pub rel_path: String,
    pub skill_id: String,
    #[serde(default)]
    pub dir_name: String,
    pub source_id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub declared_models: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalImportSelection {
    pub rel_path: String,
    pub conflict_strategy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalImportInput {
    pub root_path: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub selections: Vec<LocalImportSelection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoImportFolderInput {
    pub folder_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoImportFolderResult {
    pub repo_key: String,
    pub skill_id: String,
    pub source_id: String,
    pub source_rel_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalImportSkipped {
    pub rel_path: String,
    pub skill_id: String,
    pub model: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalImportFailed {
    pub rel_path: String,
    pub skill_id: Option<String>,
    pub model: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalImportRepoAdded {
    pub repo_key: String,
    pub skill_id: String,
    pub source_id: String,
    pub source_rel_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LocalImportResult {
    #[serde(default)]
    pub repo_added: Vec<LocalImportRepoAdded>,
    #[serde(default)]
    pub installed: Vec<SkillRecord>,
    #[serde(default)]
    pub skipped: Vec<LocalImportSkipped>,
    #[serde(default)]
    pub failed: Vec<LocalImportFailed>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillKeyInput {
    pub model: String,
    pub skill_id: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub project_root: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoSetModelInput {
    pub repo_key: String,
    pub model: String,
    pub enabled: bool,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub project_root: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CatalogSkillKeyInput {
    pub source_id: String,
    pub skill_ref: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoSkillKeyInput {
    pub repo_key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoReloadApplyInput {
    pub repo_key: String,
    #[serde(default)]
    pub sync_to_models: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct SkillModelFilter {
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateDiff {
    pub local_markdown: String,
    pub remote_markdown: String,
    #[serde(default)]
    pub local_changed_lines: Vec<u32>,
    #[serde(default)]
    pub remote_changed_lines: Vec<u32>,
    #[serde(default)]
    pub local_changed_blocks: Vec<DiffBlock>,
    #[serde(default)]
    pub remote_changed_blocks: Vec<DiffBlock>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillDetail {
    pub skill: SkillRecord,
    pub markdown: String,
    pub local_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CatalogSkillDetail {
    pub skill: CatalogSkill,
    pub markdown: String,
    pub source_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiffBlock {
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReloadChangedFile {
    pub path: String,
    pub status: String,
    pub is_binary: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReloadTextDiff {
    pub path: String,
    pub before_content: String,
    pub after_content: String,
    #[serde(default)]
    pub before_changed_lines: Vec<u32>,
    #[serde(default)]
    pub after_changed_lines: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ReloadPreview {
    pub before_label: String,
    pub after_label: String,
    #[serde(default)]
    pub changed_files: Vec<ReloadChangedFile>,
    #[serde(default)]
    pub text_diffs: Vec<ReloadTextDiff>,
    #[serde(default)]
    pub installed_models: Vec<String>,
    pub has_changes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReloadApplyResult {
    pub index_refreshed: bool,
    #[serde(default)]
    pub synced_models: Vec<String>,
    pub updated_files_count: u64,
    pub applied_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CatalogOpenFolderResult {
    pub repo_key: String,
    pub opened_path: String,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn skills_root() -> Result<PathBuf, String> {
    let p = crate::get_data_dir()?.join("data").join("skills");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_local_root() -> Result<PathBuf, String> {
    let p = skills_local_cache_base_root()?.join("local_state");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_models_root() -> Result<PathBuf, String> {
    let p = skills_local_root()?.join("models");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_meta_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_local_cache_base_root() -> Result<PathBuf, String> {
    let p = if let Some(home) = dirs::home_dir() {
        home.join(".config").join("onespace").join("skills")
    } else {
        // Fallback to app-local config directory if home is unavailable.
        config::get_app_dir()?.join("skills")
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_cache_root() -> Result<PathBuf, String> {
    // Remote git source caches are always local to reduce iCloud/git sync pressure.
    let p = skills_local_cache_base_root()?.join("remote_cache");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_state_path() -> Result<PathBuf, String> {
    Ok(skills_meta_root()?.join("state.json"))
}

fn skills_local_meta_root() -> Result<PathBuf, String> {
    let p = skills_local_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_local_state_path() -> Result<PathBuf, String> {
    Ok(skills_local_meta_root()?.join("installed_state.json"))
}

fn sync_state_path() -> Result<PathBuf, String> {
    Ok(skills_meta_root()?.join("sync_state.json"))
}

fn model_dir(model: &str) -> Result<PathBuf, String> {
    if !MODELS.contains(&model) {
        return Err(format!("unsupported model: {}", model));
    }
    let p = skills_models_root()?.join(model);
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

fn project_primary_dir(model: &str, project_root: &Path) -> Result<PathBuf, String> {
    let p = match model {
        "claude" => project_root.join(".claude").join("skills"),
        "codex" => project_root.join(".agents").join("skills"),
        "gemini" => project_root.join(".gemini").join("skills"),
        "opencode" => project_root.join(".opencode").join("skills"),
        _ => return Err(format!("unsupported model: {}", model)),
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn project_compat_dirs(model: &str, project_root: &Path) -> Vec<PathBuf> {
    match model {
        "codex" => vec![project_root.join(".codex").join("skills")],
        _ => vec![],
    }
}

fn mirror_dir(model: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("home directory not found")?;
    let p = match model {
        "claude" => home.join(".claude").join("skills"),
        "codex" => home.join(".codex").join("skills"),
        "gemini" => home.join(".gemini").join("skills"),
        "opencode" => home.join(".config").join("opencode").join("skills"),
        _ => return Err(format!("unsupported model: {}", model)),
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(fs::canonicalize(&p).unwrap_or(p))
}

fn resolve_skill_target_dir(
    model: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("skills/project_root_required")?;
        let project_root_path = PathBuf::from(root);
        let primary = project_primary_dir(model, &project_root_path)?;
        let compat = project_compat_dirs(model, &project_root_path);
        for path in &compat {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        return Ok((primary, compat));
    }
    let primary = model_dir(model)?;
    let mirror = mirror_dir(model)?;
    Ok((primary, vec![mirror]))
}

fn make_repo_key(source_id: &str, source_rel_path: &str) -> String {
    format!("{}::{}", source_id, source_rel_path)
}

fn repo_storage_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn repo_storage_dir(repo_key: &str) -> Result<PathBuf, String> {
    let digest = sha256_hex(repo_key);
    Ok(repo_storage_root()?.join(digest))
}

fn repo_index_baseline_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("index_baselines");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn repo_index_baseline_dir(repo_key: &str) -> Result<PathBuf, String> {
    let digest = sha256_hex(repo_key);
    Ok(repo_index_baseline_root()?.join(digest))
}

fn snapshot_repository_index_baseline(repo_key: &str, source_dir: &Path) -> Result<(), String> {
    let baseline = repo_index_baseline_dir(repo_key)?;
    replace_dir_atomic(source_dir, &baseline)
}

fn safe_slug(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn normalize_rel_path(rel: &Path) -> String {
    if rel == Path::new(".") {
        return ".".to_string();
    }
    rel.to_string_lossy().replace('\\', "/")
}

fn resolve_scan_root(root_path: &str) -> Result<PathBuf, String> {
    let raw = root_path.trim();
    if raw.is_empty() {
        return Err("skills/invalid_scan_root".to_string());
    }
    let root = PathBuf::from(raw);
    if !root.exists() {
        return Err("skills/invalid_scan_root".to_string());
    }
    if !root.is_dir() {
        return Err("skills/invalid_scan_root".to_string());
    }
    fs::canonicalize(&root).map_err(|_| "skills/invalid_scan_root".to_string())
}

fn local_source_id(root_can: &Path) -> String {
    let digest = sha256_hex(&root_can.to_string_lossy());
    format!("local-{}", &digest[..8])
}

fn local_skill_id(source_id: &str, rel_path: &str) -> String {
    let key = format!("{}:{}", source_id, rel_path);
    let digest = sha256_hex(&key);
    let slug = safe_slug(&key);
    let slug = if slug.is_empty() {
        source_id.to_string()
    } else {
        slug
    };
    format!("{}-{}", slug, &digest[..8])
}

fn has_path_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

fn is_ignored_name(name: &str) -> bool {
    IGNORE_NAMES.contains(&name)
}

fn parse_duplicate_file_name(name: &str) -> Option<String> {
    let dot_idx = name.rfind('.')?;
    let (stem, ext) = name.split_at(dot_idx);
    let space_idx = stem.rfind(' ')?;
    let suffix = &stem[space_idx + 1..];
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let base = &stem[..space_idx];
    if base.trim().is_empty() {
        return None;
    }
    Some(format!("{}{}", base, ext))
}

fn is_duplicate_clone_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };
    let Some(counterpart_name) = parse_duplicate_file_name(file_name) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let counterpart = parent.join(counterpart_name);
    if !counterpart.exists() {
        return false;
    }
    if !path.is_file() || !counterpart.is_file() {
        return false;
    }
    match (fs::read(path), fs::read(counterpart)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn ensure_within(root: &Path, target: &Path) -> Result<(), String> {
    if has_path_traversal(target) {
        return Err("skills/path_out_of_root".to_string());
    }
    let root_can = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let target_can = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    if !target_can.starts_with(&root_can) {
        return Err("skills/path_out_of_root".to_string());
    }
    Ok(())
}

fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, raw).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}

fn merge_repository_record(
    mut keep: RepositoryRecord,
    candidate: RepositoryRecord,
) -> RepositoryRecord {
    if keep.source_path.is_none() && candidate.source_path.is_some() {
        keep.source_path = candidate.source_path.clone();
    }
    if keep.hash.as_deref().unwrap_or("").is_empty()
        && !candidate.hash.as_deref().unwrap_or("").is_empty()
    {
        keep.hash = candidate.hash.clone();
    }
    if keep.updated_at.unwrap_or(0) < candidate.updated_at.unwrap_or(0) {
        keep.updated_at = candidate.updated_at;
    }
    if keep.models.is_empty() && !candidate.models.is_empty() {
        keep.models = candidate.models.clone();
    }
    if keep.created_at == 0 {
        keep.created_at = candidate.created_at;
    } else if candidate.created_at > 0 {
        keep.created_at = keep.created_at.min(candidate.created_at);
    }
    keep.ever_installed = keep.ever_installed || candidate.ever_installed;
    keep
}

fn is_local_duplicate_repository(a: &RepositoryRecord, b: &RepositoryRecord) -> bool {
    if a.source_type != "local_import" || b.source_type != "local_import" {
        return false;
    }
    if a.skill_id == b.skill_id {
        return true;
    }
    let ah = a.hash.as_deref().unwrap_or("").trim();
    let bh = b.hash.as_deref().unwrap_or("").trim();
    if ah.is_empty() || bh.is_empty() || ah != bh {
        return false;
    }
    a.name.trim().eq_ignore_ascii_case(b.name.trim())
}

fn normalize_repositories(state: &mut SkillsState) -> bool {
    let mut changed = false;
    for repo in &mut state.repositories {
        if repo.source_type == "mirror" {
            repo.source_type = "local_import".to_string();
            changed = true;
        }
        if repo.dir_name.trim().is_empty() {
            repo.dir_name = repo.skill_id.clone();
            changed = true;
        }
        if repo.created_at == 0 {
            repo.created_at = repo.updated_at.unwrap_or_else(now_ts);
            changed = true;
        }
    }

    let mut deduped: Vec<RepositoryRecord> = vec![];
    for repo in state.repositories.drain(..) {
        if let Some(idx) = deduped
            .iter()
            .position(|existing| is_local_duplicate_repository(existing, &repo))
        {
            let merged = merge_repository_record(deduped[idx].clone(), repo);
            deduped[idx] = merged;
            changed = true;
        } else {
            deduped.push(repo);
        }
    }
    state.repositories = deduped;
    changed
}

fn load_skills_state() -> Result<SkillsState, String> {
    let mut state: SkillsState = read_json_or_default(&skills_state_path()?)?;
    let mut changed = false;
    if ensure_repositories_migrated(&mut state)? {
        changed = true;
    }
    if normalize_repositories(&mut state) {
        changed = true;
    }
    if !state.skills.is_empty() {
        // Installed records must not be persisted in shared storage.
        state.skills.clear();
        changed = true;
    }
    if changed {
        state = save_skills_state(state)?;
    }
    Ok(state)
}

fn save_skills_state(mut state: SkillsState) -> Result<SkillsState, String> {
    state.skills.clear();
    state.revision = state.revision.saturating_add(1);
    write_json(&skills_state_path()?, &state)?;
    Ok(state)
}

fn load_local_skills_state() -> Result<SkillsLocalState, String> {
    read_json_or_default(&skills_local_state_path()?)
}

fn save_local_skills_state(mut state: SkillsLocalState) -> Result<SkillsLocalState, String> {
    state.revision = state.revision.saturating_add(1);
    write_json(&skills_local_state_path()?, &state)?;
    Ok(state)
}

fn combined_revision(shared: &SkillsState, local: &SkillsLocalState) -> u64 {
    shared.revision.max(local.revision)
}

fn load_sync_state() -> Result<SkillsSyncState, String> {
    read_json_or_default(&sync_state_path()?)
}

fn save_sync_state(state: &SkillsSyncState) -> Result<(), String> {
    write_json(&sync_state_path()?, state)
}

fn api_ok<T: Serialize>(data: T, revision: u64) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            revision,
            ts: now_ts(),
        },
    })
}

fn collect_files(base: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(current).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
        if is_ignored_name(name) {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_files(base, &path, files)?;
        } else if meta.is_file() {
            if is_duplicate_clone_file(&path) {
                continue;
            }
            let rel = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            files.push(rel);
        }
    }
    Ok(())
}

fn hash_dir(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let mut files = vec![];
    collect_files(path, path, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in files {
        let rel_str = rel.to_string_lossy();
        hasher.update(rel_str.as_bytes());
        hasher.update([0]);
        let abs = path.join(&rel);
        let content = fs::read(&abs).map_err(|e| e.to_string())?;
        hasher.update((content.len() as u64).to_le_bytes());
        hasher.update([0]);
        hasher.update(&content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn source_type_from_source_id(source_id: &str) -> String {
    if source_id.starts_with("local-") || source_id == "local" || source_id == "mirror-local" {
        "local_import".to_string()
    } else {
        "remote".to_string()
    }
}

fn upsert_repository_record(repositories: &mut Vec<RepositoryRecord>, record: RepositoryRecord) {
    if let Some(idx) = repositories
        .iter()
        .position(|r| r.repo_key == record.repo_key)
    {
        let mut next = record;
        let existing = &repositories[idx];
        if existing.created_at > 0
            && (next.created_at == 0 || next.created_at > existing.created_at)
        {
            next.created_at = existing.created_at;
        }
        next.ever_installed = next.ever_installed || existing.ever_installed;
        repositories[idx] = next;
    } else {
        let mut next = record;
        if next.created_at == 0 {
            next.created_at = now_ts();
        }
        repositories.push(next);
    }
}

fn mark_repo_ever_installed(state: &mut SkillsState, repo_key: &str) -> bool {
    if let Some(repo) = state
        .repositories
        .iter_mut()
        .find(|r| r.repo_key == repo_key)
    {
        if !repo.ever_installed {
            repo.ever_installed = true;
            repo.updated_at = Some(now_ts());
            return true;
        }
    }
    false
}

fn skill_matches_repository(skill: &SkillRecord, repo: &RepositoryRecord) -> bool {
    let same_repo_key = make_repo_key(&skill.source_id, &skill.source_rel_path) == repo.repo_key;
    let same_skill_id = skill.id == repo.skill_id;
    if same_repo_key || same_skill_id {
        return true;
    }

    let source_matches = if skill.source_id == repo.source_id {
        true
    } else if repo.source_type == "local_import" {
        (skill.source_id == "local" && repo.source_id.starts_with("local-"))
            || (repo.source_id == "local" && skill.source_id.starts_with("local-"))
    } else {
        false
    };
    if !source_matches {
        return false;
    }

    let skill_dir_name = skill.dir_name.trim();
    let repo_dir_name = repo.dir_name.trim();
    let skill_dir_name = if skill_dir_name.is_empty() {
        skill.id.as_str()
    } else {
        skill_dir_name
    };
    let repo_dir_name = if repo_dir_name.is_empty() {
        repo.skill_id.as_str()
    } else {
        repo_dir_name
    };
    if skill_dir_name == repo_dir_name {
        return true;
    }

    let skill_rel_tail = skill
        .source_rel_path
        .rsplit('/')
        .next()
        .unwrap_or(skill.source_rel_path.as_str());
    let repo_rel_tail = repo
        .source_rel_path
        .rsplit('/')
        .next()
        .unwrap_or(repo.source_rel_path.as_str());
    skill_rel_tail == repo_rel_tail
}

fn build_repo_install_state(
    installed_skills: &[SkillRecord],
    repo: &RepositoryRecord,
) -> RepoModelInstallState {
    let mut installed = RepoModelInstallState::default();
    for skill in installed_skills {
        if !skill_matches_repository(skill, repo) {
            continue;
        }
        match skill.model.as_str() {
            "claude" => installed.claude = true,
            "gemini" => installed.gemini = true,
            "codex" => installed.codex = true,
            "opencode" => installed.opencode = true,
            _ => {}
        }
    }
    installed
}

fn scoped_installed_skills(
    installed_skills: &[SkillRecord],
    scope: &str,
    project_root: Option<&str>,
) -> Vec<SkillRecord> {
    installed_skills
        .iter()
        .filter(|skill| scope_project_match(skill, scope, project_root))
        .cloned()
        .collect::<Vec<_>>()
}

fn repository_pair_has_update(before: &Path, after: &Path) -> bool {
    match (hash_dir(before), hash_dir(after)) {
        (Ok(before_hash), Ok(after_hash)) => before_hash != after_hash,
        _ => false,
    }
}

fn repository_has_pending_index_update(repo: &RepositoryRecord) -> bool {
    let snapshot = match repo_storage_dir(&repo.repo_key) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !snapshot.exists() {
        return false;
    }

    let baseline = match repo_index_baseline_dir(&repo.repo_key) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !baseline.exists() {
        return true;
    }

    if repository_pair_has_update(&baseline, &snapshot) {
        return true;
    }

    false
}

fn build_repository_views(
    shared_state: &SkillsState,
    installed_skills: &[SkillRecord],
    include_update: bool,
) -> Vec<RepositorySkillView> {
    let mut out = shared_state
        .repositories
        .iter()
        .filter_map(|repo| {
            let installed = build_repo_install_state(installed_skills, repo);
            let installed_any =
                installed.claude || installed.gemini || installed.codex || installed.opencode;
            if repo.source_type == "remote" && !repo.ever_installed && !installed_any {
                return None;
            }
            let has_update = if include_update {
                repository_has_pending_index_update(repo)
            } else {
                false
            };
            Some(RepositorySkillView {
                repo_key: repo.repo_key.clone(),
                skill_id: repo.skill_id.clone(),
                dir_name: normalized_repo_dir_name(repo),
                source_id: repo.source_id.clone(),
                source_rel_path: repo.source_rel_path.clone(),
                source_type: repo.source_type.clone(),
                source_path: repo.source_path.clone(),
                name: repo.name.clone(),
                description: repo.description.clone(),
                models: repo.models.clone(),
                icon_seed: repo.icon_seed.clone(),
                hash: repo.hash.clone(),
                created_at: repo.created_at,
                updated_at: repo.updated_at,
                has_update,
                installed,
            })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.updated_at.unwrap_or(0).cmp(&a.updated_at.unwrap_or(0)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn ensure_repositories_migrated(state: &mut SkillsState) -> Result<bool, String> {
    if !state.repositories.is_empty() {
        return Ok(false);
    }

    let mut changed = false;
    let mut seen = HashSet::new();
    for skill in &state.skills {
        let repo_key = make_repo_key(&skill.source_id, &skill.source_rel_path);
        if !seen.insert(repo_key.clone()) {
            continue;
        }

        let mut repo_hash = Some(skill.local_hash.clone());
        if let Ok(src) = record_local_dir(skill) {
            if src.exists() {
                let dst = repo_storage_dir(&repo_key)?;
                if replace_dir_atomic(&src, &dst).is_ok() {
                    let _ = snapshot_repository_index_baseline(&repo_key, &dst);
                    repo_hash = hash_dir(&dst).ok().or(repo_hash);
                }
            }
        }

        state.repositories.push(RepositoryRecord {
            repo_key: repo_key.clone(),
            skill_id: skill.id.clone(),
            dir_name: normalized_record_dir_name(skill),
            source_id: skill.source_id.clone(),
            source_rel_path: skill.source_rel_path.clone(),
            source_type: source_type_from_source_id(&skill.source_id),
            source_path: None,
            name: skill.name.clone(),
            description: skill.description.clone(),
            models: skill.models.clone(),
            icon_seed: skill.icon_seed.clone(),
            hash: repo_hash,
            created_at: now_ts(),
            updated_at: Some(now_ts()),
            ever_installed: true,
        });
        changed = true;
    }
    Ok(changed)
}

fn upsert_repository_from_dir(
    state: &mut SkillsState,
    source_dir: &Path,
    source_id: &str,
    source_rel_path: &str,
    skill_id: &str,
    dir_name: &str,
    source_type: &str,
    name: &str,
    description: &str,
    models: &[String],
    icon_seed: &str,
    source_path: Option<String>,
    hash_hint: Option<String>,
    mark_ever_installed: bool,
) -> Result<RepositoryRecord, String> {
    let repo_key = make_repo_key(source_id, source_rel_path);
    let repo_dst = repo_storage_dir(&repo_key)?;
    replace_dir_atomic(source_dir, &repo_dst)?;
    snapshot_repository_index_baseline(&repo_key, &repo_dst)?;
    let repo_hash = hash_dir(&repo_dst).ok().or(hash_hint);
    let existing_ever_installed = state
        .repositories
        .iter()
        .find(|r| r.repo_key == repo_key)
        .map(|r| r.ever_installed)
        .unwrap_or(false);
    let existing_created_at = state
        .repositories
        .iter()
        .find(|r| r.repo_key == repo_key)
        .map(|r| r.created_at)
        .unwrap_or(0);
    let created_at = if existing_created_at > 0 {
        existing_created_at
    } else {
        now_ts()
    };

    let record = RepositoryRecord {
        repo_key: repo_key.clone(),
        skill_id: skill_id.to_string(),
        dir_name: dir_name.to_string(),
        source_id: source_id.to_string(),
        source_rel_path: source_rel_path.to_string(),
        source_type: source_type.to_string(),
        source_path,
        name: name.to_string(),
        description: description.to_string(),
        models: models.to_vec(),
        icon_seed: icon_seed.to_string(),
        hash: repo_hash,
        created_at,
        updated_at: Some(now_ts()),
        ever_installed: mark_ever_installed || existing_ever_installed,
    };
    upsert_repository_record(&mut state.repositories, record.clone());
    Ok(record)
}

fn normalized_model(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    if MODELS.contains(&v.as_str()) {
        Some(v)
    } else {
        None
    }
}

fn normalize_models(models: &[String]) -> Vec<String> {
    let mut out = vec![];
    for raw in models {
        if let Some(m) = normalized_model(raw) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
}

fn all_models_vec() -> Vec<String> {
    MODELS.iter().map(|v| v.to_string()).collect()
}

fn resolve_effective_models(
    declared_models: &[String],
    source_allowed_models: &[String],
) -> Vec<String> {
    let mut declared = normalize_models(declared_models);
    if declared.is_empty() {
        declared = all_models_vec();
    }
    let allowed = normalize_models(source_allowed_models);
    if allowed.is_empty() {
        return declared;
    }
    declared
        .into_iter()
        .filter(|model| allowed.contains(model))
        .collect::<Vec<_>>()
}

fn parse_models(text: &str, source_default: &[String]) -> Vec<String> {
    let mut out = vec![];
    for line in text.lines() {
        let lower = line.trim().to_lowercase();
        if lower.starts_with("models:") {
            let body = line.split_once(':').map(|(_, v)| v).unwrap_or("").trim();
            let body = body.trim_matches('[').trim_matches(']');
            for item in body.split(',') {
                let m = item
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_lowercase();
                if MODELS.contains(&m.as_str()) && !out.contains(&m) {
                    out.push(m);
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    for v in normalize_models(source_default) {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    if out.is_empty() {
        all_models_vec()
    } else {
        out
    }
}

fn normalize_skill_markdown_for_parse(md: &str) -> String {
    let no_bom = md.strip_prefix('\u{feff}').unwrap_or(md);
    no_bom.replace("\r\n", "\n").replace('\r', "\n")
}

fn split_frontmatter_block(md: &str) -> (Option<&str>, &str) {
    let trimmed = md.trim_start_matches(|c: char| c.is_whitespace());
    if !trimmed.starts_with("---\n") {
        return (None, md);
    }

    let body = &trimmed[4..];
    let mut cursor = 0usize;
    for segment in body.split_inclusive('\n') {
        let line = segment.trim_end_matches('\n').trim_end_matches('\r');
        if line.trim() == "---" {
            let frontmatter = if cursor > 0 {
                &body[..cursor.saturating_sub(1)]
            } else {
                ""
            };
            let content = &body[cursor + segment.len()..];
            return (Some(frontmatter), content);
        }
        cursor += segment.len();
    }

    if body.trim() == "---" {
        return (Some(""), "");
    }

    (None, md)
}

fn parse_title_and_description(content: &str) -> (Option<String>, Option<String>) {
    let mut title = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let candidate = trimmed.trim_start_matches('#').trim().to_string();
            if !candidate.is_empty() {
                title = Some(candidate);
                break;
            }
        }
    }

    let mut desc = String::new();
    let mut in_para = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_para {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        in_para = true;
        if !desc.is_empty() {
            desc.push(' ');
        }
        desc.push_str(trimmed);
    }

    let description = if desc.is_empty() { None } else { Some(desc) };
    (title, description)
}

fn parse_skill_md(md: &str, source_default_models: &[String]) -> (String, String, Vec<String>) {
    let normalized = normalize_skill_markdown_for_parse(md);
    let (frontmatter, mut content) = split_frontmatter_block(&normalized);
    let mut name_from_frontmatter = None;
    let mut description_from_frontmatter = None;
    let mut models = parse_models(&normalized, source_default_models);
    if let Some(front) = frontmatter {
        models = parse_models(front, source_default_models);
        name_from_frontmatter = parse_frontmatter_value(front, "name");
        description_from_frontmatter = parse_frontmatter_value(front, "description");
        content = content.trim_start_matches('\n');
    }

    let (title_from_content, paragraph_from_content) = parse_title_and_description(content);
    let name = title_from_content
        .or(name_from_frontmatter)
        .unwrap_or_else(|| "Unnamed Skill".to_string());
    let desc = description_from_frontmatter
        .or(paragraph_from_content)
        .unwrap_or_else(|| "No description".to_string());
    (name, desc, models)
}

fn validate_frontmatter_name_as_dir(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err("skills/invalid_frontmatter_name".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("skills/invalid_frontmatter_name".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err("skills/invalid_frontmatter_name".to_string());
    }
    Ok(name.to_string())
}

fn parse_required_skill_dir_name(md: &str) -> Result<String, String> {
    let normalized = normalize_skill_markdown_for_parse(md);
    let (frontmatter, _) = split_frontmatter_block(&normalized);
    let frontmatter = frontmatter.ok_or("skills/invalid_frontmatter_name".to_string())?;
    let raw_name = parse_frontmatter_value(frontmatter, "name")
        .ok_or("skills/invalid_frontmatter_name".to_string())?;
    validate_frontmatter_name_as_dir(&raw_name)
}

fn read_required_skill_dir_name(skill_dir: &Path) -> Result<String, String> {
    let raw = fs::read_to_string(skill_dir.join("SKILL.md"))
        .map_err(|_| "skills/invalid_skill_dir".to_string())?;
    parse_required_skill_dir_name(&raw)
}

fn normalized_record_dir_name(record: &SkillRecord) -> String {
    let name = record.dir_name.trim();
    if name.is_empty() {
        record.id.clone()
    } else {
        name.to_string()
    }
}

fn normalized_repo_dir_name(repo: &RepositoryRecord) -> String {
    let name = repo.dir_name.trim();
    if name.is_empty() {
        repo.skill_id.clone()
    } else {
        name.to_string()
    }
}

fn record_project_root(record: &SkillRecord) -> Option<String> {
    record
        .project_root
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn record_target_root(record: &SkillRecord) -> Result<PathBuf, String> {
    let scope = record_scope(record);
    let project_root = record_project_root(record);
    let (root, _) = resolve_skill_target_dir(&record.model, &scope, project_root.as_deref())?;
    Ok(root)
}

fn locate_existing_record_local_dir(record: &SkillRecord) -> Result<PathBuf, String> {
    let root = record_target_root(record)?;
    let mut candidates = vec![];
    let dir_name = record.dir_name.trim();
    if !dir_name.is_empty() {
        candidates.push(dir_name.to_string());
    }
    candidates.push(record.id.clone());
    candidates.dedup();

    for candidate in &candidates {
        let path = root.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    let fallback = candidates
        .first()
        .cloned()
        .unwrap_or_else(|| record.id.clone());
    Ok(root.join(fallback))
}

fn has_dir_name_conflict(
    state: &SkillsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    dir_name: &str,
    ignore_skill_id: Option<&str>,
) -> bool {
    state.skills.iter().any(|s| {
        if s.model != model {
            return false;
        }
        if record_scope(s) != scope {
            return false;
        }
        let s_root = record_project_root(s);
        let target_root = project_root
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if s_root != target_root {
            return false;
        }
        if ignore_skill_id.is_some() && ignore_skill_id == Some(s.id.as_str()) {
            return false;
        }
        normalized_record_dir_name(s) == dir_name
    })
}

fn ensure_model_dir_name_available(
    state: &SkillsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    dir_name: &str,
    ignore_skill_id: Option<&str>,
) -> Result<(), String> {
    if has_dir_name_conflict(state, model, scope, project_root, dir_name, ignore_skill_id) {
        return Err("skills/dir_name_conflict".to_string());
    }
    let (root, _) = resolve_skill_target_dir(model, scope, project_root)?;
    let dest = root.join(dir_name);
    ensure_within(&root, &dest)?;
    if dest.exists() {
        if let Some(skill_id) = ignore_skill_id {
            if let Some(existing) = state.skills.iter().find(|s| {
                s.model == model
                    && s.id == skill_id
                    && record_scope(s) == scope
                    && record_project_root(s)
                        == project_root
                            .map(|v| v.trim().to_string())
                            .filter(|v| !v.is_empty())
            }) {
                let existing_path = locate_existing_record_local_dir(existing)?;
                if existing_path == dest {
                    return Ok(());
                }
            }
        }
        return Err("skills/dir_name_conflict".to_string());
    }
    Ok(())
}

fn remove_existing_record_dir_if_moved(
    state: &SkillsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    skill_id: &str,
    new_dest: &Path,
) -> Result<(), String> {
    let Some(existing) = state.skills.iter().find(|s| {
        s.model == model
            && s.id == skill_id
            && record_scope(s) == scope
            && record_project_root(s)
                == project_root
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
    }) else {
        return Ok(());
    };
    let old_dir = locate_existing_record_local_dir(existing)?;
    if old_dir != new_dest && old_dir.exists() {
        fs::remove_dir_all(old_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn upsert_repo_dir_name(
    state: &mut SkillsState,
    source_id: &str,
    source_rel_path: &str,
    skill_id: &str,
    dir_name: &str,
) -> bool {
    let repo_key = make_repo_key(source_id, source_rel_path);
    let mut changed = false;
    for repo in &mut state.repositories {
        if repo.repo_key == repo_key || repo.skill_id == skill_id {
            if repo.dir_name != dir_name {
                repo.dir_name = dir_name.to_string();
                repo.updated_at = Some(now_ts());
                changed = true;
            }
        }
    }
    changed
}

fn parse_frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        if !k.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        let value = v
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn find_skill_dirs(base: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(current).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
        if is_ignored_name(name) {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let rel = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                out.push(rel);
            } else {
                find_skill_dirs(base, &path, out)?;
            }
        }
    }
    Ok(())
}

fn find_local_skill_dirs(base: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = vec![];
    if base.join("SKILL.md").exists() {
        out.push(PathBuf::from("."));
    }
    find_skill_dirs(base, base, &mut out)?;
    out.sort_by(|a, b| normalize_rel_path(a).cmp(&normalize_rel_path(b)));
    Ok(out)
}

fn scan_local_candidates(root_can: &Path) -> Result<Vec<LocalSkillCandidate>, String> {
    let source_id = local_source_id(root_can);
    let skill_dirs = find_local_skill_dirs(root_can)?;
    let mut out = vec![];
    for rel in skill_dirs {
        let rel_str = normalize_rel_path(&rel);
        let abs = if rel_str == "." {
            root_can.to_path_buf()
        } else {
            root_can.join(&rel)
        };
        let md = abs.join("SKILL.md");
        let md_content = fs::read_to_string(&md).map_err(|e| e.to_string())?;
        let (name, description, declared_models) = parse_skill_md(&md_content, &[]);
        let dir_name = parse_required_skill_dir_name(&md_content).unwrap_or_default();
        out.push(LocalSkillCandidate {
            rel_path: rel_str.clone(),
            skill_id: local_skill_id(&source_id, &rel_str),
            dir_name,
            source_id: source_id.clone(),
            name,
            description,
            declared_models,
        });
    }
    Ok(out)
}

fn copy_dir_secure_internal(
    src_root: &Path,
    src: &Path,
    dst_root: &Path,
    dst: &Path,
) -> Result<(), String> {
    if !src.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let src_root_can =
        fs::canonicalize(src_root).map_err(|_| "skills/path_out_of_root".to_string())?;
    let src_can = fs::canonicalize(src).map_err(|_| "skills/path_out_of_root".to_string())?;
    if !src_can.starts_with(&src_root_can) {
        return Err("skills/path_out_of_root".to_string());
    }
    ensure_within(dst_root, dst)?;
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
        if is_ignored_name(name) {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let target = dst.join(entry.file_name());
        if meta.is_dir() {
            copy_dir_secure_internal(src_root, &path, dst_root, &target)?;
        } else if meta.is_file() {
            if is_duplicate_clone_file(&path) {
                continue;
            }
            ensure_within(dst_root, &target)?;
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn copy_dir_secure(src: &Path, dst: &Path) -> Result<(), String> {
    let dst_root = dst
        .parent()
        .map(|v| v.to_path_buf())
        .unwrap_or_else(|| dst.to_path_buf());
    copy_dir_secure_internal(src, src, &dst_root, dst)
}

fn replace_dir_atomic(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let parent = dst.parent().ok_or("invalid destination")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stage = parent.join(format!(".stage-{}", now_ts()));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    copy_dir_secure_internal(src, src, parent, &stage)?;

    let backup = parent.join(format!(".backup-{}", now_ts()));
    if dst.exists() {
        fs::rename(dst, &backup).map_err(|e| e.to_string())?;
    }
    fs::rename(&stage, dst).map_err(|e| e.to_string())?;
    if backup.exists() {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

fn get_source<'a>(cfg: &'a StorageConfig, source_id: &str) -> Option<&'a SkillSourceConfig> {
    cfg.skills_sources.iter().find(|s| s.id == source_id)
}

fn source_base_dir(source: &SkillSourceConfig) -> String {
    let raw = source.base_dir.clone().unwrap_or_else(|| "/".to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn source_branch(source: &SkillSourceConfig) -> String {
    let b = source.branch.clone().unwrap_or_else(|| "main".to_string());
    if b.trim().is_empty() {
        "main".to_string()
    } else {
        b
    }
}

fn git_run(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = crate::get_git_command();
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    // Never block on interactive auth prompts in background sync.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_ASKPASS", "echo");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sync_source_repo(source: &SkillSourceConfig) -> Result<PathBuf, String> {
    let cache_root = skills_cache_root()?;
    let repo_dir = cache_root.join(&source.id);
    let branch = source_branch(source);

    if repo_dir.join(".git").exists() {
        let _ = git_run(
            Some(&repo_dir),
            &["fetch", "--depth", "1", "origin", &branch],
        );
        let _ = git_run(Some(&repo_dir), &["checkout", &branch]);
        let _ = git_run(
            Some(&repo_dir),
            &["reset", "--hard", &format!("origin/{}", branch)],
        );
    } else {
        if repo_dir.exists() {
            fs::remove_dir_all(&repo_dir).map_err(|e| e.to_string())?;
        }
        let repo_dir_str = repo_dir.to_string_lossy().to_string();
        git_run(
            None,
            &[
                "clone",
                "--depth",
                "1",
                "--branch",
                &branch,
                &source.repo_url,
                &repo_dir_str,
            ],
        )?;
    }

    Ok(repo_dir)
}

fn source_scan_root(repo_dir: &Path, source: &SkillSourceConfig) -> Result<PathBuf, String> {
    let base_dir = source_base_dir(source);
    let rel = base_dir.trim_start_matches('/');
    let root = if rel.is_empty() {
        repo_dir.to_path_buf()
    } else {
        repo_dir.join(rel)
    };
    if !root.exists() {
        return Err("skills/source_fetch_failed".to_string());
    }
    Ok(root)
}

fn scan_source_catalog(
    repo_dir: &Path,
    source: &SkillSourceConfig,
) -> Result<Vec<CatalogSkill>, String> {
    let scan_root = source_scan_root(&repo_dir, source)?;
    let mut skill_dirs = vec![];
    find_skill_dirs(&scan_root, &scan_root, &mut skill_dirs)?;
    let mut catalog = vec![];
    for rel in skill_dirs {
        let abs = scan_root.join(&rel);
        let md = abs.join("SKILL.md");
        let md_content = fs::read_to_string(&md).map_err(|e| e.to_string())?;
        // Keep declared/all models in catalog; source allow-list is applied at query time.
        let (name, description, models) = parse_skill_md(&md_content, &[]);
        let rel_str = rel.to_string_lossy().to_string();
        let dir_name = parse_required_skill_dir_name(&md_content)
            .unwrap_or_else(|_| rel_str.rsplit('/').next().unwrap_or_default().to_string());
        let id = safe_slug(&format!("{}-{}", source.id, rel_str));
        let remote_hash = hash_dir(&abs)?;
        catalog.push(CatalogSkill {
            source_id: source.id.clone(),
            id,
            rel_path: rel_str,
            dir_name,
            name,
            description,
            models,
            remote_hash,
            icon_seed: source.id.clone(),
            first_seen_at: None,
        });
    }
    Ok(catalog)
}

fn assign_catalog_first_seen(
    previous_catalog: &[CatalogSkill],
    mut scanned_catalog: Vec<CatalogSkill>,
) -> Vec<CatalogSkill> {
    let previous_map = previous_catalog
        .iter()
        .map(|c| (make_repo_key(&c.source_id, &c.rel_path), c.first_seen_at))
        .collect::<HashMap<_, _>>();
    let now = now_ts();
    for item in &mut scanned_catalog {
        let key = make_repo_key(&item.source_id, &item.rel_path);
        item.first_seen_at = previous_map.get(&key).copied().unwrap_or(Some(now));
    }
    scanned_catalog
}

fn source_skill_abs_path(source: &SkillSourceConfig, rel_path: &str) -> Result<PathBuf, String> {
    let repo_dir = skills_cache_root()?.join(&source.id);
    let root = source_scan_root(&repo_dir, source)?;
    let rel = PathBuf::from(rel_path);
    if has_path_traversal(&rel) {
        return Err("skills/path_out_of_root".to_string());
    }
    let p = root.join(rel);
    ensure_within(&root, &p)?;
    Ok(p)
}

fn read_repository_skill_markdown(repo: &RepositoryRecord, cfg: &StorageConfig) -> Option<String> {
    if let Ok(snapshot) = repo_storage_dir(&repo.repo_key) {
        let md = snapshot.join("SKILL.md");
        if md.exists() {
            if let Ok(content) = fs::read_to_string(&md) {
                return Some(content);
            }
        }
    }

    if let Some(src) = repo.source_path.as_ref() {
        let md = PathBuf::from(src).join("SKILL.md");
        if md.exists() {
            if let Ok(content) = fs::read_to_string(&md) {
                return Some(content);
            }
        }
    }

    if repo.source_type == "remote" {
        if let Some(source) = get_source(cfg, &repo.source_id) {
            if let Ok(path) = source_skill_abs_path(source, &repo.source_rel_path) {
                let md = path.join("SKILL.md");
                if md.exists() {
                    if let Ok(content) = fs::read_to_string(&md) {
                        return Some(content);
                    }
                }
            }
        }
    }

    None
}

fn refresh_repository_metadata_from_snapshots(
    state: &mut SkillsState,
    cfg: &StorageConfig,
) -> bool {
    let mut changed = false;
    for repo in &mut state.repositories {
        let Some(markdown) = read_repository_skill_markdown(repo, cfg) else {
            continue;
        };
        let (name, description, _models) = parse_skill_md(&markdown, &[]);
        let parsed_dir_name = parse_required_skill_dir_name(&markdown).ok();
        if repo.name != name
            || repo.description != description
            || parsed_dir_name
                .as_ref()
                .map(|dir| repo.dir_name != *dir)
                .unwrap_or(false)
        {
            repo.name = name;
            repo.description = description;
            if let Some(dir_name) = parsed_dir_name {
                repo.dir_name = dir_name;
            }
            repo.updated_at = Some(now_ts());
            changed = true;
        }
    }
    changed
}

fn normalize_text_content(content: String) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn read_markdown_for_compare(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(normalize_text_content)
}

fn skill_has_markdown_update(skill: &SkillRecord, cfg: &StorageConfig) -> Option<bool> {
    let local = record_local_dir(skill).ok()?.join("SKILL.md");
    let local_md = read_markdown_for_compare(&local)?;
    let source = get_source(cfg, &skill.source_id)?;
    let remote_dir = source_skill_abs_path(source, &skill.source_rel_path).ok()?;
    let remote_md = read_markdown_for_compare(&remote_dir.join("SKILL.md"))?;
    Some(local_md != remote_md)
}

fn lines_to_blocks(lines: &[u32], content: &str) -> Vec<DiffBlock> {
    if lines.is_empty() {
        return vec![];
    }
    let all_lines: Vec<&str> = content.lines().collect();
    let mut blocks = vec![];
    let mut start = lines[0];
    let mut prev = lines[0];

    for &line in lines.iter().skip(1) {
        if line == prev + 1 {
            prev = line;
            continue;
        }
        let slice = (start..=prev)
            .filter_map(|ln| all_lines.get((ln.saturating_sub(1)) as usize).copied())
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(DiffBlock {
            start_line: start,
            end_line: prev,
            content: slice,
        });
        start = line;
        prev = line;
    }

    let slice = (start..=prev)
        .filter_map(|ln| all_lines.get((ln.saturating_sub(1)) as usize).copied())
        .collect::<Vec<_>>()
        .join("\n");
    blocks.push(DiffBlock {
        start_line: start,
        end_line: prev,
        content: slice,
    });
    blocks
}

fn calculate_changes(
    local_md: &str,
    remote_md: &str,
) -> (Vec<u32>, Vec<u32>, Vec<DiffBlock>, Vec<DiffBlock>) {
    let left: Vec<&str> = local_md.lines().collect();
    let right: Vec<&str> = remote_md.lines().collect();
    let max_len = left.len().max(right.len());
    let mut l_changed = vec![];
    let mut r_changed = vec![];
    for i in 0..max_len {
        let l = left.get(i).copied().unwrap_or("");
        let r = right.get(i).copied().unwrap_or("");
        if l != r {
            if i < left.len() {
                l_changed.push((i + 1) as u32);
            }
            if i < right.len() {
                r_changed.push((i + 1) as u32);
            }
        }
    }
    let l_blocks = lines_to_blocks(&l_changed, local_md);
    let r_blocks = lines_to_blocks(&r_changed, remote_md);
    (l_changed, r_changed, l_blocks, r_blocks)
}

fn collect_file_map(root: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut rel_files = vec![];
    if !root.exists() {
        return Ok(HashMap::new());
    }
    collect_files(root, root, &mut rel_files)?;
    let mut out = HashMap::new();
    for rel in rel_files {
        let normalized = normalize_rel_path(&rel);
        out.insert(normalized, root.join(&rel));
    }
    Ok(out)
}

fn read_text_file_for_diff(path: &Path) -> Result<Option<String>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    match String::from_utf8(bytes) {
        Ok(content) => Ok(Some(content.replace("\r\n", "\n").replace('\r', "\n"))),
        Err(_) => Ok(None),
    }
}

fn compare_snapshot_dirs(
    before_dir: Option<&Path>,
    after_dir: &Path,
) -> Result<(Vec<ReloadChangedFile>, Vec<ReloadTextDiff>), String> {
    let before = if let Some(dir) = before_dir {
        collect_file_map(dir)?
    } else {
        HashMap::new()
    };
    let after = collect_file_map(after_dir)?;

    let mut keys = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();

    let mut changed_files = vec![];
    let mut text_diffs = vec![];
    for rel in keys {
        let before_path = before.get(&rel);
        let after_path = after.get(&rel);
        let status = match (before_path, after_path) {
            (Some(_), None) => Some("deleted"),
            (None, Some(_)) => Some("added"),
            (Some(b), Some(a)) => {
                let b_content = fs::read(b).map_err(|e| e.to_string())?;
                let a_content = fs::read(a).map_err(|e| e.to_string())?;
                if b_content == a_content {
                    None
                } else {
                    Some("modified")
                }
            }
            (None, None) => None,
        };

        let Some(status) = status else {
            continue;
        };

        let before_text = if let Some(path) = before_path {
            read_text_file_for_diff(path)?
        } else {
            Some(String::new())
        };
        let after_text = if let Some(path) = after_path {
            read_text_file_for_diff(path)?
        } else {
            Some(String::new())
        };
        let is_binary = before_text.is_none() || after_text.is_none();

        changed_files.push(ReloadChangedFile {
            path: rel.clone(),
            status: status.to_string(),
            is_binary,
        });

        if !is_binary {
            let before_content = before_text.unwrap_or_default();
            let after_content = after_text.unwrap_or_default();
            let (before_changed_lines, after_changed_lines, _, _) =
                calculate_changes(&before_content, &after_content);
            text_diffs.push(ReloadTextDiff {
                path: rel.clone(),
                before_content,
                after_content,
                before_changed_lines,
                after_changed_lines,
            });
        }
    }

    Ok((changed_files, text_diffs))
}

fn resolve_repo_reload_after_dir(
    repo: &RepositoryRecord,
    baseline: Option<&Path>,
    repo_snapshot: &Path,
) -> Result<(PathBuf, String), String> {
    let _ = repo;
    let _ = baseline;
    Ok((
        repo_snapshot.to_path_buf(),
        "After Reload (Current Snapshot)".to_string(),
    ))
}

fn installed_models_for_repo(
    local_state: &SkillsLocalState,
    repo: &RepositoryRecord,
) -> Vec<String> {
    let mut out = vec![];
    for model in MODELS {
        let installed = local_state
            .skills
            .iter()
            .any(|s| s.model == model && skill_matches_repository(s, repo));
        if installed {
            out.push(model.to_string());
        }
    }
    out
}

fn refresh_repository_record_from_snapshot(repo: &mut RepositoryRecord) -> Result<(), String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    let markdown = fs::read_to_string(repo_snapshot.join("SKILL.md")).unwrap_or_default();
    let (name, description, models) = parse_skill_md(&markdown, &[]);
    let dir_name = parse_required_skill_dir_name(&markdown)?;
    repo.name = name;
    repo.description = description;
    repo.models = models;
    repo.dir_name = dir_name;
    repo.hash = Some(hash_dir(&repo_snapshot)?);
    repo.updated_at = Some(now_ts());
    Ok(())
}

fn materialize_repository_snapshot_if_missing(
    repo: &RepositoryRecord,
    local_state: &SkillsLocalState,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if repo_snapshot.exists() {
        return Ok(false);
    }

    if let Some(src) = repo.source_path.as_ref() {
        let src_path = PathBuf::from(src);
        if src_path.join("SKILL.md").exists() {
            replace_dir_atomic(&src_path, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
            return Ok(true);
        }
    }

    if let Some(local_record) = local_state
        .skills
        .iter()
        .find(|s| skill_matches_repository(s, repo))
    {
        let local_dir = record_local_dir(local_record)?;
        if local_dir.join("SKILL.md").exists() {
            replace_dir_atomic(&local_dir, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
            return Ok(true);
        }
    }

    if repo.source_type == "remote" {
        if let Some(source) = get_source(cfg, &repo.source_id) {
            if let Ok(source_path) = source_skill_abs_path(source, &repo.source_rel_path) {
                if source_path.join("SKILL.md").exists() {
                    replace_dir_atomic(&source_path, &repo_snapshot)?;
                    snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

fn ensure_repository_snapshots_materialized(
    state: &mut SkillsState,
    local_state: &SkillsLocalState,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let mut changed = false;
    for repo in &mut state.repositories {
        if materialize_repository_snapshot_if_missing(repo, local_state, cfg)? {
            refresh_repository_record_from_snapshot(repo)?;
            changed = true;
        }
    }
    Ok(changed)
}

fn record_local_dir(record: &SkillRecord) -> Result<PathBuf, String> {
    Ok(record_target_root(record)?.join(normalized_record_dir_name(record)))
}

fn migrate_installed_dir_names(
    shared_state: &mut SkillsState,
    local_state: &mut SkillsLocalState,
) -> Result<(bool, bool), String> {
    let mut shared_changed = false;
    let mut local_changed = false;

    for model in MODELS {
        let model_root = model_dir(model)?;
        let indices = local_state
            .skills
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                if s.model == model && record_scope(s) == INSTALL_SCOPE_GLOBAL {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for idx in indices {
            let record = local_state.skills[idx].clone();
            let current_dir = locate_existing_record_local_dir(&record)?;
            if !current_dir.exists() {
                continue;
            }
            let md = current_dir.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let md_raw = fs::read_to_string(&md).unwrap_or_default();
            let desired_dir_name = match parse_required_skill_dir_name(&md_raw) {
                Ok(name) => name,
                Err(_) => continue,
            };

            if has_dir_name_conflict(
                local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &desired_dir_name,
                Some(record.id.as_str()),
            ) {
                continue;
            }

            let target_dir = model_root.join(&desired_dir_name);
            ensure_within(&model_root, &target_dir)?;

            if current_dir != target_dir {
                if target_dir.exists() {
                    continue;
                }
                match fs::rename(&current_dir, &target_dir) {
                    Ok(_) => {}
                    Err(_) => {
                        replace_dir_atomic(&current_dir, &target_dir)?;
                        if current_dir.exists() {
                            fs::remove_dir_all(&current_dir).map_err(|e| e.to_string())?;
                        }
                    }
                }
                local_changed = true;
            }

            let new_hash = hash_dir(&target_dir)?;
            if local_state.skills[idx].dir_name != desired_dir_name {
                local_state.skills[idx].dir_name = desired_dir_name.clone();
                local_changed = true;
            }
            if local_state.skills[idx].local_hash != new_hash {
                local_state.skills[idx].local_hash = new_hash;
                local_changed = true;
            }

            if upsert_repo_dir_name(
                shared_state,
                &local_state.skills[idx].source_id,
                &local_state.skills[idx].source_rel_path,
                &local_state.skills[idx].id,
                &desired_dir_name,
            ) {
                shared_changed = true;
            }
        }
    }

    Ok((shared_changed, local_changed))
}

fn touch_sync_timestamp(cfg: &mut StorageConfig) {
    cfg.skills_last_synced_at = Some(now_ts() as i64);
}

fn trigger_storage_sync(app: tauri::AppHandle, reason: &str) {
    let reason = reason.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, reason).await;
    });
}

fn update_record_remote_flags(
    state: &mut SkillsLocalState,
    sync_state: &SkillsSyncState,
    cfg: &StorageConfig,
) {
    let mut map = HashMap::new();
    for c in &sync_state.catalog {
        map.insert(
            (c.source_id.clone(), c.rel_path.clone()),
            c.remote_hash.clone(),
        );
    }
    for s in &mut state.skills {
        if let Some(remote_hash) = map.get(&(s.source_id.clone(), s.source_rel_path.clone())) {
            s.remote_hash = Some(remote_hash.clone());
            s.has_update = skill_has_markdown_update(s, cfg).unwrap_or(false);
            s.last_synced_at = Some(now_ts());
        }
    }
}

fn refresh_local_hashes(
    state: &mut SkillsLocalState,
    model_filter: Option<&str>,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let mut changed = false;
    for skill in &mut state.skills {
        if let Some(model) = model_filter {
            if skill.model != model {
                continue;
            }
        }
        let local_dir = record_local_dir(skill)?;
        let local_hash = hash_dir(&local_dir)?;
        if skill.local_hash != local_hash {
            skill.local_hash = local_hash;
            changed = true;
        }
        let has_update = skill_has_markdown_update(skill, cfg).unwrap_or(false);
        if skill.has_update != has_update {
            skill.has_update = has_update;
            changed = true;
        }
    }
    Ok(changed)
}

fn hydrate_local_records_from_catalog(state: &mut SkillsLocalState, sync_state: &SkillsSyncState) {
    let mut catalog_by_hash: HashMap<String, Vec<&CatalogSkill>> = HashMap::new();
    let mut catalog_by_dir_name: HashMap<String, Vec<&CatalogSkill>> = HashMap::new();
    for item in &sync_state.catalog {
        catalog_by_hash
            .entry(item.remote_hash.clone())
            .or_default()
            .push(item);
        if !item.dir_name.trim().is_empty() {
            catalog_by_dir_name
                .entry(item.dir_name.clone())
                .or_default()
                .push(item);
        }
    }

    for skill in &mut state.skills {
        if skill.source_id != "local" {
            continue;
        }

        let match_by_hash =
            catalog_by_hash
                .get(&skill.local_hash)
                .and_then(|items| match items.as_slice() {
                    [item] => Some(*item),
                    _ => None,
                });
        let matched = match_by_hash.or_else(|| {
            let dir_name = normalized_record_dir_name(skill);
            let candidates = catalog_by_dir_name.get(&dir_name)?;
            let matches = candidates
                .iter()
                .copied()
                .filter(|item| {
                    skill.models.is_empty()
                        || item.models.is_empty()
                        || item.models.iter().any(|model| skill.models.contains(model))
                })
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [item] => Some(*item),
                _ => None,
            }
        });
        let Some(item) = matched else {
            continue;
        };

        skill.id = item.id.clone();
        skill.dir_name = item.dir_name.clone();
        skill.name = item.name.clone();
        skill.description = item.description.clone();
        skill.models = item.models.clone();
        skill.source_id = item.source_id.clone();
        skill.source_rel_path = item.rel_path.clone();
        skill.remote_hash = Some(item.remote_hash.clone());
        skill.has_update = false;
        skill.last_synced_at = Some(now_ts());
        skill.icon_seed = item.icon_seed.clone();
    }
}

fn refresh_remote_repositories_from_catalog(
    state: &mut SkillsState,
    local_state: &SkillsLocalState,
    sync_state: &SkillsSyncState,
    cfg: &StorageConfig,
) -> Result<(), String> {
    let existing_remote_usage = state
        .repositories
        .iter()
        .filter(|r| r.source_type == "remote")
        .map(|r| (r.repo_key.clone(), r.ever_installed))
        .collect::<HashMap<_, _>>();
    let mut tracked_remote_keys = HashSet::new();
    let installed_keys = local_state
        .skills
        .iter()
        .map(|s| make_repo_key(&s.source_id, &s.source_rel_path))
        .collect::<HashSet<_>>();
    let installed_skill_ids = local_state
        .skills
        .iter()
        .map(|s| s.id.clone())
        .collect::<HashSet<_>>();

    for item in &sync_state.catalog {
        let repo_key = make_repo_key(&item.source_id, &item.rel_path);
        let ever_installed = existing_remote_usage
            .get(&repo_key)
            .copied()
            .unwrap_or(false);
        let should_track = ever_installed
            || installed_keys.contains(&repo_key)
            || installed_skill_ids.contains(&item.id);
        if !should_track {
            continue;
        }
        tracked_remote_keys.insert(repo_key.clone());

        let source = get_source(cfg, &item.source_id);
        if let Some(src_cfg) = source {
            if let Ok(source_path) = source_skill_abs_path(src_cfg, &item.rel_path) {
                if source_path.join("SKILL.md").exists() {
                    let dir_name = read_required_skill_dir_name(&source_path)
                        .unwrap_or_else(|_| item.id.clone());
                    let _ = upsert_repository_from_dir(
                        state,
                        &source_path,
                        &item.source_id,
                        &item.rel_path,
                        &item.id,
                        &dir_name,
                        "remote",
                        &item.name,
                        &item.description,
                        &item.models,
                        &item.icon_seed,
                        Some(source_path.to_string_lossy().to_string()),
                        Some(item.remote_hash.clone()),
                        ever_installed,
                    );
                    continue;
                }
            }
        }

        let existing_created_at = state
            .repositories
            .iter()
            .find(|r| r.repo_key == repo_key)
            .map(|r| r.created_at)
            .unwrap_or(0);
        upsert_repository_record(
            &mut state.repositories,
            RepositoryRecord {
                repo_key: repo_key.clone(),
                skill_id: item.id.clone(),
                dir_name: item.id.clone(),
                source_id: item.source_id.clone(),
                source_rel_path: item.rel_path.clone(),
                source_type: "remote".to_string(),
                source_path: None,
                name: item.name.clone(),
                description: item.description.clone(),
                models: item.models.clone(),
                icon_seed: item.icon_seed.clone(),
                hash: Some(item.remote_hash.clone()),
                created_at: if existing_created_at > 0 {
                    existing_created_at
                } else {
                    now_ts()
                },
                updated_at: Some(now_ts()),
                ever_installed,
            },
        );
    }

    state.repositories.retain(|repo| {
        if repo.source_type != "remote" {
            return true;
        }
        tracked_remote_keys.contains(&repo.repo_key)
            || installed_keys.contains(&repo.repo_key)
            || repo.ever_installed
    });

    Ok(())
}

#[tauri::command]
pub fn skills_config_get() -> Result<ApiOk<SkillsConfigPayload>, String> {
    let cfg = config::get_storage_config()?;
    let payload = SkillsConfigPayload {
        skills_sync_enabled: cfg.skills_sync_enabled.unwrap_or(true),
        skills_sync_interval_minutes: cfg.skills_sync_interval_minutes.unwrap_or(60).max(5),
        skills_new_badge_hours: cfg.skills_new_badge_hours.unwrap_or(72).clamp(1, 720),
        skills_sources: cfg.skills_sources,
    };
    let state = load_skills_state()?;
    api_ok(payload, state.revision)
}

#[tauri::command]
pub async fn skills_config_save(
    app: tauri::AppHandle,
    config_payload: SkillsConfigPayload,
) -> Result<ApiOk<SkillsConfigPayload>, String> {
    {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let mut cfg = config::get_storage_config()?;
        cfg.skills_sync_enabled = Some(config_payload.skills_sync_enabled);
        cfg.skills_sync_interval_minutes = Some(config_payload.skills_sync_interval_minutes.max(5));
        cfg.skills_new_badge_hours = Some(config_payload.skills_new_badge_hours.clamp(1, 720));
        cfg.skills_sources = config_payload.skills_sources.clone();
        drop(_guard);
        config::save_storage_config(app.clone(), cfg).await?;
    }
    let state = load_skills_state()?;
    api_ok(config_payload, state.revision)
}

#[tauri::command]
pub fn skills_sources_export_to_path(
    output_path: String,
    skills_sources: Vec<SkillSourceConfig>,
) -> Result<String, String> {
    let payload = SkillsSourcesExportPayload {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        skills_sources,
    };
    let content = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    let path = PathBuf::from(&output_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(output_path)
}

#[tauri::command]
pub fn skills_list_installed(
    model: Option<String>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let list_scope = normalize_install_scope(scope.as_deref());
    let list_project_root = normalize_project_root_for_scope(&list_scope, project_root.as_deref())?;
    let lock_guard = match job_lock().try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Poisoned(err)) => return Err(err.to_string()),
    };
    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;

    if lock_guard.is_some() {
        let cfg = config::get_storage_config()?;
        let (shared_changed, migrated_local_changed) =
            migrate_installed_dir_names(&mut shared_state, &mut local_state)?;
        let refreshed_local_changed = if model.is_some() {
            refresh_local_hashes(&mut local_state, model.as_deref(), &cfg)?
        } else {
            false
        };
        let local_changed = migrated_local_changed || refreshed_local_changed;
        if shared_changed {
            shared_state = save_skills_state(shared_state)?;
        }
        if local_changed {
            local_state = save_local_skills_state(local_state)?;
        }
    }

    let mut list = local_state
        .skills
        .iter()
        .filter(|s| model.as_ref().map(|m| m == &s.model).unwrap_or(true))
        .filter(|s| scope_project_match(s, &list_scope, list_project_root.as_deref()))
        .cloned()
        .collect::<Vec<_>>();
    if model.is_some() {
        if let Ok(cfg) = config::get_storage_config() {
            for skill in &mut list {
                skill.has_update = skill_has_markdown_update(skill, &cfg).unwrap_or(false);
            }
        }
    }
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_list_catalog(model: Option<String>) -> Result<ApiOk<Vec<CatalogSkill>>, String> {
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let source_allow_map: HashMap<String, Vec<String>> = cfg
        .skills_sources
        .iter()
        .map(|s| (s.id.clone(), normalize_models(&s.default_models)))
        .collect();
    let requested_model = model.as_ref().and_then(|m| normalized_model(m));
    if model.is_some() && requested_model.is_none() {
        let revision = load_skills_state()?.revision;
        return api_ok(Vec::<CatalogSkill>::new(), revision);
    }
    let list = sync_state
        .catalog
        .iter()
        .filter_map(|s| {
            let source_allowed = source_allow_map.get(&s.source_id)?;
            let effective_models = resolve_effective_models(&s.models, source_allowed);
            if effective_models.is_empty() {
                return None;
            }
            if let Some(target) = requested_model.as_ref() {
                if !effective_models.contains(target) {
                    return None;
                }
            }
            let mut entry = s.clone();
            entry.models = effective_models;
            Some(entry)
        })
        .collect::<Vec<_>>();
    let revision = load_skills_state()?.revision;
    api_ok(list, revision)
}

#[tauri::command]
pub async fn skills_sync_now(app: tauri::AppHandle) -> Result<ApiOk<SkillsSyncState>, String> {
    tauri::async_runtime::spawn_blocking(move || skills_sync_now_blocking(app))
        .await
        .map_err(|e| e.to_string())?
}

fn skills_sync_now_blocking(app: tauri::AppHandle) -> Result<ApiOk<SkillsSyncState>, String> {
    let _job = match acquire_job_key("sync:all")? {
        Some(v) => v,
        None => {
            let sync_state = load_sync_state()?;
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let revision = combined_revision(&shared_state, &local_state);
            return api_ok(sync_state, revision);
        }
    };

    let (cfg, previous_sync_state) = {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let cfg = config::get_storage_config()?;
        let previous_sync_state = load_sync_state()?;
        let mut in_progress_sync_state = previous_sync_state.clone();
        in_progress_sync_state.status = "fetching_source".to_string();
        in_progress_sync_state.last_error = None;
        // Persist early so frontend can immediately detect in-progress sync.
        save_sync_state(&in_progress_sync_state)?;
        (cfg, previous_sync_state)
    };

    let mut next_catalog = vec![];
    let mut next_sources = vec![];
    let mut sync_last_error = None;
    for source in &cfg.skills_sources {
        let _source_job = match acquire_job_key(format!("sync:{}", source.id))? {
            Some(v) => Some(v),
            None => {
                let prev = previous_sync_state
                    .sources
                    .iter()
                    .find(|s| s.source_id == source.id)
                    .cloned()
                    .unwrap_or_default();
                next_sources.push(SourceSyncState {
                    source_id: source.id.clone(),
                    last_synced_at: prev.last_synced_at,
                    last_commit_sha: prev.last_commit_sha,
                    last_status: "skipped_busy".to_string(),
                    last_error: None,
                });
                None
            }
        };
        if _source_job.is_none() {
            continue;
        }

        if !source.enabled {
            let prev = previous_sync_state
                .sources
                .iter()
                .find(|s| s.source_id == source.id)
                .cloned()
                .unwrap_or_default();
            next_sources.push(SourceSyncState {
                source_id: source.id.clone(),
                last_synced_at: prev.last_synced_at,
                last_commit_sha: prev.last_commit_sha,
                last_status: "skipped".to_string(),
                last_error: None,
            });
            continue;
        }

        let mut retry = 0;
        let mut ok = false;
        let mut last_err = None;
        let mut commit = None;
        let mut indexed: Vec<CatalogSkill> = vec![];
        while retry < 5 {
            let sync_one = || -> Result<(String, Vec<CatalogSkill>), String> {
                let repo_dir = sync_source_repo(source)?;
                let current_commit = git_run(Some(&repo_dir), &["rev-parse", "HEAD"])?;
                let prev_commit = previous_sync_state
                    .sources
                    .iter()
                    .find(|s| s.source_id == source.id)
                    .and_then(|s| s.last_commit_sha.clone());

                if prev_commit.as_deref() == Some(current_commit.as_str()) {
                    let reused = previous_sync_state
                        .catalog
                        .iter()
                        .filter(|c| c.source_id == source.id)
                        .cloned()
                        .collect::<Vec<_>>();
                    Ok((current_commit, reused))
                } else {
                    let previous_source_catalog = previous_sync_state
                        .catalog
                        .iter()
                        .filter(|c| c.source_id == source.id)
                        .cloned()
                        .collect::<Vec<_>>();
                    let scanned = scan_source_catalog(&repo_dir, source)?;
                    let scanned = assign_catalog_first_seen(&previous_source_catalog, scanned);
                    Ok((current_commit, scanned))
                }
            };

            match sync_one() {
                Ok((c, list)) => {
                    commit = Some(c.clone());
                    indexed = list;
                    ok = true;
                    break;
                }
                Err(err) => {
                    last_err = Some(err);
                    retry += 1;
                    std::thread::sleep(std::time::Duration::from_secs(2u64.pow(retry)));
                }
            }
        }

        if ok {
            let status = if previous_sync_state
                .sources
                .iter()
                .find(|s| s.source_id == source.id)
                .and_then(|s| s.last_commit_sha.clone())
                .as_deref()
                == commit.as_deref()
            {
                "done_no_change"
            } else {
                "done"
            };
            next_catalog.extend(indexed);
            next_sources.push(SourceSyncState {
                source_id: source.id.clone(),
                last_synced_at: Some(now_ts()),
                last_commit_sha: commit,
                last_status: status.to_string(),
                last_error: None,
            });
        } else {
            next_sources.push(SourceSyncState {
                source_id: source.id.clone(),
                last_synced_at: Some(now_ts()),
                last_commit_sha: None,
                last_status: "error".to_string(),
                last_error: last_err.clone(),
            });
            sync_last_error = last_err;
        }
    }

    let (sync_state, revision, cfg_save) = {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let mut sync_state = load_sync_state()?;
        sync_state.status = if sync_last_error.is_some() {
            "error".to_string()
        } else {
            "done".to_string()
        };
        sync_state.last_error = sync_last_error;
        sync_state.last_sync_at = Some(now_ts());
        sync_state.catalog = next_catalog;
        sync_state.sources = next_sources;
        save_sync_state(&sync_state)?;

        let mut shared_state = load_skills_state()?;
        let mut local_state = load_local_skills_state()?;
        rebuild_local_installed_from_models(&mut local_state)?;
        hydrate_local_records_from_catalog(&mut local_state, &sync_state);
        update_record_remote_flags(&mut local_state, &sync_state, &cfg);
        refresh_remote_repositories_from_catalog(
            &mut shared_state,
            &local_state,
            &sync_state,
            &cfg,
        )?;
        let _ = refresh_repository_metadata_from_snapshots(&mut shared_state, &cfg);
        shared_state = save_skills_state(shared_state)?;
        local_state = save_local_skills_state(local_state)?;

        let mut cfg_save = config::get_storage_config()?;
        touch_sync_timestamp(&mut cfg_save);

        (
            sync_state,
            combined_revision(&shared_state, &local_state),
            cfg_save,
        )
    };

    tauri::async_runtime::block_on(config::save_storage_config(app.clone(), cfg_save))?;
    trigger_storage_sync(app, "skills_sync_now");

    api_ok(sync_state, revision)
}

#[tauri::command]
pub fn skills_sync_status_get() -> Result<ApiOk<SkillsSyncState>, String> {
    let sync_state = load_sync_state()?;
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(sync_state, revision)
}

#[tauri::command]
pub fn skills_repo_list(
    include_update: Option<bool>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<RepositorySkillView>>, String> {
    let repo_scope = normalize_install_scope(scope.as_deref());
    let repo_project_root = normalize_project_root_for_scope(&repo_scope, project_root.as_deref())?;
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let installed = scoped_installed_skills(
        &local_state.skills,
        &repo_scope,
        repo_project_root.as_deref(),
    );
    let list = build_repository_views(&shared_state, &installed, include_update.unwrap_or(false));
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_repo_list_with_update(
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<RepositorySkillView>>, String> {
    let repo_scope = normalize_install_scope(scope.as_deref());
    let repo_project_root = normalize_project_root_for_scope(&repo_scope, project_root.as_deref())?;
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let installed = scoped_installed_skills(
        &local_state.skills,
        &repo_scope,
        repo_project_root.as_deref(),
    );
    let list = build_repository_views(&shared_state, &installed, true);
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_repo_refresh(
    app: tauri::AppHandle,
) -> Result<ApiOk<Vec<RepositorySkillView>>, String> {
    let _ = skills_sync_now(app.clone()).await?;
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let list = build_repository_views(&shared_state, &local_state.skills, true);
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_repo_refresh_background(app: tauri::AppHandle) -> Result<ApiOk<bool>, String> {
    std::thread::spawn(move || {
        let _ = tauri::async_runtime::block_on(skills_repo_refresh(app));
    });
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    api_ok(true, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_repo_set_model(
    app: tauri::AppHandle,
    input: RepoSetModelInput,
) -> Result<ApiOk<RepositorySkillView>, String> {
    if !MODELS.contains(&input.model.as_str()) {
        return Err("unsupported model".to_string());
    }

    let repo_scope = normalize_install_scope(input.scope.as_deref());
    let repo_project_root =
        normalize_project_root_for_scope(&repo_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "repo_set:{}:{}:{}:{}:{}",
        input.repo_key,
        input.model,
        input.enabled,
        repo_scope,
        repo_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let installed = scoped_installed_skills(
                &local_state.skills,
                &repo_scope,
                repo_project_root.as_deref(),
            );
            let view = build_repository_views(&shared_state, &installed, false)
                .into_iter()
                .find(|v| v.repo_key == input.repo_key)
                .ok_or("repo skill not found")?;
            return api_ok(view, combined_revision(&shared_state, &local_state));
        }
    };

    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo skill not found")?;
    let mut shared_changed = false;
    let mut local_changed = false;

    let repo_src = repo_storage_dir(&repo.repo_key)?;
    if input.enabled && !repo_src.exists() {
        if repo.source_type == "remote" {
            let cfg = config::get_storage_config()?;
            let source = get_source(&cfg, &repo.source_id).ok_or("source not found")?;
            let source_path = source_skill_abs_path(source, &repo.source_rel_path)?;
            if !source_path.join("SKILL.md").exists() {
                return Err("skills/invalid_skill_dir".to_string());
            }
            let dir_name = read_required_skill_dir_name(&source_path)?;
            let _ = upsert_repository_from_dir(
                &mut shared_state,
                &source_path,
                &repo.source_id,
                &repo.source_rel_path,
                &repo.skill_id,
                &dir_name,
                &repo.source_type,
                &repo.name,
                &repo.description,
                &repo.models,
                &repo.icon_seed,
                Some(source_path.to_string_lossy().to_string()),
                repo.hash.clone(),
                true,
            )?;
            shared_changed = true;
        } else {
            return Err("repository_snapshot_missing".to_string());
        }
    }

    if input.enabled {
        let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
        let repo_dir_name = read_required_skill_dir_name(&repo_snapshot)?;
        shared_changed = upsert_repo_dir_name(
            &mut shared_state,
            &repo.source_id,
            &repo.source_rel_path,
            &repo.skill_id,
            &repo_dir_name,
        ) || shared_changed;
        shared_changed =
            mark_repo_ever_installed(&mut shared_state, &repo.repo_key) || shared_changed;
        ensure_model_dir_name_available(
            &local_state,
            &input.model,
            &repo_scope,
            repo_project_root.as_deref(),
            &repo_dir_name,
            Some(repo.skill_id.as_str()),
        )?;
        let (model_root, compat_roots) =
            resolve_skill_target_dir(&input.model, &repo_scope, repo_project_root.as_deref())?;
        let dest = model_root.join(&repo_dir_name);
        ensure_within(&model_root, &dest)?;
        remove_existing_record_dir_if_moved(
            &local_state,
            &input.model,
            &repo_scope,
            repo_project_root.as_deref(),
            &repo.skill_id,
            &dest,
        )?;
        let src = repo_storage_dir(&repo.repo_key)?;
        replace_dir_atomic(&src, &dest)?;
        for compat_root in compat_roots {
            let compat_dest = compat_root.join(&repo_dir_name);
            ensure_within(&compat_root, &compat_dest)?;
            replace_dir_atomic(&dest, &compat_dest)?;
        }
        let local_hash = hash_dir(&dest)?;
        local_state.skills.retain(|s| {
            !(s.model == input.model
                && s.id == repo.skill_id
                && scope_project_match(s, &repo_scope, repo_project_root.as_deref()))
        });
        local_state.skills.push(SkillRecord {
            id: repo.skill_id.clone(),
            dir_name: repo_dir_name,
            model: input.model.clone(),
            models: repo.models.clone(),
            name: repo.name.clone(),
            description: repo.description.clone(),
            source_id: repo.source_id.clone(),
            source_rel_path: repo.source_rel_path.clone(),
            installed_at: now_ts(),
            updated_at: None,
            last_synced_at: shared_state.last_sync_at,
            local_hash,
            remote_hash: repo.hash.clone(),
            has_update: false,
            icon_seed: repo.icon_seed.clone(),
            scope: repo_scope.clone(),
            project_root: repo_project_root.clone(),
            target_path: Some(dest.to_string_lossy().to_string()),
        });
        local_changed = true;
    } else {
        let (_, compat_roots) =
            resolve_skill_target_dir(&input.model, &repo_scope, repo_project_root.as_deref())?;
        let records_to_remove = local_state
            .skills
            .iter()
            .filter(|s| {
                s.model == input.model
                    && scope_project_match(s, &repo_scope, repo_project_root.as_deref())
                    && (s.id == repo.skill_id
                        || make_repo_key(&s.source_id, &s.source_rel_path) == repo.repo_key)
            })
            .cloned()
            .collect::<Vec<_>>();
        for record in records_to_remove {
            let dest = locate_existing_record_local_dir(&record)?;
            if dest.exists() {
                fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
            }
            let dir_name = normalized_record_dir_name(&record);
            for compat_root in &compat_roots {
                let compat = compat_root.join(&dir_name);
                let _ = ensure_within(compat_root, &compat);
                if compat.exists() {
                    let _ = fs::remove_dir_all(&compat);
                }
            }
        }
        let before = local_state.skills.len();
        local_state.skills.retain(|s| {
            !(s.model == input.model
                && scope_project_match(s, &repo_scope, repo_project_root.as_deref())
                && (s.id == repo.skill_id
                    || make_repo_key(&s.source_id, &s.source_rel_path) == repo.repo_key))
        });
        local_changed = local_changed || before != local_state.skills.len();
    }

    if shared_changed {
        shared_state = save_skills_state(shared_state)?;
    }
    if local_changed {
        local_state = save_local_skills_state(local_state)?;
    }
    let _ = reconcile_internal(
        Some(&input.model),
        Some(repo_scope.as_str()),
        repo_project_root.as_deref(),
    );
    if shared_changed {
        trigger_storage_sync(app, "skills_repo_set_model");
    }

    let installed = scoped_installed_skills(
        &local_state.skills,
        &repo_scope,
        repo_project_root.as_deref(),
    );
    let view = build_repository_views(&shared_state, &installed, false)
        .into_iter()
        .find(|v| v.repo_key == input.repo_key)
        .ok_or("repo skill not found")?;
    api_ok(view, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_repo_delete(
    app: tauri::AppHandle,
    input: RepoSkillKeyInput,
) -> Result<ApiOk<bool>, String> {
    let dedupe_key = format!("repo_delete:{}", input.repo_key);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            return api_ok(true, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned();

    let in_use = local_state.skills.iter().any(|s| {
        make_repo_key(&s.source_id, &s.source_rel_path) == input.repo_key
            || repo.as_ref().map(|r| s.id == r.skill_id).unwrap_or(false)
    });
    if in_use {
        return Err("skills/repo_in_use".to_string());
    }

    let before = shared_state.repositories.len();
    shared_state
        .repositories
        .retain(|r| r.repo_key != input.repo_key);
    let changed = before != shared_state.repositories.len();

    if changed {
        let repo_src = repo_storage_dir(&input.repo_key)?;
        if repo_src.exists() {
            fs::remove_dir_all(&repo_src).map_err(|e| e.to_string())?;
        }
        shared_state = save_skills_state(shared_state)?;
        trigger_storage_sync(app, "skills_repo_delete");
    }

    api_ok(true, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_local_scan(input: LocalScanInput) -> Result<ApiOk<Vec<LocalSkillCandidate>>, String> {
    let root_can = resolve_scan_root(&input.root_path)?;
    let list = scan_local_candidates(&root_can)?;
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(list, revision)
}

#[tauri::command]
pub async fn skills_repo_import_folder(
    app: tauri::AppHandle,
    input: RepoImportFolderInput,
) -> Result<ApiOk<RepoImportFolderResult>, String> {
    let folder_can = resolve_scan_root(&input.folder_path)?;
    let dedupe_key = format!(
        "repo_import_folder:{}",
        sha256_hex(&folder_can.to_string_lossy())
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            return Err("skills/import_busy".to_string());
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let skill_md = folder_can.join("SKILL.md");
    if !skill_md.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let md_content = fs::read_to_string(&skill_md).map_err(|e| e.to_string())?;
    let dir_name = read_required_skill_dir_name(&folder_can)?;
    let (name, description, declared_models) = parse_skill_md(&md_content, &[]);
    let source_id = local_source_id(&folder_can);
    let source_rel_path = ".".to_string();
    let skill_id = local_skill_id(&source_id, &source_rel_path);

    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let record = upsert_repository_from_dir(
        &mut shared_state,
        &folder_can,
        &source_id,
        &source_rel_path,
        &skill_id,
        &dir_name,
        "local_import",
        &name,
        &description,
        &declared_models,
        &source_id,
        Some(folder_can.to_string_lossy().to_string()),
        None,
        false,
    )?;
    let _ = upsert_repo_dir_name(
        &mut shared_state,
        &source_id,
        &source_rel_path,
        &skill_id,
        &dir_name,
    );
    shared_state = save_skills_state(shared_state)?;
    trigger_storage_sync(app, "skills_repo_import_folder");

    let result = RepoImportFolderResult {
        repo_key: record.repo_key,
        skill_id: record.skill_id,
        source_id: record.source_id,
        source_rel_path: record.source_rel_path,
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_local_import(
    app: tauri::AppHandle,
    input: LocalImportInput,
) -> Result<ApiOk<LocalImportResult>, String> {
    let root_can = resolve_scan_root(&input.root_path)?;
    let source_id = local_source_id(&root_can);
    let dedupe_key = format!("local_import:{}", source_id);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            return api_ok(
                LocalImportResult::default(),
                combined_revision(&shared_state, &local_state),
            );
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut models = vec![];
    let mut model_seen = HashSet::new();
    for model in &input.models {
        if !MODELS.contains(&model.as_str()) {
            return Err(format!("unsupported model: {}", model));
        }
        if model_seen.insert(model.clone()) {
            models.push(model.clone());
        }
    }
    if models.is_empty() {
        return Err("skills/models_required".to_string());
    }
    if input.selections.is_empty() {
        return Err("skills/selections_required".to_string());
    }

    let candidates = scan_local_candidates(&root_can)?;
    let mut candidate_map: HashMap<String, LocalSkillCandidate> = HashMap::new();
    for c in candidates {
        candidate_map.insert(c.rel_path.clone(), c);
    }

    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let mut result = LocalImportResult::default();
    let mut shared_changed = false;
    let mut local_changed = false;

    for selection in &input.selections {
        let strategy = selection.conflict_strategy.trim().to_lowercase();
        if strategy != "overwrite" && strategy != "skip" {
            for model in &models {
                result.failed.push(LocalImportFailed {
                    rel_path: selection.rel_path.clone(),
                    skill_id: None,
                    model: model.clone(),
                    reason: "invalid_conflict_strategy".to_string(),
                });
            }
            continue;
        }

        let Some(candidate) = candidate_map.get(&selection.rel_path) else {
            for model in &models {
                result.failed.push(LocalImportFailed {
                    rel_path: selection.rel_path.clone(),
                    skill_id: None,
                    model: model.clone(),
                    reason: "skill_not_found".to_string(),
                });
            }
            continue;
        };

        let src = if candidate.rel_path == "." {
            root_can.clone()
        } else {
            root_can.join(&candidate.rel_path)
        };
        if !src.join("SKILL.md").exists() {
            for model in &models {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: "skills/invalid_skill_dir".to_string(),
                });
            }
            continue;
        }
        let candidate_dir_name = match read_required_skill_dir_name(&src) {
            Ok(name) => name,
            Err(err) => {
                for model in &models {
                    result.failed.push(LocalImportFailed {
                        rel_path: candidate.rel_path.clone(),
                        skill_id: Some(candidate.skill_id.clone()),
                        model: model.clone(),
                        reason: err.clone(),
                    });
                }
                continue;
            }
        };

        let repo_key = make_repo_key(&source_id, &candidate.rel_path);
        let repo_exists = shared_state
            .repositories
            .iter()
            .any(|r| r.repo_key == repo_key);
        let repo_record = match upsert_repository_from_dir(
            &mut shared_state,
            &src,
            &source_id,
            &candidate.rel_path,
            &candidate.skill_id,
            &candidate_dir_name,
            "local_import",
            &candidate.name,
            &candidate.description,
            &candidate.declared_models,
            &source_id,
            Some(src.to_string_lossy().to_string()),
            None,
            true,
        ) {
            Ok(v) => v,
            Err(err) => {
                for model in &models {
                    result.failed.push(LocalImportFailed {
                        rel_path: candidate.rel_path.clone(),
                        skill_id: Some(candidate.skill_id.clone()),
                        model: model.clone(),
                        reason: err.clone(),
                    });
                }
                continue;
            }
        };
        shared_changed = true;
        shared_changed = upsert_repo_dir_name(
            &mut shared_state,
            &source_id,
            &candidate.rel_path,
            &candidate.skill_id,
            &candidate_dir_name,
        ) || shared_changed;
        if !repo_exists {
            result.repo_added.push(LocalImportRepoAdded {
                repo_key: repo_record.repo_key.clone(),
                skill_id: repo_record.skill_id.clone(),
                source_id: repo_record.source_id.clone(),
                source_rel_path: repo_record.source_rel_path.clone(),
            });
        }

        let repo_src = repo_storage_dir(&repo_record.repo_key)?;
        for model in &models {
            let model_root = model_dir(model)?;
            let dest = model_root.join(&candidate_dir_name);
            ensure_within(&model_root, &dest)?;
            let existing_same_id = local_state
                .skills
                .iter()
                .any(|s| s.model == *model && s.id == candidate.skill_id);
            if strategy == "skip" && existing_same_id {
                result.skipped.push(LocalImportSkipped {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: candidate.skill_id.clone(),
                    model: model.clone(),
                    reason: "conflict_exists".to_string(),
                });
                continue;
            }
            if let Err(err) = ensure_model_dir_name_available(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &candidate_dir_name,
                Some(candidate.skill_id.as_str()),
            ) {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: err,
                });
                continue;
            }
            if let Err(err) = remove_existing_record_dir_if_moved(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &candidate.skill_id,
                &dest,
            ) {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: err,
                });
                continue;
            }

            if let Err(err) = replace_dir_atomic(&repo_src, &dest) {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: err,
                });
                continue;
            }

            let local_hash = match hash_dir(&dest) {
                Ok(hash) => hash,
                Err(err) => {
                    result.failed.push(LocalImportFailed {
                        rel_path: candidate.rel_path.clone(),
                        skill_id: Some(candidate.skill_id.clone()),
                        model: model.clone(),
                        reason: err,
                    });
                    continue;
                }
            };

            local_state.skills.retain(|s| {
                !(s.model == *model
                    && s.id == candidate.skill_id
                    && record_scope(s) == INSTALL_SCOPE_GLOBAL)
            });
            let record = SkillRecord {
                id: candidate.skill_id.clone(),
                dir_name: candidate_dir_name.clone(),
                model: model.clone(),
                models: candidate.declared_models.clone(),
                name: candidate.name.clone(),
                description: candidate.description.clone(),
                source_id: source_id.clone(),
                source_rel_path: candidate.rel_path.clone(),
                installed_at: now_ts(),
                updated_at: None,
                last_synced_at: None,
                local_hash,
                remote_hash: None,
                has_update: false,
                icon_seed: source_id.clone(),
                scope: INSTALL_SCOPE_GLOBAL.to_string(),
                project_root: None,
                target_path: Some(dest.to_string_lossy().to_string()),
            };
            local_state.skills.push(record.clone());
            result.installed.push(record);
            local_changed = true;
        }
    }

    if shared_changed {
        shared_state = save_skills_state(shared_state)?;
    }
    if local_changed {
        local_state = save_local_skills_state(local_state)?;
    }
    for model in &models {
        let _ = reconcile_internal(Some(model), Some(INSTALL_SCOPE_GLOBAL), None);
    }
    if shared_changed {
        trigger_storage_sync(app, "skills_local_import");
    }
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_install(
    app: tauri::AppHandle,
    input: InstallInput,
) -> Result<ApiOk<SkillRecord>, String> {
    let install_scope = normalize_install_scope(input.scope.as_deref());
    let install_project_root =
        normalize_project_root_for_scope(&install_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "install:{}:{}:{}:{}",
        input.model,
        input.skill_ref,
        install_scope,
        install_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            if let Some(found) = state
                .skills
                .iter()
                .find(|s| {
                    s.model == input.model
                        && s.source_id == input.source_id
                        && scope_project_match(s, &install_scope, install_project_root.as_deref())
                })
                .cloned()
            {
                return api_ok(found, state.revision);
            }
            return Err("duplicate job skipped".to_string());
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    if !MODELS.contains(&input.model.as_str()) {
        return Err("unsupported model".to_string());
    }

    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
    let sync_state = load_sync_state()?;
    let catalog = sync_state
        .catalog
        .iter()
        .find(|c| {
            c.source_id == input.source_id
                && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
        })
        .cloned()
        .ok_or("catalog skill not found")?;
    let allowed_models = resolve_effective_models(&catalog.models, &source.default_models);
    if !allowed_models.contains(&input.model) {
        return Err("skills/model_not_allowed".to_string());
    }

    let src = source_skill_abs_path(source, &catalog.rel_path)?;
    if !src.join("SKILL.md").exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let catalog_dir_name = read_required_skill_dir_name(&src)?;

    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let expected_repo_key = make_repo_key(&catalog.source_id, &catalog.rel_path);
    let existing_repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == expected_repo_key)
        .cloned();
    let mut shared_changed = false;
    let repo_record = if let Some(existing) = existing_repo {
        let repo_src = repo_storage_dir(&existing.repo_key)?;
        if repo_src.exists() {
            existing
        } else {
            shared_changed = true;
            upsert_repository_from_dir(
                &mut shared_state,
                &src,
                &catalog.source_id,
                &catalog.rel_path,
                &catalog.id,
                &catalog_dir_name,
                "remote",
                &catalog.name,
                &catalog.description,
                &allowed_models,
                &catalog.icon_seed,
                Some(src.to_string_lossy().to_string()),
                Some(catalog.remote_hash.clone()),
                true,
            )?
        }
    } else {
        shared_changed = true;
        upsert_repository_from_dir(
            &mut shared_state,
            &src,
            &catalog.source_id,
            &catalog.rel_path,
            &catalog.id,
            &catalog_dir_name,
            "remote",
            &catalog.name,
            &catalog.description,
            &allowed_models,
            &catalog.icon_seed,
            Some(src.to_string_lossy().to_string()),
            Some(catalog.remote_hash.clone()),
            true,
        )?
    };
    shared_changed =
        mark_repo_ever_installed(&mut shared_state, &repo_record.repo_key) || shared_changed;
    shared_changed = upsert_repo_dir_name(
        &mut shared_state,
        &repo_record.source_id,
        &repo_record.source_rel_path,
        &repo_record.skill_id,
        &catalog_dir_name,
    ) || shared_changed;

    ensure_model_dir_name_available(
        &local_state,
        &input.model,
        &install_scope,
        install_project_root.as_deref(),
        &catalog_dir_name,
        Some(repo_record.skill_id.as_str()),
    )?;
    let (model_root, compat_roots) = resolve_skill_target_dir(
        &input.model,
        &install_scope,
        install_project_root.as_deref(),
    )?;
    let dest = model_root.join(&catalog_dir_name);
    ensure_within(&model_root, &dest)?;
    remove_existing_record_dir_if_moved(
        &local_state,
        &input.model,
        &install_scope,
        install_project_root.as_deref(),
        &repo_record.skill_id,
        &dest,
    )?;
    let repo_src = repo_storage_dir(&repo_record.repo_key)?;
    replace_dir_atomic(&repo_src, &dest)?;

    let local_hash = hash_dir(&dest)?;
    local_state.skills.retain(|s| {
        !(s.model == input.model
            && s.id == repo_record.skill_id
            && scope_project_match(s, &install_scope, install_project_root.as_deref()))
    });

    let now = now_ts();
    let record = SkillRecord {
        id: repo_record.skill_id.clone(),
        dir_name: catalog_dir_name.clone(),
        model: input.model.clone(),
        models: allowed_models,
        name: catalog.name.clone(),
        description: catalog.description.clone(),
        source_id: repo_record.source_id.clone(),
        source_rel_path: repo_record.source_rel_path.clone(),
        installed_at: now,
        updated_at: None,
        last_synced_at: sync_state.last_sync_at,
        local_hash,
        remote_hash: repo_record.hash.clone(),
        has_update: false,
        icon_seed: repo_record.icon_seed.clone(),
        scope: install_scope.clone(),
        project_root: install_project_root.clone(),
        target_path: Some(dest.to_string_lossy().to_string()),
    };

    local_state.skills.push(record.clone());
    for compat_root in compat_roots {
        let compat_dest = compat_root.join(&catalog_dir_name);
        ensure_within(&compat_root, &compat_dest)?;
        replace_dir_atomic(&dest, &compat_dest)?;
    }
    if shared_changed {
        shared_state = save_skills_state(shared_state)?;
    }
    local_state = save_local_skills_state(local_state)?;

    let _ = reconcile_internal(
        Some(&input.model),
        Some(install_scope.as_str()),
        install_project_root.as_deref(),
    );
    if shared_changed {
        trigger_storage_sync(app, "skills_install");
    }
    api_ok(record, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_uninstall(
    _app: tauri::AppHandle,
    input: SkillKeyInput,
) -> Result<ApiOk<bool>, String> {
    let uninstall_scope = normalize_install_scope(input.scope.as_deref());
    let uninstall_project_root =
        normalize_project_root_for_scope(&uninstall_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "uninstall:{}:{}:{}:{}",
        input.model,
        input.skill_id,
        uninstall_scope,
        uninstall_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_local_skills_state()?;
    if let Some(record) = state
        .skills
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &uninstall_scope, uninstall_project_root.as_deref())
        })
        .cloned()
    {
        let local = locate_existing_record_local_dir(&record)?;
        let (root, compat_roots) = resolve_skill_target_dir(
            &input.model,
            &uninstall_scope,
            uninstall_project_root.as_deref(),
        )?;
        ensure_within(&root, &local)?;
        if local.exists() {
            fs::remove_dir_all(&local).map_err(|e| e.to_string())?;
        }
        let dir_name = normalized_record_dir_name(&record);
        for compat_root in compat_roots {
            let compat_path = compat_root.join(&dir_name);
            let _ = ensure_within(&compat_root, &compat_path);
            if compat_path.exists() {
                let _ = fs::remove_dir_all(&compat_path);
            }
        }
    }
    state.skills.retain(|s| {
        !(s.model == input.model
            && s.id == input.skill_id
            && scope_project_match(s, &uninstall_scope, uninstall_project_root.as_deref()))
    });
    let state = save_local_skills_state(state)?;

    let _ = reconcile_internal(
        Some(&input.model),
        Some(uninstall_scope.as_str()),
        uninstall_project_root.as_deref(),
    );
    api_ok(true, state.revision)
}

#[tauri::command]
pub fn skills_detail_get(input: SkillKeyInput) -> Result<ApiOk<SkillDetail>, String> {
    let detail_scope = normalize_install_scope(input.scope.as_deref());
    let detail_project_root =
        normalize_project_root_for_scope(&detail_scope, input.project_root.as_deref())?;
    let state = load_local_skills_state()?;
    let record = state
        .skills
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &detail_scope, detail_project_root.as_deref())
        })
        .cloned()
        .ok_or("skill not found")?;
    let local = record_local_dir(&record)?;
    let markdown = fs::read_to_string(local.join("SKILL.md")).unwrap_or_default();
    let detail = SkillDetail {
        skill: record,
        markdown,
        local_path: local.to_string_lossy().to_string(),
    };
    api_ok(detail, state.revision)
}

#[tauri::command]
pub fn skills_catalog_detail_get(
    input: CatalogSkillKeyInput,
) -> Result<ApiOk<CatalogSkillDetail>, String> {
    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
    let sync_state = load_sync_state()?;
    let mut catalog = sync_state
        .catalog
        .iter()
        .find(|c| {
            c.source_id == input.source_id
                && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
        })
        .cloned()
        .ok_or("catalog skill not found")?;
    let effective_models = resolve_effective_models(&catalog.models, &source.default_models);
    if effective_models.is_empty() {
        return Err("catalog skill not found".to_string());
    }
    catalog.models = effective_models;
    let source_path = source_skill_abs_path(source, &catalog.rel_path)?;
    let markdown = fs::read_to_string(source_path.join("SKILL.md")).unwrap_or_default();
    let detail = CatalogSkillDetail {
        skill: catalog,
        markdown,
        source_path: source_path.to_string_lossy().to_string(),
    };
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(detail, revision)
}

#[tauri::command]
pub fn skills_repo_detail_get(
    input: RepoSkillKeyInput,
) -> Result<ApiOk<CatalogSkillDetail>, String> {
    let mut state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut state, &local_state, &cfg)? {
        state = save_skills_state(state)?;
    }
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo skill not found")?;

    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    let mut markdown = String::new();
    let mut source_path = repo_snapshot.to_string_lossy().to_string();

    if repo_snapshot.join("SKILL.md").exists() {
        markdown = fs::read_to_string(repo_snapshot.join("SKILL.md")).unwrap_or_default();
    } else if let Some(src) = repo.source_path.clone() {
        let src_path = PathBuf::from(&src);
        if src_path.join("SKILL.md").exists() {
            markdown = fs::read_to_string(src_path.join("SKILL.md")).unwrap_or_default();
            source_path = src;
        }
    } else if repo.source_type == "remote" {
        if let Ok(cfg) = config::get_storage_config() {
            if let Some(source) = get_source(&cfg, &repo.source_id) {
                if let Ok(remote_path) = source_skill_abs_path(source, &repo.source_rel_path) {
                    if remote_path.join("SKILL.md").exists() {
                        markdown =
                            fs::read_to_string(remote_path.join("SKILL.md")).unwrap_or_default();
                        source_path = remote_path.to_string_lossy().to_string();
                    }
                }
            }
        }
    }

    let detail = CatalogSkillDetail {
        skill: CatalogSkill {
            source_id: repo.source_id.clone(),
            id: repo.skill_id.clone(),
            rel_path: repo.source_rel_path.clone(),
            dir_name: normalized_repo_dir_name(&repo),
            name: repo.name.clone(),
            description: repo.description.clone(),
            models: repo.models.clone(),
            remote_hash: repo.hash.clone().unwrap_or_default(),
            icon_seed: repo.icon_seed.clone(),
            first_seen_at: None,
        },
        markdown,
        source_path,
    };
    api_ok(detail, combined_revision(&state, &local_state))
}

#[tauri::command]
pub fn skills_repo_reload_preview(
    input: RepoSkillKeyInput,
) -> Result<ApiOk<ReloadPreview>, String> {
    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut shared_state, &local_state, &cfg)? {
        shared_state = save_skills_state(shared_state)?;
    }
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo skill not found")?;

    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let baseline = repo_index_baseline_dir(&repo.repo_key)?;
    let before_exists = baseline.exists();
    let (after_dir, after_label) = resolve_repo_reload_after_dir(
        &repo,
        if before_exists {
            Some(baseline.as_path())
        } else {
            None
        },
        &repo_snapshot,
    )?;
    let (changed_files, text_diffs) = compare_snapshot_dirs(
        if before_exists {
            Some(baseline.as_path())
        } else {
            None
        },
        &after_dir,
    )?;
    let installed_models = installed_models_for_repo(&local_state, &repo);

    let preview = ReloadPreview {
        before_label: "Before Reload (Indexed Baseline)".to_string(),
        after_label,
        changed_files: changed_files.clone(),
        text_diffs,
        installed_models,
        has_changes: !changed_files.is_empty(),
    };
    api_ok(preview, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_repo_reload_apply(
    app: tauri::AppHandle,
    input: RepoReloadApplyInput,
) -> Result<ApiOk<ReloadApplyResult>, String> {
    let dedupe_key = format!("repo_reload_apply:{}", input.repo_key);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let result = ReloadApplyResult {
                index_refreshed: false,
                synced_models: vec![],
                updated_files_count: 0,
                applied_at: now_ts(),
            };
            return api_ok(result, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut shared_state, &local_state, &cfg)? {
        shared_state = save_skills_state(shared_state)?;
    }
    let repo_idx = shared_state
        .repositories
        .iter()
        .position(|r| r.repo_key == input.repo_key)
        .ok_or("repo skill not found")?;
    let repo_snapshot = repo_storage_dir(&input.repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let baseline = repo_index_baseline_dir(&input.repo_key)?;
    let (apply_source_dir, _after_label) = resolve_repo_reload_after_dir(
        &shared_state.repositories[repo_idx],
        if baseline.exists() {
            Some(baseline.as_path())
        } else {
            None
        },
        &repo_snapshot,
    )?;
    let (changed_files, _) = compare_snapshot_dirs(
        if baseline.exists() {
            Some(baseline.as_path())
        } else {
            None
        },
        &apply_source_dir,
    )?;
    let updated_files_count = changed_files.len() as u64;
    if apply_source_dir != repo_snapshot {
        replace_dir_atomic(&apply_source_dir, &repo_snapshot)?;
    }

    {
        let repo = shared_state
            .repositories
            .get_mut(repo_idx)
            .ok_or("repo skill not found")?;
        refresh_repository_record_from_snapshot(repo)?;
    }

    let repo = shared_state.repositories[repo_idx].clone();
    let installed_models = installed_models_for_repo(&local_state, &repo);
    let should_sync_to_models = input.sync_to_models || !installed_models.is_empty();
    let now = now_ts();
    let repo_dir_name = normalized_repo_dir_name(&repo);
    let mut model_match_skill_ids = HashMap::<String, String>::new();
    for model in MODELS {
        if let Some(skill) = local_state
            .skills
            .iter()
            .find(|s| s.model == model && skill_matches_repository(s, &repo))
        {
            model_match_skill_ids.insert(model.to_string(), skill.id.clone());
        }
    }
    let previous_local_dirs = local_state
        .skills
        .iter()
        .filter_map(|s| {
            if !skill_matches_repository(s, &repo) {
                return None;
            }
            locate_existing_record_local_dir(s)
                .ok()
                .map(|dir| (s.model.clone(), dir))
        })
        .collect::<HashMap<_, _>>();

    for s in &mut local_state.skills {
        if !skill_matches_repository(s, &repo) {
            continue;
        }
        s.dir_name = repo_dir_name.clone();
        s.name = repo.name.clone();
        s.description = repo.description.clone();
        s.models = repo.models.clone();
        s.remote_hash = repo.hash.clone();
        s.has_update = skill_has_markdown_update(s, &cfg).unwrap_or(false);
        s.updated_at = Some(now);
    }

    let mut synced_models = vec![];
    if should_sync_to_models {
        let installed_set = installed_models
            .iter()
            .cloned()
            .collect::<HashSet<String>>();
        for model in MODELS {
            if !installed_set.contains(model) {
                continue;
            }
            let ignore_skill_id = model_match_skill_ids
                .get(model)
                .map(|s| s.as_str())
                .unwrap_or(repo.skill_id.as_str());
            ensure_model_dir_name_available(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &repo_dir_name,
                Some(ignore_skill_id),
            )?;
            let model_root = model_dir(model)?;
            let dest = model_root.join(&repo_dir_name);
            ensure_within(&model_root, &dest)?;
            remove_existing_record_dir_if_moved(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                ignore_skill_id,
                &dest,
            )?;
            replace_dir_atomic(&repo_snapshot, &dest)?;
            if let Some(previous_dir) = previous_local_dirs.get(model) {
                if previous_dir != &dest && previous_dir.exists() {
                    let _ = fs::remove_dir_all(previous_dir);
                }
            }
            let local_hash = hash_dir(&dest)?;
            for s in &mut local_state.skills {
                if s.model == model && skill_matches_repository(s, &repo) {
                    s.dir_name = repo_dir_name.clone();
                    s.local_hash = local_hash.clone();
                    s.remote_hash = repo.hash.clone();
                    s.has_update = false;
                    s.last_synced_at = Some(now);
                    s.updated_at = Some(now);
                }
            }
            synced_models.push(model.to_string());
        }
    }

    // Only move baseline forward after model sync path has completed successfully.
    // This prevents "has update" from being cleared while installed models are still stale.
    snapshot_repository_index_baseline(&input.repo_key, &repo_snapshot)?;

    shared_state = save_skills_state(shared_state)?;
    local_state = save_local_skills_state(local_state)?;

    for model in &synced_models {
        let _ = reconcile_internal(Some(model), Some(INSTALL_SCOPE_GLOBAL), None);
    }
    trigger_storage_sync(app, "skills_repo_reload_apply");

    let result = ReloadApplyResult {
        index_refreshed: true,
        synced_models,
        updated_files_count,
        applied_at: now,
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_update_check(input: SkillKeyInput) -> Result<ApiOk<bool>, String> {
    let update_scope = normalize_install_scope(input.scope.as_deref());
    let update_project_root =
        normalize_project_root_for_scope(&update_scope, input.project_root.as_deref())?;
    let mut state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let mut changed = false;
    for s in &mut state.skills {
        if s.model == input.model
            && s.id == input.skill_id
            && scope_project_match(s, &update_scope, update_project_root.as_deref())
        {
            if let Some(c) = sync_state
                .catalog
                .iter()
                .find(|c| c.source_id == s.source_id && c.rel_path == s.source_rel_path)
            {
                s.remote_hash = Some(c.remote_hash.clone());
                s.has_update = skill_has_markdown_update(s, &cfg).unwrap_or(false);
                changed = true;
            }
        }
    }
    let has_update = state
        .skills
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &update_scope, update_project_root.as_deref())
        })
        .map(|s| s.has_update)
        .unwrap_or(false);
    let state = if changed {
        save_local_skills_state(state)?
    } else {
        state
    };
    api_ok(has_update, state.revision)
}

#[tauri::command]
pub fn skills_update_diff_preview(input: SkillKeyInput) -> Result<ApiOk<UpdateDiff>, String> {
    let diff_scope = normalize_install_scope(input.scope.as_deref());
    let diff_project_root =
        normalize_project_root_for_scope(&diff_scope, input.project_root.as_deref())?;
    let state = load_local_skills_state()?;
    let record = state
        .skills
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &diff_scope, diff_project_root.as_deref())
        })
        .cloned()
        .ok_or("skill not found")?;

    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let local_md =
        fs::read_to_string(record_local_dir(&record)?.join("SKILL.md")).unwrap_or_default();
    let remote_md = fs::read_to_string(
        source_skill_abs_path(source, &record.source_rel_path)?.join("SKILL.md"),
    )
    .unwrap_or_default();

    let (local_changed, remote_changed, local_blocks, remote_blocks) =
        calculate_changes(&local_md, &remote_md);
    let diff = UpdateDiff {
        local_markdown: local_md,
        remote_markdown: remote_md,
        local_changed_lines: local_changed,
        remote_changed_lines: remote_changed,
        local_changed_blocks: local_blocks,
        remote_changed_blocks: remote_blocks,
    };
    api_ok(diff, state.revision)
}

#[tauri::command]
pub async fn skills_update_apply(
    _app: tauri::AppHandle,
    input: SkillKeyInput,
) -> Result<ApiOk<SkillRecord>, String> {
    let update_scope = normalize_install_scope(input.scope.as_deref());
    let update_project_root =
        normalize_project_root_for_scope(&update_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "update:{}:{}:{}:{}",
        input.model,
        input.skill_id,
        update_scope,
        update_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            let record = state
                .skills
                .iter()
                .find(|s| {
                    s.model == input.model
                        && s.id == input.skill_id
                        && scope_project_match(s, &update_scope, update_project_root.as_deref())
                })
                .cloned()
                .ok_or("skill not found")?;
            return api_ok(record, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let cfg = config::get_storage_config()?;
    let mut state = load_local_skills_state()?;
    let idx = state
        .skills
        .iter()
        .position(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &update_scope, update_project_root.as_deref())
        })
        .ok_or("skill not found")?;

    let mut record = state.skills[idx].clone();
    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let remote = source_skill_abs_path(source, &record.source_rel_path)?;
    let remote_dir_name = read_required_skill_dir_name(&remote)?;
    let record_scope_value = record_scope(&record);
    let record_project_root = record_project_root(&record);
    ensure_model_dir_name_available(
        &state,
        &input.model,
        &record_scope_value,
        record_project_root.as_deref(),
        &remote_dir_name,
        Some(record.id.as_str()),
    )?;
    let (model_root, compat_roots) = resolve_skill_target_dir(
        &input.model,
        &record_scope_value,
        record_project_root.as_deref(),
    )?;
    let local = model_root.join(&remote_dir_name);
    ensure_within(&model_root, &local)?;
    remove_existing_record_dir_if_moved(
        &state,
        &input.model,
        &record_scope_value,
        record_project_root.as_deref(),
        &record.id,
        &local,
    )?;

    replace_dir_atomic(&remote, &local)?;
    for compat_root in compat_roots {
        let compat_dest = compat_root.join(&remote_dir_name);
        ensure_within(&compat_root, &compat_dest)?;
        replace_dir_atomic(&local, &compat_dest)?;
    }
    record.dir_name = remote_dir_name;
    record.local_hash = hash_dir(&local)?;
    record.remote_hash = Some(hash_dir(&remote)?);
    record.updated_at = Some(now_ts());
    record.has_update = false;
    state.skills[idx] = record.clone();
    let state = save_local_skills_state(state)?;

    let _ = reconcile_internal(
        Some(&input.model),
        Some(record_scope_value.as_str()),
        record_project_root.as_deref(),
    );
    api_ok(record, state.revision)
}

fn reconcile_one_model(model: &str, scope: &str, project_root: Option<&str>) -> Result<(), String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("skills/project_root_required")?;
        let project_root_path = PathBuf::from(root);
        let primary = project_primary_dir(model, &project_root_path)?;
        for compat in project_compat_dirs(model, &project_root_path) {
            ensure_dir(&compat)?;
            let mut primary_map: HashMap<String, PathBuf> = HashMap::new();
            for entry in fs::read_dir(&primary).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let p = entry.path();
                if p.is_dir() {
                    primary_map.insert(entry.file_name().to_string_lossy().to_string(), p);
                }
            }
            let mut compat_names = HashSet::new();
            for entry in fs::read_dir(&compat).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let p = entry.path();
                if p.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    compat_names.insert(name.clone());
                    if let Some(src) = primary_map.get(&name) {
                        let dst = compat.join(&name);
                        if hash_dir(src)? != hash_dir(&dst)? {
                            replace_dir_atomic(src, &dst)?;
                        }
                    } else {
                        fs::remove_dir_all(p).map_err(|e| e.to_string())?;
                    }
                }
            }
            for (name, src) in primary_map {
                if !compat_names.contains(&name) {
                    replace_dir_atomic(&src, &compat.join(name))?;
                }
            }
        }
        return Ok(());
    }

    let sot = model_dir(model)?;
    let mirror = mirror_dir(model)?;

    let mut sot_map: HashMap<String, PathBuf> = HashMap::new();
    for entry in fs::read_dir(&sot).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            sot_map.insert(name, p);
        }
    }

    let mut mirror_names = HashSet::new();
    for entry in fs::read_dir(&mirror).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            mirror_names.insert(name.clone());
            if let Some(src) = sot_map.get(&name) {
                let dst = mirror.join(&name);
                if hash_dir(src)? != hash_dir(&dst)? {
                    replace_dir_atomic(src, &dst)?;
                }
            } else {
                fs::remove_dir_all(p).map_err(|e| e.to_string())?;
            }
        }
    }

    for (name, src) in sot_map {
        if !mirror_names.contains(&name) {
            let dst = mirror.join(name);
            replace_dir_atomic(&src, &dst)?;
        }
    }

    Ok(())
}

fn reconcile_internal(
    model: Option<&str>,
    scope: Option<&str>,
    project_root: Option<&str>,
) -> Result<(), String> {
    let target_scope = normalize_install_scope(scope);
    match model {
        Some(m) => reconcile_one_model(m, &target_scope, project_root),
        None => {
            for m in MODELS {
                let _ = reconcile_one_model(m, &target_scope, project_root);
            }
            Ok(())
        }
    }
}

fn rebuild_local_installed_from_models(state: &mut SkillsLocalState) -> Result<(), String> {
    let mut existing = HashSet::new();
    for model in MODELS {
        let root = model_dir(model)?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let md = p.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let content = fs::read_to_string(&md).unwrap_or_default();
            let (name, desc, models) = parse_skill_md(&content, &[]);
            let hash = hash_dir(&p)?;
            existing.insert((model.to_string(), dir_name.clone()));

            if let Some(record) = state.skills.iter_mut().find(|s| {
                s.model == model
                    && record_scope(s) == INSTALL_SCOPE_GLOBAL
                    && normalized_record_dir_name(s) == dir_name
            }) {
                record.dir_name = dir_name.clone();
                record.name = name.clone();
                record.description = desc.clone();
                record.models = models.clone();
                record.local_hash = hash.clone();
                record.has_update = false;
                record.scope = INSTALL_SCOPE_GLOBAL.to_string();
                record.project_root = None;
                record.target_path = Some(p.to_string_lossy().to_string());
            } else {
                state.skills.push(SkillRecord {
                    id: dir_name.clone(),
                    dir_name: dir_name.clone(),
                    model: model.to_string(),
                    models: models.clone(),
                    name: name.clone(),
                    description: desc.clone(),
                    source_id: "local".to_string(),
                    source_rel_path: dir_name.clone(),
                    installed_at: now_ts(),
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash.clone(),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: dir_name.clone(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: Some(p.to_string_lossy().to_string()),
                });
            }
        }
    }

    state.skills.retain(|s| {
        if record_scope(s) != INSTALL_SCOPE_GLOBAL {
            return true;
        }
        existing.contains(&(s.model.clone(), normalized_record_dir_name(s)))
    });
    state.last_rescan_at = Some(now_ts());
    Ok(())
}

#[tauri::command]
pub async fn skills_reconcile(
    _app: tauri::AppHandle,
    model: Option<String>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<bool>, String> {
    let target_scope = normalize_install_scope(scope.as_deref());
    let target_project_root =
        normalize_project_root_for_scope(&target_scope, project_root.as_deref())?;
    let dedupe_key = format!(
        "reconcile:{}:{}:{}",
        model.clone().unwrap_or_else(|| "all".to_string()),
        target_scope,
        target_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(
        model.as_deref(),
        Some(target_scope.as_str()),
        target_project_root.as_deref(),
    )
    .map_err(|_| "skills/mirror_apply_failed".to_string())?;
    let state = load_local_skills_state()?;
    api_ok(true, state.revision)
}

#[tauri::command]
pub async fn skills_rescan_local(
    _app: tauri::AppHandle,
) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let _job = match acquire_job_key("rescan:local")? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(state.skills.clone(), state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_local_skills_state()?;
    rebuild_local_installed_from_models(&mut state)?;
    let state = save_local_skills_state(state)?;
    api_ok(state.skills.clone(), state.revision)
}

#[tauri::command]
pub async fn skills_rescan_mirror(
    _app: tauri::AppHandle,
) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let _job = match acquire_job_key("rescan:mirror")? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(state.skills.clone(), state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    for model in MODELS {
        if let Ok(mirror_root) = mirror_dir(model) {
            if let Ok(model_root) = model_dir(model) {
                if let Ok(entries) = fs::read_dir(&mirror_root) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if !p.is_dir() {
                            continue;
                        }
                        let id = entry.file_name().to_string_lossy().to_string();
                        let md = p.join("SKILL.md");
                        if !md.exists() {
                            continue;
                        }
                        let sot_dir = model_root.join(&id);
                        if let Ok(()) = ensure_within(&model_root, &sot_dir) {
                            let _ = replace_dir_atomic(&p, &sot_dir);
                        }
                    }
                }
            }
        }
    }

    // 关键修复：同步仓库记录
    let mut state = load_skills_state()?;

    for model in MODELS {
        let root = model_dir(model)?;
        let entries = fs::read_dir(&root).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let md = p.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let content = fs::read_to_string(&md).unwrap_or_default();
            let (name, desc, models) = parse_skill_md(&content, &[]);
            let dir_name = parse_required_skill_dir_name(&content)
                .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string());

            // 尝试匹配或创建仓库记录
            let source_id = "local".to_string(); // 本地扫描的统一标识
            let rel_path = entry.file_name().to_string_lossy().to_string();
            let repo_key = make_repo_key(&source_id, &rel_path);

            if !state.repositories.iter().any(|r| r.repo_key == repo_key) {
                state.repositories.push(RepositoryRecord {
                    repo_key,
                    skill_id: local_skill_id(&source_id, &rel_path),
                    dir_name,
                    source_id,
                    source_rel_path: rel_path,
                    source_type: "local_import".to_string(),
                    source_path: Some(p.to_string_lossy().to_string()),
                    name,
                    description: desc,
                    models,
                    icon_seed: "local".to_string(),
                    hash: Some(hash_dir(&p)?),
                    created_at: now_ts(),
                    updated_at: Some(now_ts()),
                    ever_installed: true,
                });
            }
        }
    }

    save_skills_state(state)?;

    let mut local_state = load_local_skills_state()?;
    rebuild_local_installed_from_models(&mut local_state)?;
    let local_state = save_local_skills_state(local_state)?;
    api_ok(local_state.skills.clone(), local_state.revision)
}

fn open_folder_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn skills_catalog_open_folder(
    app: tauri::AppHandle,
    input: CatalogSkillKeyInput,
) -> Result<ApiOk<CatalogOpenFolderResult>, String> {
    let dedupe_key = format!(
        "catalog_open_folder:{}:{}",
        input.source_id, input.skill_ref
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let result = CatalogOpenFolderResult {
                repo_key: make_repo_key(&input.source_id, &input.skill_ref),
                opened_path: String::new(),
            };
            return api_ok(result, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let mut shared_changed = false;

    let existing_repo = shared_state
        .repositories
        .iter()
        .find(|r| {
            r.source_id == input.source_id
                && (r.source_rel_path == input.skill_ref || r.skill_id == input.skill_ref)
        })
        .cloned();

    let (repo_key, _repo_snapshot) = if let Some(repo) = existing_repo {
        let repo_key = repo.repo_key.clone();
        let repo_snapshot = repo_storage_dir(&repo_key)?;
        if !repo_snapshot.exists() {
            let mut materialized = false;

            if let Some(src) = repo.source_path.as_ref() {
                let src_path = PathBuf::from(src);
                if src_path.join("SKILL.md").exists() {
                    replace_dir_atomic(&src_path, &repo_snapshot)?;
                    snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                    materialized = true;
                }
            }

            if !materialized {
                if let Some(local_record) = local_state.skills.iter().find(|s| {
                    s.source_id == repo.source_id
                        && (s.source_rel_path == repo.source_rel_path || s.id == repo.skill_id)
                }) {
                    let local_dir = record_local_dir(local_record)?;
                    if local_dir.join("SKILL.md").exists() {
                        replace_dir_atomic(&local_dir, &repo_snapshot)?;
                        snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                        materialized = true;
                    }
                }
            }

            if !materialized && repo.source_type == "remote" {
                if let Ok(cfg) = config::get_storage_config() {
                    if let Some(source) = get_source(&cfg, &repo.source_id) {
                        if let Ok(source_path) =
                            source_skill_abs_path(source, &repo.source_rel_path)
                        {
                            if source_path.join("SKILL.md").exists() {
                                replace_dir_atomic(&source_path, &repo_snapshot)?;
                                snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                                materialized = true;
                            }
                        }
                    }
                }
            }

            if materialized {
                if let Some(repo_mut) = shared_state
                    .repositories
                    .iter_mut()
                    .find(|r| r.repo_key == repo_key)
                {
                    refresh_repository_record_from_snapshot(repo_mut)?;
                    shared_changed = true;
                }
            } else {
                return Err("repository_snapshot_missing".to_string());
            }
        }
        (repo_key, repo_snapshot)
    } else {
        let cfg = config::get_storage_config()?;
        let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
        let sync_state = load_sync_state()?;
        let catalog = sync_state
            .catalog
            .iter()
            .find(|c| {
                c.source_id == input.source_id
                    && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
            })
            .cloned()
            .ok_or("catalog skill not found")?;
        let effective_models = resolve_effective_models(&catalog.models, &source.default_models);
        if effective_models.is_empty() {
            return Err("catalog skill not found".to_string());
        }
        let src = source_skill_abs_path(source, &catalog.rel_path)?;
        if !src.join("SKILL.md").exists() {
            return Err("skills/invalid_skill_dir".to_string());
        }
        let catalog_dir_name =
            read_required_skill_dir_name(&src).unwrap_or_else(|_| catalog.id.clone());
        let repo_key = make_repo_key(&catalog.source_id, &catalog.rel_path);
        let repo_snapshot = repo_storage_dir(&repo_key)?;
        if !repo_snapshot.exists() {
            let _ = upsert_repository_from_dir(
                &mut shared_state,
                &src,
                &catalog.source_id,
                &catalog.rel_path,
                &catalog.id,
                &catalog_dir_name,
                "remote",
                &catalog.name,
                &catalog.description,
                &effective_models,
                &catalog.icon_seed,
                Some(src.to_string_lossy().to_string()),
                Some(catalog.remote_hash.clone()),
                false,
            )?;
            shared_changed = true;
        }
        (repo_key, repo_snapshot)
    };

    if shared_changed {
        shared_state = save_skills_state(shared_state)?;
        trigger_storage_sync(app, "skills_catalog_open_folder");
    }

    let open_path = repo_storage_dir(&repo_key)?;
    open_folder_path(&open_path)?;
    let result = CatalogOpenFolderResult {
        repo_key,
        opened_path: open_path.to_string_lossy().to_string(),
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_open_folder(input: SkillKeyInput) -> Result<ApiOk<bool>, String> {
    let open_scope = normalize_install_scope(input.scope.as_deref());
    let open_project_root =
        normalize_project_root_for_scope(&open_scope, input.project_root.as_deref())?;
    let state = load_local_skills_state()?;
    let skill = state
        .skills
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &open_scope, open_project_root.as_deref())
        })
        .ok_or("skill not found")?;
    let path = record_local_dir(skill)?;
    open_folder_path(&path)?;

    api_ok(true, state.revision)
}

pub fn skills_reconcile_for_tool(
    tool: &str,
    scope: Option<&str>,
    project_root: Option<&str>,
) -> Result<(), String> {
    if !MODELS.contains(&tool) {
        return Ok(());
    }
    let normalized_scope = normalize_install_scope(scope);
    let normalized_project_root =
        normalize_project_root_for_scope(&normalized_scope, project_root)?;
    let key = format!(
        "reconcile:{}:{}:{}",
        tool,
        normalized_scope,
        normalized_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(key)? {
        Some(v) => v,
        None => return Ok(()),
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(
        Some(tool),
        Some(normalized_scope.as_str()),
        normalized_project_root.as_deref(),
    )
    .map_err(|_| "skills/mirror_apply_failed".to_string())
}

pub fn skills_installed_count_all_scopes() -> Result<usize, String> {
    let state = load_local_skills_state()?;
    Ok(state.skills.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn hash_dir_ignores_file_mtime_when_content_is_unchanged() {
        let unique = format!("onespace-skills-hash-{}-{}", std::process::id(), now_ts());
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("create temp test dir");
        let skill_md = root.join("SKILL.md");
        fs::write(&skill_md, "hello\nworld\n").expect("write initial content");

        let before = hash_dir(&root).expect("hash before");
        std::thread::sleep(Duration::from_millis(1200));
        fs::write(&skill_md, "hello\nworld\n").expect("rewrite same content");
        let after = hash_dir(&root).expect("hash after");

        assert_eq!(before, after);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_skill_md_prefers_title_for_name_and_frontmatter_for_description() {
        let md = r#"---
name: Frontmatter Skill
description: Description from frontmatter
models: [codex]
---
# Header Name
Paragraph description.
"#;
        let (name, description, models) = parse_skill_md(md, &[]);
        assert_eq!(name, "Header Name");
        assert_eq!(description, "Description from frontmatter");
        assert_eq!(models, vec!["codex".to_string()]);
    }

    #[test]
    fn parse_required_skill_dir_name_accepts_frontmatter_name() {
        let md = r#"---
name: git-commit
description: Description from frontmatter
models: [gemini]
---
First line.
Second line.
"#;
        let dir_name = parse_required_skill_dir_name(md).expect("should parse dir name");
        assert_eq!(dir_name, "git-commit");
    }

    #[test]
    fn parse_required_skill_dir_name_rejects_missing_frontmatter_name() {
        let md = r#"---
models: [gemini]
---
# Header Name
First line.
"#;
        let err = parse_required_skill_dir_name(md).expect_err("missing name should fail");
        assert_eq!(err, "skills/invalid_frontmatter_name");
    }

    #[test]
    fn parse_required_skill_dir_name_rejects_invalid_name() {
        let md = r#"---
name: Git Commit
description: desc
---
# Header Name
"#;
        let err = parse_required_skill_dir_name(md).expect_err("invalid name should fail");
        assert_eq!(err, "skills/invalid_frontmatter_name");
    }

    #[test]
    fn has_dir_name_conflict_detects_same_model_only() {
        let state = SkillsLocalState {
            skills: vec![
                SkillRecord {
                    id: "legacy-1".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "codex".to_string(),
                    models: vec![],
                    name: "n".to_string(),
                    description: "d".to_string(),
                    source_id: "local".to_string(),
                    source_rel_path: "a".to_string(),
                    installed_at: 0,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: "".to_string(),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: "".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: None,
                },
                SkillRecord {
                    id: "legacy-2".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "claude".to_string(),
                    models: vec![],
                    name: "n".to_string(),
                    description: "d".to_string(),
                    source_id: "local".to_string(),
                    source_rel_path: "b".to_string(),
                    installed_at: 0,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: "".to_string(),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: "".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: None,
                },
            ],
            revision: 0,
            last_rescan_at: None,
        };

        assert!(has_dir_name_conflict(
            &state,
            "codex",
            INSTALL_SCOPE_GLOBAL,
            None,
            "git-commit",
            Some("other-id"),
        ));
        assert!(!has_dir_name_conflict(
            &state,
            "gemini",
            INSTALL_SCOPE_GLOBAL,
            None,
            "git-commit",
            Some("other-id"),
        ));
        assert!(!has_dir_name_conflict(
            &state,
            "codex",
            INSTALL_SCOPE_GLOBAL,
            None,
            "git-commit",
            Some("legacy-1"),
        ));
    }

    #[test]
    fn hydrate_local_records_from_catalog_recovers_remote_metadata() {
        let mut state = SkillsLocalState {
            skills: vec![SkillRecord {
                id: "git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                model: "codex".to_string(),
                models: vec!["codex".to_string()],
                name: "Git Commit".to_string(),
                description: "local copy".to_string(),
                source_id: "local".to_string(),
                source_rel_path: "git-commit".to_string(),
                installed_at: 0,
                updated_at: None,
                last_synced_at: None,
                local_hash: "same-hash".to_string(),
                remote_hash: None,
                has_update: false,
                icon_seed: "git-commit".to_string(),
                scope: INSTALL_SCOPE_GLOBAL.to_string(),
                project_root: None,
                target_path: None,
            }],
            revision: 0,
            last_rescan_at: None,
        };
        let sync_state = SkillsSyncState {
            status: "done".to_string(),
            last_error: None,
            last_sync_at: Some(1),
            sources: vec![],
            catalog: vec![CatalogSkill {
                source_id: "official".to_string(),
                id: "official-git-commit".to_string(),
                rel_path: "automation/git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                name: "Git Commit".to_string(),
                description: "remote copy".to_string(),
                models: vec!["codex".to_string()],
                remote_hash: "same-hash".to_string(),
                icon_seed: "official".to_string(),
                first_seen_at: Some(1),
            }],
        };

        hydrate_local_records_from_catalog(&mut state, &sync_state);

        let skill = &state.skills[0];
        assert_eq!(skill.id, "official-git-commit");
        assert_eq!(skill.source_id, "official");
        assert_eq!(skill.source_rel_path, "automation/git-commit");
        assert_eq!(skill.remote_hash.as_deref(), Some("same-hash"));
        assert!(!skill.has_update);
        assert_eq!(skill.icon_seed, "official");
    }
}
