use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::normalization::{
    default_ai_model_launch_commands, default_ai_model_permission_modes,
    normalize_ai_news_keywords,
};
use super::storage::{default_storage_type_for, is_dev_build};

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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncPolicy {
    #[serde(default = "default_true")]
    pub providers: bool,
    #[serde(default = "default_true")]
    pub mcp: bool,
    #[serde(default = "default_true")]
    pub content: bool,
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
            skills_sources: true,
            skills_repository: false,
            subagents_sources: true,
            subagents_repository: false,
            ai_news: false,
        }
    }
}

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
    pub(super) fn is_effectively_empty(&self) -> bool {
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
    pub default_sftp_download_dir: Option<String>,
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
    pub default_sftp_download_dir: Option<String>,
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
        Self {
            storage_type: default_storage_type_for(is_dev_build()).to_string(),
            git_url: None,
            auth_method: Some("http".to_string()),
            http_username: None,
            http_token: None,
            ssh_key_path: None,
            main_shortcut: Some("Alt+Space".to_string()),
            quick_ai_shortcut: Some("Alt+Shift+A".to_string()),
            default_ai_dir: None,
            default_sftp_download_dir: None,
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
                ai_news_keywords: None,
                ai_news_last_synced_at: None,
                ai_news_rss_sources: Some(Vec::new()),
                subagents_sources: vec![],
                sync_policy: SyncPolicy::default(),
            },
        );
        merged
    }
}

pub(super) fn storage_from_device(device: DeviceConfig) -> StorageConfig {
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
        default_sftp_download_dir: device.default_sftp_download_dir,
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
        ai_news_keywords: None,
        ai_news_last_synced_at: None,
        ai_news_rss_sources: Vec::new(),
        subagents_sources: vec![],
        sync_policy: SyncPolicy::default(),
        is_encrypted: device.is_encrypted,
    }
}

pub(super) fn device_from_storage(config: &StorageConfig) -> DeviceConfig {
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
        default_sftp_download_dir: config.default_sftp_download_dir.clone(),
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

pub(super) fn shared_profile_from_storage(config: &StorageConfig) -> SharedProfile {
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

pub(super) fn apply_shared_profile(config: &mut StorageConfig, profile: &SharedProfile) {
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
    config.ai_news_keywords = normalize_ai_news_keywords(profile.ai_news_keywords.clone());
    config.ai_news_last_synced_at = profile.ai_news_last_synced_at;
    config.ai_news_rss_sources = profile.ai_news_rss_sources.clone().unwrap_or_default();
    config.subagents_sources = profile.subagents_sources.clone();
    config.sync_policy = profile.sync_policy.clone();
}
