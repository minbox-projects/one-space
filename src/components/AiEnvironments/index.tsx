import { useState, useEffect, useRef, useMemo, useLayoutEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import { v4 as uuidv4 } from 'uuid';
import { useTranslation } from 'react-i18next';
import { Loader2, Pencil, Plus, Settings2, TerminalSquare, Trash2, Upload, X } from 'lucide-react';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './icons';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { CliVersionCards } from './CliVersionCards';
import { ToolSectionHeader } from './ToolSectionHeader';
import { SyncedDevices } from './SyncedDevices';
import { ServiceProviderDetail } from './ServiceProviderDetail';
import { ServiceProviderList, type ServiceProviderListItem } from './ServiceProviderList';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';
import { IconPicker } from './IconPicker';
import { ModelMappingTable } from './ModelMappingTable';
import { TerminalPermissionConfirmDialog } from '../TerminalPermissionConfirmDialog';
import { useToast } from '../ToastProvider';
import { safeRecordMessage } from '@/lib/messages';
import { openLocalPath } from '@/lib/externalActions';
import { runUserAction } from '@/lib/userActions';
import type { TerminalPermissionMode } from '@/lib/terminalPermissions';
import {
  applyProviderPresetToDraft,
  normalizeClaudePresetTemplate,
  type ClaudePresetModelMapping,
  type ServiceProviderPresetRecord,
  type ServiceProviderPresetsState,
} from './providerPresets';

const TOOLS = ['claude', 'codex', 'gemini', 'opencode'] as const;
const MANAGED_TOOLS = ['claude', 'codex', 'gemini'] as const;
type CliTool = (typeof TOOLS)[number];
type EnvManagedState = 'enabled' | 'disabled' | 'unsupported';
type CliVersionState = { version: string; isInstalled: boolean };
type DetectCliVersionResult = { version: string; is_installed: boolean };
type CliInstallCommand = { label: string; command: string };
type CliInstallGuide = { docs_url: string; commands: CliInstallCommand[] };
type CliEnvProbeResult = {
  tool: string;
  installed: boolean;
  version: string;
  configured: boolean;
  importable: boolean;
  install_guide: CliInstallGuide;
};
type CliUpdateInfo = {
  tool: string;
  installed: boolean;
  current_version: string;
  current_version_normalized?: string;
  latest_version?: string;
  latest_source: string;
  latest_url: string;
  update_available: boolean;
  compare_status: string;
  update_command: string;
  error?: string;
};
type CliUpdateApplyResult = {
  tool: string;
  success: boolean;
  terminal_launched: boolean;
  error?: string;
};
type AutoImportResult = {
  imported: boolean;
  reason?: string;
  provider_id?: string;
  tool?: string;
  activated?: boolean;
  missing_fields?: string[];
};
type ProvidersExportResult = {
  path: string;
  count: number;
};
type ProviderImportPreviewItem = {
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
type ProvidersImportPreview = {
  active: Record<string, string>;
  total: number;
  conflicts: number;
  items: ProviderImportPreviewItem[];
};
type ProviderImportDecision = {
  import_key: string;
  action: 'overwrite' | 'new';
};
type ProvidersImportApplyResult = {
  imported: number;
  overwritten: number;
  created: number;
  active_restored: number;
  total: number;
};
type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
  code?: string;
  message?: string;
  details?: unknown;
};
type SyncedDeviceProvider = {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
  provider_key?: string;
  is_enabled?: boolean;
};
type SyncedDeviceProvidersView = {
  device_id: string;
  active?: Record<string, string>;
  providers: SyncedDeviceProvider[];
};
type PresetDialogDraft = {
  id: string;
  name: string;
  description: string;
  icon: string;
  openai_base_url: string;
  anthropic_base_url: string;
  claude_default_model: string;
  claude_reasoning_effort: string;
  claude_model_mappings: ClaudePresetModelMapping[];
};

function errorToDisplayMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message || error.stack || String(error);
  }
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object') {
    const maybe = error as { code?: unknown; message?: unknown; error?: unknown; details?: unknown };
    const code = typeof maybe.code === 'string' ? maybe.code : '';
    const message =
      typeof maybe.message === 'string'
        ? maybe.message
        : typeof maybe.error === 'string'
          ? maybe.error
          : '';
    if (code && message) {
      return `${code}: ${message}`;
    }
    if (message) {
      return message;
    }
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }
  return String(error);
}

function getInvokeErrorCode(error: unknown): string | null {
  if (error && typeof error === 'object') {
    const maybe = error as { code?: unknown };
    if (typeof maybe.code === 'string') return maybe.code;
  }
  return null;
}

function unwrapApiResp<T>(response: ApiResp<T>, fallbackMessage = 'Request failed'): T {
  if (response?.ok && response.data !== undefined) {
    return response.data;
  }
  throw new Error(errorToDisplayMessage(response) || fallbackMessage);
}

export function buildSyncedProviderActivationPayload(
  deviceId: string,
  provider: SyncedDeviceProvider,
  targetId: string = uuidv4(),
  now: () => number = Date.now,
): { targetId: string; targetTool: CliTool; payload: Record<string, any> } | null {
  const deviceSlug = String(deviceId || '')
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  const targetTool = String(provider.tool || '').toLowerCase();
  if (!TOOLS.includes(targetTool as CliTool)) {
    return null;
  }

  const payload: Record<string, any> = {
    id: targetId,
    name: `${provider.name} (${deviceId})`,
    tool: targetTool,
    api_key: String(provider.api_key || '').trim(),
    base_url: provider.base_url || '',
    model: provider.model || '',
    is_enabled: targetTool === 'opencode' ? provider.is_enabled ?? true : true,
    env_managed: targetTool !== 'opencode' ? true : undefined,
  };
  if (targetTool === 'opencode') {
    payload.provider_key =
      provider.provider_key ||
      `synced_${deviceSlug || 'device'}_${now()}`.replace(/[^a-zA-Z]/g, '');
  }

  return { targetId, targetTool: targetTool as CliTool, payload };
}

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
  
  // Gemini 高级配置
  gemini_auth_type?: string;
  
  // Gemini 新增配置参数
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

type ClaudeModelMappingDraft = {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
  supported_capabilities?: string[];
};

type ClaudeRoutingFieldSource = {
  claude_api_format?: string;
  claude_connection_mode?: string;
  tool_config?: Record<string, any>;
};

const getClaudeApiFormat = (source: ClaudeRoutingFieldSource) =>
  source.claude_api_format || source.tool_config?.claude_api_format || 'anthropic_messages';

const getClaudeConnectionMode = (source: ClaudeRoutingFieldSource) => {
  const explicitMode = source.claude_connection_mode || source.tool_config?.claude_connection_mode;
  if (explicitMode) return explicitMode;
  const apiFormat = getClaudeApiFormat(source);
  return apiFormat === 'open_ai_chat' || apiFormat === 'open_ai_responses'
    ? 'protocol_router'
    : 'native_anthropic';
};

const normalizeClaudeModelMappingDraft = (mapping: Partial<ClaudeModelMappingDraft>): ClaudeModelMappingDraft => ({
  family: String(mapping.family || '').trim(),
  display_name: String(mapping.display_name || '').trim(),
  upstream_model: String(mapping.upstream_model || ''),
  supports_1m: !!mapping.supports_1m,
  supported_capabilities: Array.isArray(mapping.supported_capabilities)
    ? mapping.supported_capabilities
        .map((value) => String(value ?? '').trim())
        .filter((value) => value.length > 0)
    : undefined,
});

const normalizeClaudeDefaultModel = (value?: string) => {
  if (typeof value !== 'string') return undefined;
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : undefined;
};

const PRESET_CLAUDE_FAMILIES: Array<Pick<ClaudePresetModelMapping, 'family' | 'display_name'>> = [
  { family: 'haiku', display_name: 'Haiku' },
  { family: 'sonnet', display_name: 'Sonnet' },
  { family: 'opus', display_name: 'Opus' },
];

const buildPresetClaudeMappings = (
  source?: Array<Partial<ClaudePresetModelMapping>> | null,
): ClaudePresetModelMapping[] =>
  PRESET_CLAUDE_FAMILIES.map(({ family, display_name }) => {
    const mapping = source?.find((item) => String(item?.family || '').trim() === family);
    return {
      family,
      display_name: String(mapping?.display_name || display_name).trim() || display_name,
      upstream_model: String(mapping?.upstream_model || ''),
      supports_1m: !!mapping?.supports_1m,
      supported_capabilities: Array.isArray(mapping?.supported_capabilities)
        ? mapping.supported_capabilities
            .map((value) => String(value ?? '').trim())
            .filter((value) => value.length > 0)
        : undefined,
    };
  });

export interface AiProvidersState {
  active_claude: string | null;
  active_codex: string | null;
  active_gemini: string | null;
  active_opencode: string[];
  providers: AiProvider[];
  is_encrypted?: boolean;
}

type FavoriteSortableItem = {
  isActiveForSort: boolean;
  isFavorite: boolean;
  favoriteAt?: number | null;
};

const sortServiceProviderListItems = <T extends FavoriteSortableItem>(items: T[]) =>
  items
    .map((item, index) => ({ item, index }))
    .sort((a, b) => {
      if (a.item.isActiveForSort !== b.item.isActiveForSort) {
        return a.item.isActiveForSort ? -1 : 1;
      }
      if (a.item.isActiveForSort && b.item.isActiveForSort) {
        return a.index - b.index;
      }
      if (a.item.isFavorite !== b.item.isFavorite) {
        return a.item.isFavorite ? -1 : 1;
      }
      if (a.item.isFavorite && b.item.isFavorite) {
        const aTs = a.item.favoriteAt ?? 0;
        const bTs = b.item.favoriteAt ?? 0;
        if (aTs !== bTs) {
          return bTs - aTs;
        }
      }
      return a.index - b.index;
    })
    .map(({ item }) => item);

type SavePresetResult = {
  ok: boolean;
  providerId?: string;
  provider?: AiProvider;
};

type RequiredProviderField = 'api_key' | 'base_url' | 'provider_key' | 'code';

export function getMissingRequiredProviderFields(provider: Partial<AiProvider>): RequiredProviderField[] {
  const missing: RequiredProviderField[] = [];
  if (!String(provider.api_key || '').trim()) {
    missing.push('api_key');
  }
  if (!String(provider.base_url || '').trim()) {
    missing.push('base_url');
  }

  if (String(provider.tool || '').toLowerCase() === 'opencode') {
    if (!String(provider.provider_key || '').trim()) {
      missing.push('provider_key');
    }
  } else if (!String(provider.code || '').trim()) {
    missing.push('code');
  }

  return missing;
}

const DEFAULT_STATE: AiProvidersState = {
  active_claude: null,
  active_codex: null,
  active_gemini: null,
  active_opencode: [],
  providers: [],
  is_encrypted: false
};

export const ToolIcon = ({ tool, className }: { tool: string, className?: string }) => {
  switch (tool.toLowerCase()) {
    case 'claude': return <ClaudeIcon className={className} />;
    case 'codex': return <OpenAIIcon className={className} />;
    case 'gemini': return <GeminiIcon className={className} />;
    case 'opencode': return <OpenCodeIcon className={className} />;
    default: return <TerminalSquare className={className} />;
  }
};

export function AiEnvironments({ isVisible = false }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const confirmDialog = useConfirmDialog();
  const [state, setState] = useState<AiProvidersState>(DEFAULT_STATE);
  const [activeTool, setActiveTool] = useState('claude');
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  
  const [rawJson, setRawJson] = useState('');
  const [originalJson, setOriginalJson] = useState('');
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [applyingGlobal, setApplyingGlobal] = useState(false);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  const [savingDetail, setSavingDetail] = useState(false);
  const [isRollbackMode, setIsRollbackMode] = useState(false);
  const [cliVersions, setCliVersions] = useState<Partial<Record<CliTool, CliVersionState>>>({});
  const [checkingVersions, setCheckingVersions] = useState<Partial<Record<CliTool, boolean>>>({});
  const [checkingAllVersions, setCheckingAllVersions] = useState(false);
  const [cliUpdates, setCliUpdates] = useState<Partial<Record<CliTool, CliUpdateInfo>>>({});
  const [checkingUpdates, setCheckingUpdates] = useState<Partial<Record<CliTool, boolean>>>({});
  const [updatingTool, setUpdatingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const [probingTool, setProbingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const [, setAutoImportInactiveNotice] = useState<Partial<Record<CliTool, string>>>({});
  const [unsavedNewProviderIds, setUnsavedNewProviderIds] = useState<Set<string>>(new Set());
  const [favoritePendingIds, setFavoritePendingIds] = useState<Set<string>>(new Set());
  const [syncedOtherDeviceProviders, setSyncedOtherDeviceProviders] = useState<SyncedDeviceProvidersView[]>([]);
  const [activatingSyncedKey, setActivatingSyncedKey] = useState<string | null>(null);
  const [claudeProfiles, setClaudeProfiles] = useState<ClaudeProfileSummary[]>([]);
  const [copiedClaudeProfileId, setCopiedClaudeProfileId] = useState<string | null>(null);
  const [claudeLaunchCommand, setClaudeLaunchCommand] = useState('claude --session-id {session_id}');
  const [launchingClaudeProfileId, setLaunchingClaudeProfileId] = useState<string | null>(null);
  const [permissionDialogClaudeProfileId, setPermissionDialogClaudeProfileId] = useState<string | null>(null);
  const [permissionDialogOpen, setPermissionDialogOpen] = useState(false);
  const [applyingClaudeProfileId, setApplyingClaudeProfileId] = useState<string | null>(null);
  const [exportingProviders, setExportingProviders] = useState(false);
  const [previewingImport, setPreviewingImport] = useState(false);
  const [applyingImport, setApplyingImport] = useState(false);
  const [importPreview, setImportPreview] = useState<ProvidersImportPreview | null>(null);
  const [importPath, setImportPath] = useState<string | null>(null);
  const [importDecisions, setImportDecisions] = useState<Record<string, 'overwrite' | 'new'>>({});
  const [providerPresets, setProviderPresets] = useState<ServiceProviderPresetRecord[]>([]);
  const [presetPickerOpen, setPresetPickerOpen] = useState(false);
  const [presetManagerOpen, setPresetManagerOpen] = useState(false);
  const [editingPresetId, setEditingPresetId] = useState<string | null>(null);
  const [presetDraft, setPresetDraft] = useState<PresetDialogDraft>({
    id: '',
    name: '',
    description: '',
    icon: '',
    openai_base_url: '',
    anthropic_base_url: '',
    claude_default_model: '',
    claude_reasoning_effort: '',
    claude_model_mappings: buildPresetClaudeMappings(),
  });

  // Accordion state
  const [searchQuery, setSearchQuery] = useState('');

  // Service provider list/detail view mode
  const [viewMode, setViewMode] = useState<'list' | 'detail'>('list');
  const [detailProvider, setDetailProvider] = useState<any | null>(null);
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );

  const versionCheckRunIdRef = useRef(0);
  const probeRunIdRef = useRef(0);
  const isVisibleRef = useRef(isVisible);
  const cliProbeInitializedRef = useRef(false);
  const autoImportInitializedRef = useRef(false);
  const listScrollContainerRef = useRef<HTMLDivElement | null>(null);
  const savedListScrollTopRef = useRef(0);
  const pendingRestoreListScrollTopRef = useRef<number | null>(null);
  const rollbackDraftBeforeRef = useRef<{ provider: AiProvider | null; rawJson: string } | null>(null);

  const isTauri = '__TAURI_INTERNALS__' in window;
  const isManagedTool = (tool: string): tool is (typeof MANAGED_TOOLS)[number] =>
    (MANAGED_TOOLS as readonly string[]).includes(tool);
  const getManagedStateForTool = (tool: CliTool): EnvManagedState => {
    if (!isManagedTool(tool)) return 'unsupported';
    const toolActiveProviderId = state[`active_${tool}` as keyof AiProvidersState] as string | null;
    if (!toolActiveProviderId) return 'disabled';
    const toolActiveProvider =
      state.providers.find(p => p.id === toolActiveProviderId && p.tool === tool) || null;
    if (!toolActiveProvider) return 'disabled';
    return toolActiveProvider.env_managed !== false ? 'enabled' : 'disabled';
  };

  const getIsGlobalForTool = (tool: string, id: string) => {
    if (tool === 'opencode') {
      return state.active_opencode.includes(id);
    }
    return (state[`active_${tool}` as keyof AiProvidersState] as string | null) === id;
  };

  const uniqueProviderCode = (toolName: string, presetName?: string) => {
    const base = `${presetName || toolName}-provider`
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 32) || `${toolName}-provider`;
    const used = new Set(
      state.providers
        .filter((provider) => provider.tool === toolName)
        .map((provider) => String(provider.code || '').trim().toLowerCase())
        .filter(Boolean),
    );
    let candidate = base;
    let index = 2;
    while (used.has(candidate.toLowerCase())) {
      candidate = `${base}-${index}`;
      index += 1;
    }
    return candidate;
  };

  const uniqueOpenCodeProviderKey = (presetName?: string) => {
    const base = `${presetName || 'provider'}`
      .toLowerCase()
      .replace(/[^a-z0-9_]+/g, '_')
      .replace(/^_+|_+$/g, '')
      .slice(0, 32) || 'provider';
    const used = new Set(
      state.providers
        .filter((provider) => provider.tool === 'opencode')
        .map((provider) => String(provider.provider_key || '').trim().toLowerCase())
        .filter(Boolean),
    );
    let candidate = base;
    let index = 2;
    while (used.has(candidate.toLowerCase())) {
      candidate = `${base}_${index}`;
      index += 1;
    }
    return candidate;
  };

  const buildClaudeModelMappings = (source: Record<string, any>): ClaudeModelMappingDraft[] => {
    const explicitMappings = source.claude_model_mappings || source.tool_config?.claude_model_mappings;
    if (Array.isArray(explicitMappings) && explicitMappings.length > 0) {
      return explicitMappings.map((mapping: Partial<ClaudeModelMappingDraft>) =>
        normalizeClaudeModelMappingDraft(mapping),
      );
    }

    const fromLegacyFields = [
      {
        family: 'haiku',
        display_name: 'Haiku',
        upstream_model: String(
          source.claude_haiku_model ||
          source.tool_config?.claude_haiku_model ||
          '',
        ),
        supports_1m: false,
        supported_capabilities: undefined,
      },
      {
        family: 'sonnet',
        display_name: 'Sonnet',
        upstream_model: String(
          source.claude_sonnet_model ||
          source.tool_config?.claude_sonnet_model ||
          '',
        ),
        supports_1m: false,
        supported_capabilities: undefined,
      },
      {
        family: 'opus',
        display_name: 'Opus',
        upstream_model: String(
          source.claude_opus_model ||
          source.tool_config?.claude_opus_model ||
          '',
        ),
        supports_1m: false,
        supported_capabilities: undefined,
      },
    ].filter((mapping) => mapping.upstream_model.trim().length > 0);

    return fromLegacyFields;
  };

  const buildClaudeProviderFromProfile = (profile: ClaudeProfileSummary): Partial<AiProvider> => {
    const defaultModel = normalizeClaudeDefaultModel(
      profile.tool_config?.claude_default_model || profile.model || undefined,
    );
    return ({
    id: profile.id,
    tool: 'claude',
    name: profile.name,
    icon: profile.icon || undefined,
    code: profile.code || undefined,
    api_key: profile.raw_api_key || '',
    base_url: profile.raw_base_url || '',
    model: defaultModel,
    claude_api_format: getClaudeApiFormat(profile),
    claude_connection_mode: getClaudeConnectionMode(profile),
    claude_auth_env_key: profile.tool_config?.claude_auth_env_key || 'ANTHROPIC_API_KEY',
    claude_model_mappings: buildClaudeModelMappings(profile),
    claude_enable_tool_search:
      profile.tool_config?.claude_enable_tool_search ?? profile.tool_config?.enable_tool_search ?? false,
    claude_enable_attribution:
      profile.tool_config?.claude_enable_attribution ?? profile.tool_config?.enable_attribution ?? false,
    claude_auto_memory_enabled: profile.tool_config?.claude_auto_memory_enabled ?? false,
    claude_always_thinking_enabled: profile.tool_config?.claude_always_thinking_enabled ?? false,
    claude_away_summary_enabled: profile.tool_config?.claude_away_summary_enabled ?? false,
    claude_include_git_instructions: profile.tool_config?.claude_include_git_instructions ?? false,
    remark: profile.tool_config?.remark || '',
    claude_default_model: defaultModel,
    claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
    dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
    enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
    enable_mcp: profile.tool_config?.enable_mcp || false,
    allowed_tools: profile.tool_config?.allowed_tools || [],
    blocked_tools: profile.tool_config?.blocked_tools || [],
    max_session_turns: profile.tool_config?.max_session_turns,
    env_managed: true,
    is_enabled: true,
  })};

  const buildClaudeProviderFromState = (provider: AiProvider): Partial<AiProvider> => {
    const defaultModel = normalizeClaudeDefaultModel(
      provider.claude_default_model || provider.model || provider.tool_config?.claude_default_model,
    );
    return ({
    ...provider,
    tool: 'claude',
    model: defaultModel,
    claude_default_model: defaultModel,
    remark: provider.tool_config?.remark || '',
    claude_api_format: getClaudeApiFormat(provider),
    claude_connection_mode: getClaudeConnectionMode(provider),
    claude_model_mappings: buildClaudeModelMappings(provider),
  })};

  const normalizeProviderForSave = (provider: Partial<AiProvider>) => {
    const next: Record<string, any> = { ...provider };
    const nextToolConfig = { ...(provider.tool_config || {}) };
    const remark = typeof provider.remark === 'string' ? provider.remark : '';

    if (remark.trim()) {
      nextToolConfig.remark = remark;
    } else {
      delete nextToolConfig.remark;
    }

    if (provider.tool === 'claude') {
      const defaultModel = normalizeClaudeDefaultModel(provider.claude_default_model || provider.model);
      if (Array.isArray(provider.claude_model_mappings)) {
        next.claude_model_mappings = provider.claude_model_mappings.map((mapping) =>
          normalizeClaudeModelMappingDraft(mapping),
        );
      }
      if (defaultModel) {
        next.model = defaultModel;
        next.claude_default_model = defaultModel;
        nextToolConfig.claude_default_model = defaultModel;
      } else {
        delete next.model;
        delete next.claude_default_model;
        delete nextToolConfig.claude_default_model;
      }
      if (typeof provider.claude_reasoning_effort === 'string') {
        nextToolConfig.claude_reasoning_effort = provider.claude_reasoning_effort;
      } else {
        delete nextToolConfig.claude_reasoning_effort;
      }
    }

    if (provider.tool === 'opencode') {
      for (const key of ['opencode_default_model', 'opencode_default_agent', 'opencode_sessions_dir', 'small_model', 'timeout', 'share_mode']) {
        const value = (provider as Record<string, any>)[key];
        if (value !== undefined && value !== null && value !== '') {
          nextToolConfig[key] = value;
        } else {
          delete nextToolConfig[key];
        }
      }
    }

    next.tool_config = nextToolConfig;
    return next;
  };

  const getOpenCodeJson = (provider: Partial<AiProvider>) => {
    const internalFields = [
      'id', 'tool', 'is_enabled', 'provider_key', 'api_key', 'base_url', 'model',
      'claude_default_model', 'dangerously_skip_permissions', 'history',
      'enable_all_memory_features', 'enable_mcp', 'allowed_tools', 'blocked_tools',
      'max_session_turns', 'disable_response_storage', 'personality', 'wire_api',
      'gemini_auth_type', 'opencode_default_model', 'opencode_default_agent',
      'opencode_sessions_dir', 'model_reasoning_effort', 'model_reasoning_summary',
      'approval_policy', 'sandbox_mode', 'theme', 'vim_mode', 'default_approval_mode',
      'small_model', 'timeout', 'share_mode', 'env_managed', 'claude_reasoning_effort',
      'tool_config', 'extra', 'favorite_at', 'fetched_models', 'icon', 'code'
    ];
    
    const filtered: any = {};
    Object.keys(provider).forEach(key => {
      if (!internalFields.includes(key)) {
        filtered[key] = provider[key];
      }
    });

    return JSON.stringify(filtered, null, 2);
  };

  const loadProviders = async (silent = false) => {
    if (!isTauri) return;
    if (!silent) setLoading(true);
    try {
      const res = await invoke<ApiResp<AiProvidersState>>('service_providers_list');
      if (silent && !isVisibleRef.current) return;

      if (res.data.providers && res.data.providers.length > 0) {
        setState(res.data);
      } else {
        // Only set default if it was truly empty and we didn't have existing state
        // This prevents wiping state if backend temporarily returns empty
        setState(prev => prev.providers.length > 0 ? prev : DEFAULT_STATE);
      }
      try {
        const presetsRes = await invoke<ApiResp<ServiceProviderPresetsState>>('service_provider_presets_list');
        if (silent && !isVisibleRef.current) return;
        setProviderPresets(presetsRes.data.presets || []);
      } catch (presetErr) {
        console.warn('Failed to load service provider presets:', presetErr);
      }
      setUnsavedNewProviderIds(new Set());
      try {
        const syncedRes = await invoke<ApiResp<SyncedDeviceProvidersView[]>>('service_providers_list_synced_other_devices');
        if (silent && !isVisibleRef.current) return;
        setSyncedOtherDeviceProviders(syncedRes.data || []);
      } catch (syncErr) {
        console.warn('Failed to load synced other-device providers:', syncErr);
        if (!silent) {
          setSyncedOtherDeviceProviders([]);
        }
      }
    } catch (e: any) {
      console.error('Failed to load AI providers:', e);
      setMessage({ type: 'error', text: `Failed to load providers: ${e.toString()}` });
    } finally {
      if (!silent) setLoading(false);
    }
  };

  const loadProviderPresets = async () => {
    const res = await invoke<ApiResp<ServiceProviderPresetsState>>('service_provider_presets_list');
    setProviderPresets(res.data.presets || []);
  };

  const openPresetEditor = (preset?: ServiceProviderPresetRecord) => {
    const claudeTemplate = normalizeClaudePresetTemplate(preset?.template);
    setEditingPresetId(preset?.id || null);
    setPresetDraft({
      id: preset?.id || '',
      name: preset?.name || '',
      description: preset?.description || '',
      icon: preset?.icon || '',
      openai_base_url: preset?.endpoints?.openai_base_url || '',
      anthropic_base_url: preset?.endpoints?.anthropic_base_url || '',
      claude_default_model: claudeTemplate.claude_default_model || '',
      claude_reasoning_effort: claudeTemplate.claude_reasoning_effort || '',
      claude_model_mappings: buildPresetClaudeMappings(claudeTemplate.claude_model_mappings),
    });
    setPresetManagerOpen(true);
  };

  const savePresetDraft = async () => {
    const now = Math.floor(Date.now() / 1000);
    const template = normalizeClaudePresetTemplate({
      claude_default_model: presetDraft.claude_default_model,
      claude_reasoning_effort: presetDraft.claude_reasoning_effort,
      claude_model_mappings: presetDraft.claude_model_mappings,
    });
    const payload: ServiceProviderPresetRecord = {
      id: presetDraft.id || `preset-${uuidv4()}`,
      name: presetDraft.name.trim(),
      description: presetDraft.description.trim() || undefined,
      icon: presetDraft.icon.trim() || undefined,
      endpoints: {
        openai_base_url: presetDraft.openai_base_url.trim() || undefined,
        anthropic_base_url: presetDraft.anthropic_base_url.trim() || undefined,
      },
      template,
      created_at: now,
      updated_at: now,
    };
    if (!payload.name) {
      setMessage({ type: 'error', text: t('providerPresetNameRequired', 'Preset name is required') });
      return;
    }
    try {
      await invoke<ApiResp<ServiceProviderPresetRecord>>('service_provider_presets_upsert', { preset: payload });
      await loadProviderPresets();
      setPresetManagerOpen(false);
      setEditingPresetId(null);
    } catch (error) {
      setMessage({ type: 'error', text: errorToDisplayMessage(error) });
    }
  };

  const deleteProviderPreset = async (presetId: string) => {
    try {
      await invoke<ApiResp<{ deleted: boolean }>>('service_provider_presets_delete', { presetId });
      await loadProviderPresets();
    } catch (error) {
      setMessage({ type: 'error', text: errorToDisplayMessage(error) });
    }
  };

  const loadClaudeProfiles = async () => {
    if (!isTauri) return;
    try {
      const res = await invoke<ApiResp<ClaudeProfileSummary[]>>('claude_profile_list');
      if (res.data) {
        setClaudeProfiles(res.data);
      }
    } catch (e: any) {
      console.error('Failed to load Claude profiles:', e);
    }
  };

  useEffect(() => {
    if (activeTool === 'claude') {
      void loadClaudeProfiles();
    }
  }, [activeTool, state.providers]);

  useEffect(() => {
    if (!isTauri) return;
    (async () => {
      try {
        const cfg = await invoke<any>('get_storage_config');
        if (cfg.ai_model_launch_commands?.claude) {
          setClaudeLaunchCommand(cfg.ai_model_launch_commands.claude);
        }
      } catch (e) {
        console.error('Failed to load Claude launch command:', e);
      }
    })();
  }, [isTauri]);

  useEffect(() => {
    isVisibleRef.current = isVisible;
  }, [isVisible]);

  async function detectAllVersions(runId: number = ++versionCheckRunIdRef.current) {
    if (!isTauri) return;
    const initialCheckingState = TOOLS.reduce((acc, tool) => {
      acc[tool] = true;
      return acc;
    }, {} as Partial<Record<CliTool, boolean>>);
    setCheckingAllVersions(true);
    setCheckingVersions(initialCheckingState);
    setCheckingUpdates(initialCheckingState);
    try {
      const results = await Promise.all(
        TOOLS.map(async tool => {
          try {
            const result = await invoke<DetectCliVersionResult>('detect_cli_version', { tool });
            return { tool, state: { version: result.version, isInstalled: result.is_installed } };
          } catch (e) {
            console.error(`Failed to detect ${tool} version:`, e);
            return { tool, state: { version: '', isInstalled: false } };
          }
        })
      );
      if (versionCheckRunIdRef.current !== runId) return;
      const nextVersions = results.reduce((acc, item) => {
        acc[item.tool] = item.state;
        return acc;
      }, {} as Partial<Record<CliTool, CliVersionState>>);
      setCliVersions(prev => ({
        ...prev,
        ...nextVersions
      }));
      setCheckingVersions({});

      // Fetch update info for all tools
      const updateResults = await Promise.all(
        TOOLS.map(async tool => {
          try {
            const updateInfo = await invoke<CliUpdateInfo>('check_cli_update', { tool });
            return { tool, info: updateInfo };
          } catch (e) {
            console.error(`Failed to check ${tool} update:`, e);
            return { tool, info: undefined };
          }
        })
      );
      if (versionCheckRunIdRef.current !== runId) return;
      const nextUpdates = updateResults.reduce((acc, item) => {
        if (item.info) {
          acc[item.tool] = item.info;
        } else {
          delete acc[item.tool];
        }
        return acc;
      }, {} as Partial<Record<CliTool, CliUpdateInfo>>);
      setCliUpdates(prev => ({ ...prev, ...nextUpdates }));
    } finally {
      if (versionCheckRunIdRef.current === runId) {
        setCheckingUpdates({});
        setCheckingAllVersions(false);
      }
    }
  }

  async function preloadCliMetaAndAutoImport(runId: number = ++probeRunIdRef.current) {
    if (!isTauri) return;

    if (!cliProbeInitializedRef.current) {
      const initialProbingState = TOOLS.reduce((acc, tool) => {
        acc[tool] = true;
        return acc;
      }, {} as Partial<Record<CliTool, boolean>>);
      setProbingTool(initialProbingState);
      const results = await Promise.all(
        TOOLS.map(async tool => {
          try {
            const res = await invoke<ApiResp<CliEnvProbeResult>>('cli_env_probe', { tool });
            return { tool, data: res.data };
          } catch (e) {
            console.error(`Failed to probe ${tool} cli env:`, e);
            return { tool, data: undefined };
          }
        })
      );
      if (probeRunIdRef.current !== runId) return;
      const nextProbe = results.reduce((acc, item) => {
        if (item.data) {
          acc[item.tool] = item.data;
        }
        return acc;
      }, {} as Partial<Record<CliTool, CliEnvProbeResult>>);
      if (Object.keys(nextProbe).length > 0) {
        // Probing results are tracked via cliProbeInitializedRef only; individual results
        // are passed through CliVersionCards props, so no local state update is needed here.
      }
      setProbingTool({});
      cliProbeInitializedRef.current = true;
    }
    if (probeRunIdRef.current !== runId) return;

    if (!autoImportInitializedRef.current) {
      const autoImportResults = await Promise.all(
        MANAGED_TOOLS.map(async tool => {
          try {
            const res = await invoke<ApiResp<AutoImportResult>>('service_providers_auto_import_from_system', { tool });
            return { tool, data: res.data };
          } catch (e) {
            console.error(`Auto import failed for ${tool}:`, e);
            return { tool, data: null as AutoImportResult | null };
          }
        })
      );
      if (probeRunIdRef.current !== runId) return;
      const importedAny = autoImportResults.some(item => !!item.data?.imported);
      setAutoImportInactiveNotice(prev => {
        const next = { ...prev };
        for (const item of autoImportResults) {
          if (!item.data?.imported) continue;
          if (item.data.activated === false) {
            const missingFieldLabels = (item.data.missing_fields || []).map(field => {
              if (field === 'api_key') return t('apiKey', 'API Key');
              if (field === 'base_url') return t('baseUrl', 'Base URL');
              return field;
            });
            const missingText = missingFieldLabels.join(' + ');
            next[item.tool] = missingText
              ? t(
                  'autoImportedButInactiveMissingFields',
                  { fields: missingText }
                )
              : t('autoImportedButInactive');
          } else {
            delete next[item.tool];
          }
        }
        return next;
      });
      autoImportInitializedRef.current = true;
      if (importedAny) {
        await loadProviders(true);
        if (probeRunIdRef.current !== runId) return;
        setMessage({ type: 'success', text: t('systemConfigImported') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      }
    }
  }

  useEffect(() => {
    if (!isVisible || !isTauri) return;
    void loadProviders(true);
    const versionRunId = ++versionCheckRunIdRef.current;
    const probeRunId = ++probeRunIdRef.current;
    const runCheck = () => {
      void detectAllVersions(versionRunId);
      void preloadCliMetaAndAutoImport(probeRunId);
    };
    const idleCallback = (window as Window & {
      requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
      cancelIdleCallback?: (id: number) => void;
    }).requestIdleCallback;
    const cancelIdleCallback = (window as Window & {
      cancelIdleCallback?: (id: number) => void;
    }).cancelIdleCallback;
    if (idleCallback) {
      const id = idleCallback(runCheck, { timeout: 2500 });
      return () => {
        versionCheckRunIdRef.current += 1;
        probeRunIdRef.current += 1;
        if (cancelIdleCallback) cancelIdleCallback(id);
      };
    }
    const timer = window.setTimeout(runCheck, 600);
    return () => {
      versionCheckRunIdRef.current += 1;
      probeRunIdRef.current += 1;
      window.clearTimeout(timer);
    };
  }, [isVisible]);

  useEffect(() => {
    const current = state.providers.find(p => p.id === currentProviderId);
    if (!current || current.tool !== activeTool) {
      setCurrentProviderId(null);
    }
  }, [activeTool]);

  const activateProvider = async (tool: string, providerId: string) => {
    try {
      setLoading(true);
      setMessage({ type: '', text: '' });
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'activate',
          action: 'activate-provider',
          target: { tab: 'ai-environments', entity_id: providerId },
          dedupeKey: `ai-environments:activate:${tool}:${providerId}`,
          metadata: { tool, provider_id: providerId },
          confirm: {
            message: t(
              'confirmActivateProvider',
              'Activate this Service Provider and apply it to the current environment?',
            ),
            title: t('confirmActivateProviderTitle', 'Activate Service Provider'),
            okLabel: t('activate', 'Activate'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('aiEnvironmentActivatedMessageTitle', 'Service Provider activated'),
            summary: t('appliedSuccess', 'Service Provider activated successfully!'),
          },
          error: {
            title: t('activationFailed', 'Activation failed'),
          },
        },
        async () => {
          await invoke('service_providers_set_active', { tool, providerId });
          await loadProviders(true);
          await invoke('projection_apply', { tool, providerId });
          return true;
        },
      );
      if (result === null) return false;
      setMessage({ type: 'success', text: t('appliedSuccess', 'Service Provider activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return true;
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      return false;
    } finally {
      setLoading(false);
    }
  };

  const syncOpenCodeProviderWithJson = (provider: Partial<AiProvider>, parsed: Record<string, any>) => {
    const next: Record<string, any> = {
      ...parsed,
      id: provider.id,
      tool: 'opencode',
      is_enabled: true,
      provider_key: provider.provider_key,
      opencode_default_model: provider.opencode_default_model,
      opencode_default_agent: provider.opencode_default_agent,
      opencode_sessions_dir: provider.opencode_sessions_dir,
      small_model: provider.small_model,
      timeout: provider.timeout,
      share_mode: provider.share_mode,
      history: provider.history || [],
    };

    next.name = provider.name || parsed.name || '';
    next.options = typeof parsed.options === 'object' && parsed.options !== null ? { ...parsed.options } : {};
    next.models = typeof parsed.models === 'object' && parsed.models !== null ? { ...parsed.models } : {};

    if (provider.api_key !== undefined) {
      next.options.apiKey = provider.api_key || '';
    }
    if (provider.base_url !== undefined) {
      next.options.baseURL = provider.base_url || '';
    }

    const selectedModel = String(provider.model || '').trim();
    if (selectedModel) {
      const existingModelConfig =
        typeof next.models[selectedModel] === 'object' && next.models[selectedModel] !== null
          ? next.models[selectedModel]
          : {};
      next.models = {
        ...next.models,
        [selectedModel]: existingModelConfig,
      };
    } else if (Object.keys(next.models).length > 0) {
      const [firstModel] = Object.keys(next.models);
      next.model = firstModel;
    }

    return next;
  };

  const buildProviderForSave = (provider: Partial<AiProvider>): AiProvider => {
    const newId = provider.id || uuidv4();
    let baseProvider: Record<string, any> = { ...provider };

    if (provider.tool === 'opencode') {
      let parsed: Record<string, any>;
      try {
        parsed = JSON.parse(rawJson || '{}');
      } catch {
        throw new Error(t('invalidJson', 'Invalid JSON syntax'));
      }

      baseProvider = {
        ...parsed,
        id: provider.id,
        tool: 'opencode',
        icon: provider.icon,
        is_enabled: true,
        provider_key: provider.provider_key,
        opencode_default_model: provider.opencode_default_model,
        opencode_default_agent: provider.opencode_default_agent,
        opencode_sessions_dir: provider.opencode_sessions_dir,
        small_model: provider.small_model,
        timeout: provider.timeout,
        share_mode: provider.share_mode,
        history: provider.history || [],
      };

      const options = parsed.options && typeof parsed.options === 'object' ? parsed.options : {};
      baseProvider.api_key = typeof options.apiKey === 'string' ? options.apiKey : '';
      baseProvider.base_url = typeof options.baseURL === 'string' ? options.baseURL : '';

      const models = parsed.models && typeof parsed.models === 'object' ? parsed.models : {};
      baseProvider.model = Object.keys(models)[0] || '';
    }

    return {
      ...baseProvider,
      id: newId,
      name: String(baseProvider.name || 'Unnamed'),
      tool: String(provider.tool || activeTool),
      api_key: String(baseProvider.api_key || ''),
      provider_key: baseProvider.provider_key,
      is_enabled: provider.tool === 'opencode' ? true : (baseProvider.is_enabled ?? true),
      env_managed: provider.tool !== 'opencode' ? (baseProvider.env_managed ?? true) : undefined,
      history: Array.isArray(provider.history) ? provider.history : [],
    } as AiProvider;
  };

  const saveDetailProvider = async (
    provider: Partial<AiProvider>,
    options: { showSavedMessage?: boolean } = {},
  ): Promise<SavePresetResult> => {
    const { showSavedMessage = true } = options;
    if (!provider.name) {
      setMessage({ type: 'error', text: t('providePresetName', 'Please provide a preset name') });
      return { ok: false };
    }

    const newId = provider.id || uuidv4();

    try {
      const finalProvider = buildProviderForSave(provider);
      const missingRequiredFields = getMissingRequiredProviderFields(finalProvider);
      if (missingRequiredFields.length > 0) {
        const labelByField: Record<RequiredProviderField, string> = {
          api_key: t('apiKey', 'API Key'),
          base_url: t('baseUrl', 'Base URL'),
          provider_key: t('providerIdentifier', 'Service Provider Identifier'),
          code: t('providerIdentifier', 'Service Provider Identifier'),
        };
        const fields = missingRequiredFields.map((field) => labelByField[field]).join(' + ');
        const text = t('requiredProviderFieldsMissing', 'Please fill required fields: {{fields}}', { fields });
        setMessage({ type: 'error', text });
        pushToast({ title: t('saveFailed', 'Save failed'), description: text, kind: 'error' });
        return { ok: false };
      }
      const savedData = unwrapApiResp(
        await invoke<ApiResp<AiProvider>>('service_providers_upsert', {
          provider: normalizeProviderForSave(finalProvider),
        }),
        t('saveFailed', 'Save failed'),
      );
      const savedProvider = { ...finalProvider, ...savedData } as AiProvider;
      await loadProviders(true);
      if (savedProvider.tool === 'claude') {
        await loadClaudeProfiles();
      }
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.delete(newId);
        return next;
      });
      setCurrentProviderId(savedProvider.id);
      emit('refresh-counts');
      setDetailProvider(savedProvider);
      setIsRollbackMode(false);
      rollbackDraftBeforeRef.current = null;
      setJsonError(null);
      if (savedProvider.tool === 'opencode') {
        setOriginalJson(rawJson);
        if (state.active_opencode.includes(savedProvider.id)) {
          try {
            await invoke('projection_apply', { tool: 'opencode', providerId: savedProvider.id });
          } catch (e) {
            console.error('Failed to sync opencode.json after save:', e);
          }
        }
      }

      if (showSavedMessage) {
        await safeRecordMessage({
          source: 'ai_environments',
          category: 'save',
          severity: 'success',
          title: t('aiEnvironmentSavedMessageTitle', 'Service Provider saved'),
          summary: t('providerSaved', 'Service Provider saved'),
          dedupe_key: `ai-environments:save:${savedProvider.tool}:${savedProvider.id}`,
          target: { tab: 'ai-environments', entity_id: savedProvider.id },
          metadata: { tool: savedProvider.tool, provider_id: savedProvider.id },
        });
        setMessage({ type: 'success', text: t('providerSaved', 'Service Provider saved') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        pushToast({ title: t('providerSaved', 'Service Provider saved'), kind: 'success' });
      }

      return {
        ok: true,
        providerId: savedProvider.id,
        provider: savedProvider,
      };
    } catch (e: any) {
      const errorMessage = errorToDisplayMessage(e);
      await safeRecordMessage({
        source: 'ai_environments',
        category: 'save',
        severity: 'error',
        title: t('saveFailed', 'Save failed'),
        summary: errorMessage,
        detail: errorMessage,
        dedupe_key: `ai-environments:save:${provider.tool || activeTool}:${newId}`,
        target: { tab: 'ai-environments', entity_id: newId },
        metadata: { tool: provider.tool || activeTool, provider_id: newId },
      });
      pushToast({ title: t('saveFailed', 'Save failed'), description: errorMessage, kind: 'error' });
      setMessage({ type: 'error', text: errorMessage });
      return { ok: false };
    }
  };

  const returnToProviderList = (options: { preserveScroll?: boolean } = {}) => {
    if (options.preserveScroll) {
      pendingRestoreListScrollTopRef.current = savedListScrollTopRef.current;
    }
    setViewMode('list');
    setDetailProvider(null);
  };

  const handleSaveDetailAndReturnToList = async (provider: Partial<AiProvider>) => {
    if (savingDetail) return;
    setSavingDetail(true);
    try {
      const result = await saveDetailProvider(provider);
      if (!result.ok) return;
      returnToProviderList({ preserveScroll: true });
    } finally {
      setSavingDetail(false);
    }
  };

  const handleRollback = (entry: HistoryEntry) => {
    if (entry.snapshot && typeof entry.snapshot === 'object') {
      if (!isRollbackMode) {
        rollbackDraftBeforeRef.current = {
          provider: detailProvider ? { ...detailProvider } : null,
          rawJson,
        };
      }
      const draft = {
        ...entry.snapshot,
        history: detailProvider?.history || entry.snapshot.history || [],
      } as AiProvider;
      setDetailProvider(draft);
      if (draft.tool === 'opencode') {
        setRawJson(getOpenCodeJson(draft));
      }
      setIsRollbackMode(true);
      setJsonError(null);
      setMessage({ type: 'success', text: t('rollbackModeTitle', 'History version loaded.') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    if (entry.content) {
      try {
        JSON.parse(entry.content); // Verify syntax
        if (!isRollbackMode) {
          rollbackDraftBeforeRef.current = {
            provider: detailProvider ? { ...detailProvider } : null,
            rawJson,
          };
        }
        setRawJson(entry.content);
        setIsRollbackMode(true);
        setJsonError(null);
        setMessage({ type: 'success', text: t('rollbackModeTitle', 'History version loaded.') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      } catch (e) {
        setMessage({ type: 'error', text: t('parseHistoryFailed') });
      }
      return;
    }

    setMessage({ type: 'error', text: t('parseHistoryFailed') });
  };

  const handleAddCustom = (toolName: string, preset?: ServiceProviderPresetRecord) => {
    const newId = uuidv4();
    const baseProvider: AiProvider = {
      id: newId,
      name: preset?.name || `${t('newPreset', 'New Service Provider')} (${toolName})`,
      tool: toolName,
      api_key: '',
      base_url: '',
      model: '',
      icon: preset?.icon,
      code: toolName !== 'opencode' ? uniqueProviderCode(toolName, preset?.name) : undefined,
      env_managed: toolName !== 'opencode' ? true : undefined,
      provider_key: toolName === 'opencode' ? uniqueOpenCodeProviderKey(preset?.name) : undefined,
      is_enabled: true,
      ...(toolName === 'claude' ? {
        claude_api_format: 'anthropic_messages',
        claude_auth_env_key: 'ANTHROPIC_API_KEY',
        claude_model_mappings: [
          { family: 'haiku', display_name: 'Haiku', upstream_model: '', supports_1m: false },
          { family: 'sonnet', display_name: 'Sonnet', upstream_model: '', supports_1m: false },
          { family: 'opus', display_name: 'Opus', upstream_model: '', supports_1m: false },
        ],
        claude_enable_tool_search: false,
        claude_enable_attribution: false,
        claude_auto_memory_enabled: false,
        claude_always_thinking_enabled: false,
        claude_away_summary_enabled: false,
        claude_include_git_instructions: false,
      } : {}),
      ...(toolName === 'opencode' ? {
        npm: '@ai-sdk/openai-compatible',
        options: { apiKey: '', baseURL: '' },
        models: {}
      } : {})
    };
    const newProvider = preset
      ? (applyProviderPresetToDraft(baseProvider, preset, toolName) as AiProvider)
      : baseProvider;

    // Switch tool first (if different) — this may trigger the active-provider auto-expand effect
    if (toolName !== activeTool) {
      setActiveTool(toolName);
    }

    const newState = {
      ...state,
      providers: [...state.providers, newProvider]
    };

    setState(newState);
    setCurrentProviderId(newId);
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.add(newId);
        return next;
      });
    setRawJson(toolName === 'opencode' ? getOpenCodeJson(newProvider) : JSON.stringify(newProvider, null, 2));
    setOriginalJson(toolName === 'opencode' ? getOpenCodeJson(newProvider) : JSON.stringify(newProvider, null, 2));
    setJsonError(null);
    setIsRollbackMode(false);
    rollbackDraftBeforeRef.current = null;
    setDetailProvider(newProvider);
    setViewMode('detail');
    setPresetPickerOpen(false);
  };

  const handleDelete = async (providerId?: string, toolName?: string) => {
    const targetId = providerId || currentProviderId;
    if (!targetId) return;
    const targetTool = toolName || detailProvider?.tool || activeTool;
    const isUnsavedNewProvider = unsavedNewProviderIds.has(targetId);
    const providerToDelete =
      targetTool === 'claude'
        ? claudeProfiles.find((p) => p.id === targetId)
        : state.providers.find((p) => p.id === targetId && p.tool === targetTool);
    if (!providerToDelete) return;
    const activeProviderIdForTool = (state as any)[`active_${targetTool}`] as string | null;
    const isDefaultImportedForTool =
      isManagedTool(targetTool) && providerToDelete.code === `default-${targetTool}`;
    const isDeletingActiveDefaultImported =
      isDefaultImportedForTool && activeProviderIdForTool === providerToDelete.id;
    const isDeletingInactiveDefaultImported =
      isDefaultImportedForTool && activeProviderIdForTool !== providerToDelete.id;
    if (isDeletingActiveDefaultImported) return;
    if (
      !isUnsavedNewProvider &&
      !isDeletingInactiveDefaultImported &&
      targetTool !== 'claude' &&
      state.providers.filter(p => p.tool === targetTool).length <= 1
    ) {
      return;
    }

    if (isUnsavedNewProvider) {
      const confirmed = await confirmDialog(t('confirmDelete', { name: providerToDelete.name }), {
        title: t('confirmDeleteTitle', 'Delete Service Provider'),
        okLabel: t('delete', 'Delete'),
        cancelLabel: t('cancel', 'Cancel'),
        kind: 'error',
      });
      if (!confirmed) return;
      setState(prev => ({
        ...prev,
        providers: prev.providers.filter(p => p.id !== targetId)
      }));
      if (currentProviderId === targetId) {
        setCurrentProviderId(null);
      }
      if (detailProvider?.id === targetId) {
        setDetailProvider(null);
        setViewMode('list');
      }
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.delete(targetId);
        return next;
      });
      await safeRecordMessage({
        source: 'ai_environments',
        category: 'delete',
        severity: 'success',
        title: t('aiEnvironmentDeletedMessageTitle', 'Service Provider deleted'),
        summary: t('deleteSuccess', 'Preset deleted successfully'),
        dedupe_key: `ai-environments:delete:${targetTool}:${targetId}`,
        target: { tab: 'ai-environments', entity_id: targetId },
        metadata: { tool: targetTool, provider_id: targetId, unsaved: true },
      });
      pushToast({ title: t('deleteSuccess', 'Preset deleted successfully'), kind: 'success' });
      setMessage({ type: 'success', text: t('deleteSuccess', 'Preset deleted successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    try {
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'delete',
          action: 'delete-provider',
          target: { tab: 'ai-environments', entity_id: targetId },
          dedupeKey: `ai-environments:delete:${targetTool}:${targetId}`,
          metadata: { tool: targetTool, provider_id: targetId },
          confirm: {
            message: t('confirmDelete', { name: providerToDelete.name }),
            title: t('confirmDeleteTitle', 'Delete Service Provider'),
            okLabel: t('delete', 'Delete'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'error',
          },
          success: {
            title: t('aiEnvironmentDeletedMessageTitle', 'Service Provider deleted'),
            summary: t('deleteSuccess', 'Preset deleted successfully'),
          },
          error: {
            title: t('deleteFailed', 'Delete failed'),
          },
        },
        () => invoke('service_providers_delete', { providerId: targetId }),
      );
      if (result === null) return;
      await loadProviders(true);
      if (currentProviderId === targetId) {
        setCurrentProviderId(null);
      }
      if (detailProvider?.id === targetId) {
        setDetailProvider(null);
        setViewMode('list');
      }
      emit('refresh-counts');
      setMessage({ type: 'success', text: t('deleteSuccess', 'Preset deleted successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    }
  };

  const handleApplyCliUpdate = async (tool: CliTool) => {
    const updateInfo = cliUpdates[tool];
    if (!updateInfo) return;

    const toolLabel = tool.charAt(0).toUpperCase() + tool.slice(1);
    const confirmMsg = t('confirmCliUpdateMessage', {
      tool: toolLabel,
      command: updateInfo.update_command,
    });

    setUpdatingTool(prev => ({ ...prev, [tool]: true }));
    try {
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'cli_update',
          action: 'apply-cli-update',
          target: { tab: 'ai-environments', entity_id: tool },
          dedupeKey: `ai-environments:cli-update:${tool}`,
          metadata: { tool, command: updateInfo.update_command },
          confirm: {
            message: confirmMsg,
            title: t('confirmCliUpdateTitle') + `: ${toolLabel}`,
            okLabel: t('cliUpdate'),
            cancelLabel: t('cancel'),
            kind: 'warning',
          },
          success: {
            title: t('cliUpdateTerminalLaunched', 'Update command opened in terminal'),
            summary: t('cliUpdateTerminalLaunched', 'Update command opened in terminal'),
          },
          error: {
            title: t('cliUpdateFailedTitle', 'CLI update failed'),
          },
        },
        () => invoke<CliUpdateApplyResult>('apply_cli_update', { tool }),
      );
      if (result === null) return;
      if (result.success && result.terminal_launched) {
        setMessage({ type: 'success', text: t('cliUpdateTerminalLaunched') });
        setTimeout(() => setMessage({ type: '', text: '' }), 5000);
      } else {
        await safeRecordMessage({
          source: 'ai_environments',
          category: 'cli_update',
          severity: 'error',
          title: t('cliUpdateFailedTitle', 'CLI update failed'),
          summary: t('cliUpdateFailed', { error: result.error || 'Unknown error' }),
          detail: result.error || 'Unknown error',
          dedupe_key: `ai-environments:cli-update:${tool}`,
          target: { tab: 'ai-environments', entity_id: tool },
          metadata: result,
        });
        pushToast({
          title: t('cliUpdateFailedTitle', 'CLI update failed'),
          description: result.error || 'Unknown error',
          kind: 'error',
        });
        setMessage({ type: 'error', text: t('cliUpdateFailed', { error: result.error || 'Unknown error' }) });
        setTimeout(() => setMessage({ type: '', text: '' }), 5000);
      }
    } catch (e: any) {
      setMessage({ type: 'error', text: t('cliUpdateFailed', { error: e.toString() }) });
      setTimeout(() => setMessage({ type: '', text: '' }), 5000);
    } finally {
      setUpdatingTool(prev => ({ ...prev, [tool]: false }));
    }
  };

  const handleClaudeCopyCommand = async (profileId: string, configDir: string) => {
    const cmd = `CLAUDE_CONFIG_DIR='${configDir}' ${claudeLaunchCommand.replace('{session_id}', 'new')}`;
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(cmd);
      } else {
        const input = document.createElement('textarea');
        input.value = cmd;
        input.setAttribute('readonly', 'true');
        input.style.position = 'fixed';
        input.style.left = '-9999px';
        document.body.appendChild(input);
        input.select();
        const copied = document.execCommand('copy');
        document.body.removeChild(input);
        if (!copied) throw new Error('copy_failed');
      }
      setCopiedClaudeProfileId(profileId);
      window.setTimeout(() => setCopiedClaudeProfileId(null), 2000);
      setMessage({ type: 'success', text: t('claudeProfileCopySuccess', 'Command copied to clipboard') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: t('claudeProfileCopyFailed', 'Failed to copy command') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    }
  };

  const handleClaudeOpenDir = async (profileId: string) => {
    if (!isTauri) return;
    try {
      const configDir = await invoke<string>('get_claude_config_dir', { providerId: profileId });
      if (!configDir) throw new Error(t('configDirNotFound', 'Config directory not found'));
      await openLocalPath(configDir);
    } catch (e: any) {
      console.error('handleClaudeOpenDir error:', e);
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    }
  };

  const handleClaudeApplyGlobal = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setApplyingGlobal(true);
      setApplyingClaudeProfileId(profileId);
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'activate',
          action: 'apply-claude-profile',
          target: { tab: 'ai-environments', entity_id: profileId },
          dedupeKey: `ai-environments:activate:claude:${profileId}`,
          metadata: { tool: 'claude', provider_id: profileId },
          confirm: {
            message: t(
              'confirmActivateProvider',
              'Activate this Service Provider and apply it to the current environment?',
            ),
            title: t('confirmActivateProviderTitle', 'Activate Service Provider'),
            okLabel: t('activate', 'Activate'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('aiEnvironmentActivatedMessageTitle', 'Service Provider activated'),
            summary: t('appliedSuccess', 'Environment activated successfully!'),
          },
          error: {
            title: t('activationFailed', 'Activation failed'),
          },
        },
        async () => {
          await invoke('service_providers_set_active', { tool: 'claude', providerId: profileId });
          await invoke('projection_apply', { tool: 'claude', providerId: profileId });
        },
      );
      if (result === null) return;
      await loadProviders(true);
      emit('refresh-counts');
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setApplyingGlobal(false);
      setApplyingClaudeProfileId(null);
    }
  };

  const createClaudeProviderSession = async (
    profileId: string,
    permissionMode?: TerminalPermissionMode,
  ) => {
    const response = await invoke<{
      ok: boolean;
      data: { id?: string; provider_id?: string; model_type?: string };
    }>('sessions_create', {
      session: {
        name: '',
        working_dir: '',
        tool: 'claude',
        provider_id: profileId,
        status: 'active',
        ...(permissionMode ? { permission_mode: permissionMode } : {}),
      },
    });

    if (response.ok) {
      setCurrentProviderId(response.data.provider_id || profileId);
      await emit('session-created');
      await emit('refresh-counts');
    }
  };

  const handleClaudeLaunch = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setLaunchingClaudeProfileId(profileId);
      await createClaudeProviderSession(profileId);
      setMessage({ type: 'success', text: t('sessionCreated', 'Session created') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: unknown) {
      const code = getInvokeErrorCode(e);
      if (code === 'PERMISSION_CONFIRMATION_REQUIRED') {
        setPermissionDialogClaudeProfileId(profileId);
        setPermissionDialogOpen(true);
        return;
      }
      console.error('handleClaudeLaunch error:', e);
      setMessage({ type: 'error', text: errorToDisplayMessage(e) });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLaunchingClaudeProfileId(null);
    }
  };

  const handleClaudePermissionConfirm = async (mode: TerminalPermissionMode) => {
    if (!permissionDialogClaudeProfileId) return;
    const profileId = permissionDialogClaudeProfileId;
    setPermissionDialogOpen(false);
    setPermissionDialogClaudeProfileId(null);
    try {
      setLaunchingClaudeProfileId(profileId);
      await createClaudeProviderSession(profileId, mode);
      setMessage({ type: 'success', text: t('sessionCreated', 'Session created') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: unknown) {
      console.error('handleClaudePermissionConfirm error:', e);
      setMessage({ type: 'error', text: errorToDisplayMessage(e) });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLaunchingClaudeProfileId(null);
    }
  };

  const handleClaudePermissionCancel = () => {
    setPermissionDialogOpen(false);
    setPermissionDialogClaudeProfileId(null);
  };

  const handleToggleFavorite = async (providerId: string, favorite: boolean) => {
    if (!isTauri) return;
    setFavoritePendingIds(prev => new Set(prev).add(providerId));
    try {
      await invoke('service_providers_set_favorite', { providerId, favorite });
      await loadProviders(true);
      if (activeTool === 'claude') {
        await loadClaudeProfiles();
      }
    } catch (e: any) {
      const text = String(e?.message || e);
      setMessage({ type: 'error', text });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      pushToast({ title: t('saveFailed', 'Save failed'), description: text, kind: 'error' });
    } finally {
      setFavoritePendingIds(prev => {
        const next = new Set(prev);
        next.delete(providerId);
        return next;
      });
    }
  };

  const handleActivateSyncedProvider = async (deviceId: string, provider: SyncedDeviceProvider) => {
    const apiKey = String(provider.api_key || '').trim();
    if (!apiKey) {
      setMessage({
        type: 'error',
        text: t(
          'syncedProviderMissingApiKey',
          'This Service Provider is missing a decryptable API Key and cannot be activated directly.'
        ),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    const activation = buildSyncedProviderActivationPayload(deviceId, provider);
    if (!activation) {
      return;
    }
    const { targetId, targetTool, payload } = activation;

    const actionKey = `${deviceId}:${provider.tool}:${provider.id}`;
    try {
      setLoading(true);
      setActivatingSyncedKey(actionKey);
      const savedData = unwrapApiResp(
        await invoke<ApiResp<{ id?: string } & Record<string, any>>>('service_providers_upsert', { provider: payload }),
        t('saveFailed', 'Save failed'),
      );
      const savedProviderId = String(savedData?.id || targetId);
      await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'activate',
          action: 'activate-synced-provider',
          target: { tab: 'ai-environments', entity_id: savedProviderId },
          dedupeKey: `ai-environments:activate:${targetTool}:${savedProviderId}`,
          metadata: { tool: targetTool, provider_id: savedProviderId, device_id: deviceId },
          confirm: {
            message: t(
              'confirmActivateSyncedProvider',
              'Import this synced Service Provider and apply it on this device now?',
            ),
            title: t('confirmActivateProviderTitle', 'Activate Service Provider'),
            okLabel: t('activate', 'Activate'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
            summary: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
          },
          error: {
            title: t('activationFailed', 'Activation failed'),
          },
        },
        async () => {
          await invoke('service_providers_set_active', { tool: targetTool, providerId: savedProviderId });
          await invoke('projection_apply', { tool: targetTool, providerId: savedProviderId });
          return true;
        },
      );
      await loadProviders(true);
      setActiveTool(targetTool);
      setCurrentProviderId(savedProviderId);
      emit('refresh-counts');
      setMessage({
        type: 'success',
        text: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: errorToDisplayMessage(e) });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLoading(false);
      setActivatingSyncedKey(null);
    }
  };

  const closeImportModal = () => {
    setImportPreview(null);
    setImportPath(null);
    setImportDecisions({});
  };

  const handleExportProviders = async () => {
    if (!isTauri) return;
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const outputPath = await save({
        defaultPath: `onespace-service-providers-${stamp}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;

      setExportingProviders(true);
      const res = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'export',
          action: 'export-providers',
          target: { tab: 'ai-environments' },
          dedupeKey: 'ai-environments:export',
          metadata: { output_path: outputPath },
          confirm: {
            message: t('providersExportConfirm', 'Export all Service Providers to the selected JSON file?'),
            title: t('providersExportConfirmTitle', 'Export Service Providers'),
            okLabel: t('export', 'Export'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('providersExportSuccessTitle', 'Service Providers exported'),
          },
          error: {
            title: t('providersExportFailedTitle', 'Failed to export Service Providers'),
          },
        },
        () => invoke<ApiResp<ProvidersExportResult>>('service_providers_export', {
          outputPath,
        }),
      );
      if (res === null) return;
      const successText = t('providersExportSuccess', {
        count: res.data?.count ?? 0,
        path: res.data?.path || outputPath,
        defaultValue: 'Exported {{count}} Service Provider(s) to {{path}}',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      const errorText = t('providersExportFailed', {
        error: String(e),
        defaultValue: 'Failed to export Service Providers: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setExportingProviders(false);
    }
  };

  const handleImportProviders = async () => {
    if (!isTauri) return;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!selected || Array.isArray(selected)) return;

      setPreviewingImport(true);
      const selectedPath = selected as string;
      const res = await invoke<ApiResp<ProvidersImportPreview>>('service_providers_import_preview', {
        importPath: selectedPath,
      });
      if (!res.data?.items?.length) {
        const emptyText = t('providersImportEmpty', 'No Service Providers found in the selected file.');
        await safeRecordMessage({
          source: 'ai_environments',
          category: 'import',
          severity: 'warning',
          title: t('providersImportEmptyTitle', 'Import file is empty'),
          summary: emptyText,
          dedupe_key: 'ai-environments:import-preview-empty',
          target: { tab: 'ai-environments' },
          metadata: { import_path: selectedPath },
        });
        pushToast({
          title: t('providersImportEmptyTitle', 'Import file is empty'),
          description: emptyText,
          kind: 'warning',
        });
        setMessage({ type: 'error', text: emptyText });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        return;
      }

      const defaults = (res.data.items || []).reduce((acc, item) => {
        if (item.conflict) {
          acc[item.import_key] = 'overwrite';
        }
        return acc;
      }, {} as Record<string, 'overwrite' | 'new'>);
      setImportDecisions(defaults);
      setImportPath(selectedPath);
      setImportPreview(res.data);
    } catch (e: any) {
      const errorText = t('providersImportPreviewFailed', {
        error: String(e),
        defaultValue: 'Failed to read import file: {{error}}',
      });
      await safeRecordMessage({
        source: 'ai_environments',
        category: 'import',
        severity: 'error',
        title: t('providersImportPreviewFailedTitle', 'Failed to preview import'),
        summary: errorText,
        detail: String(e),
        dedupe_key: 'ai-environments:import-preview',
        target: { tab: 'ai-environments' },
      });
      pushToast({
        title: t('providersImportPreviewFailedTitle', 'Failed to preview import'),
        description: String(e),
        kind: 'error',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setPreviewingImport(false);
    }
  };

  const handleSetAllImportConflictActions = (action: 'overwrite' | 'new') => {
    if (!importPreview) return;
    const next = { ...importDecisions };
    for (const item of importPreview.items) {
      if (item.conflict) {
        next[item.import_key] = action;
      }
    }
    setImportDecisions(next);
  };

  const handleApplyImport = async () => {
    if (!importPreview || !importPath) return;
    try {
      setApplyingImport(true);
      const decisions: ProviderImportDecision[] = importPreview.items
        .filter(item => item.conflict)
        .map(item => ({
          import_key: item.import_key,
          action: importDecisions[item.import_key] || 'overwrite',
        }));

      const res = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'import',
          action: 'apply-provider-import',
          target: { tab: 'ai-environments' },
          dedupeKey: 'ai-environments:import-apply',
          metadata: { import_path: importPath, decisions },
          confirm: {
            message: t(
              'providersImportConfirm',
              'Apply the selected import actions and update existing Service Providers where required?',
            ),
            title: t('providersImportConfirmTitle', 'Import Service Providers'),
            okLabel: t('import', 'Import'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('providersImportAppliedTitle', 'Service Providers imported'),
          },
          error: {
            title: t('providersImportApplyFailedTitle', 'Failed to import Service Providers'),
          },
        },
        () => invoke<ApiResp<ProvidersImportApplyResult>>('service_providers_import_apply', {
          importPath,
          decisions,
        }),
      );
      if (res === null) return;

      await loadProviders(true);
      emit('refresh-counts');
      closeImportModal();

      const successText = t('providersImportApplied', {
        imported: res.data?.imported ?? 0,
        overwritten: res.data?.overwritten ?? 0,
        created: res.data?.created ?? 0,
        activeRestored: res.data?.active_restored ?? 0,
        defaultValue:
          'Imported {{imported}} Service Provider(s): {{overwritten}} overwritten, {{created}} created, {{activeRestored}} active binding(s) restored.',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      const errorText = t('providersImportApplyFailed', {
        error: String(e),
        defaultValue: 'Failed to import Service Providers: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setApplyingImport(false);
    }
  };

  const importConflictItems = importPreview?.items.filter(item => item.conflict) || [];
  const importNewItems = importPreview?.items.filter(item => !item.conflict) || [];

  const getClaudeMappingTags = (profile: ClaudeProfileSummary) => {
    const explicitMappings = Array.isArray(profile.claude_model_mappings)
      ? profile.claude_model_mappings
      : Array.isArray(profile.tool_config?.claude_model_mappings)
        ? profile.tool_config.claude_model_mappings
        : [];

    const explicitTags = explicitMappings
      .map((mapping: any) => {
        const family = String(mapping?.family || '').trim();
        const upstream = String(mapping?.upstream_model || '').trim();
        if (!upstream) return null;
        return family ? `${family}: ${upstream}` : upstream;
      })
      .filter((value: string | null): value is string => Boolean(value));

    if (explicitTags.length > 0) {
      return explicitTags;
    }

    return [
      ['default', profile.tool_config?.claude_default_model || profile.model],
    ]
      .map(([family, value]) => {
        const upstream = String(value || '').trim();
        return upstream ? `${family}: ${upstream}` : null;
      })
      .filter((value): value is string => Boolean(value));
  };

  const currentToolListItems = useMemo<ServiceProviderListItem[]>(() => {
    if (activeTool === 'claude') {
      const items = claudeProfiles.map((profile) => {
        const upstreamTags = getClaudeMappingTags(profile);

        const modelTags = [
          (profile.tool_config?.claude_default_model || profile.model || '').trim(),
        ].filter((value): value is string => Boolean(value));

        const description =
          profile.tilde_config_dir || profile.config_dir || profile.code || '';
        const apiFormatKey = getClaudeApiFormat(profile);
        const apiFormatTag =
          apiFormatKey === 'open_ai_chat'
            ? t('openAiChatFormat', 'OpenAI Chat')
            : apiFormatKey === 'open_ai_responses'
              ? t('openAiResponsesFormat', 'OpenAI Responses')
              : t('anthropicMessagesFormat', 'Anthropic Messages');
        const isActiveForSort = !!profile.is_default;

        return {
          id: profile.id,
          name: profile.name,
          tool: 'claude',
          icon: profile.icon || undefined,
          description,
          remark: profile.tool_config?.remark || '',
          modelTags,
          claudeUpstreamModelTags: upstreamTags,
          apiFormatTag,
          isGlobal: getIsGlobalForTool('claude', profile.id),
          isFavorite: !!profile.favorite_at,
          favoriteAt: profile.favorite_at ?? null,
          canFavorite: !unsavedNewProviderIds.has(profile.id),
          favoritePending: favoritePendingIds.has(profile.id),
          isActiveForSort,
          canLaunch: true,
          canDelete: true,
          launchBusy: launchingClaudeProfileId === profile.id,
          applyBusy: applyingClaudeProfileId === profile.id,
          deleteBusy: false,
          copiedCommand: copiedClaudeProfileId === profile.id,
        } satisfies ServiceProviderListItem;
      });

      return sortServiceProviderListItems(items);
    }

    const items = state.providers
      .filter((provider) => provider.tool === activeTool)
      .map((provider) => {
        const description = [
          provider.base_url?.trim(),
          activeTool === 'opencode' ? provider.provider_key?.trim() : '',
        ].find((value) => Boolean(value)) || '';

        const footerTags = [
          provider.model?.trim(),
          activeTool === 'codex' && provider.personality ? `personality: ${provider.personality}` : '',
          activeTool === 'codex' && provider.wire_api ? `wire: ${provider.wire_api}` : '',
          activeTool === 'codex' && provider.approval_policy ? `approval: ${provider.approval_policy}` : '',
          activeTool === 'codex' && provider.sandbox_mode ? `sandbox: ${provider.sandbox_mode}` : '',
          activeTool === 'gemini' && provider.theme ? `theme: ${provider.theme}` : '',
          activeTool === 'gemini' && provider.default_approval_mode
            ? `approval: ${provider.default_approval_mode}`
            : '',
          activeTool === 'opencode' && provider.opencode_default_model
            ? `default: ${provider.opencode_default_model}`
            : '',
          activeTool === 'opencode' && provider.opencode_default_agent
            ? `agent: ${provider.opencode_default_agent}`
            : '',
        ].filter((value): value is string => Boolean(value));

        const authLabel =
          activeTool === 'gemini' && provider.gemini_auth_type
            ? provider.gemini_auth_type
            : provider.api_key
              ? t('apiKey', 'API Key')
              : undefined;

        return {
          id: provider.id,
          name: provider.name,
          tool: provider.tool,
          icon: provider.icon,
          description,
          remark: provider.tool_config?.remark || provider.remark || '',
          authLabel,
          modelTags: footerTags,
          claudeUpstreamModelTags: [],
          apiFormatTag: null,
          isGlobal: getIsGlobalForTool(activeTool, provider.id),
          isFavorite: !!provider.favorite_at,
          favoriteAt: provider.favorite_at ?? null,
          canFavorite: !unsavedNewProviderIds.has(provider.id),
          favoritePending: favoritePendingIds.has(provider.id),
          isActiveForSort: getIsGlobalForTool(activeTool, provider.id),
          canLaunch: false,
          canDelete: true,
          launchBusy: false,
          applyBusy: loading,
          deleteBusy: false,
          copiedCommand: false,
        } satisfies ServiceProviderListItem;
      });

    return sortServiceProviderListItems(items);
  }, [
    activeTool,
    applyingGlobal,
    claudeProfiles,
    copiedClaudeProfileId,
    favoritePendingIds,
    launchingClaudeProfileId,
    applyingClaudeProfileId,
    loading,
    getClaudeMappingTags,
    state,
    unsavedNewProviderIds,
  ]);

  const providerCountsByTool = useMemo<Record<CliTool, number>>(
    () => ({
      claude: claudeProfiles.length,
      codex: state.providers.filter((provider) => provider.tool === 'codex').length,
      gemini: state.providers.filter((provider) => provider.tool === 'gemini').length,
      opencode: state.providers.filter((provider) => provider.tool === 'opencode').length,
    }),
    [claudeProfiles.length, state.providers],
  );

  useLayoutEffect(() => {
    if (viewMode !== 'list') return;
    const pendingScrollTop = pendingRestoreListScrollTopRef.current;
    if (pendingScrollTop === null) return;
    const container = listScrollContainerRef.current;
    if (!container) return;
    container.scrollTop = pendingScrollTop;
    pendingRestoreListScrollTopRef.current = null;
  }, [viewMode, currentToolListItems, syncedOtherDeviceProviders]);

  const openServiceProviderDetail = (id: string) => {
    rollbackDraftBeforeRef.current = null;
    if (activeTool === 'claude') {
      const storedProvider = state.providers.find((item) => item.id === id && item.tool === 'claude');
      if (storedProvider) {
        const adapted = buildClaudeProviderFromState(storedProvider);
        setCurrentProviderId(id);
        setDetailProvider(adapted);
        setRawJson(JSON.stringify(adapted, null, 2));
        setOriginalJson(JSON.stringify(adapted, null, 2));
        setJsonError(null);
        setIsRollbackMode(false);
        setViewMode('detail');
        return;
      }

      const profile = claudeProfiles.find((item) => item.id === id);
      if (!profile) return;
      const adapted = buildClaudeProviderFromProfile(profile);
      setCurrentProviderId(id);
      setDetailProvider(adapted);
      setRawJson(JSON.stringify(adapted, null, 2));
      setOriginalJson(JSON.stringify(adapted, null, 2));
      setJsonError(null);
      setIsRollbackMode(false);
      setViewMode('detail');
      return;
    }
    const provider = state.providers.find(p => p.id === id && p.tool === activeTool);
    if (!provider) return;
    const adaptedProvider = {
      ...provider,
      remark: provider.tool_config?.remark || '',
    };
    setCurrentProviderId(id);
    setDetailProvider(adaptedProvider);
    const json = activeTool === 'opencode' ? getOpenCodeJson(adaptedProvider) : JSON.stringify(adaptedProvider, null, 2);
    setRawJson(json);
    setOriginalJson(json);
    setJsonError(null);
    setIsRollbackMode(false);
    setViewMode('detail');
  };

  const renderServiceProviderDetail = () => {
    if (viewMode !== 'detail' || !detailProvider) return null;
    const isDetailActive =
      (detailProvider?.tool === 'claude' && state.active_claude === detailProvider?.id) ||
      (detailProvider?.tool === 'codex' && state.active_codex === detailProvider?.id) ||
      (detailProvider?.tool === 'gemini' && state.active_gemini === detailProvider?.id) ||
      (detailProvider?.tool === 'opencode' && state.active_opencode.includes(detailProvider?.id));

    const isManagedImportedDetail =
      !!detailProvider &&
      isManagedTool(detailProvider.tool) &&
      detailProvider.code === `default-${detailProvider.tool}` &&
      !isDetailActive;
    const detailMissingFieldLabels = isManagedImportedDetail
      ? [
          ...(detailProvider.api_key?.trim() ? [] : [t('apiKey', 'API Key')]),
          ...(detailProvider.base_url?.trim() ? [] : [t('baseUrl', 'Base URL')]),
        ]
      : [];
    const importedInactiveNotice = detailMissingFieldLabels.length > 0
      ? t('autoImportedButInactiveMissingFields', {
          fields: detailMissingFieldLabels.join(' + '),
        })
      : null;

    return (
      <div className="flex-1 min-h-0 overflow-hidden border rounded-xl bg-background">
        <ServiceProviderDetail
          provider={detailProvider}
          onChange={(changes) => {
            setDetailProvider((prev: any) => {
              if (!prev) return prev;
              const next = { ...prev, ...changes };
              if (next.tool === 'opencode') {
                try {
                  const parsed = JSON.parse(rawJson || '{}');
                  const synced = syncOpenCodeProviderWithJson(next, parsed);
                  setRawJson(getOpenCodeJson(synced));
                  setJsonError(null);
                } catch {
                  // Keep existing raw JSON if it is temporarily invalid.
                }
              }
              return next;
            });
          }}
          onSave={async () => {
            if (!isTauri || !detailProvider || savingDetail) return;
            try {
              await handleSaveDetailAndReturnToList(detailProvider);
            } catch (e: any) {
              const errorMessage = errorToDisplayMessage(e);
              setMessage({ type: 'error', text: errorMessage || t('saveFailed', 'Save failed') });
              pushToast({ title: t('saveFailed', 'Save failed'), description: errorMessage, kind: 'error' });
            }
          }}
          onActivate={async () => {
            if (!isTauri || !detailProvider || savingDetail) return;
            try {
              await invoke('service_providers_set_active', {
                tool: detailProvider.tool,
                providerId: detailProvider.id,
              });
              if (detailProvider.tool === 'claude') {
                await invoke('claude_profile_materialize', { providerId: detailProvider.id });
              }
              await invoke('projection_apply', { tool: detailProvider.tool, providerId: detailProvider.id });
              setMessage({ type: 'success', text: t('activated', 'Activated') });
              pushToast({ title: t('activated', 'Activated'), kind: 'success' });
              await loadProviders(true);
            } catch (e: any) {
              const errorMessage = errorToDisplayMessage(e);
              setMessage({ type: 'error', text: errorMessage || t('activationFailed', 'Activation failed') });
              pushToast({ title: t('activationFailed', 'Activation failed'), description: errorMessage, kind: 'error' });
            }
          }}
          onDelete={async () => {
            if (!isTauri || !detailProvider || savingDetail) return;
            try {
              await handleDelete(detailProvider.id, detailProvider.tool);
            } catch (e: any) {
              setMessage({ type: 'error', text: e?.message || t('deleteFailed', 'Delete failed') });
              pushToast({ title: t('deleteFailed', 'Delete failed'), description: String(e?.message || e), kind: 'error' });
            }
          }}
          onBack={() => { returnToProviderList({ preserveScroll: true }); }}
          isActive={isDetailActive}
          t={(key: string, fallback: string, options?: Record<string, any>) => String(t(key, fallback, options))}
          onFetchModels={async (provider: any) => {
            if (!isTauri) return [];
            return invoke<string[]>('service_provider_fetch_models', { provider });
          }}
          jsonMode={
            detailProvider.tool === 'claude'
              ? 'claude'
              : detailProvider.tool === 'opencode'
                ? 'opencode'
                : 'generic'
          }
          jsonValue={
            detailProvider.tool === 'opencode'
              ? rawJson
              : JSON.stringify(detailProvider, null, 2)
          }
          jsonHistory={detailProvider.history || []}
          jsonError={jsonError}
          isRollbackMode={isRollbackMode}
          onJsonChange={(value) => {
            if (detailProvider.tool === 'opencode') {
              setRawJson(value);
              if (isRollbackMode) setIsRollbackMode(false);
              try {
                JSON.parse(value);
                setJsonError(null);
              } catch (e: any) {
                setJsonError(e?.message || t('invalidJson', 'Invalid JSON syntax'));
              }
              return;
            }

            try {
              const parsed = JSON.parse(value);
              setDetailProvider((prev: any) => prev ? { ...prev, ...parsed } : prev);
              setJsonError(null);
            } catch (e: any) {
              setJsonError(e?.message || t('invalidJson', 'Invalid JSON syntax'));
            }
          }}
          onJsonError={setJsonError}
          onRollback={handleRollback}
          onFormatJson={() => {
            try {
              const parsed = JSON.parse(rawJson);
              setRawJson(JSON.stringify(parsed, null, 2));
              setJsonError(null);
            } catch (e) {
              setMessage({ type: 'error', text: t('invalidJson', 'Invalid JSON syntax') });
            }
          }}
          onCancelRollback={() => {
            const rollbackDraft = rollbackDraftBeforeRef.current;
            if (rollbackDraft?.provider) {
              setDetailProvider(rollbackDraft.provider);
              setRawJson(rollbackDraft.rawJson);
            } else {
              setRawJson(originalJson);
            }
            rollbackDraftBeforeRef.current = null;
            setIsRollbackMode(false);
            setJsonError(null);
          }}
          importedInactiveNotice={importedInactiveNotice}
          saving={savingDetail}
          message={message}
        />
      </div>
    );
  };

  if (viewMode === 'detail' && detailProvider) {
    return (
      <div className="flex flex-col h-full space-y-6">
        {renderServiceProviderDetail()}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('aiEnvironments')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('aiEnvironmentsDesc')}</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => { setPresetPickerOpen(true); }}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
            {t('addProvider', 'Add Service Provider')}
          </button>
          <button
            type="button"
            onClick={() => { openPresetEditor(); }}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md border bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={t('manageProviderPresets', 'Manage provider presets')}
            aria-label={t('manageProviderPresets', 'Manage provider presets')}
          >
            <Settings2 className="h-4 w-4" />
          </button>
        </div>
      </div>

      <CliVersionCards
        cliVersions={cliVersions}
        activeTool={activeTool}
        checkingVersions={checkingVersions}
        cliUpdates={cliUpdates}
        checkingAllVersions={checkingAllVersions}
        probingTool={probingTool}
        checkingUpdates={checkingUpdates}
        updatingTool={updatingTool}
        stateProviders={state.providers}
        providerCounts={providerCountsByTool}
        unsavedNewProviderIds={unsavedNewProviderIds}
        setActiveTool={setActiveTool}
        setCurrentProviderId={setCurrentProviderId}
        detectAllVersions={detectAllVersions}
        preloadCliMetaAndAutoImport={preloadCliMetaAndAutoImport}
        handleApplyCliUpdate={handleApplyCliUpdate}
        getManagedStateForTool={getManagedStateForTool}
        t={t}
        versionCheckRunIdRef={versionCheckRunIdRef}
        probeRunIdRef={probeRunIdRef}
        cliProbeInitializedRef={cliProbeInitializedRef}
      />

      {/* Main accordion container */}
      <div className="flex-1 flex flex-col min-h-0 border rounded-xl overflow-hidden bg-background">
        {/* Tool section header */}
        <div className="shrink-0 border-b bg-card px-4 py-3">
          <ToolSectionHeader
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onImport={() => { void handleImportProviders(); }}
            onExport={() => { void handleExportProviders(); }}
            loading={loading}
            previewingImport={previewingImport}
            applyingImport={applyingImport}
            exportingProviders={exportingProviders}
            t={t}
          />
        </div>

        <div
          ref={listScrollContainerRef}
          className="flex-1 overflow-y-auto p-4"
          onScroll={(event) => {
            savedListScrollTopRef.current = event.currentTarget.scrollTop;
          }}
        >
          <ServiceProviderList
            providers={currentToolListItems}
            onProviderClick={openServiceProviderDetail}
            onEdit={openServiceProviderDetail}
            onToggleFavorite={(id, favorite) => {
              void handleToggleFavorite(id, favorite);
            }}
            onApplyGlobal={(id) => {
              if (activeTool === 'claude') {
                void handleClaudeApplyGlobal(id);
                return;
              }
              void activateProvider(activeTool, id);
            }}
            onDelete={(id) => { void handleDelete(id); }}
            onLaunch={(id) => {
              if (activeTool === 'claude') {
                void handleClaudeLaunch(id);
              }
            }}
            onCopyLaunchCommand={(id) => {
              const profile = claudeProfiles.find((item) => item.id === id);
              if (profile) {
                void handleClaudeCopyCommand(profile.id, profile.config_dir);
              }
            }}
            onOpenDirectory={(id) => {
              if (activeTool === 'claude') {
                void handleClaudeOpenDir(id);
              }
            }}
            onAdd={() => { setPresetPickerOpen(true); }}
            tool={activeTool}
            t={(key: string, fallback: string, options?: Record<string, any>) =>
              String(t(key, fallback, options))}
            searchTerm={searchQuery}
            loading={loading}
          />

          <SyncedDevices
            syncedOtherDeviceProviders={syncedOtherDeviceProviders}
            activeTool={activeTool}
            onActivate={handleActivateSyncedProvider}
            loading={loading}
            activatingSyncedKey={activatingSyncedKey}
            t={t}
          />
        </div>
      </div>

      {presetPickerOpen && (
        <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
          <div className="w-full max-w-3xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
            <div className="p-5 border-b flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold">{t('selectProviderPreset', 'Select provider preset')}</h3>
                <p className="text-sm text-muted-foreground mt-1">
                  {t('selectProviderPresetDesc', 'Create a new service provider for the current tool from a reusable endpoint preset.')}
                </p>
              </div>
              <button
                type="button"
                onClick={() => { setPresetPickerOpen(false); }}
                className="rounded-md p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label={t('close', 'Close')}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-5 space-y-3">
              <button
                type="button"
                onClick={() => { handleAddCustom(activeTool); }}
                className="w-full rounded-lg border border-dashed border-primary/40 bg-primary/5 p-4 text-left transition-colors hover:border-primary/60 hover:bg-primary/10"
              >
                <div className="flex items-start gap-3">
                  <div className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground shadow-sm">
                    <Plus className="h-5 w-5" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{t('blankProviderPreset', 'Create manually')}</div>
                    <div className="mt-1 text-sm text-muted-foreground">
                      {t('blankProviderPresetDesc', 'Skip presets and create a provider from an empty form.')}
                    </div>
                  </div>
                </div>
              </button>
              {providerPresets.map((preset) => (
                <div key={preset.id} className="rounded-lg border p-4">
                  <div className="flex items-start justify-between gap-3">
                    <button
                      type="button"
                      onClick={() => { handleAddCustom(activeTool, preset); }}
                      className="min-w-0 flex flex-1 items-start gap-3 text-left"
                    >
                      <ServiceProviderAvatar
                        icon={preset.icon}
                        name={preset.name}
                        id={preset.id}
                        tool={activeTool}
                        size={40}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="font-medium">{preset.name}</div>
                        {preset.description && (
                          <div className="mt-1 text-sm text-muted-foreground">{preset.description}</div>
                        )}
                        <div className="mt-3 grid gap-1 text-xs text-muted-foreground sm:grid-cols-2">
                          <span>{t('providerPresetOpenAILabel', 'OpenAI')}: {preset.endpoints.openai_base_url || t('notSet', 'Not set')}</span>
                          <span>{t('providerPresetAnthropicLabel', 'Anthropic')}: {preset.endpoints.anthropic_base_url || t('notSet', 'Not set')}</span>
                        </div>
                      </div>
                    </button>
                    <button
                      type="button"
                      onClick={() => { openPresetEditor(preset); }}
                      className="rounded-md border p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
                      title={t('editProviderPreset', 'Edit preset')}
                      aria-label={t('editProviderPreset', 'Edit preset')}
                    >
                      <Pencil className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {presetManagerOpen && (
        <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
          <div className="w-full max-w-2xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
            <div className="p-5 border-b flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold">
                  {editingPresetId
                    ? t('editProviderPreset', 'Edit preset')
                    : t('newProviderPreset', 'New preset')}
                </h3>
                <p className="text-sm text-muted-foreground mt-1">
                  {t('providerPresetEndpointDesc', 'Store protocol-specific API URLs only. API keys and instance identifiers stay on service providers.')}
                </p>
              </div>
              <button
                type="button"
                onClick={() => { setPresetManagerOpen(false); }}
                className="rounded-md p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label={t('close', 'Close')}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-5 space-y-4">
              <label className="block text-sm font-medium">
                {t('providerPresetName', 'Preset name')}
                <input
                  value={presetDraft.name}
                  onChange={(event) => { setPresetDraft(prev => ({ ...prev, name: event.target.value })); }}
                  className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
              <label className="block text-sm font-medium">
                {t('providerPresetDescription', 'Description')}
                <input
                  value={presetDraft.description}
                  onChange={(event) => { setPresetDraft(prev => ({ ...prev, description: event.target.value })); }}
                  className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
              <div className="block text-sm font-medium">
                <div className="mb-1">{t('providerPresetIcon', 'Icon')}</div>
                <IconPicker
                  value={presetDraft.icon || undefined}
                  name={presetDraft.name}
                  providerId={presetDraft.id}
                  onChange={(icon) => { setPresetDraft(prev => ({ ...prev, icon: icon || '' })); }}
                  t={(key: string, fallback: string, options?: Record<string, any>) =>
                    String(t(key, fallback, options))}
                  trigger={(
                    <div className="flex w-full items-center justify-between gap-3">
                      <div className="flex min-w-0 items-center gap-3">
                        <ServiceProviderAvatar
                          icon={presetDraft.icon || undefined}
                          name={presetDraft.name || t('newProviderPreset', 'New preset')}
                          id={presetDraft.id || 'preset'}
                          size={32}
                        />
                        <span className="truncate">
                          {presetDraft.icon || t('iconAuto', 'Auto')}
                        </span>
                      </div>
                      <Pencil className="h-4 w-4 shrink-0 text-muted-foreground" />
                    </div>
                  )}
                />
              </div>
              <div className="grid gap-4 sm:grid-cols-1">
                <label className="block text-sm font-medium">
                  {t('providerPresetOpenAIUrl', 'OpenAI-compatible API URL')}
                  <input
                    value={presetDraft.openai_base_url}
                    onChange={(event) => { setPresetDraft(prev => ({ ...prev, openai_base_url: event.target.value })); }}
                    placeholder="https://api.openai.com/v1"
                    className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                  />
                </label>
                <label className="block text-sm font-medium">
                  {t('providerPresetAnthropicUrl', 'Anthropic-compatible API URL')}
                  <input
                    value={presetDraft.anthropic_base_url}
                    onChange={(event) => { setPresetDraft(prev => ({ ...prev, anthropic_base_url: event.target.value })); }}
                    placeholder="https://api.anthropic.com"
                    className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                  />
                </label>
              </div>
              <div className="rounded-lg border border-border/70 bg-muted/20 p-4 space-y-4">
                <div>
                  <div className="text-sm font-medium">
                    {t('providerPresetClaudeSectionTitle', 'Claude-only preset fields')}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t(
                      'providerPresetClaudeSectionDesc',
                      'These fields are stored on the preset, but only applied when creating a new Claude service provider from it.',
                    )}
                  </p>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                  <label className="block text-sm font-medium">
                    {t('providerPresetClaudeDefaultModel', 'Claude default model')}
                    <input
                      value={presetDraft.claude_default_model}
                      onChange={(event) => {
                        setPresetDraft(prev => ({ ...prev, claude_default_model: event.target.value }));
                      }}
                      placeholder="claude-sonnet-4-5"
                      className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                  <label className="block text-sm font-medium">
                    {t('providerPresetClaudeReasoningEffort', 'Claude reasoning effort')}
                    <input
                      value={presetDraft.claude_reasoning_effort}
                      onChange={(event) => {
                        setPresetDraft(prev => ({ ...prev, claude_reasoning_effort: event.target.value }));
                      }}
                      placeholder="high / xhigh / max / auto / custom"
                      className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                </div>
                <div className="space-y-2">
                  <div className="text-sm font-medium">
                    {t('providerPresetClaudeMappings', 'Claude model mappings')}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t(
                      'providerPresetClaudeMappingsDesc',
                      'Configure Haiku, Sonnet, and Opus upstream models for new Claude providers. Empty rows are ignored when saving.',
                    )}
                  </p>
                  <ModelMappingTable
                    mappings={presetDraft.claude_model_mappings}
                    onChange={(mappings) => {
                      setPresetDraft(prev => ({
                        ...prev,
                        claude_model_mappings: buildPresetClaudeMappings(mappings),
                      }));
                    }}
                    t={(key: string, fallback: string) => String(t(key, fallback))}
                  />
                </div>
              </div>
            </div>
            <div className="border-t p-4 flex items-center justify-between gap-3">
              <div>
                {editingPresetId && (
                  <button
                    type="button"
                    onClick={() => { void deleteProviderPreset(editingPresetId); setPresetManagerOpen(false); }}
                    className="inline-flex items-center gap-2 rounded-md border border-destructive/40 px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                  >
                    <Trash2 className="h-4 w-4" />
                    {t('delete', 'Delete')}
                  </button>
                )}
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => { setPresetManagerOpen(false); }}
                  className="rounded-md border px-3 py-2 text-sm hover:bg-muted"
                >
                  {t('cancel', 'Cancel')}
                </button>
                <button
                  type="button"
                  onClick={() => { void savePresetDraft(); }}
                  className="rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                >
                  {t('save', 'Save')}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

    {importPreview && (
      <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
        <div className="w-full max-w-4xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
          <div className="p-5 border-b flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h3 className="text-lg font-semibold">
                {t('providersImportReviewTitle', 'Review Service Provider import')}
              </h3>
              <p className="text-sm text-muted-foreground mt-1">
                {t('providersImportReviewDesc', {
                  total: importPreview.total || 0,
                  conflicts: importPreview.conflicts || 0,
                  defaultValue:
                    'Found {{total}} Service Provider(s), including {{conflicts}} conflict(s). Choose how to handle conflicts before importing.',
                })}
              </p>
              {importPath && (
                <p className="text-xs text-muted-foreground mt-2 truncate">{importPath}</p>
              )}
            </div>
            <button
              type="button"
              onClick={closeImportModal}
              disabled={applyingImport}
              className="p-2 hover:bg-muted rounded-md transition-colors disabled:opacity-50"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          <div className="p-5 overflow-y-auto space-y-4">
            <div className="flex flex-wrap gap-2 text-xs">
              <span className="px-2 py-1 rounded-md border bg-muted/40">
                {t('providersImportTotalBadge', {
                  count: importPreview.total || 0,
                  defaultValue: '{{count}} total',
                })}
              </span>
              <span className="px-2 py-1 rounded-md border bg-amber-500/10 text-amber-700 border-amber-500/30">
                {t('providersImportConflictBadge', {
                  count: importPreview.conflicts || 0,
                  defaultValue: '{{count}} conflict(s)',
                })}
              </span>
              <span className="badge-pill bg-green-500/10 text-green-700">
                {t('providersImportNewBadge', {
                  count: importNewItems.length,
                  defaultValue: '{{count}} new',
                })}
              </span>
              <span className="px-2 py-1 rounded-md border bg-blue-500/10 text-blue-700 border-blue-500/30">
                {t('providersImportActiveBadge', {
                  count: Object.keys(importPreview.active || {}).length,
                  defaultValue: '{{count}} active binding(s) in file',
                })}
              </span>
            </div>

            {importConflictItems.length > 0 && (
              <div className="rounded-lg border border-amber-200 bg-amber-50/70 p-4 space-y-3">
                <div>
                  <div className="font-medium text-amber-900">
                    {t('providersImportConflictTitle', 'Conflict handling')}
                  </div>
                  <p className="text-sm text-amber-800/90 mt-1">
                    {t(
                      'providersImportConflictDesc',
                      'Overwrite will update the existing Service Provider. Create new will keep both versions.',
                    )}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => handleSetAllImportConflictActions('overwrite')}
                    disabled={applyingImport}
                    className="px-3 py-1.5 rounded-md border border-amber-300 bg-background hover:bg-amber-100 text-sm transition-colors disabled:opacity-50"
                  >
                    {t('providersImportSetAllOverwrite', 'Set all to overwrite')}
                  </button>
                  <button
                    type="button"
                    onClick={() => handleSetAllImportConflictActions('new')}
                    disabled={applyingImport}
                    className="px-3 py-1.5 rounded-md border border-amber-300 bg-background hover:bg-amber-100 text-sm transition-colors disabled:opacity-50"
                  >
                    {t('providersImportSetAllNew', 'Set all to create new')}
                  </button>
                </div>
              </div>
            )}

            <div className="space-y-3">
              {importPreview.items.map(item => {
                const selectedAction = importDecisions[item.import_key] || 'overwrite';
                const conflictText = item.conflict_reason === 'name'
                  ? t(
                      'providersImportConflictByName',
                      'Same tool and same name already exist locally: {{name}} ({{id}})',
                      { name: item.existing_name || '', id: item.existing_id || '' },
                    )
                  : t(
                      'providersImportConflictById',
                      'Same tool and same ID already exist locally: {{name}} ({{id}})',
                      { name: item.existing_name || '', id: item.existing_id || '' },
                    );

                return (
                  <div
                    key={item.import_key}
                    className={`rounded-lg border p-4 ${
                      item.conflict ? 'border-amber-200 bg-amber-50/40' : 'border-border bg-card'
                    }`}
                  >
                    <div className="flex gap-3">
                      <div className="w-10 h-10 rounded-lg border bg-muted/40 flex items-center justify-center shrink-0">
                        <ToolIcon tool={item.tool} className="w-5 h-5" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">{item.name}</span>
                          <span className="text-[10px] uppercase px-1.5 py-0.5 rounded border bg-muted/50 text-muted-foreground">
                            {item.tool}
                          </span>
                          {item.model && (
                            <span className="text-[10px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-700 border-blue-500/30">
                              {item.model}
                            </span>
                          )}
                          {item.conflict ? (
                            <span className="badge-pill bg-amber-500/10 text-amber-700">
                              {t('providersImportConflict', 'Conflict')}
                            </span>
                          ) : (
                            <span className="badge-pill bg-green-500/10 text-green-700">
                              {t('providersImportWillCreate', 'Create new')}
                            </span>
                          )}
                        </div>
                        <p className="text-xs font-mono text-muted-foreground mt-1">{item.id}</p>
                        {item.conflict && (
                          <p className="text-xs text-amber-800 mt-2">{conflictText}</p>
                        )}
                      </div>
                      {item.conflict && (
                        <div className="shrink-0 flex flex-col gap-2 min-w-[188px]">
                          <button
                            type="button"
                            onClick={() => {
                              setImportDecisions(prev => ({
                                ...prev,
                                [item.import_key]: 'overwrite',
                              }));
                            }}
                            disabled={applyingImport}
                            className={`px-3 py-2 rounded-md text-sm border transition-colors ${
                              selectedAction === 'overwrite'
                                ? 'border-primary bg-primary text-primary-foreground'
                                : 'border-border bg-background hover:bg-muted'
                            }`}
                          >
                            {t('providersImportOverwrite', 'Overwrite')}
                          </button>
                          <button
                            type="button"
                            onClick={() => {
                              setImportDecisions(prev => ({
                                ...prev,
                                [item.import_key]: 'new',
                              }));
                            }}
                            disabled={applyingImport}
                            className={`px-3 py-2 rounded-md text-sm border transition-colors ${
                              selectedAction === 'new'
                                ? 'border-primary bg-primary text-primary-foreground'
                                : 'border-border bg-background hover:bg-muted'
                            }`}
                          >
                            {t('providersImportCreateNew', 'Create new')}
                          </button>
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="p-4 border-t bg-muted/10 flex items-center justify-between gap-4">
            <div className="text-xs text-muted-foreground">
              {t('providersImportFooterHint', {
                count: importConflictItems.length,
                defaultValue:
                  '{{count}} conflict(s) require a choice. Non-conflicting Service Providers will be imported directly.',
              })}
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={closeImportModal}
                disabled={applyingImport}
                className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md transition-colors disabled:opacity-50"
              >
                {t('cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleApplyImport();
                }}
                disabled={applyingImport}
                className="px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md flex items-center gap-2 transition-colors disabled:opacity-50"
              >
                {applyingImport ? <Loader2 className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />}
                {t('import', 'Import')}
              </button>
            </div>
          </div>
        </div>
      </div>
    )}
    {permissionDialogClaudeProfileId && (
      <TerminalPermissionConfirmDialog
        open={permissionDialogOpen}
        toolId="claude"
        toolLabel="Claude Code"
        onConfirm={handleClaudePermissionConfirm}
        onCancel={handleClaudePermissionCancel}
      />
    )}
  </div>
);
}
