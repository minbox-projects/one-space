export type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
};

export type TrayShortcutConfig = {
  main_shortcut?: string | null;
  quick_ai_shortcut?: string | null;
};

export type AppStorageConfig = {
  language?: string;
  storage_type?: "local" | "git" | "icloud";
  update_ignored_version?: string | null;
  auto_update_enabled?: boolean;
  update_check_interval_minutes?: number;
  message_retention_days?: number;
  skills_sync_enabled?: boolean;
  skills_sync_interval_minutes?: number;
  skills_auto_update_enabled?: boolean;
  ai_news_enabled?: boolean;
  ai_news_sync_interval_minutes?: number;
  ai_news_rss_sources?: Array<{
    id: string;
    name: string;
    url: string;
    enabled: boolean;
  }>;
  subagents_sync_enabled?: boolean;
  subagents_sync_interval_minutes?: number;
};

export type RepoAutoUpdateResult = {
  updated_repo_keys: string[];
  updated_skill_names: string[];
  synced_targets: Array<{
    model: string;
    scope: string;
    project_root?: string | null;
    dir_name: string;
  }>;
  updated_repo_count: number;
  synced_target_count: number;
  applied_at: number;
};

export type DashboardCounts = {
  launcher: number;
  workspaces: number;
  sessions: number;
  ssh: number;
  snippets: number;
  bookmarks: number;
  notes: number;
  ai_news: number;
  environments: number;
  skills: number;
  subagents: number;
  mcp_servers: number;
  storage_type?: "local" | "git" | "icloud";
};

export type AppWindowBindings = typeof window & {
  setActiveTab?: (tab: string) => void;
  setSettingsOpen?: (open: boolean) => void;
  setSettingsTab?: (tab: string) => void;
};

export type AppI18n = {
  language: string;
  on: (event: "languageChanged", listener: () => void) => void;
  off: (event: "languageChanged", listener: () => void) => void;
  changeLanguage: (lng: string) => Promise<unknown>;
};

export type AppCounts = {
  launcher: number;
  workspaces: number;
  sessions: number;
  ssh: number;
  snippets: number;
  bookmarks: number;
  notes: number;
  aiNews: number;
  mail: number;
  environments: number;
  skills: number;
  subagents: number;
  mcpServers: number;
};
