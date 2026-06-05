import { useState, useEffect, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { message, open, save } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Loader2, Plus, TerminalSquare, Upload, X } from 'lucide-react';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './icons';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { CliVersionCards } from './CliVersionCards';
import { ToolSectionHeader } from './ToolSectionHeader';
import { SyncedDevices } from './SyncedDevices';
import { ServiceProviderDetail } from './ServiceProviderDetail';
import { ServiceProviderList, type ServiceProviderListItem } from './ServiceProviderList';
import { useToast } from '../ToastProvider';

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
type ApiResp<T> = { ok: boolean; data: T; meta: { schema_version: number; revision: number } };
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

export interface HistoryEntry {
  timestamp: number;
  content: string;
}

export interface AiProvider {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
  
  // Claude 专属模型路由
  claude_reasoning_model?: string;
  claude_haiku_model?: string;
  claude_sonnet_model?: string;
  claude_opus_model?: string;
  claude_default_model?: string; // ANTHROPIC_MODEL - 通用默认模型
  
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
  auth_type: string;
  model: string | null;
  tool_config: Record<string, any>;
  raw_api_key?: string;
  raw_base_url?: string | null;
  tilde_config_dir?: string;
  claude_model_mappings?: Array<{
    family?: string;
    display_name?: string;
    upstream_model?: string;
    supports_1m?: boolean;
  }>;
}

type ClaudeModelMappingDraft = {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
};

export interface AiProvidersState {
  active_claude: string | null;
  active_codex: string | null;
  active_gemini: string | null;
  active_opencode: string | null;
  providers: AiProvider[];
  is_encrypted?: boolean;
}

type SavePresetResult = {
  ok: boolean;
  providerId?: string;
  provider?: AiProvider;
  wasActiveBeforeSave?: boolean;
};

const DEFAULT_STATE: AiProvidersState = {
  active_claude: null,
  active_codex: null,
  active_gemini: null,
  active_opencode: null,
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
  const [_message, setMessage] = useState({ type: '', text: '' });
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
  const [syncedOtherDeviceProviders, setSyncedOtherDeviceProviders] = useState<SyncedDeviceProvidersView[]>([]);
  const [activatingSyncedKey, setActivatingSyncedKey] = useState<string | null>(null);
  const [claudeProfiles, setClaudeProfiles] = useState<ClaudeProfileSummary[]>([]);
  const [copiedClaudeProfileId, setCopiedClaudeProfileId] = useState<string | null>(null);
  const [claudeLaunchCommand, setClaudeLaunchCommand] = useState('claude --session-id {session_id}');
  const [launchingClaudeProfileId, setLaunchingClaudeProfileId] = useState<string | null>(null);
  const [applyingClaudeProfileId, setApplyingClaudeProfileId] = useState<string | null>(null);
  const [exportingProviders, setExportingProviders] = useState(false);
  const [previewingImport, setPreviewingImport] = useState(false);
  const [applyingImport, setApplyingImport] = useState(false);
  const [importPreview, setImportPreview] = useState<ProvidersImportPreview | null>(null);
  const [importPath, setImportPath] = useState<string | null>(null);
  const [importDecisions, setImportDecisions] = useState<Record<string, 'overwrite' | 'new'>>({});

  // Accordion state
  const [searchQuery, setSearchQuery] = useState('');

  // Service provider list/detail view mode
  const [viewMode, setViewMode] = useState<'list' | 'detail'>('list');
  const [detailProvider, setDetailProvider] = useState<any | null>(null);

  const versionCheckRunIdRef = useRef(0);
  const probeRunIdRef = useRef(0);
  const isVisibleRef = useRef(isVisible);
  const cliProbeInitializedRef = useRef(false);
  const autoImportInitializedRef = useRef(false);

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

  const getIsGlobalForTool = (tool: string, id: string) =>
    (state[`active_${tool}` as keyof AiProvidersState] as string | null) === id;

  const buildClaudeModelMappings = (source: Record<string, any>): ClaudeModelMappingDraft[] => {
    const explicitMappings = source.claude_model_mappings || source.tool_config?.claude_model_mappings;
    if (Array.isArray(explicitMappings) && explicitMappings.length > 0) {
      return explicitMappings;
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
      },
    ].filter((mapping) => mapping.upstream_model.trim().length > 0);

    return fromLegacyFields;
  };

  const buildClaudeProviderFromProfile = (profile: ClaudeProfileSummary): Partial<AiProvider> => ({
    id: profile.id,
    tool: 'claude',
    name: profile.name,
    icon: profile.icon || undefined,
    code: profile.code || undefined,
    api_key: profile.raw_api_key || '',
    base_url: profile.raw_base_url || '',
    model: profile.model || undefined,
    claude_api_format: profile.tool_config?.claude_api_format || 'anthropic_messages',
    claude_auth_env_key: profile.tool_config?.claude_auth_env_key || 'ANTHROPIC_AUTH_TOKEN',
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
    claude_reasoning_model: profile.tool_config?.claude_reasoning_model,
    claude_haiku_model: profile.tool_config?.claude_haiku_model,
    claude_sonnet_model: profile.tool_config?.claude_sonnet_model,
    claude_opus_model: profile.tool_config?.claude_opus_model,
    claude_default_model: profile.tool_config?.claude_default_model,
    claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
    dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
    enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
    enable_mcp: profile.tool_config?.enable_mcp || false,
    allowed_tools: profile.tool_config?.allowed_tools || [],
    blocked_tools: profile.tool_config?.blocked_tools || [],
    max_session_turns: profile.tool_config?.max_session_turns,
    env_managed: true,
    is_enabled: true,
  });

  const buildClaudeProviderFromState = (provider: AiProvider): Partial<AiProvider> => ({
    ...provider,
    tool: 'claude',
    remark: provider.tool_config?.remark || '',
    claude_model_mappings: buildClaudeModelMappings(provider),
  });

  const normalizeProviderForSave = (provider: Partial<AiProvider>) => {
    const next: Record<string, any> = { ...provider };
    const nextToolConfig = { ...(provider.tool_config || {}) };
    const remark = typeof provider.remark === 'string' ? provider.remark : '';

    if (remark.trim()) {
      nextToolConfig.remark = remark;
    } else {
      delete nextToolConfig.remark;
    }

    next.tool_config = nextToolConfig;
    return next;
  };

  const getOpenCodeJson = (provider: Partial<AiProvider>) => {
    const internalFields = [
      'id', 'tool', 'is_enabled', 'provider_key', 'api_key', 'base_url', 'model',
      'claude_reasoning_model', 'claude_haiku_model', 'claude_sonnet_model', 
      'claude_opus_model', 'claude_default_model', 'dangerously_skip_permissions', 'history',
      'enable_all_memory_features', 'enable_mcp', 'allowed_tools', 'blocked_tools',
      'max_session_turns', 'disable_response_storage', 'personality', 'wire_api',
      'gemini_auth_type', 'opencode_default_model', 'opencode_default_agent',
      'opencode_sessions_dir', 'model_reasoning_effort', 'model_reasoning_summary',
      'approval_policy', 'sandbox_mode', 'theme', 'vim_mode', 'default_approval_mode',
      'small_model', 'timeout', 'share_mode', 'env_managed', 'claude_reasoning_effort'
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
      const res = await invoke<ApiResp<AiProvidersState>>('providers_list');
      if (silent && !isVisibleRef.current) return;

      if (res.data.providers && res.data.providers.length > 0) {
        setState(res.data);
      } else {
        // Only set default if it was truly empty and we didn't have existing state
        // This prevents wiping state if backend temporarily returns empty
        setState(prev => prev.providers.length > 0 ? prev : DEFAULT_STATE);
      }
      setUnsavedNewProviderIds(new Set());
      try {
        const syncedRes = await invoke<ApiResp<SyncedDeviceProvidersView[]>>('providers_list_synced_other_devices');
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
            const res = await invoke<ApiResp<AutoImportResult>>('providers_auto_import_from_system', { tool });
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

      await invoke('providers_set_active', { tool, providerId });
      await loadProviders(true);
      await invoke('projection_apply', { tool, providerId });

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
        [selectedModel]: existingModelConfig,
      };
    } else if (Object.keys(next.models).length > 0) {
      const [firstModel] = Object.keys(next.models);
      next.model = firstModel;
    }

    return next;
  };

  const buildProviderForSave = (provider: Partial<AiProvider>): AiProvider => {
    const newId = provider.id || `custom-${Date.now()}`;
    let baseProvider: Record<string, any> = { ...provider };
    let currentHistory = Array.isArray(provider.history) ? [...provider.history] : [];

    if (provider.tool === 'opencode') {
      let parsed: Record<string, any>;
      try {
        parsed = JSON.parse(rawJson || '{}');
      } catch {
        throw new Error(t('invalidJson', 'Invalid JSON syntax'));
      }

      if (rawJson !== originalJson && originalJson) {
        currentHistory = [
          { timestamp: Date.now(), content: originalJson },
          ...currentHistory,
        ].slice(0, 50);
      }

      baseProvider = syncOpenCodeProviderWithJson({ ...provider, history: currentHistory }, parsed);

      if (parsed.options && typeof parsed.options === 'object') {
        baseProvider.api_key = parsed.options.apiKey || baseProvider.api_key || '';
        baseProvider.base_url = parsed.options.baseURL || baseProvider.base_url || '';
      }

      if (parsed.models && typeof parsed.models === 'object') {
        const firstModel = Object.keys(parsed.models)[0];
        if (firstModel) {
          baseProvider.model = firstModel;
        }
      }
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
      history: currentHistory,
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

    const newId = provider.id || `custom-${Date.now()}`;
    const wasActiveBeforeSave =
      provider.tool !== 'opencode' &&
      ((state as any)[`active_${provider.tool || activeTool}`] as string | null) === newId;

    try {
      const finalProvider = buildProviderForSave(provider);
      await invoke('service_providers_upsert', { provider: normalizeProviderForSave(finalProvider) });
      await loadProviders(true);
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.delete(newId);
        return next;
      });
      setCurrentProviderId(finalProvider.id);
      emit('refresh-counts');
      setDetailProvider(finalProvider);
      setIsRollbackMode(false);
      setJsonError(null);
      if (finalProvider.tool === 'opencode') {
        setOriginalJson(rawJson);
      }

      if (wasActiveBeforeSave || finalProvider.tool === 'opencode') {
        await invoke('projection_apply', { tool: finalProvider.tool, providerId: finalProvider.id });
      }

      if (showSavedMessage) {
        setMessage({ type: 'success', text: t('providerSaved', 'Service Provider saved') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        pushToast({ title: t('providerSaved', 'Service Provider saved'), kind: 'success' });
      }

      return {
        ok: true,
        providerId: finalProvider.id,
        provider: finalProvider,
        wasActiveBeforeSave
      };
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      return { ok: false };
    }
  };

  const handleSavePresetWithActivationPrompt = async (provider: Partial<AiProvider>) => {
    const result = await saveDetailProvider(provider);
    if (!result.ok || provider.tool === 'opencode' || !result.providerId || result.wasActiveBeforeSave) return;

    const canActivate =
      !!result.provider?.api_key &&
      !(isManagedTool(String(provider.tool || activeTool)) && result.provider?.env_managed === false);
    if (!canActivate) return;

    const confirmed = await confirmDialog(
      t('confirmActivateAfterSave', 'Service Provider saved. Activate it now?'),
      {
        okLabel: t('applyToCli'),
        cancelLabel: t('cancel')
      }
    );
    if (!confirmed) return;

    await activateProvider(String(provider.tool || activeTool), result.providerId);
  };

  const handleRollback = (entry: HistoryEntry) => {
    try {
      JSON.parse(entry.content); // Verify syntax
      setRawJson(entry.content);
      setIsRollbackMode(true);
      setJsonError(null);
      setMessage({ type: 'success', text: t('rollbackModeTitle', 'History version loaded.') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e) {
      setMessage({ type: 'error', text: t('parseHistoryFailed') });
    }
  };

  const handleAddCustom = (toolName: string) => {
    const newId = `custom-${Date.now()}`;
    const newProvider: AiProvider = {
      id: newId,
      name: `${t('newPreset', 'New Service Provider')} (${toolName})`,
      tool: toolName,
      api_key: '',
      base_url: '',
      model: '',
      code: toolName === 'claude' ? 'new-profile' : undefined,
      env_managed: toolName !== 'opencode' ? true : undefined,
      provider_key: toolName === 'opencode' ? `provider_${Date.now()}` : undefined,
      is_enabled: true,
      ...(toolName === 'claude' ? {
        claude_api_format: 'anthropic_messages',
        claude_auth_env_key: 'ANTHROPIC_AUTH_TOKEN',
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
    setDetailProvider(newProvider);
    setViewMode('detail');
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
      isManagedTool(targetTool) && providerToDelete.id === `default-${targetTool}`;
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

    const confirmMsg = t('confirmDelete', { name: providerToDelete.name });

    const confirmed = await confirmDialog(confirmMsg, {
      okLabel: t('ok'),
      cancelLabel: t('cancel')
    });
    if (!confirmed) return;

    if (isUnsavedNewProvider) {
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
      setMessage({ type: 'success', text: t('deleteSuccess', 'Preset deleted successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    try {
      await invoke('service_providers_delete', { providerId: targetId });
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
    const confirmed = await confirmDialog(confirmMsg, {
      title: t('confirmCliUpdateTitle') + `: ${toolLabel}`,
      okLabel: t('cliUpdate'),
      cancelLabel: t('cancel'),
    });
    if (!confirmed) return;

    setUpdatingTool(prev => ({ ...prev, [tool]: true }));
    try {
      const result = await invoke<CliUpdateApplyResult>('apply_cli_update', { tool });
      if (result.success && result.terminal_launched) {
        setMessage({ type: 'success', text: t('cliUpdateTerminalLaunched') });
        setTimeout(() => setMessage({ type: '', text: '' }), 5000);
      } else {
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
      await invoke('open_local_path', { path: configDir });
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
      await invoke('projection_apply', { tool: 'claude', providerId: profileId });
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

  const handleClaudeLaunch = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setLaunchingClaudeProfileId(profileId);
      const result = await invoke<{ ok: boolean; data: { providerId?: string; tool?: string; activated?: boolean } }>('sessions_create', {
        session: {
          name: '',
          working_dir: '',
          tool: 'claude',
          provider_id: profileId,
          status: 'active'
        }
      });
      if (result.ok && result.data.providerId && result.data.tool) {
        setCurrentProviderId(result.data.providerId);
        await emit('session-created');
      }
      setMessage({ type: 'success', text: t('sessionCreated', 'Session created') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      console.error('handleClaudeLaunch error:', e);
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLaunchingClaudeProfileId(null);
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

    const deviceSlug = String(deviceId || '')
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, '-')
      .replace(/^-+|-+$/g, '');
    const sourceId = String(provider.id || `synced-${Date.now()}`);
    const targetId = `synced-${deviceSlug || 'device'}-${sourceId}`;
    const targetTool = String(provider.tool || '').toLowerCase();
    if (!TOOLS.includes(targetTool as CliTool)) {
      return;
    }

    const payload: Record<string, any> = {
      id: targetId,
      name: `${provider.name} (${deviceId})`,
      tool: targetTool,
      api_key: apiKey,
      base_url: provider.base_url || '',
      model: provider.model || '',
      is_enabled: targetTool === 'opencode' ? provider.is_enabled ?? true : true,
      env_managed: targetTool !== 'opencode' ? true : undefined,
    };
    if (targetTool === 'opencode') {
      payload.provider_key =
        provider.provider_key ||
        `synced_${deviceSlug || 'device'}_${Date.now()}`.replace(/[^a-zA-Z]/g, '');
    }

    const actionKey = `${deviceId}:${provider.tool}:${provider.id}`;
    try {
      setLoading(true);
      setActivatingSyncedKey(actionKey);
      await invoke('providers_upsert', { provider: payload });
      await invoke('providers_set_active', { tool: targetTool, providerId: targetId });
      await invoke('projection_apply', { tool: targetTool, providerId: targetId });
      await loadProviders(true);
      setActiveTool(targetTool);
      setCurrentProviderId(targetId);
      emit('refresh-counts');
      setMessage({
        type: 'success',
        text: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: String(e) });
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
      const res = await invoke<ApiResp<ProvidersExportResult>>('providers_export', {
        outputPath,
      });
      const successText = t('providersExportSuccess', {
        count: res.data?.count ?? 0,
        path: res.data?.path || outputPath,
        defaultValue: 'Exported {{count}} Service Provider(s) to {{path}}',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(successText, {
        title: t('aiEnvironments', 'AI Terminal Service Providers'),
        kind: 'info',
      });
    } catch (e: any) {
      const errorText = t('providersExportFailed', {
        error: String(e),
        defaultValue: 'Failed to export Service Providers: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(errorText, {
        title: t('aiEnvironments', 'AI Terminal Service Providers'),
        kind: 'error',
      });
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
      const res = await invoke<ApiResp<ProvidersImportPreview>>('providers_import_preview', {
        importPath: selectedPath,
      });
      if (!res.data?.items?.length) {
        const emptyText = t('providersImportEmpty', 'No Service Providers found in the selected file.');
        setMessage({ type: 'error', text: emptyText });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        await message(emptyText, {
          title: t('aiEnvironments', 'AI Terminal Service Providers'),
          kind: 'warning',
        });
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
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(errorText, {
        title: t('aiEnvironments', 'AI Terminal Service Providers'),
        kind: 'error',
      });
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

      const res = await invoke<ApiResp<ProvidersImportApplyResult>>('providers_import_apply', {
        importPath,
        decisions,
      });

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
      await message(successText, {
        title: t('aiEnvironments', 'AI Terminal Service Providers'),
        kind: 'info',
      });
    } catch (e: any) {
      const errorText = t('providersImportApplyFailed', {
        error: String(e),
        defaultValue: 'Failed to import Service Providers: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(errorText, {
        title: t('aiEnvironments', 'AI Terminal Service Providers'),
        kind: 'error',
      });
    } finally {
      setApplyingImport(false);
    }
  };

  const syncedProvidersByTool = useMemo(() => {
    const grouped: Record<string, Array<{ deviceId: string; activeId: string | null; providers: SyncedDeviceProvider[] }>> = {
      claude: [],
      codex: [],
      gemini: [],
      opencode: [],
    };
    for (const device of syncedOtherDeviceProviders) {
      const activeMap = device.active || {};
      for (const tool of TOOLS) {
        const providers = (device.providers || []).filter((item) => item.tool === tool);
        if (providers.length === 0) continue;
        grouped[tool].push({
          deviceId: device.device_id,
          activeId: activeMap[tool] || null,
          providers,
        });
      }
    }
    return grouped;
  }, [syncedOtherDeviceProviders]);

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
      ['haiku', profile.tool_config?.claude_haiku_model],
      ['sonnet', profile.tool_config?.claude_sonnet_model],
      ['opus', profile.tool_config?.claude_opus_model],
      ['reasoning', profile.tool_config?.claude_reasoning_model],
      ['default', profile.tool_config?.claude_default_model],
    ]
      .map(([family, value]) => {
        const upstream = String(value || '').trim();
        return upstream ? `${family}: ${upstream}` : null;
      })
      .filter((value): value is string => Boolean(value));
  };

  const currentToolListItems = useMemo<ServiceProviderListItem[]>(() => {
    if (activeTool === 'claude') {
      return claudeProfiles.map((profile) => {
        const upstreamTags = getClaudeMappingTags(profile);

        const modelTags = [
          profile.model?.trim(),
        ].filter((value): value is string => Boolean(value));

        const description =
          profile.tilde_config_dir || profile.config_dir || profile.code || '';
        const apiFormatKey = profile.tool_config?.claude_api_format || 'anthropic_messages';
        const apiFormatTag =
          apiFormatKey === 'open_ai_chat'
            ? t('openAiChatFormat', 'OpenAI Chat')
            : apiFormatKey === 'open_ai_responses'
              ? t('openAiResponsesFormat', 'OpenAI Responses')
              : t('anthropicMessagesFormat', 'Anthropic Messages');

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
          isGlobal: !!profile.is_global || getIsGlobalForTool('claude', profile.id),
          canLaunch: true,
          canDelete: true,
          launchBusy: launchingClaudeProfileId === profile.id,
          applyBusy: applyingClaudeProfileId === profile.id,
          deleteBusy: false,
          copiedCommand: copiedClaudeProfileId === profile.id,
        } satisfies ServiceProviderListItem;
      });
    }

    return state.providers
      .filter((provider) => provider.tool === activeTool)
      .map((provider) => ({
        id: provider.id,
        name: provider.name,
        tool: provider.tool,
        icon: provider.icon,
        description: provider.base_url || provider.provider_key || '',
        remark: provider.tool_config?.remark || '',
        authLabel:
          activeTool === 'gemini' && provider.gemini_auth_type
            ? provider.gemini_auth_type
            : provider.api_key
              ? 'API Key'
              : undefined,
        modelTags: [provider.model].filter((value): value is string => Boolean(value)),
        claudeUpstreamModelTags: [],
        apiFormatTag: null,
        isGlobal: getIsGlobalForTool(activeTool, provider.id),
        canLaunch: false,
        canDelete: true,
        launchBusy: false,
        applyBusy: loading,
        deleteBusy: false,
        copiedCommand: false,
      }));
  }, [
    activeTool,
    applyingGlobal,
    claudeProfiles,
    copiedClaudeProfileId,
    launchingClaudeProfileId,
    applyingClaudeProfileId,
    loading,
    getClaudeMappingTags,
    state,
  ]);

  const openServiceProviderDetail = (id: string) => {
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
      (detailProvider?.tool === 'opencode' && state.active_opencode === detailProvider?.id);

    const isManagedImportedDetail =
      !!detailProvider &&
      isManagedTool(detailProvider.tool) &&
      detailProvider.id === `default-${detailProvider.tool}` &&
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
            if (!isTauri || !detailProvider) return;
            try {
              await handleSavePresetWithActivationPrompt(detailProvider);
            } catch (e: any) {
              setMessage({ type: 'error', text: e?.message || t('saveFailed', 'Save failed') });
              pushToast({ title: t('saveFailed', 'Save failed'), description: String(e?.message || e), kind: 'error' });
            }
          }}
          onActivate={async () => {
            if (!isTauri || !detailProvider) return;
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
              setMessage({ type: 'error', text: e?.message || t('activationFailed', 'Activation failed') });
              pushToast({ title: t('activationFailed', 'Activation failed'), description: String(e?.message || e), kind: 'error' });
            }
          }}
          onDelete={async () => {
            if (!isTauri || !detailProvider) return;
            try {
              await handleDelete(detailProvider.id, detailProvider.tool);
            } catch (e: any) {
              setMessage({ type: 'error', text: e?.message || t('deleteFailed', 'Delete failed') });
              pushToast({ title: t('deleteFailed', 'Delete failed'), description: String(e?.message || e), kind: 'error' });
            }
          }}
          onBack={() => { setViewMode('list'); setDetailProvider(null); }}
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
            setRawJson(originalJson);
            setIsRollbackMode(false);
            setJsonError(null);
          }}
          importedInactiveNotice={importedInactiveNotice}
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
        <button
          type="button"
          onClick={() => { handleAddCustom(activeTool); }}
          className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Plus className="h-4 w-4" />
          {t('addProvider', 'Add Service Provider')}
        </button>
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
            activeTool={activeTool}
            providerCount={(() => {
              const toolProviders = state.providers.filter(p => p.tool === activeTool);
              const syncedGroups = syncedProvidersByTool[activeTool] || [];
              const syncedCount = syncedGroups.reduce((sum, g) => sum + g.providers.length, 0);
              return toolProviders.length + syncedCount;
            })()}
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

        <div className="flex-1 overflow-y-auto p-4">
          <ServiceProviderList
            providers={currentToolListItems}
            onProviderClick={openServiceProviderDetail}
            onEdit={openServiceProviderDetail}
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
            onAdd={() => { handleAddCustom(activeTool); }}
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
  </div>
);
}
