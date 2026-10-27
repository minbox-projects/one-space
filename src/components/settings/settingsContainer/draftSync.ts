import type { Dispatch, SetStateAction } from "react";
import type {
  LegacySettingsSection,
  ProxyConfig,
  StorageConfig,
} from "../types";
import {
  detectNewsRetentionPreset,
  formatAiNewsSyncIntervalInput,
  normalizeAiModelLaunchCommandsForUi,
  normalizeAiModelPermissionModesForUi,
  normalizeSyncPolicyForUi,
  type NewsRetentionPreset,
} from "../helpers";

export interface DraftSyncSetters {
  setProxyConfig: Dispatch<SetStateAction<ProxyConfig>>;
  setAuthEnabled: Dispatch<SetStateAction<boolean>>;
  setNewsRetentionPreset: Dispatch<SetStateAction<NewsRetentionPreset>>;
  setAiNewsSyncIntervalInput: Dispatch<SetStateAction<string>>;
  setSavedAiNewsSyncIntervalInput: Dispatch<SetStateAction<string>>;
  setConfig: Dispatch<SetStateAction<StorageConfig>>;
}

export function syncDraftWithLatestForTab(
  tab: LegacySettingsSection,
  latestCfg: StorageConfig,
  latestProxy: ProxyConfig,
  setters: DraftSyncSetters,
) {
  const {
    setProxyConfig,
    setAuthEnabled,
    setNewsRetentionPreset,
    setAiNewsSyncIntervalInput,
    setSavedAiNewsSyncIntervalInput,
    setConfig,
  } = setters;

  if (tab === "proxy") {
    setProxyConfig(latestProxy);
    setAuthEnabled(
      !!(latestProxy.proxy_username || latestProxy.proxy_password),
    );
    return;
  }

  if (tab === "news") {
    setNewsRetentionPreset(
      detectNewsRetentionPreset(
        latestCfg.ai_news_retention_days,
        latestCfg.ai_news_retention_max_items,
      ),
    );
    const nextIntervalInput = formatAiNewsSyncIntervalInput(
      latestCfg.ai_news_sync_interval_minutes,
    );
    setAiNewsSyncIntervalInput(nextIntervalInput);
    setSavedAiNewsSyncIntervalInput(nextIntervalInput);
  }

  setConfig((prev) => {
    const next = { ...prev };
    switch (tab) {
      case "storage":
        next.storage_type = latestCfg.storage_type;
        next.git_url = latestCfg.git_url;
        next.auth_method = latestCfg.auth_method;
        next.http_username = latestCfg.http_username;
        next.http_token = latestCfg.http_token;
        next.ssh_key_path = latestCfg.ssh_key_path;
        next.local_storage_path = latestCfg.local_storage_path;
        next.icloud_storage_path = latestCfg.icloud_storage_path;
        next.sync_policy = normalizeSyncPolicyForUi(latestCfg.sync_policy);
        break;
      case "news":
        next.ai_news_enabled = latestCfg.ai_news_enabled;
        next.ai_news_sync_interval_minutes =
          latestCfg.ai_news_sync_interval_minutes;
        next.ai_news_retention_days = latestCfg.ai_news_retention_days;
        next.ai_news_retention_max_items =
          latestCfg.ai_news_retention_max_items;
        next.ai_news_keywords = latestCfg.ai_news_keywords;
        next.ai_news_rss_sources = [
          ...(latestCfg.ai_news_rss_sources || []),
        ];
        break;
      case "updates":
        next.auto_update_enabled = latestCfg.auto_update_enabled;
        next.update_check_interval_minutes =
          latestCfg.update_check_interval_minutes;
        break;
      case "general":
        next.launch_at_login = latestCfg.launch_at_login;
        next.message_retention_days = latestCfg.message_retention_days;
        next.default_sftp_download_dir = latestCfg.default_sftp_download_dir;
        break;
      case "skills":
        next.skills_sync_enabled = latestCfg.skills_sync_enabled;
        next.skills_auto_update_enabled = latestCfg.skills_auto_update_enabled;
        next.skills_sync_interval_minutes =
          latestCfg.skills_sync_interval_minutes;
        next.skills_new_badge_hours = latestCfg.skills_new_badge_hours;
        next.skills_sources = [...(latestCfg.skills_sources || [])];
        break;
      case "subagents":
        next.subagents_sync_enabled = latestCfg.subagents_sync_enabled;
        next.subagents_sync_interval_minutes =
          latestCfg.subagents_sync_interval_minutes;
        next.subagents_new_badge_hours = latestCfg.subagents_new_badge_hours;
        next.subagents_sources = [...(latestCfg.subagents_sources || [])];
        break;
      case "shortcuts":
        next.main_shortcut = latestCfg.main_shortcut;
        next.quick_ai_shortcut = latestCfg.quick_ai_shortcut;
        break;
      case "ai":
        next.default_ai_model = latestCfg.default_ai_model;
        next.ai_terminal_app = latestCfg.ai_terminal_app;
        next.default_ai_dir = latestCfg.default_ai_dir;
        next.claude_provider_launch_dir = latestCfg.claude_provider_launch_dir;
        next.ai_model_launch_commands = normalizeAiModelLaunchCommandsForUi(
          latestCfg.ai_model_launch_commands,
        );
        next.ai_model_permission_modes = normalizeAiModelPermissionModesForUi(
          latestCfg.ai_model_permission_modes,
        );
        next.ai_sessions_history_days = latestCfg.ai_sessions_history_days;
        break;
      case "appearance":
        next.language = latestCfg.language;
        break;
      default:
        break;
    }
    return next;
  });
}
