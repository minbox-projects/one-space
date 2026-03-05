use crate::config::{self, SkillSourceConfig, StorageConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MODELS: [&str; 4] = ["claude", "gemini", "codex", "opencode"];
const IGNORE_NAMES: [&str; 5] = [".git", ".DS_Store", "node_modules", "dist", "target"];

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillRecord {
    pub id: String,
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
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillsState {
    #[serde(default)]
    pub skills: Vec<SkillRecord>,
    pub revision: u64,
    pub last_rescan_at: Option<u64>,
    pub last_sync_at: Option<u64>,
    #[serde(default)]
    pub errors: Vec<String>,
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
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub remote_hash: String,
    pub icon_seed: String,
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalScanInput {
    pub root_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalSkillCandidate {
    pub rel_path: String,
    pub skill_id: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LocalImportResult {
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
pub struct DiffBlock {
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
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

fn skills_models_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("models");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_meta_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_cache_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("remote_cache");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_state_path() -> Result<PathBuf, String> {
    Ok(skills_meta_root()?.join("state.json"))
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
    Ok(p)
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

fn load_skills_state() -> Result<SkillsState, String> {
    read_json_or_default(&skills_state_path()?)
}

fn save_skills_state(mut state: SkillsState) -> Result<SkillsState, String> {
    state.revision = state.revision.saturating_add(1);
    write_json(&skills_state_path()?, &state)?;
    Ok(state)
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
        hasher.update(rel.to_string_lossy().as_bytes());
        let abs = path.join(&rel);
        let content = fs::read(&abs).map_err(|e| e.to_string())?;
        hasher.update(&content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_models(text: &str, source_default: &[String]) -> Vec<String> {
    let mut out = vec![];
    for line in text.lines() {
        let lower = line.trim().to_lowercase();
        if lower.starts_with("models:") {
            let body = line.split_once(':').map(|(_, v)| v).unwrap_or("").trim();
            let body = body.trim_matches('[').trim_matches(']');
            for item in body.split(',') {
                let m = item.trim().trim_matches('"').trim_matches('\'').to_lowercase();
                if MODELS.contains(&m.as_str()) && !out.contains(&m) {
                    out.push(m);
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    for m in source_default {
        let v = m.trim().to_lowercase();
        if MODELS.contains(&v.as_str()) && !out.contains(&v) {
            out.push(v);
        }
    }
    if out.is_empty() {
        MODELS.iter().map(|m| m.to_string()).collect()
    } else {
        out
    }
}

fn parse_skill_md(md: &str, source_default_models: &[String]) -> (String, String, Vec<String>) {
    let mut content = md;
    if md.starts_with("---\n") {
        if let Some(idx) = md[4..].find("\n---") {
            let front = &md[4..4 + idx];
            let models = parse_models(front, source_default_models);
            content = &md[(4 + idx + 4)..];
            let (name, desc) = parse_name_desc(content);
            return (name, desc, models);
        }
    }
    let (name, desc) = parse_name_desc(content);
    let models = parse_models(md, source_default_models);
    (name, desc, models)
}

fn parse_name_desc(content: &str) -> (String, String) {
    let mut name = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            name = trimmed.trim_start_matches('#').trim().to_string();
            if !name.is_empty() {
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

    if name.is_empty() {
        name = "Unnamed Skill".to_string();
    }
    if desc.is_empty() {
        desc = "No description".to_string();
    }

    (name, desc)
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
                let rel = path.strip_prefix(base).map_err(|e| e.to_string())?.to_path_buf();
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
        out.push(LocalSkillCandidate {
            rel_path: rel_str.clone(),
            skill_id: local_skill_id(&source_id, &rel_str),
            source_id: source_id.clone(),
            name,
            description,
            declared_models,
        });
    }
    Ok(out)
}

fn copy_dir_secure_internal(src_root: &Path, src: &Path, dst_root: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let src_root_can = fs::canonicalize(src_root).map_err(|_| "skills/path_out_of_root".to_string())?;
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
            ensure_within(dst_root, &target)?;
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

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
        let _ = git_run(Some(&repo_dir), &["fetch", "--depth", "1", "origin", &branch]);
        let _ = git_run(Some(&repo_dir), &["checkout", &branch]);
        let _ = git_run(Some(&repo_dir), &["reset", "--hard", &format!("origin/{}", branch)]);
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

fn scan_source_catalog(repo_dir: &Path, source: &SkillSourceConfig) -> Result<Vec<CatalogSkill>, String> {
    let scan_root = source_scan_root(&repo_dir, source)?;
    let mut skill_dirs = vec![];
    find_skill_dirs(&scan_root, &scan_root, &mut skill_dirs)?;
    let mut catalog = vec![];
    for rel in skill_dirs {
        let abs = scan_root.join(&rel);
        let md = abs.join("SKILL.md");
        let md_content = fs::read_to_string(&md).map_err(|e| e.to_string())?;
        let (name, description, models) = parse_skill_md(&md_content, &source.default_models);
        let rel_str = rel.to_string_lossy().to_string();
        let id = safe_slug(&format!("{}-{}", source.id, rel_str));
        let remote_hash = hash_dir(&abs)?;
        catalog.push(CatalogSkill {
            source_id: source.id.clone(),
            id,
            rel_path: rel_str,
            name,
            description,
            models,
            remote_hash,
            icon_seed: source.id.clone(),
        });
    }
    Ok(catalog)
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

fn calculate_changes(local_md: &str, remote_md: &str) -> (Vec<u32>, Vec<u32>, Vec<DiffBlock>, Vec<DiffBlock>) {
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

fn record_local_dir(record: &SkillRecord) -> Result<PathBuf, String> {
    Ok(model_dir(&record.model)?.join(&record.id))
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

fn update_record_remote_flags(state: &mut SkillsState, sync_state: &SkillsSyncState) {
    let mut map = HashMap::new();
    for c in &sync_state.catalog {
        map.insert((c.source_id.clone(), c.rel_path.clone()), c.remote_hash.clone());
    }
    for s in &mut state.skills {
        if let Some(remote_hash) = map.get(&(s.source_id.clone(), s.source_rel_path.clone())) {
            s.remote_hash = Some(remote_hash.clone());
            s.has_update = s.local_hash != *remote_hash;
            s.last_synced_at = Some(now_ts());
        }
    }
    state.last_sync_at = Some(now_ts());
}

#[tauri::command]
pub fn skills_config_get() -> Result<ApiOk<SkillsConfigPayload>, String> {
    let cfg = config::get_storage_config()?;
    let payload = SkillsConfigPayload {
        skills_sync_enabled: cfg.skills_sync_enabled.unwrap_or(true),
        skills_sync_interval_minutes: cfg.skills_sync_interval_minutes.unwrap_or(60).max(5),
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
pub fn skills_list_installed(model: Option<String>) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let state = load_skills_state()?;
    let list = state
        .skills
        .iter()
        .filter(|s| model.as_ref().map(|m| m == &s.model).unwrap_or(true))
        .cloned()
        .collect::<Vec<_>>();
    api_ok(list, state.revision)
}

#[tauri::command]
pub fn skills_list_catalog(model: Option<String>) -> Result<ApiOk<Vec<CatalogSkill>>, String> {
    let sync_state = load_sync_state()?;
    let list = sync_state
        .catalog
        .iter()
        .filter(|s| {
            model
                .as_ref()
                .map(|m| s.models.iter().any(|v| v == m))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    let revision = load_skills_state()?.revision;
    api_ok(list, revision)
}

#[tauri::command]
pub async fn skills_sync_now(app: tauri::AppHandle) -> Result<ApiOk<SkillsSyncState>, String> {
    let _job = match acquire_job_key("sync:all")? {
        Some(v) => v,
        None => {
            let sync_state = load_sync_state()?;
            let revision = load_skills_state()?.revision;
            return api_ok(sync_state, revision);
        }
    };

    let (sync_state, revision, cfg_save) = {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;

        let cfg = config::get_storage_config()?;
        let previous_sync_state = load_sync_state()?;
        let mut sync_state = previous_sync_state.clone();
        sync_state.status = "fetching_source".to_string();
        sync_state.last_error = None;

        let mut next_catalog = vec![];
        let mut next_sources = vec![];
        for source in &cfg.skills_sources {
            let _source_job = match acquire_job_key(format!("sync:{}", source.id))? {
                Some(v) => Some(v),
                None => {
                    let prev = sync_state
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
                let prev = sync_state
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
                        let scanned = scan_source_catalog(&repo_dir, source)?;
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
                sync_state.last_error = last_err;
            }
        }

        sync_state.status = if sync_state.last_error.is_some() {
            "error".to_string()
        } else {
            "done".to_string()
        };
        sync_state.last_sync_at = Some(now_ts());
        sync_state.catalog = next_catalog;
        sync_state.sources = next_sources;
        save_sync_state(&sync_state)?;

        let mut state = load_skills_state()?;
        update_record_remote_flags(&mut state, &sync_state);
        state = save_skills_state(state)?;

        let mut cfg_save = config::get_storage_config()?;
        touch_sync_timestamp(&mut cfg_save);

        (sync_state, state.revision, cfg_save)
    };

    config::save_storage_config(app.clone(), cfg_save).await?;
    trigger_storage_sync(app, "skills_sync_now");

    api_ok(sync_state, revision)
}

#[tauri::command]
pub fn skills_sync_status_get() -> Result<ApiOk<SkillsSyncState>, String> {
    let sync_state = load_sync_state()?;
    let revision = load_skills_state()?.revision;
    api_ok(sync_state, revision)
}

#[tauri::command]
pub fn skills_local_scan(input: LocalScanInput) -> Result<ApiOk<Vec<LocalSkillCandidate>>, String> {
    let root_can = resolve_scan_root(&input.root_path)?;
    let list = scan_local_candidates(&root_can)?;
    let revision = load_skills_state()?.revision;
    api_ok(list, revision)
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
            let state = load_skills_state()?;
            return api_ok(LocalImportResult::default(), state.revision);
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

    let mut state = load_skills_state()?;
    let mut result = LocalImportResult::default();
    let mut changed = false;

    for model in &models {
        let model_root = model_dir(model)?;
        for selection in &input.selections {
            let strategy = selection.conflict_strategy.trim().to_lowercase();
            if strategy != "overwrite" && strategy != "skip" {
                result.failed.push(LocalImportFailed {
                    rel_path: selection.rel_path.clone(),
                    skill_id: None,
                    model: model.clone(),
                    reason: "invalid_conflict_strategy".to_string(),
                });
                continue;
            }

            let Some(candidate) = candidate_map.get(&selection.rel_path) else {
                result.failed.push(LocalImportFailed {
                    rel_path: selection.rel_path.clone(),
                    skill_id: None,
                    model: model.clone(),
                    reason: "skill_not_found".to_string(),
                });
                continue;
            };

            let src = if candidate.rel_path == "." {
                root_can.clone()
            } else {
                root_can.join(&candidate.rel_path)
            };
            if !src.join("SKILL.md").exists() {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: "skills/invalid_skill_dir".to_string(),
                });
                continue;
            }

            let dest = model_root.join(&candidate.skill_id);
            ensure_within(&model_root, &dest)?;
            let exists_in_state = state
                .skills
                .iter()
                .any(|s| s.model == *model && s.id == candidate.skill_id);
            let has_conflict = exists_in_state || dest.exists();
            if has_conflict && strategy == "skip" {
                result.skipped.push(LocalImportSkipped {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: candidate.skill_id.clone(),
                    model: model.clone(),
                    reason: "conflict_exists".to_string(),
                });
                continue;
            }

            if let Err(err) = replace_dir_atomic(&src, &dest) {
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

            state
                .skills
                .retain(|s| !(s.model == *model && s.id == candidate.skill_id));
            let record = SkillRecord {
                id: candidate.skill_id.clone(),
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
            };
            state.skills.push(record.clone());
            result.installed.push(record);
            changed = true;
        }
    }

    let state = if changed { save_skills_state(state)? } else { state };
    for model in &models {
        let _ = reconcile_internal(Some(model));
    }
    trigger_storage_sync(app, "skills_local_import");
    api_ok(result, state.revision)
}

#[tauri::command]
pub async fn skills_install(
    app: tauri::AppHandle,
    input: InstallInput,
) -> Result<ApiOk<SkillRecord>, String> {
    let dedupe_key = format!("install:{}:{}", input.model, input.skill_ref);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_skills_state()?;
            if let Some(found) = state
                .skills
                .iter()
                .find(|s| s.model == input.model && s.source_id == input.source_id)
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
            c.source_id == input.source_id && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
        })
        .cloned()
        .ok_or("catalog skill not found")?;

    let src = source_skill_abs_path(source, &catalog.rel_path)?;
    if !src.join("SKILL.md").exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }

    let dest = model_dir(&input.model)?.join(&catalog.id);
    let model_root = model_dir(&input.model)?;
    ensure_within(&model_root, &dest)?;
    replace_dir_atomic(&src, &dest)?;

    let local_hash = hash_dir(&dest)?;
    let mut state = load_skills_state()?;
    state.skills.retain(|s| !(s.model == input.model && s.id == catalog.id));

    let now = now_ts();
    let record = SkillRecord {
        id: catalog.id.clone(),
        model: input.model.clone(),
        models: catalog.models.clone(),
        name: catalog.name.clone(),
        description: catalog.description.clone(),
        source_id: catalog.source_id.clone(),
        source_rel_path: catalog.rel_path.clone(),
        installed_at: now,
        updated_at: None,
        last_synced_at: sync_state.last_sync_at,
        local_hash,
        remote_hash: Some(catalog.remote_hash.clone()),
        has_update: false,
        icon_seed: catalog.icon_seed.clone(),
    };

    state.skills.push(record.clone());
    let state = save_skills_state(state)?;

    let _ = reconcile_internal(Some(&input.model));
    trigger_storage_sync(app, "skills_install");
    api_ok(record, state.revision)
}

#[tauri::command]
pub async fn skills_uninstall(
    app: tauri::AppHandle,
    input: SkillKeyInput,
) -> Result<ApiOk<bool>, String> {
    let dedupe_key = format!("uninstall:{}:{}", input.model, input.skill_id);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_skills_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let local = model_dir(&input.model)?.join(&input.skill_id);
    let root = model_dir(&input.model)?;
    ensure_within(&root, &local)?;
    if local.exists() {
        fs::remove_dir_all(&local).map_err(|e| e.to_string())?;
    }

    let mut state = load_skills_state()?;
    state
        .skills
        .retain(|s| !(s.model == input.model && s.id == input.skill_id));
    let state = save_skills_state(state)?;

    let _ = reconcile_internal(Some(&input.model));
    trigger_storage_sync(app, "skills_uninstall");
    api_ok(true, state.revision)
}

#[tauri::command]
pub fn skills_detail_get(input: SkillKeyInput) -> Result<ApiOk<SkillDetail>, String> {
    let state = load_skills_state()?;
    let record = state
        .skills
        .iter()
        .find(|s| s.model == input.model && s.id == input.skill_id)
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
pub fn skills_update_check(input: SkillKeyInput) -> Result<ApiOk<bool>, String> {
    let mut state = load_skills_state()?;
    let sync_state = load_sync_state()?;
    let mut changed = false;
    for s in &mut state.skills {
        if s.model == input.model && s.id == input.skill_id {
            if let Some(c) = sync_state
                .catalog
                .iter()
                .find(|c| c.source_id == s.source_id && c.rel_path == s.source_rel_path)
            {
                s.remote_hash = Some(c.remote_hash.clone());
                s.has_update = s.local_hash != c.remote_hash;
                changed = true;
            }
        }
    }
    let has_update = state
        .skills
        .iter()
        .find(|s| s.model == input.model && s.id == input.skill_id)
        .map(|s| s.has_update)
        .unwrap_or(false);
    let state = if changed { save_skills_state(state)? } else { state };
    api_ok(has_update, state.revision)
}

#[tauri::command]
pub fn skills_update_diff_preview(input: SkillKeyInput) -> Result<ApiOk<UpdateDiff>, String> {
    let state = load_skills_state()?;
    let record = state
        .skills
        .iter()
        .find(|s| s.model == input.model && s.id == input.skill_id)
        .cloned()
        .ok_or("skill not found")?;

    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let local_md = fs::read_to_string(record_local_dir(&record)?.join("SKILL.md")).unwrap_or_default();
    let remote_md = fs::read_to_string(source_skill_abs_path(source, &record.source_rel_path)?.join("SKILL.md"))
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
    app: tauri::AppHandle,
    input: SkillKeyInput,
) -> Result<ApiOk<SkillRecord>, String> {
    let dedupe_key = format!("update:{}:{}", input.model, input.skill_id);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_skills_state()?;
            let record = state
                .skills
                .iter()
                .find(|s| s.model == input.model && s.id == input.skill_id)
                .cloned()
                .ok_or("skill not found")?;
            return api_ok(record, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let cfg = config::get_storage_config()?;
    let mut state = load_skills_state()?;
    let idx = state
        .skills
        .iter()
        .position(|s| s.model == input.model && s.id == input.skill_id)
        .ok_or("skill not found")?;

    let mut record = state.skills[idx].clone();
    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let remote = source_skill_abs_path(source, &record.source_rel_path)?;
    let local = record_local_dir(&record)?;

    replace_dir_atomic(&remote, &local)?;
    record.local_hash = hash_dir(&local)?;
    record.remote_hash = Some(hash_dir(&remote)?);
    record.updated_at = Some(now_ts());
    record.has_update = false;
    state.skills[idx] = record.clone();
    let state = save_skills_state(state)?;

    let _ = reconcile_internal(Some(&input.model));
    trigger_storage_sync(app, "skills_update_apply");
    api_ok(record, state.revision)
}

fn reconcile_one_model(model: &str) -> Result<(), String> {
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

fn reconcile_internal(model: Option<&str>) -> Result<(), String> {
    match model {
        Some(m) => reconcile_one_model(m),
        None => {
            for m in MODELS {
                let _ = reconcile_one_model(m);
            }
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn skills_reconcile(
    app: tauri::AppHandle,
    model: Option<String>,
) -> Result<ApiOk<bool>, String> {
    let dedupe_key = format!("reconcile:{}", model.clone().unwrap_or_else(|| "all".to_string()));
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_skills_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(model.as_deref()).map_err(|_| "skills/mirror_apply_failed".to_string())?;
    let state = load_skills_state()?;
    trigger_storage_sync(app, "skills_reconcile");
    api_ok(true, state.revision)
}

#[tauri::command]
pub async fn skills_rescan_local(app: tauri::AppHandle) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let _job = match acquire_job_key("rescan:local")? {
        Some(v) => v,
        None => {
            let state = load_skills_state()?;
            return api_ok(state.skills.clone(), state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_skills_state()?;
    let mut existing = HashSet::new();

    for model in MODELS {
        let root = model_dir(model)?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            let md = p.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let content = fs::read_to_string(&md).unwrap_or_default();
            let (name, desc, models) = parse_skill_md(&content, &[]);
            let hash = hash_dir(&p)?;
            existing.insert((model.to_string(), id.clone()));

            if let Some(record) = state
                .skills
                .iter_mut()
                .find(|s| s.model == model && s.id == id)
            {
                record.name = name;
                record.description = desc;
                record.models = models;
                record.local_hash = hash;
                record.has_update = record
                    .remote_hash
                    .as_ref()
                    .map(|h| h != &record.local_hash)
                    .unwrap_or(false);
            } else {
                state.skills.push(SkillRecord {
                    id: id.clone(),
                    model: model.to_string(),
                    models,
                    name,
                    description: desc,
                    source_id: "local".to_string(),
                    source_rel_path: id.clone(),
                    installed_at: now_ts(),
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash,
                    remote_hash: None,
                    has_update: false,
                    icon_seed: id,
                });
            }
        }
    }

    state
        .skills
        .retain(|s| existing.contains(&(s.model.clone(), s.id.clone())));
    state.last_rescan_at = Some(now_ts());
    let state = save_skills_state(state)?;
    trigger_storage_sync(app, "skills_rescan_local");
    api_ok(state.skills.clone(), state.revision)
}

#[tauri::command]
pub fn skills_open_folder(input: SkillKeyInput) -> Result<ApiOk<bool>, String> {
    let state = load_skills_state()?;
    let skill = state
        .skills
        .iter()
        .find(|s| s.model == input.model && s.id == input.skill_id)
        .ok_or("skill not found")?;
    let path = record_local_dir(skill)?;

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    api_ok(true, state.revision)
}

pub fn skills_reconcile_for_tool(tool: &str) -> Result<(), String> {
    if !MODELS.contains(&tool) {
        return Ok(());
    }
    let key = format!("reconcile:{}", tool);
    let _job = match acquire_job_key(key)? {
        Some(v) => v,
        None => return Ok(()),
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(Some(tool)).map_err(|_| "skills/mirror_apply_failed".to_string())
}
