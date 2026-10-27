import type {
  LegacySettingsSection,
  ProxyConfig,
  StorageConfig,
} from "../types";
import {
  normalizeAiModelLaunchCommandsForUi,
  normalizeAiModelPermissionModesForUi,
  normalizeAiNewsRssSourcesForCompare,
  normalizeSyncPolicyForUi,
} from "../helpers";

export function buildPayloadForTab(
  tab: LegacySettingsSection,
  draftCfg: StorageConfig,
  draftProxy: ProxyConfig,
  baseCfg: StorageConfig,
): StorageConfig {
  const next: StorageConfig = {
    ...baseCfg,
    skills_sources: [...(baseCfg.skills_sources || [])],
    subagents_sources: [...(baseCfg.subagents_sources || [])],
  };

  switch (tab) {
    case "storage":
      next.storage_type = draftCfg.storage_type;
      next.git_url = draftCfg.git_url;
      next.auth_method = draftCfg.auth_method;
      next.http_username = draftCfg.http_username;
      next.http_token = draftCfg.http_token;
      next.ssh_key_path = draftCfg.ssh_key_path;
      next.local_storage_path = draftCfg.local_storage_path;
      next.icloud_storage_path = draftCfg.icloud_storage_path;
      next.sync_policy = normalizeSyncPolicyForUi(draftCfg.sync_policy);
      break;
    case "news":
      next.ai_news_enabled = draftCfg.ai_news_enabled;
      next.ai_news_sync_interval_minutes =
        draftCfg.ai_news_sync_interval_minutes;
      next.ai_news_retention_days = draftCfg.ai_news_retention_days;
      next.ai_news_retention_max_items = draftCfg.ai_news_retention_max_items;
      next.ai_news_keywords = draftCfg.ai_news_keywords;
      next.ai_news_rss_sources = normalizeAiNewsRssSourcesForCompare(
        draftCfg.ai_news_rss_sources || [],
      );
      break;
    case "updates":
      next.auto_update_enabled = draftCfg.auto_update_enabled;
      next.update_check_interval_minutes =
        draftCfg.update_check_interval_minutes;
      break;
    case "general":
      next.launch_at_login = draftCfg.launch_at_login;
      next.message_retention_days = draftCfg.message_retention_days;
      next.default_sftp_download_dir = draftCfg.default_sftp_download_dir;
      break;
    case "skills":
      next.skills_sync_enabled = draftCfg.skills_sync_enabled;
      next.skills_auto_update_enabled = draftCfg.skills_auto_update_enabled;
      next.skills_sync_interval_minutes =
        draftCfg.skills_sync_interval_minutes;
      next.skills_new_badge_hours = draftCfg.skills_new_badge_hours;
      next.skills_sources = [...(draftCfg.skills_sources || [])];
      break;
    case "subagents":
      next.subagents_sync_enabled = draftCfg.subagents_sync_enabled;
      next.subagents_sync_interval_minutes =
        draftCfg.subagents_sync_interval_minutes;
      next.subagents_new_badge_hours = draftCfg.subagents_new_badge_hours;
      next.subagents_sources = [...(draftCfg.subagents_sources || [])];
      break;
    case "proxy":
      next.proxy = { ...draftProxy };
      break;
    case "shortcuts":
      next.main_shortcut = draftCfg.main_shortcut;
      next.quick_ai_shortcut = draftCfg.quick_ai_shortcut;
      break;
    case "ai":
      next.default_ai_model = draftCfg.default_ai_model;
      next.ai_terminal_app = draftCfg.ai_terminal_app;
      next.default_ai_dir = draftCfg.default_ai_dir;
      next.claude_provider_launch_dir = draftCfg.claude_provider_launch_dir;
      next.ai_model_launch_commands = normalizeAiModelLaunchCommandsForUi(
        draftCfg.ai_model_launch_commands,
      );
      next.ai_model_permission_modes = normalizeAiModelPermissionModesForUi(
        draftCfg.ai_model_permission_modes,
      );
      next.ai_sessions_history_days = draftCfg.ai_sessions_history_days;
      break;
    case "appearance":
      next.language = draftCfg.language;
      break;
    default:
      break;
  }

  return next;
}
