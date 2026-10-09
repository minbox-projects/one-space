import { invoke } from '@tauri-apps/api/core';

export type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
  code?: string;
  message?: string;
  details?: unknown;
};

export type OpenCodeProviderConfig = Record<string, unknown>;

export type CliTool = 'claude' | 'codex' | 'antigravity' | 'opencode';

export interface HistoryEntry {
  timestamp: number;
  ts?: number;
  content?: string;
  snapshot?: AiProvider;
  action?: string;
  summary?: string;
}

export interface AiProvider {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
  favorite_at?: number | null;
  tool_config?: Record<string, any>;

  // Claude 专属模型路由
  claude_api_format?: string;
  claude_connection_mode?: string;
  claude_default_model?: string; // ANTHROPIC_MODEL - 通用默认模型
  claude_reasoning_effort?: string;

  // Claude 高级配置
  dangerously_skip_permissions?: boolean;
  enable_all_memory_features?: boolean;
  enable_mcp?: boolean;
  allowed_tools?: string[];
  blocked_tools?: string[];
  max_session_turns?: number;

  // Codex 高级配置
  disable_response_storage?: boolean;
  personality?: string;
  wire_api?: string;

  // Codex 新增配置参数
  model_reasoning_effort?: string;  // "minimal" | "low" | "medium" | "high" | "xhigh"
  model_reasoning_summary?: string; // "auto" | "concise" | "detailed" | "none"
  approval_policy?: string;         // "untrusted" | "on-failure" | "on-request" | "never"
  sandbox_mode?: string;            // "read-only" | "workspace-write"

  // Antigravity 高级配置
  antigravity_auth_type?: string;

  // Antigravity 新增配置参数
  theme?: string;                   // "Default" | "GitHub Dark" | "Light"
  vim_mode?: boolean;               // Vim 键盘绑定
  default_approval_mode?: string;   // "default" | "auto_edit" | "plan"

  // OpenCode 全局配置
  opencode_default_model?: string;
  opencode_default_agent?: string;
  opencode_sessions_dir?: string;

  // OpenCode 新增配置参数
  small_model?: string;             // 轻量任务模型
  timeout?: number;                 // 请求超时 (毫秒)
  share_mode?: string;              // "manual" | "auto" | "disabled"
  env_managed?: boolean;

  is_enabled?: boolean;
  provider_key?: string;
  code?: string;
  history?: HistoryEntry[];
  [key: string]: any;
}

export interface AiProvidersState {
  active_claude: string | null;
  active_codex: string | null;
  active_antigravity: string | null;
  active_opencode: string[];
  providers: AiProvider[];
  is_encrypted?: boolean;
}

export interface ClaudeProfileSummary {
  id: string;
  name: string;
  icon?: string | null;
  code: string | null;
  config_dir: string;
  is_default: boolean;
  is_global: boolean;
  favorite_at?: number | null;
  auth_type: string;
  model: string | null;
  claude_api_format?: string;
  claude_connection_mode?: string;
  tool_config: Record<string, any>;
  raw_api_key?: string;
  raw_base_url?: string | null;
  tilde_config_dir?: string;
  claude_model_mappings?: Array<{
    family?: string;
    display_name?: string;
    upstream_model?: string;
    supports_1m?: boolean;
    supported_capabilities?: string[];
  }>;
}

export type AutoImportResult = {
  imported: boolean;
  reason?: string;
  provider_id?: string;
  tool?: string;
  activated?: boolean;
  missing_fields?: string[];
};

export type ProvidersExportResult = {
  path: string;
  count: number;
};

export type ProviderImportPreviewItem = {
  import_key: string;
  id: string;
  name: string;
  tool: string;
  model?: string;
  conflict: boolean;
  conflict_reason?: 'id' | 'name';
  existing_id?: string;
  existing_name?: string;
};

export type ProvidersImportPreview = {
  active: Record<string, string>;
  total: number;
  conflicts: number;
  items: ProviderImportPreviewItem[];
};

export type ProviderImportDecision = {
  import_key: string;
  action: 'overwrite' | 'new';
};

export type ProvidersImportApplyResult = {
  imported: number;
  overwritten: number;
  created: number;
  active_restored: number;
  total: number;
};

export type SyncedDeviceProvider = {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
  provider_key?: string;
  is_enabled?: boolean;
};

export type SyncedDeviceProvidersView = {
  device_id: string;
  active?: Record<string, string>;
  providers: SyncedDeviceProvider[];
};

export function getActiveProviderIds(state: AiProvidersState, tool: string): string[] {
  let activeId: string | null;
  switch (tool) {
    case 'opencode': return state.active_opencode;
    case 'claude': activeId = state.active_claude; break;
    case 'codex': activeId = state.active_codex; break;
    case 'antigravity': activeId = state.active_antigravity; break;
    default: return [];
  }
  return activeId ? [activeId] : [];
}

export function serviceProvidersList(): Promise<ApiResp<AiProvidersState>> {
  return invoke<ApiResp<AiProvidersState>>('service_providers_list');
}

export function serviceProvidersUpsert(provider: AiProvider): Promise<ApiResp<AiProvider>> {
  return invoke<ApiResp<AiProvider>>('service_providers_upsert', { provider });
}

export function serviceProvidersSetActive(
  tool: string,
  providerId: string,
): Promise<ApiResp<{ tool: string; provider_id: string }>> {
  return invoke<ApiResp<{ tool: string; provider_id: string }>>('service_providers_set_active', {
    tool,
    providerId,
  });
}

export function serviceProvidersDelete(providerId: string): Promise<ApiResp<{ deleted: boolean }>> {
  return invoke<ApiResp<{ deleted: boolean }>>('service_providers_delete', { providerId });
}

export function serviceProvidersSetFavorite(
  providerId: string,
  favorite: boolean,
): Promise<ApiResp<AiProvider>> {
  return invoke<ApiResp<AiProvider>>('service_providers_set_favorite', { providerId, favorite });
}

export function serviceProvidersListSyncedOtherDevices(): Promise<ApiResp<SyncedDeviceProvidersView[]>> {
  return invoke<ApiResp<SyncedDeviceProvidersView[]>>('service_providers_list_synced_other_devices');
}

export function serviceProvidersAutoImportFromSystem(tool: CliTool): Promise<ApiResp<AutoImportResult>> {
  return invoke<ApiResp<AutoImportResult>>('service_providers_auto_import_from_system', { tool });
}

export function serviceProvidersExport(outputPath: string): Promise<ApiResp<ProvidersExportResult>> {
  return invoke<ApiResp<ProvidersExportResult>>('service_providers_export', { outputPath });
}

export function serviceProvidersImportPreview(importPath: string): Promise<ApiResp<ProvidersImportPreview>> {
  return invoke<ApiResp<ProvidersImportPreview>>('service_providers_import_preview', { importPath });
}

export function serviceProvidersImportApply(
  importPath: string,
  decisions: ProviderImportDecision[],
): Promise<ApiResp<ProvidersImportApplyResult>> {
  return invoke<ApiResp<ProvidersImportApplyResult>>('service_providers_import_apply', {
    importPath,
    decisions,
  });
}

export function serviceProviderFetchModels(provider: Partial<AiProvider>): Promise<string[]> {
  return invoke<string[]>('service_provider_fetch_models', { provider });
}

export function projectionApply(tool: string, providerId: string): Promise<ApiResp<unknown>> {
  return invoke<ApiResp<unknown>>('projection_apply', { tool, providerId });
}

export function claudeProfileList(): Promise<ApiResp<ClaudeProfileSummary[]>> {
  return invoke<ApiResp<ClaudeProfileSummary[]>>('claude_profile_list');
}

export function claudeProfileMaterialize(providerId: string): Promise<ApiResp<unknown>> {
  return invoke<ApiResp<unknown>>('claude_profile_materialize', { providerId });
}

export function getClaudeConfigDir(providerId: string): Promise<string> {
  return invoke<string>('get_claude_config_dir', { providerId });
}

export function serviceProviderReadOpenCodeConfig(
  providerKey: string,
): Promise<ApiResp<OpenCodeProviderConfig>> {
  return invoke<ApiResp<OpenCodeProviderConfig>>('service_provider_read_opencode_config', {
    providerKey,
  });
}
