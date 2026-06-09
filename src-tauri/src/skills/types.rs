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

fn metadata_timestamp(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok().or_else(|| meta.created().ok()))
        .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_else(now_ts)
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
    pub skills_auto_update_enabled: bool,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct InstalledSkillTarget {
    pub model: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
    pub dir_name: String,
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
    #[serde(default)]
    pub installed_targets: Vec<InstalledSkillTarget>,
    pub has_changes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReloadApplyResult {
    pub index_refreshed: bool,
    #[serde(default)]
    pub synced_models: Vec<String>,
    #[serde(default)]
    pub synced_targets: Vec<InstalledSkillTarget>,
    pub updated_files_count: u64,
    pub applied_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RepoAutoUpdateResult {
    #[serde(default)]
    pub updated_repo_keys: Vec<String>,
    #[serde(default)]
    pub updated_skill_names: Vec<String>,
    #[serde(default)]
    pub synced_targets: Vec<InstalledSkillTarget>,
    pub updated_repo_count: u64,
    pub synced_target_count: u64,
    pub applied_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CatalogOpenFolderResult {
    pub repo_key: String,
    pub opened_path: String,
}
