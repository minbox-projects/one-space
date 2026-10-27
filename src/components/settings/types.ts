import type { ProtocolRouterConfig, ProtocolRouterStatus } from "@/lib/protocolRouter";
import type { SslConfig } from "@/lib/sslCertificate";
import type { AiModelPermissionModes } from "@/lib/terminalPermissions";
import { Settings, Bot, Network, ShieldCheck, Newspaper } from "lucide-react";

export type { ProtocolRouterConfig, ProtocolRouterStatus, SslConfig, AiModelPermissionModes };

export type MainSettingsTab =
  | "general"
  | "ai"
  | "services"
  | "data-security"
  | "news";

export type LegacySettingsSection =
  | "storage"
  | "news"
  | "general"
  | "updates"
  | "skills"
  | "subagents"
  | "proxy"
  | "protocol-router"
  | "ai-gateway"
  | "ssl-certificate"
  | "server-monitor"
  | "shortcuts"
  | "ai"
  | "appearance"
  | "security";

export interface ResolvedSettingsRoute {
  mainTab: MainSettingsTab;
  subTab: LegacySettingsSection;
}

export function resolveSettingsTab(input?: string): ResolvedSettingsRoute {
  const normalized = (input || "").trim().toLowerCase();
  switch (normalized) {
    case "appearance":
      return { mainTab: "general", subTab: "appearance" };
    case "shortcuts":
      return { mainTab: "general", subTab: "shortcuts" };
    case "updates":
      return { mainTab: "general", subTab: "updates" };
    case "general":
      return { mainTab: "general", subTab: "general" };

    case "skills":
      return { mainTab: "ai", subTab: "skills" };
    case "subagents":
      return { mainTab: "ai", subTab: "subagents" };
    case "ai":
      return { mainTab: "ai", subTab: "ai" };

    case "proxy":
      return { mainTab: "services", subTab: "proxy" };
    case "protocol-router":
      return { mainTab: "services", subTab: "protocol-router" };
    case "ai-gateway":
      return { mainTab: "services", subTab: "ai-gateway" };
    case "ssl-certificate":
      return { mainTab: "services", subTab: "ssl-certificate" };
    case "server-monitor":
      return { mainTab: "services", subTab: "server-monitor" };
    case "services":
      return { mainTab: "services", subTab: "proxy" };

    case "storage":
      return { mainTab: "data-security", subTab: "storage" };
    case "security":
      return { mainTab: "data-security", subTab: "security" };
    case "data-security":
      return { mainTab: "data-security", subTab: "storage" };

    case "news":
      return { mainTab: "news", subTab: "news" };

    default:
      return { mainTab: "general", subTab: "general" };
  }
}

export interface SyncPolicy {
  providers: boolean;
  mcp: boolean;
  content: boolean;
  skills_sources: boolean;
  skills_repository: boolean;
  subagents_sources: boolean;
  subagents_repository: boolean;
  ai_news: boolean;
}

export interface StorageConfig {
  storage_type: "local" | "git" | "icloud";
  git_url?: string;
  auth_method?: "http" | "ssh";
  http_username?: string;
  http_token?: string;
  ssh_key_path?: string;
  main_shortcut?: string;
  quick_ai_shortcut?: string;
  default_ai_dir?: string;
  default_sftp_download_dir?: string;
  claude_provider_launch_dir?: string;
  default_ai_model?: "claude" | "antigravity" | "codex" | "opencode";
  ai_terminal_app?: string;
  ai_model_launch_commands?: AiModelLaunchCommands;
  ai_model_permission_modes?: AiModelPermissionModes;
  ai_sessions_history_days?: number;
  message_retention_days?: number;
  language?: string;
  local_storage_path?: string;
  icloud_storage_path?: string;
  proxy?: ProxyConfig;
  launch_at_login?: boolean;
  auto_update_enabled?: boolean;
  update_check_interval_minutes?: number;
  update_last_checked_at?: number;
  skills_sync_enabled?: boolean;
  skills_auto_update_enabled?: boolean;
  skills_sync_interval_minutes?: number;
  skills_new_badge_hours?: number;
  skills_last_synced_at?: number;
  skills_sources?: SkillSourceConfig[];
  subagents_sync_enabled?: boolean;
  subagents_sync_interval_minutes?: number;
  subagents_new_badge_hours?: number;
  subagents_last_synced_at?: number;
  subagents_sources?: SkillSourceConfig[];
  ai_news_enabled?: boolean;
  ai_news_sync_interval_minutes?: number;
  ai_news_retention_days?: number;
  ai_news_retention_max_items?: number;
  ai_news_keywords?: string;
  ai_news_last_synced_at?: number;
  ai_news_rss_sources?: AiNewsRssSource[];
  sync_policy?: SyncPolicy;
}

export type AiModelId = "claude" | "antigravity" | "codex" | "opencode";

export interface AiModelLaunchCommands {
  claude?: string;
  antigravity?: string;
  codex?: string;
  opencode?: string;
}

export interface SkillSourceConfig {
  id: string;
  name: string;
  repo_url: string;
  branch?: string;
  base_dir?: string;
  enabled: boolean;
  default_models?: string[];
}

export interface AiNewsRssSource {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
}

export interface AiNewsRssSourceValidation {
  id?: string;
  name?: string;
  url?: string;
}

export interface SkillSourceValidation {
  id?: string;
  repo_url?: string;
  base_dir?: string;
  default_models?: string;
}

export interface ProxyConfig {
  proxy_enabled: boolean;
  proxy_type: "http" | "https" | "socks5";
  proxy_host: string;
  proxy_port: number;
  proxy_username?: string;
  proxy_password?: string;
  check_interval: number;
}

export interface ProxyStatus {
  is_available: boolean;
  latency_ms: number;
  message: string;
  proxy_type: string;
  proxy_host: string;
}

export interface SkillsSourceSyncState {
  source_id: string;
  last_synced_at?: number;
  last_status: string;
  last_error?: string;
}

export interface SkillsSyncState {
  status: string;
  last_error?: string;
  last_sync_at?: number;
  sources: SkillsSourceSyncState[];
}

export interface SubagentSourceDiagnoseSkippedSample {
  rel_path: string;
  reason: string;
}

export interface SubagentSourceDiagnoseResult {
  source_id: string;
  scan_root: string;
  last_commit_sha?: string;
  total_entries: number;
  accepted_entries: number;
  skipped_entries: number;
  skipped_missing_frontmatter: number;
  skipped_missing_name: number;
  skipped_invalid_name: number;
  skipped_read_error: number;
  skipped_other: number;
  skipped_samples: SubagentSourceDiagnoseSkippedSample[];
}

export const DEFAULT_PROXY_CONFIG: ProxyConfig = {
  proxy_enabled: false,
  proxy_type: "socks5",
  proxy_host: "",
  proxy_port: 1080,
  proxy_username: "",
  proxy_password: "",
  check_interval: 15,
};

export const DEFAULT_PROTOCOL_ROUTER_CONFIG: ProtocolRouterConfig = {
  enabled: false,
  port: 17687,
  token: "",
  retention_days: 30,
  routes: [],
};

export const DEFAULT_AI_MODEL_LAUNCH_COMMANDS: Required<AiModelLaunchCommands> = {
  claude: "claude --session-id {session_id}",
  antigravity: "agy",
  codex: "codex",
  opencode: "opencode",
};

export const DEFAULT_SKILL_SOURCE_MODELS = [
  "claude",
  "antigravity",
  "codex",
  "opencode",
] as const;

export const BUILTIN_AI_NEWS_RSS_SOURCES: AiNewsRssSource[] = [
  {
    id: "36kr",
    name: "36Kr",
    url: "https://www.36kr.com/feed",
    enabled: true,
  },
  {
    id: "oschina",
    name: "开源中国",
    url: "https://www.oschina.net/news/rss",
    enabled: true,
  },
];

export const MAIN_SETTINGS_CATEGORIES: {
  id: MainSettingsTab;
  titleKey: string;
  fallbackTitle: string;
  icon: typeof Settings;
  subTabs: { id: LegacySettingsSection; titleKey: string; fallbackTitle: string }[];
}[] = [
  {
    id: "general",
    titleKey: "categoryGeneral",
    fallbackTitle: "常规与系统",
    icon: Settings,
    subTabs: [
      { id: "general", titleKey: "general", fallbackTitle: "常规" },
      { id: "appearance", titleKey: "appearance", fallbackTitle: "外观与语言" },
      { id: "shortcuts", titleKey: "shortcuts", fallbackTitle: "快捷键" },
      { id: "updates", titleKey: "updates", fallbackTitle: "软件更新" },
    ],
  },
  {
    id: "ai",
    titleKey: "categoryAi",
    fallbackTitle: "AI 与扩展生态",
    icon: Bot,
    subTabs: [
      { id: "ai", titleKey: "aiSessions", fallbackTitle: "终端与模型" },
      { id: "skills", titleKey: "skillsSourcesMenu", fallbackTitle: "Skills 源" },
      { id: "subagents", titleKey: "subagentsSourcesMenu", fallbackTitle: "Subagents 源" },
    ],
  },
  {
    id: "services",
    titleKey: "categoryServices",
    fallbackTitle: "网络与服务",
    icon: Network,
    subTabs: [
      { id: "proxy", titleKey: "proxy", fallbackTitle: "网络代理" },
      { id: "protocol-router", titleKey: "protocolRouter", fallbackTitle: "协议路由" },
      { id: "ai-gateway", titleKey: "aiGatewaySettings", fallbackTitle: "AI 网关" },
      { id: "ssl-certificate", titleKey: "sslCertificate", fallbackTitle: "SSL 证书" },
      { id: "server-monitor", titleKey: "serverMonitor", fallbackTitle: "服务器监控" },
    ],
  },
  {
    id: "data-security",
    titleKey: "categoryDataSecurity",
    fallbackTitle: "数据与安全",
    icon: ShieldCheck,
    subTabs: [
      { id: "storage", titleKey: "dataStorageMenu", fallbackTitle: "存储与多端同步" },
      { id: "security", titleKey: "security", fallbackTitle: "主密码与加密" },
    ],
  },
  {
    id: "news",
    titleKey: "newsSettingsMenu",
    fallbackTitle: "资讯与订阅",
    icon: Newspaper,
    subTabs: [
      { id: "news", titleKey: "newsSettingsMenu", fallbackTitle: "资讯设置" },
    ],
  },
];
