import { md5Hex } from "@/lib/md5";
import {
  normalizeAiModelPermissionModesForUi,
} from "@/lib/terminalPermissions";
import { skillModelOptions } from "@/components/skillsModelOptions";
import {
  type StorageConfig,
  type ProxyConfig,
  type ProtocolRouterConfig,
  type SyncPolicy,
  type SkillSourceConfig,
  type AiNewsRssSource,
  type AiModelLaunchCommands,
  type LegacySettingsSection,
  DEFAULT_PROXY_CONFIG,
  DEFAULT_PROTOCOL_ROUTER_CONFIG,
  DEFAULT_AI_MODEL_LAUNCH_COMMANDS,
  DEFAULT_SKILL_SOURCE_MODELS,
  BUILTIN_AI_NEWS_RSS_SOURCES,
} from "./types";

export {
  DEFAULT_PROXY_CONFIG,
  DEFAULT_PROTOCOL_ROUTER_CONFIG,
  DEFAULT_AI_MODEL_LAUNCH_COMMANDS,
  DEFAULT_SKILL_SOURCE_MODELS,
  BUILTIN_AI_NEWS_RSS_SOURCES,
  normalizeAiModelPermissionModesForUi,
};

export type NewsRetentionPreset = "7d_200" | "30d_500" | "90d_1000" | "custom";

export const DEFAULT_SYNC_POLICY: SyncPolicy = {
  providers: true,
  mcp: true,
  content: true,
  skills_sources: true,
  skills_repository: false,
  subagents_sources: true,
  subagents_repository: false,
  ai_news: false,
};

export function normalizeSyncPolicyForUi(
  policy?: Partial<SyncPolicy>,
): SyncPolicy {
  return {
    ...DEFAULT_SYNC_POLICY,
    ...(policy || {}),
  };
}

export function normalizeAiModelLaunchCommandsForUi(
  commands?: AiModelLaunchCommands,
): Required<AiModelLaunchCommands> {
  return {
    claude:
      typeof commands?.claude === "string"
        ? commands.claude
        : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.claude,
    antigravity:
      typeof commands?.antigravity === "string"
        ? commands.antigravity
        : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.antigravity,
    codex:
      typeof commands?.codex === "string"
        ? commands.codex
        : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.codex,
    opencode:
      typeof commands?.opencode === "string"
        ? commands.opencode
        : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.opencode,
  };
}

export function normalizeSkillSourcesForUi(
  sources: StorageConfig["skills_sources"],
): SkillSourceConfig[] {
  const validModelIds = new Set<string>(
    skillModelOptions.map((item) => item.id),
  );
  const safeSources = Array.isArray(sources) ? sources : [];
  return safeSources
    .filter(
      (source): source is SkillSourceConfig =>
        !!source && typeof source === "object",
    )
    .map((source) => {
      const normalizedModels = Array.isArray(source.default_models)
        ? source.default_models
            .map((m) => String(m).trim())
            .filter((m) => validModelIds.has(m))
        : [];
      return {
        id: String(source.id || "").trim(),
        name: String(source.name || ""),
        repo_url: String(source.repo_url || "").trim(),
        branch: String(source.branch || "main").trim() || "main",
        base_dir: String(source.base_dir || "/").trim() || "/",
        enabled: source.enabled !== false,
        default_models:
          normalizedModels.length > 0
            ? normalizedModels
            : [...DEFAULT_SKILL_SOURCE_MODELS],
      };
    });
}

export function normalizeAiNewsRssSourcesForUi(
  sources: StorageConfig["ai_news_rss_sources"],
): AiNewsRssSource[] {
  const safeSources = Array.isArray(sources) ? sources : [];
  return safeSources
    .filter(
      (source): source is AiNewsRssSource =>
        !!source && typeof source === "object",
    )
    .map((source) => ({
      id: String(source.id || "").trim(),
      name: String(source.name || ""),
      url: String(source.url || "").trim(),
      enabled: source.enabled !== false,
    }));
}

export function isAiNewsRssSourceConfigured(
  source: AiNewsRssSource,
  sources: AiNewsRssSource[],
): boolean {
  const sourceId = source.id.trim();
  const sourceUrl = source.url.trim();
  return sources.some((configured) => {
    const configuredId = configured.id.trim();
    const configuredUrl = configured.url.trim();
    return (
      (sourceId.length > 0 && configuredId === sourceId) ||
      (sourceUrl.length > 0 && configuredUrl === sourceUrl)
    );
  });
}

export function detectNewsRetentionPreset(
  days?: number,
  maxItems?: number,
): NewsRetentionPreset {
  if (days === 7 && maxItems === 200) return "7d_200";
  if (days === 30 && maxItems === 500) return "30d_500";
  if (days === 90 && maxItems === 1000) return "90d_1000";
  return "custom";
}

export function formatAiNewsSyncIntervalInput(value?: number | null): string {
  return String(value ?? 60);
}

export function parseAiNewsSyncIntervalInput(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed < 10 || parsed > 1440) {
    return null;
  }
  return parsed;
}

export function parseUsageRetentionInput(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 365) {
    return null;
  }
  return parsed;
}

export function parseTemplateAutoRefreshInput(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const parsed = Number(trimmed);
  if (parsed === 0) return 0;
  if (!Number.isInteger(parsed) || parsed < 10 || parsed > 1440) {
    return null;
  }
  return parsed;
}

export function generateRandomMd5String(): string {
  const seed = `${crypto.randomUUID()}-${Date.now()}-${Math.random()}-${Math.random()}`;
  const raw = md5Hex(seed);
  return `${raw.slice(0, 8)}-${raw.slice(8, 12)}-${raw.slice(12, 16)}-${raw.slice(16, 20)}-${raw.slice(20)}`;
}

export function normalizeConfigForUi(
  cfg: StorageConfig,
  fallbackTerminalApp: string,
): StorageConfig {
  return {
    ...cfg,
    storage_type: cfg.storage_type || "local",
    auth_method: cfg.auth_method || "http",
    main_shortcut: cfg.main_shortcut || "Alt+Space",
    quick_ai_shortcut: cfg.quick_ai_shortcut || "Alt+Shift+A",
    default_ai_model: cfg.default_ai_model || "claude",
    claude_provider_launch_dir: cfg.claude_provider_launch_dir || "",
    ai_terminal_app: cfg.ai_terminal_app || fallbackTerminalApp,
    ai_model_launch_commands: normalizeAiModelLaunchCommandsForUi(
      cfg.ai_model_launch_commands,
    ),
    ai_model_permission_modes: normalizeAiModelPermissionModesForUi(
      cfg.ai_model_permission_modes,
    ),
    message_retention_days: cfg.message_retention_days ?? 30,
    launch_at_login: cfg.launch_at_login ?? false,
    auto_update_enabled: cfg.auto_update_enabled ?? false,
    update_check_interval_minutes: cfg.update_check_interval_minutes ?? 360,
    skills_sync_enabled: cfg.skills_sync_enabled ?? true,
    skills_auto_update_enabled: cfg.skills_auto_update_enabled ?? false,
    skills_sync_interval_minutes: cfg.skills_sync_interval_minutes ?? 60,
    skills_new_badge_hours: cfg.skills_new_badge_hours ?? 72,
    skills_sources: normalizeSkillSourcesForUi(cfg.skills_sources),
    subagents_sync_enabled: cfg.subagents_sync_enabled ?? true,
    subagents_sync_interval_minutes: cfg.subagents_sync_interval_minutes ?? 60,
    subagents_new_badge_hours: cfg.subagents_new_badge_hours ?? 72,
    subagents_sources: normalizeSkillSourcesForUi(cfg.subagents_sources),
    ai_news_enabled: cfg.ai_news_enabled ?? false,
    ai_news_sync_interval_minutes: cfg.ai_news_sync_interval_minutes ?? 60,
    ai_news_retention_days: cfg.ai_news_retention_days ?? 90,
    ai_news_retention_max_items: cfg.ai_news_retention_max_items ?? 1000,
    ai_news_keywords: cfg.ai_news_keywords ?? "",
    ai_news_rss_sources: normalizeAiNewsRssSourcesForUi(
      cfg.ai_news_rss_sources,
    ),
    sync_policy: normalizeSyncPolicyForUi(cfg.sync_policy),
  };
}

export function normalizeProxyConfigForUi(proxy?: ProxyConfig): ProxyConfig {
  return {
    ...DEFAULT_PROXY_CONFIG,
    ...(proxy || {}),
    proxy_username: proxy?.proxy_username || "",
    proxy_password: proxy?.proxy_password || "",
  };
}

export function normalizeProtocolRouterConfigForUi(
  config?: ProtocolRouterConfig,
): ProtocolRouterConfig {
  const merged = {
    ...DEFAULT_PROTOCOL_ROUTER_CONFIG,
    ...(config || {}),
    routes: config?.routes || [],
  };
  return {
    ...merged,
    port: Number(merged.port) || DEFAULT_PROTOCOL_ROUTER_CONFIG.port,
    retention_days: Math.min(
      365,
      Math.max(1, Number(merged.retention_days) || 30),
    ),
    routes: merged.routes.map((route: any) => ({
      ...route,
      auth_header: route.auth_header || "Authorization",
      wire_api: route.wire_api || "open_ai_chat",
      default_model: route.default_model || "",
      mappings: route.mappings || [],
      enabled: route.enabled !== false,
    })),
  };
}

export function normalizeSkillSourceForSyncCompare(
  source: Partial<SkillSourceConfig>,
) {
  const validModelIds = new Set<string>(
    skillModelOptions.map((item) => item.id),
  );
  const models = Array.from(
    new Set(
      (source.default_models || [])
        .map((m) => String(m).trim())
        .filter((m) => validModelIds.has(m)),
    ),
  ).sort();
  return {
    id: String(source.id || "").trim(),
    enabled: source.enabled !== false,
    repo_url: String(source.repo_url || "").trim(),
    branch: String(source.branch || "main").trim() || "main",
    base_dir: String(source.base_dir || "/").trim() || "/",
    default_models: models,
  };
}

export function normalizeSkillSourcesForSyncCompare(
  sources: SkillSourceConfig[] = [],
) {
  return sources
    .map((source) => normalizeSkillSourceForSyncCompare(source))
    .sort((a, b) => {
      const aKey = `${a.id}|${a.repo_url}|${a.branch}|${a.base_dir}|${a.enabled}|${a.default_models.join(",")}`;
      const bKey = `${b.id}|${b.repo_url}|${b.branch}|${b.base_dir}|${b.enabled}|${b.default_models.join(",")}`;
      return aKey.localeCompare(bKey);
    });
}

export function normalizeProxyForCompare(proxy: ProxyConfig) {
  return {
    proxy_enabled: !!proxy.proxy_enabled,
    proxy_type: proxy.proxy_type || "socks5",
    proxy_host: proxy.proxy_host || "",
    proxy_port: Number(proxy.proxy_port) || 0,
    proxy_username: proxy.proxy_username || "",
    proxy_password: proxy.proxy_password ? "__set__" : "",
    check_interval: Number(proxy.check_interval) || 15,
  };
}

export function normalizeProtocolRouterForCompare(
  router: ProtocolRouterConfig,
) {
  return {
    port: Number(router.port) || 0,
    token: router.token ? "__set__" : "",
    retention_days: Number(router.retention_days) || 30,
  };
}

export function normalizeAiNewsRssSourcesForCompare(
  sources: AiNewsRssSource[],
) {
  return sources
    .map((source) => ({
      id: source.id.trim(),
      name: source.name.trim(),
      url: source.url.trim(),
      enabled: source.enabled !== false,
    }))
    .sort((a, b) => {
      const aKey = `${a.id}|${a.url}|${a.enabled}`;
      const bKey = `${b.id}|${b.url}|${b.enabled}`;
      return aKey.localeCompare(bKey);
    });
}

export function getTabSnapshot(
  tab: LegacySettingsSection,
  cfg: StorageConfig,
  proxy: ProxyConfig,
) {
  switch (tab) {
    case "storage": {
      const policy = normalizeSyncPolicyForUi(cfg.sync_policy);
      return {
        storage_type: cfg.storage_type,
        git_url: cfg.git_url || "",
        auth_method: cfg.auth_method || "http",
        http_username: cfg.http_username || "",
        http_token: cfg.http_token || "",
        ssh_key_path: cfg.ssh_key_path || "",
        local_storage_path: cfg.local_storage_path || "",
        icloud_storage_path: cfg.icloud_storage_path || "",
        sync_policy: {
          providers: policy.providers,
          mcp: policy.mcp,
          content: policy.content,
          skills_sources: policy.skills_sources,
          skills_repository: policy.skills_repository,
          subagents_sources: policy.subagents_sources,
          subagents_repository: policy.subagents_repository,
          ai_news: policy.ai_news,
        },
      };
    }
    case "news":
      return {
        ai_news_enabled: !!cfg.ai_news_enabled,
        ai_news_retention_days: cfg.ai_news_retention_days ?? 90,
        ai_news_retention_max_items: cfg.ai_news_retention_max_items ?? 1000,
        ai_news_keywords: (cfg.ai_news_keywords || "").trim(),
        ai_news_rss_sources: normalizeAiNewsRssSourcesForCompare(
          cfg.ai_news_rss_sources || [],
        ),
      };
    case "updates":
      return {
        auto_update_enabled: !!cfg.auto_update_enabled,
        update_check_interval_minutes: cfg.update_check_interval_minutes ?? 360,
      };
    case "general":
      return {
        launch_at_login: !!cfg.launch_at_login,
        message_retention_days: cfg.message_retention_days ?? 30,
        default_sftp_download_dir: cfg.default_sftp_download_dir || "",
      };
    case "skills":
      return {
        skills_sync_enabled: !!cfg.skills_sync_enabled,
        skills_auto_update_enabled: !!cfg.skills_auto_update_enabled,
        skills_sync_interval_minutes: cfg.skills_sync_interval_minutes ?? 60,
        skills_new_badge_hours: cfg.skills_new_badge_hours ?? 72,
        skills_sources: normalizeSkillSourcesForSyncCompare(
          cfg.skills_sources || [],
        ),
      };
    case "subagents":
      return {
        subagents_sync_enabled: !!cfg.subagents_sync_enabled,
        subagents_sync_interval_minutes:
          cfg.subagents_sync_interval_minutes ?? 60,
        subagents_new_badge_hours: cfg.subagents_new_badge_hours ?? 72,
        subagents_sources: normalizeSkillSourcesForSyncCompare(
          cfg.subagents_sources || [],
        ),
      };
    case "proxy":
      return normalizeProxyForCompare(proxy);
    case "shortcuts":
      return {
        main_shortcut: cfg.main_shortcut || "",
        quick_ai_shortcut: cfg.quick_ai_shortcut || "",
      };
    case "ai":
      return {
        default_ai_model: cfg.default_ai_model || "claude",
        ai_terminal_app: cfg.ai_terminal_app || "",
        default_ai_dir: cfg.default_ai_dir || "",
        claude_provider_launch_dir: cfg.claude_provider_launch_dir || "",
        ai_model_launch_commands: normalizeAiModelLaunchCommandsForUi(
          cfg.ai_model_launch_commands,
        ),
        ai_model_permission_modes: normalizeAiModelPermissionModesForUi(
          cfg.ai_model_permission_modes,
        ),
        ai_sessions_history_days: cfg.ai_sessions_history_days ?? 30,
      };
    case "appearance":
      return {
        language: cfg.language || "",
      };
    case "security":
    default:
      return null;
  }
}
