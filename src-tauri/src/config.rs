use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProxyConfig {
    pub proxy_enabled: bool,
    pub proxy_type: String,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
    pub check_interval: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            proxy_enabled: false,
            proxy_type: "socks5".to_string(),
            proxy_host: String::new(),
            proxy_port: 1080,
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkillSourceConfig {
    pub id: String,
    pub name: String,
    pub repo_url: String,
    pub branch: Option<String>,
    pub base_dir: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub default_models: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SubagentSourceConfig {
    pub id: String,
    pub name: String,
    pub repo_url: String,
    pub branch: Option<String>,
    pub base_dir: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub default_models: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AiNewsRssSource {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_ai_model_permission_modes() -> HashMap<String, String> {
    HashMap::from([
        ("claude".to_string(), "default".to_string()),
        ("gemini".to_string(), "default".to_string()),
        ("codex".to_string(), "default".to_string()),
        ("opencode".to_string(), "default".to_string()),
    ])
}

/// Normalize permission modes: ensure all four tools exist and values are 'default' or 'full_access'.
fn normalize_ai_model_permission_modes(
    input: Option<HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut result = default_ai_model_permission_modes();
    if let Some(map) = input {
        for key in &["claude", "gemini", "codex", "opencode"] {
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

fn default_ai_model_launch_commands() -> HashMap<String, String> {
    HashMap::from([
        (
            "claude".to_string(),
            "claude --session-id {session_id}".to_string(),
        ),
        ("gemini".to_string(), "gemini".to_string()),
        ("codex".to_string(), "codex".to_string()),
        ("opencode".to_string(), "opencode".to_string()),
    ])
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncPolicy {
    #[serde(default = "default_true")]
    pub providers: bool,
    #[serde(default = "default_true")]
    pub mcp: bool,
    #[serde(default = "default_true")]
    pub content: bool,
    #[serde(default = "default_true")]
    pub workflow_presets: bool,
    #[serde(default = "default_true")]
    pub skills_sources: bool,
    #[serde(default)]
    pub skills_repository: bool,
    #[serde(default = "default_true")]
    pub subagents_sources: bool,
    #[serde(default)]
    pub subagents_repository: bool,
    #[serde(default)]
    pub ai_news: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            providers: true,
            mcp: true,
            content: true,
            workflow_presets: true,
            skills_sources: true,
            skills_repository: false,
            subagents_sources: true,
            subagents_repository: false,
            ai_news: false,
        }
    }
}

const DEFAULT_AI_NEWS_KEYWORDS: &str =
    "artificial intelligence, generative AI, LLM, large language model, OpenAI, Anthropic, Gemini";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SharedProfile {
    pub skills_sync_enabled: Option<bool>,
    pub skills_auto_update_enabled: Option<bool>,
    pub skills_sync_interval_minutes: Option<u64>,
    pub skills_new_badge_hours: Option<u64>,
    pub skills_last_synced_at: Option<i64>,
    #[serde(default)]
    pub skills_sources: Vec<SkillSourceConfig>,
    pub subagents_sync_enabled: Option<bool>,
    pub subagents_sync_interval_minutes: Option<u64>,
    pub subagents_new_badge_hours: Option<u64>,
    pub subagents_last_synced_at: Option<i64>,
    pub ai_news_enabled: Option<bool>,
    pub ai_news_sync_interval_minutes: Option<u64>,
    pub ai_news_retention_days: Option<u64>,
    pub ai_news_retention_max_items: Option<u64>,
    pub ai_news_keywords: Option<String>,
    pub ai_news_last_synced_at: Option<i64>,
    pub ai_news_rss_sources: Option<Vec<AiNewsRssSource>>,
    #[serde(default)]
    pub subagents_sources: Vec<SubagentSourceConfig>,
    #[serde(default)]
    pub sync_policy: SyncPolicy,
}

impl SharedProfile {
    fn is_effectively_empty(&self) -> bool {
        self.skills_sync_enabled.is_none()
            && self.skills_auto_update_enabled.is_none()
            && self.skills_sync_interval_minutes.is_none()
            && self.skills_new_badge_hours.is_none()
            && self.skills_last_synced_at.is_none()
            && self.skills_sources.is_empty()
            && self.subagents_sync_enabled.is_none()
            && self.subagents_sync_interval_minutes.is_none()
            && self.subagents_new_badge_hours.is_none()
            && self.subagents_last_synced_at.is_none()
            && self.ai_news_enabled.is_none()
            && self.ai_news_sync_interval_minutes.is_none()
            && self.ai_news_retention_days.is_none()
            && self.ai_news_retention_max_items.is_none()
            && self.ai_news_keywords.is_none()
            && self.ai_news_last_synced_at.is_none()
            && self
                .ai_news_rss_sources
                .as_ref()
                .map_or(true, Vec::is_empty)
            && self.subagents_sources.is_empty()
            && self.sync_policy == SyncPolicy::default()
    }
}

impl PartialEq for SyncPolicy {
    fn eq(&self, other: &Self) -> bool {
        self.providers == other.providers
            && self.mcp == other.mcp
            && self.content == other.content
            && self.workflow_presets == other.workflow_presets
            && self.skills_sources == other.skills_sources
            && self.skills_repository == other.skills_repository
            && self.subagents_sources == other.subagents_sources
            && self.subagents_repository == other.subagents_repository
            && self.ai_news == other.ai_news
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StorageConfig {
    pub storage_type: String,
    pub git_url: Option<String>,
    pub auth_method: Option<String>,
    pub http_username: Option<String>,
    pub http_token: Option<String>,
    pub ssh_key_path: Option<String>,

    pub main_shortcut: Option<String>,
    pub quick_ai_shortcut: Option<String>,
    pub default_ai_dir: Option<String>,
    pub claude_provider_launch_dir: Option<String>,
    pub default_ai_model: Option<String>,
    pub ai_terminal_app: Option<String>,
    pub ai_model_launch_commands: Option<HashMap<String, String>>,
    pub ai_model_permission_modes: Option<HashMap<String, String>>,
    pub ai_sessions_history_days: Option<u64>,
    pub message_retention_days: Option<u64>,
    pub language: Option<String>,

    pub local_storage_path: Option<String>,
    pub icloud_storage_path: Option<String>,

    pub proxy: Option<ProxyConfig>,

    pub launch_at_login: Option<bool>,
    pub auto_update_enabled: Option<bool>,
    pub update_check_interval_minutes: Option<u64>,
    pub update_last_checked_at: Option<i64>,
    pub update_ignored_version: Option<String>,

    pub skills_sync_enabled: Option<bool>,
    pub skills_auto_update_enabled: Option<bool>,
    pub skills_sync_interval_minutes: Option<u64>,
    pub skills_new_badge_hours: Option<u64>,
    pub skills_last_synced_at: Option<i64>,
    #[serde(default)]
    pub skills_sources: Vec<SkillSourceConfig>,
    pub subagents_sync_enabled: Option<bool>,
    pub subagents_sync_interval_minutes: Option<u64>,
    pub subagents_new_badge_hours: Option<u64>,
    pub subagents_last_synced_at: Option<i64>,
    pub ai_news_enabled: Option<bool>,
    pub ai_news_sync_interval_minutes: Option<u64>,
    pub ai_news_retention_days: Option<u64>,
    pub ai_news_retention_max_items: Option<u64>,
    pub ai_news_keywords: Option<String>,
    pub ai_news_last_synced_at: Option<i64>,
    #[serde(default)]
    pub ai_news_rss_sources: Vec<AiNewsRssSource>,
    #[serde(default)]
    pub subagents_sources: Vec<SubagentSourceConfig>,

    #[serde(default)]
    pub sync_policy: SyncPolicy,

    #[serde(default)]
    pub is_encrypted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceConfig {
    pub storage_type: String,
    pub git_url: Option<String>,
    pub auth_method: Option<String>,
    pub http_username: Option<String>,
    pub http_token: Option<String>,
    pub ssh_key_path: Option<String>,

    pub main_shortcut: Option<String>,
    pub quick_ai_shortcut: Option<String>,
    pub default_ai_dir: Option<String>,
    pub claude_provider_launch_dir: Option<String>,
    pub default_ai_model: Option<String>,
    pub ai_terminal_app: Option<String>,
    pub ai_model_launch_commands: Option<HashMap<String, String>>,
    pub ai_model_permission_modes: Option<HashMap<String, String>>,
    pub ai_sessions_history_days: Option<u64>,
    pub message_retention_days: Option<u64>,
    pub language: Option<String>,

    pub local_storage_path: Option<String>,
    pub icloud_storage_path: Option<String>,

    pub proxy: Option<ProxyConfig>,

    pub launch_at_login: Option<bool>,
    pub auto_update_enabled: Option<bool>,
    pub update_check_interval_minutes: Option<u64>,
    pub update_last_checked_at: Option<i64>,
    pub update_ignored_version: Option<String>,

    #[serde(default)]
    pub is_encrypted: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let storage_type = "icloud".to_string();
        #[cfg(not(target_os = "macos"))]
        let storage_type = "local".to_string();

        Self {
            storage_type,
            git_url: None,
            auth_method: Some("http".to_string()),
            http_username: None,
            http_token: None,
            ssh_key_path: None,
            main_shortcut: Some("Alt+Space".to_string()),
            quick_ai_shortcut: Some("Alt+Shift+A".to_string()),
            default_ai_dir: None,
            claude_provider_launch_dir: None,
            default_ai_model: Some("claude".to_string()),
            ai_terminal_app: Some("终端".to_string()),
            ai_model_launch_commands: Some(default_ai_model_launch_commands()),
            ai_model_permission_modes: Some(default_ai_model_permission_modes()),
            ai_sessions_history_days: Some(30),
            message_retention_days: Some(30),
            language: Some("zh".to_string()),
            local_storage_path: None,
            icloud_storage_path: None,
            proxy: Some(ProxyConfig::default()),
            launch_at_login: Some(false),
            auto_update_enabled: Some(false),
            update_check_interval_minutes: Some(360),
            update_last_checked_at: None,
            update_ignored_version: None,
            is_encrypted: false,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        let device = DeviceConfig::default();
        let mut merged = storage_from_device(device);
        apply_shared_profile(
            &mut merged,
            &SharedProfile {
                skills_sync_enabled: Some(true),
                skills_auto_update_enabled: Some(false),
                skills_sync_interval_minutes: Some(60),
                skills_new_badge_hours: Some(72),
                skills_last_synced_at: None,
                skills_sources: vec![],
                subagents_sync_enabled: Some(true),
                subagents_sync_interval_minutes: Some(60),
                subagents_new_badge_hours: Some(72),
                subagents_last_synced_at: None,
                ai_news_enabled: Some(false),
                ai_news_sync_interval_minutes: Some(60),
                ai_news_retention_days: Some(90),
                ai_news_retention_max_items: Some(1000),
                ai_news_keywords: Some(DEFAULT_AI_NEWS_KEYWORDS.to_string()),
                ai_news_last_synced_at: None,
                ai_news_rss_sources: Some(Vec::new()),
                subagents_sources: vec![],
                sync_policy: SyncPolicy::default(),
            },
        );
        merged
    }
}

fn storage_from_device(device: DeviceConfig) -> StorageConfig {
    StorageConfig {
        storage_type: device.storage_type,
        git_url: device.git_url,
        auth_method: device.auth_method,
        http_username: device.http_username,
        http_token: device.http_token,
        ssh_key_path: device.ssh_key_path,
        main_shortcut: device.main_shortcut,
        quick_ai_shortcut: device.quick_ai_shortcut,
        default_ai_dir: device.default_ai_dir,
        claude_provider_launch_dir: device.claude_provider_launch_dir,
        default_ai_model: device.default_ai_model,
        ai_terminal_app: device.ai_terminal_app,
        ai_model_launch_commands: device.ai_model_launch_commands,
        ai_model_permission_modes: device.ai_model_permission_modes,
        ai_sessions_history_days: device.ai_sessions_history_days,
        message_retention_days: device.message_retention_days,
        language: device.language,
        local_storage_path: device.local_storage_path,
        icloud_storage_path: device.icloud_storage_path,
        proxy: device.proxy,
        launch_at_login: device.launch_at_login,
        auto_update_enabled: device.auto_update_enabled,
        update_check_interval_minutes: device.update_check_interval_minutes,
        update_last_checked_at: device.update_last_checked_at,
        update_ignored_version: device.update_ignored_version,
        skills_sync_enabled: Some(true),
        skills_auto_update_enabled: Some(false),
        skills_sync_interval_minutes: Some(60),
        skills_new_badge_hours: Some(72),
        skills_last_synced_at: None,
        skills_sources: vec![],
        subagents_sync_enabled: Some(true),
        subagents_sync_interval_minutes: Some(60),
        subagents_new_badge_hours: Some(72),
        subagents_last_synced_at: None,
        ai_news_enabled: Some(false),
        ai_news_sync_interval_minutes: Some(60),
        ai_news_retention_days: Some(90),
        ai_news_retention_max_items: Some(1000),
        ai_news_keywords: Some(DEFAULT_AI_NEWS_KEYWORDS.to_string()),
        ai_news_last_synced_at: None,
        ai_news_rss_sources: Vec::new(),
        subagents_sources: vec![],
        sync_policy: SyncPolicy::default(),
        is_encrypted: device.is_encrypted,
    }
}

fn device_from_storage(config: &StorageConfig) -> DeviceConfig {
    DeviceConfig {
        storage_type: config.storage_type.clone(),
        git_url: config.git_url.clone(),
        auth_method: config.auth_method.clone(),
        http_username: config.http_username.clone(),
        http_token: config.http_token.clone(),
        ssh_key_path: config.ssh_key_path.clone(),
        main_shortcut: config.main_shortcut.clone(),
        quick_ai_shortcut: config.quick_ai_shortcut.clone(),
        default_ai_dir: config.default_ai_dir.clone(),
        claude_provider_launch_dir: config.claude_provider_launch_dir.clone(),
        default_ai_model: config.default_ai_model.clone(),
        ai_terminal_app: config.ai_terminal_app.clone(),
        ai_model_launch_commands: config.ai_model_launch_commands.clone(),
        ai_model_permission_modes: config.ai_model_permission_modes.clone(),
        ai_sessions_history_days: config.ai_sessions_history_days,
        message_retention_days: config.message_retention_days,
        language: config.language.clone(),
        local_storage_path: config.local_storage_path.clone(),
        icloud_storage_path: config.icloud_storage_path.clone(),
        proxy: config.proxy.clone(),
        launch_at_login: config.launch_at_login,
        auto_update_enabled: config.auto_update_enabled,
        update_check_interval_minutes: config.update_check_interval_minutes,
        update_last_checked_at: config.update_last_checked_at,
        update_ignored_version: config.update_ignored_version.clone(),
        is_encrypted: config.is_encrypted,
    }
}

fn shared_profile_from_storage(config: &StorageConfig) -> SharedProfile {
    SharedProfile {
        skills_sync_enabled: config.skills_sync_enabled,
        skills_auto_update_enabled: config.skills_auto_update_enabled,
        skills_sync_interval_minutes: config.skills_sync_interval_minutes,
        skills_new_badge_hours: config.skills_new_badge_hours,
        skills_last_synced_at: config.skills_last_synced_at,
        skills_sources: config.skills_sources.clone(),
        subagents_sync_enabled: config.subagents_sync_enabled,
        subagents_sync_interval_minutes: config.subagents_sync_interval_minutes,
        subagents_new_badge_hours: config.subagents_new_badge_hours,
        subagents_last_synced_at: config.subagents_last_synced_at,
        ai_news_enabled: config.ai_news_enabled,
        ai_news_sync_interval_minutes: config.ai_news_sync_interval_minutes,
        ai_news_retention_days: config.ai_news_retention_days,
        ai_news_retention_max_items: config.ai_news_retention_max_items,
        ai_news_keywords: config.ai_news_keywords.clone(),
        ai_news_last_synced_at: config.ai_news_last_synced_at,
        ai_news_rss_sources: Some(config.ai_news_rss_sources.clone()),
        subagents_sources: config.subagents_sources.clone(),
        sync_policy: config.sync_policy.clone(),
    }
}

fn apply_shared_profile(config: &mut StorageConfig, profile: &SharedProfile) {
    config.skills_sync_enabled = profile.skills_sync_enabled;
    config.skills_auto_update_enabled = profile.skills_auto_update_enabled;
    config.skills_sync_interval_minutes = profile.skills_sync_interval_minutes;
    config.skills_new_badge_hours = profile.skills_new_badge_hours;
    config.skills_last_synced_at = profile.skills_last_synced_at;
    config.skills_sources = profile.skills_sources.clone();
    config.subagents_sync_enabled = profile.subagents_sync_enabled;
    config.subagents_sync_interval_minutes = profile.subagents_sync_interval_minutes;
    config.subagents_new_badge_hours = profile.subagents_new_badge_hours;
    config.subagents_last_synced_at = profile.subagents_last_synced_at;
    config.ai_news_enabled = profile.ai_news_enabled;
    config.ai_news_sync_interval_minutes = profile.ai_news_sync_interval_minutes;
    config.ai_news_retention_days = profile.ai_news_retention_days;
    config.ai_news_retention_max_items = profile.ai_news_retention_max_items;
    config.ai_news_keywords = profile.ai_news_keywords.clone();
    config.ai_news_last_synced_at = profile.ai_news_last_synced_at;
    config.ai_news_rss_sources = profile.ai_news_rss_sources.clone().unwrap_or_default();
    config.subagents_sources = profile.subagents_sources.clone();
    config.sync_policy = profile.sync_policy.clone();
}

fn load_legacy_storage_config_from_device_file() -> Result<Option<StorageConfig>, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(serde_json::from_str::<StorageConfig>(&content).ok())
}

fn config_path() -> Result<PathBuf, String> {
    Ok(get_app_dir()?.join("config.json"))
}

pub fn get_app_dir() -> Result<PathBuf, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let app_dir = home_dir.join(".config").join("onespace");
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }
    Ok(app_dir)
}

fn copy_tree_if_missing(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree_if_missing(&from, &to)?;
        } else if from.is_file() && !to.exists() {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_entry_if_missing(src_root: &Path, dst_root: &Path, rel: &str) -> Result<(), String> {
    let src = src_root.join(rel);
    let dst = dst_root.join(rel);
    if src.is_dir() {
        return copy_tree_if_missing(&src, &dst);
    }
    if src.is_file() && !dst.exists() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(src, dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn resolve_selected_storage_root_from_device(cfg: &DeviceConfig) -> Result<PathBuf, String> {
    let app_dir = get_app_dir()?;
    let root = match cfg.storage_type.as_str() {
        "git" => app_dir.join("git_data"),
        "icloud" => {
            #[cfg(target_os = "macos")]
            {
                if let Some(ref custom_path) = cfg.icloud_storage_path {
                    PathBuf::from(custom_path)
                } else {
                    dirs::home_dir()
                        .ok_or("Home dir not found")?
                        .join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                dirs::home_dir()
                    .ok_or("Home dir not found")?
                    .join(".config")
                    .join("onespace")
                    .join("data")
            }
        }
        _ => {
            if let Some(ref custom_path) = cfg.local_storage_path {
                PathBuf::from(custom_path)
            } else {
                dirs::home_dir()
                    .ok_or("Home dir not found")?
                    .join(".config")
                    .join("onespace")
                    .join("data")
            }
        }
    };

    Ok(root)
}

fn local_data_init_marker(local_root: &Path) -> PathBuf {
    local_root.join(".local_mirror_initialized_v1")
}

fn ensure_local_data_mirror_initialized(local_root: &Path) -> Result<(), String> {
    let marker = local_data_init_marker(local_root);
    if marker.exists() {
        return Ok(());
    }

    fs::create_dir_all(local_root).map_err(|e| e.to_string())?;

    let device_cfg = get_device_config()?;
    let legacy_root = resolve_selected_storage_root_from_device(&device_cfg)?;
    if legacy_root.exists() && legacy_root != local_root {
        // Copy only known OneSpace data domains to avoid pulling huge storage trees
        // (e.g. git metadata or unrelated folders) into local mirror on first run.
        for rel in [
            "data",
            "shared",
            "workflow_presets.json",
            "workflow_runs.json",
            "ai_providers.json",
            "providers.json",
            "snippets.json",
            "bookmarks.json",
            "notes.json",
            "mcp_servers.json",
            "backups",
        ] {
            if let Err(err) = copy_entry_if_missing(&legacy_root, local_root, rel) {
                eprintln!(
                    "local_data_mirror_init: failed to copy {} from {} -> {}: {}",
                    rel,
                    legacy_root.display(),
                    local_root.display(),
                    err
                );
            }
        }
    }

    fs::write(&marker, now_marker_value()).map_err(|e| e.to_string())?;
    Ok(())
}

fn now_marker_value() -> String {
    format!("initialized_at={}", chrono::Utc::now().timestamp())
}

pub fn get_local_data_dir() -> Result<PathBuf, String> {
    let local_root = get_app_dir()?.join("local_data");
    if !local_root.exists() {
        fs::create_dir_all(&local_root).map_err(|e| e.to_string())?;
    }
    ensure_local_data_mirror_initialized(&local_root)?;
    Ok(local_root)
}

pub fn shared_profile_local_path() -> Result<PathBuf, String> {
    Ok(get_local_data_dir()?
        .join("shared")
        .join("profile")
        .join("skills_sources.json"))
}

pub fn load_shared_profile_local() -> Result<Option<SharedProfile>, String> {
    let path = shared_profile_local_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let parsed = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Some(parsed))
}

pub fn save_shared_profile_local(profile: &SharedProfile) -> Result<(), String> {
    let path = shared_profile_local_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    crate::atomic_write_string(&path, &content)
}

pub fn resolve_shared_storage_root(config: &StorageConfig) -> Result<PathBuf, String> {
    let app_dir = get_app_dir()?;
    let root = match config.storage_type.as_str() {
        "git" => app_dir.join("git_data"),
        "icloud" => {
            #[cfg(target_os = "macos")]
            {
                if let Some(ref custom_path) = config.icloud_storage_path {
                    PathBuf::from(custom_path)
                } else {
                    dirs::home_dir()
                        .ok_or("Home dir not found")?
                        .join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                dirs::home_dir()
                    .ok_or("Home dir not found")?
                    .join(".config")
                    .join("onespace")
                    .join("data")
            }
        }
        _ => {
            if let Some(ref custom_path) = config.local_storage_path {
                PathBuf::from(custom_path)
            } else {
                dirs::home_dir()
                    .ok_or("Home dir not found")?
                    .join(".config")
                    .join("onespace")
                    .join("data")
            }
        }
    };
    Ok(root)
}

pub fn get_shared_data_dir_for(config: &StorageConfig) -> Result<PathBuf, String> {
    let root = resolve_shared_storage_root(config)?;
    let shared = root.join("shared");
    fs::create_dir_all(&shared).map_err(|e| e.to_string())?;
    Ok(shared)
}

fn migrate_shared_storage_if_needed(
    old_cfg: &StorageConfig,
    new_cfg: &StorageConfig,
) -> Result<(), String> {
    let changed = old_cfg.storage_type != new_cfg.storage_type
        || (new_cfg.storage_type == "local"
            && old_cfg.local_storage_path != new_cfg.local_storage_path)
        || (new_cfg.storage_type == "icloud"
            && old_cfg.icloud_storage_path != new_cfg.icloud_storage_path);

    if !changed {
        return Ok(());
    }

    let src = get_shared_data_dir_for(old_cfg)?;
    let dst = get_shared_data_dir_for(new_cfg)?;
    if src != dst && src.exists() {
        copy_tree_if_missing(&src, &dst)?;
    }
    Ok(())
}

fn get_device_config() -> Result<DeviceConfig, String> {
    let config_path = config_path()?;

    if !config_path.exists() {
        return Ok(DeviceConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(DeviceConfig::default());
    }

    if let Ok(device) = serde_json::from_str::<DeviceConfig>(&content) {
        return Ok(device);
    }

    if let Ok(legacy) = serde_json::from_str::<StorageConfig>(&content) {
        return Ok(device_from_storage(&legacy));
    }

    Ok(DeviceConfig::default())
}

fn save_device_config(device: &DeviceConfig) -> Result<(), String> {
    let config_path = config_path()?;
    let content = serde_json::to_string_pretty(device).map_err(|e| e.to_string())?;
    crate::atomic_write_string(&config_path, &content)
}

pub fn get_config() -> Result<StorageConfig, String> {
    let device = get_device_config()?;
    let mut merged = storage_from_device(device);

    match load_shared_profile_local() {
        Ok(Some(shared)) => apply_shared_profile(&mut merged, &shared),
        Ok(None) => {
            if let Some(legacy) = load_legacy_storage_config_from_device_file()? {
                let profile = shared_profile_from_storage(&legacy);
                if !profile.is_effectively_empty() {
                    let _ = save_shared_profile_local(&profile);
                    apply_shared_profile(&mut merged, &profile);
                }
            } else {
                apply_shared_profile(&mut merged, &SharedProfile::default());
            }
        }
        Err(_) => {
            if let Some(legacy) = load_legacy_storage_config_from_device_file()? {
                let profile = shared_profile_from_storage(&legacy);
                if !profile.is_effectively_empty() {
                    let _ = save_shared_profile_local(&profile);
                    apply_shared_profile(&mut merged, &profile);
                }
            } else {
                apply_shared_profile(&mut merged, &SharedProfile::default());
            }
        }
    }

    Ok(merged)
}

#[tauri::command]
pub fn should_show_onboarding() -> Result<bool, String> {
    Ok(!config_path()?.exists())
}

#[tauri::command]
pub fn get_storage_config() -> Result<StorageConfig, String> {
    let mut config = get_config()?;
    let password = crate::crypto::get_or_init_master_password()?;

    if config.is_encrypted {
        if let Some(token) = &config.http_token {
            if !token.is_empty() {
                if let Ok(decrypted) = crate::crypto::decrypt(token, &password) {
                    config.http_token = Some(decrypted);
                }
            }
        }
    }

    if let Some(ref mut proxy) = config.proxy {
        if let Some(ref pass) = proxy.proxy_password {
            if !pass.is_empty() {
                proxy.proxy_password = Some("********".to_string());
            }
        }
    }
    Ok(config)
}

fn normalize_ai_news_keywords(input: Option<String>) -> Option<String> {
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

#[tauri::command]
pub async fn save_shared_profile(
    app: tauri::AppHandle,
    mut profile: SharedProfile,
) -> Result<(), String> {
    let badge_hours = profile.skills_new_badge_hours.unwrap_or(72);
    profile.skills_new_badge_hours = Some(badge_hours.clamp(1, 720));
    let subagent_badge_hours = profile.subagents_new_badge_hours.unwrap_or(72);
    profile.subagents_new_badge_hours = Some(subagent_badge_hours.clamp(1, 720));
    let news_interval = profile.ai_news_sync_interval_minutes.unwrap_or(60);
    profile.ai_news_sync_interval_minutes = Some(news_interval.clamp(5, 1440));
    let news_retention_days = profile.ai_news_retention_days.unwrap_or(90);
    profile.ai_news_retention_days = Some(news_retention_days.clamp(1, 3650));
    let news_retention_items = profile.ai_news_retention_max_items.unwrap_or(1000);
    profile.ai_news_retention_max_items = Some(news_retention_items.clamp(10, 100000));
    profile.ai_news_keywords = normalize_ai_news_keywords(profile.ai_news_keywords);
    save_shared_profile_local(&profile)?;

    let _ = crate::app_store::sync_enqueue(app, "save_shared_profile".to_string()).await;
    Ok(())
}

#[tauri::command]
pub async fn save_storage_config(
    app: tauri::AppHandle,
    mut config: StorageConfig,
) -> Result<(), String> {
    let old_config = get_config()?;
    migrate_shared_storage_if_needed(&old_config, &config)?;

    let old_device = get_device_config()?;
    let old_device_as_storage = storage_from_device(old_device);

    let master_pass = crate::crypto::get_or_init_master_password()?;

    let mut profile = shared_profile_from_storage(&config);
    let badge_hours = profile.skills_new_badge_hours.unwrap_or(72);
    profile.skills_new_badge_hours = Some(badge_hours.clamp(1, 720));
    let subagent_badge_hours = profile.subagents_new_badge_hours.unwrap_or(72);
    profile.subagents_new_badge_hours = Some(subagent_badge_hours.clamp(1, 720));
    let news_interval = profile.ai_news_sync_interval_minutes.unwrap_or(60);
    profile.ai_news_sync_interval_minutes = Some(news_interval.clamp(5, 1440));
    let news_retention_days = profile.ai_news_retention_days.unwrap_or(90);
    profile.ai_news_retention_days = Some(news_retention_days.clamp(1, 3650));
    let news_retention_items = profile.ai_news_retention_max_items.unwrap_or(1000);
    profile.ai_news_retention_max_items = Some(news_retention_items.clamp(10, 100000));
    profile.ai_news_keywords = normalize_ai_news_keywords(profile.ai_news_keywords);
    save_shared_profile_local(&profile)?;

    if let Some(ref mut proxy) = config.proxy {
        if let Some(pass) = &proxy.proxy_password {
            if pass == "********" {
                proxy.proxy_password = old_device_as_storage
                    .proxy
                    .as_ref()
                    .and_then(|p| p.proxy_password.clone());
            } else if pass.is_empty() {
                proxy.proxy_password = None;
            } else {
                proxy.proxy_password = Some(crate::crypto::encrypt(pass, &master_pass)?);
            }
        }

        if let Some(mgr) = crate::proxy::PROXY_MANAGER.get() {
            mgr.update_config(proxy.clone())?;
        }
        crate::proxy::apply_process_proxy_env(Some(proxy))?;
    } else {
        crate::proxy::apply_process_proxy_env(None)?;
    }

    if let Some(token) = &config.http_token {
        if !token.is_empty() {
            config.http_token = Some(crate::crypto::encrypt(token, &master_pass)?);
            config.is_encrypted = true;
        }
    }

    let message_retention_days = config.message_retention_days.unwrap_or(30);
    config.message_retention_days = Some(message_retention_days.clamp(1, 365));

    config.ai_model_permission_modes = Some(normalize_ai_model_permission_modes(
        config.ai_model_permission_modes,
    ));

    let device = device_from_storage(&config);
    save_device_config(&device)?;

    if let Err(err) = crate::messages::cleanup_for_current_retention(app.clone()) {
        eprintln!("messages retention cleanup failed: {}", err);
    }

    let _ = crate::app_store::sync_enqueue(app, "save_storage_config".to_string()).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_shared_profile, AiNewsRssSource, SharedProfile, StorageConfig, SyncPolicy};

    #[test]
    fn sync_policy_default_disables_skills_repository() {
        let policy = SyncPolicy::default();
        assert!(!policy.skills_repository);
        assert!(!policy.subagents_repository);
        assert!(!policy.ai_news);
    }

    #[test]
    fn sync_policy_deserialize_without_skills_repository_defaults_to_false() {
        let json = r#"{
            "providers": true,
            "mcp": true,
            "content": true,
            "workflow_presets": true,
            "skills_sources": true
        }"#;

        let policy: SyncPolicy =
            serde_json::from_str(json).expect("sync policy should deserialize");
        assert!(!policy.skills_repository);
        assert!(!policy.subagents_repository);
        assert!(!policy.ai_news);
    }

    #[test]
    fn ai_news_rss_sources_missing_field_and_explicit_empty_stay_empty() {
        let missing: SharedProfile = serde_json::from_str("{}").expect("profile");
        let mut missing_cfg = StorageConfig::default();
        apply_shared_profile(&mut missing_cfg, &missing);
        assert!(missing_cfg.ai_news_rss_sources.is_empty());

        let storage_missing: StorageConfig =
            serde_json::from_str(r#"{"storage_type":"local"}"#).expect("storage config");
        assert!(storage_missing.ai_news_rss_sources.is_empty());

        let empty: SharedProfile =
            serde_json::from_str(r#"{"ai_news_rss_sources":[]}"#).expect("profile");
        let mut empty_cfg = StorageConfig::default();
        apply_shared_profile(&mut empty_cfg, &empty);
        assert!(empty_cfg.ai_news_rss_sources.is_empty());
    }

    #[test]
    fn ai_news_rss_sources_existing_custom_and_builtin_sources_are_kept() {
        let profile = SharedProfile {
            ai_news_rss_sources: Some(vec![
                AiNewsRssSource {
                    id: "custom".to_string(),
                    name: "Custom".to_string(),
                    url: "https://example.com/feed.xml".to_string(),
                    enabled: true,
                },
                AiNewsRssSource {
                    id: "36kr".to_string(),
                    name: "Custom 36Kr Name".to_string(),
                    url: "https://example.com/36kr.xml".to_string(),
                    enabled: false,
                },
            ]),
            ..SharedProfile::default()
        };
        let mut cfg = StorageConfig::default();

        apply_shared_profile(&mut cfg, &profile);

        assert_eq!(cfg.ai_news_rss_sources.len(), 2);
        assert!(cfg.ai_news_rss_sources.iter().any(|source| {
            source.id == "custom" && source.url == "https://example.com/feed.xml"
        }));
        assert!(cfg.ai_news_rss_sources.iter().any(|source| {
            source.id == "36kr"
                && source.name == "Custom 36Kr Name"
                && source.url == "https://example.com/36kr.xml"
                && !source.enabled
        }));
    }
}
