import { useState, useEffect, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { message, open, save } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Save, Play, Trash2, ShieldAlert, KeyRound, Globe, Zap, Brain, Sparkles, Box, TerminalSquare, Code2, Eraser, History, RotateCcw, X, Settings, AlertTriangle, Loader2, Check, Upload, Star, Hash } from 'lucide-react';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './icons';
import Editor from 'react-simple-code-editor';
import { highlight, languages } from 'prismjs';
import 'prismjs/components/prism-json';
import 'prismjs/themes/prism-tomorrow.css';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { CliVersionCards } from './CliVersionCards';
import { AccordionItem } from './AccordionItem';
import { ToolSectionHeader } from './ToolSectionHeader';
import { SyncedDevices } from './SyncedDevices';

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
  code: string | null;
  config_dir: string;
  is_default: boolean;
  auth_type: string;
  model: string | null;
  tool_config: Record<string, any>;
  raw_api_key?: string;
  raw_base_url?: string | null;
  tilde_config_dir?: string;
}

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
  const confirmDialog = useConfirmDialog();
  const [state, setState] = useState<AiProvidersState>(DEFAULT_STATE);
  const [activeTool, setActiveTool] = useState('claude');
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  
  const [editingProvider, setEditingProvider] = useState<Partial<AiProvider>>({});
  const [originalProvider, setOriginalProvider] = useState<Partial<AiProvider>>({});
  const [rawJson, setRawJson] = useState('');
  const [originalJson, setOriginalJson] = useState('');
  const [saving, setSaving] = useState(false);
  const [applyingGlobal, setApplyingGlobal] = useState(false);
  const [loading, setLoading] = useState(false);
  const [_message, setMessage] = useState({ type: '', text: '' });
  const [showHistory, setShowHistory] = useState(false);
  const [isRollbackMode, setIsRollbackMode] = useState(false);
  const [cliVersions, setCliVersions] = useState<Partial<Record<CliTool, CliVersionState>>>({});
  const [checkingVersions, setCheckingVersions] = useState<Partial<Record<CliTool, boolean>>>({});
  const [checkingAllVersions, setCheckingAllVersions] = useState(false);
  const [cliUpdates, setCliUpdates] = useState<Partial<Record<CliTool, CliUpdateInfo>>>({});
  const [checkingUpdates, setCheckingUpdates] = useState<Partial<Record<CliTool, boolean>>>({});
  const [updatingTool, setUpdatingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const [cliProbe, setCliProbe] = useState<Partial<Record<CliTool, CliEnvProbeResult>>>({});
  const [probingTool, setProbingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const [, setAutoImportInactiveNotice] = useState<Partial<Record<CliTool, string>>>({});
  const [skippingClaudeOnboarding, setSkippingClaudeOnboarding] = useState(false);
  const [copiedInstallCommandKey, setCopiedInstallCommandKey] = useState<string | null>(null);
  const [unsavedNewProviderIds, setUnsavedNewProviderIds] = useState<Set<string>>(new Set());
  const [syncedOtherDeviceProviders, setSyncedOtherDeviceProviders] = useState<SyncedDeviceProvidersView[]>([]);
  const [activatingSyncedKey, setActivatingSyncedKey] = useState<string | null>(null);
  const [claudeProfiles, setClaudeProfiles] = useState<ClaudeProfileSummary[]>([]);
  const [claudeProfileLoading, setClaudeProfileLoading] = useState(false);
  const [copiedClaudeProfileId, setCopiedClaudeProfileId] = useState<string | null>(null);
  const [copiedProfileDir, setCopiedProfileDir] = useState<string | null>(null);
  const [claudeLaunchCommand, setClaudeLaunchCommand] = useState('claude --session-id {session_id}');
  const [exportingProviders, setExportingProviders] = useState(false);
  const [previewingImport, setPreviewingImport] = useState(false);
  const [applyingImport, setApplyingImport] = useState(false);
  const [importPreview, setImportPreview] = useState<ProvidersImportPreview | null>(null);
  const [importPath, setImportPath] = useState<string | null>(null);
  const [importDecisions, setImportDecisions] = useState<Record<string, 'overwrite' | 'new'>>({});

  // Accordion state
  const [openIds, setOpenIds] = useState<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = useState('');
  const [activeFilters, setActiveFilters] = useState<Set<string>>(new Set());

  const historyRef = useRef<HTMLDivElement>(null);
  const versionCheckRunIdRef = useRef(0);
  const probeRunIdRef = useRef(0);
  const isVisibleRef = useRef(isVisible);
  const cliProbeInitializedRef = useRef(false);
  const autoImportInitializedRef = useRef(false);
  const prevActiveToolRef = useRef<string>(activeTool);

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

  const hasChanges = (() => {
    if (activeTool === 'opencode') {
      // Check raw JSON changes, name, provider_key, AND global config fields
      return rawJson !== originalJson || 
        editingProvider.name !== originalProvider.name || 
        editingProvider.provider_key !== originalProvider.provider_key ||
        editingProvider.opencode_default_model !== originalProvider.opencode_default_model ||
        editingProvider.opencode_default_agent !== originalProvider.opencode_default_agent ||
        editingProvider.opencode_sessions_dir !== originalProvider.opencode_sessions_dir;
    }
    
    // For other tools, compare all fields including new advanced config
    return JSON.stringify(editingProvider) !== JSON.stringify(originalProvider);
  })();

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
    setClaudeProfileLoading(true);
    try {
      const res = await invoke<ApiResp<ClaudeProfileSummary[]>>('claude_profile_list');
      if (res.data) {
        setClaudeProfiles(res.data);
      }
    } catch (e: any) {
      console.error('Failed to load Claude profiles:', e);
    } finally {
      setClaudeProfileLoading(false);
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

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (historyRef.current && !historyRef.current.contains(event.target as Node)) {
        setShowHistory(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);
  
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
        setCliProbe(prev => ({ ...prev, ...nextProbe }));
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

  // 切换工具时：清空展开状态 + 自动加载并展开 active provider 的数据
  useEffect(() => {
    const toolChanged = prevActiveToolRef.current !== activeTool;
    prevActiveToolRef.current = activeTool;
    if (!toolChanged) return;

    setOpenIds(new Set());
    if (activeTool === 'claude') {
      const activeProfileId = state.active_claude;
      if (activeProfileId) {
        const profile = claudeProfiles.find(p => p.id === activeProfileId);
        if (profile) {
          setOpenIds(new Set([activeProfileId]));
          setEditingProvider({
            id: profile.id,
            name: profile.name,
            code: profile.code,
            api_key: profile.raw_api_key || '',
            base_url: profile.raw_base_url || '',
            model: profile.model || undefined,
            dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
            enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
            enable_mcp: profile.tool_config?.enable_mcp || false,
            allowed_tools: profile.tool_config?.allowed_tools || [],
            blocked_tools: profile.tool_config?.blocked_tools || [],
            max_session_turns: profile.tool_config?.max_session_turns,
            claude_reasoning_model: profile.tool_config?.claude_reasoning_model,
            claude_haiku_model: profile.tool_config?.claude_haiku_model,
            claude_sonnet_model: profile.tool_config?.claude_sonnet_model,
            claude_opus_model: profile.tool_config?.claude_opus_model,
            claude_default_model: profile.tool_config?.claude_default_model,
            claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
          });
          setOriginalProvider({
            id: profile.id,
            name: profile.name,
            code: profile.code,
            api_key: profile.raw_api_key || '',
            base_url: profile.raw_base_url || '',
            model: profile.model || undefined,
            dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
            enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
            enable_mcp: profile.tool_config?.enable_mcp || false,
            allowed_tools: profile.tool_config?.allowed_tools || [],
            blocked_tools: profile.tool_config?.blocked_tools || [],
            max_session_turns: profile.tool_config?.max_session_turns,
            claude_reasoning_model: profile.tool_config?.claude_reasoning_model,
            claude_haiku_model: profile.tool_config?.claude_haiku_model,
            claude_sonnet_model: profile.tool_config?.claude_sonnet_model,
            claude_opus_model: profile.tool_config?.claude_opus_model,
            claude_default_model: profile.tool_config?.claude_default_model,
            claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
          });
        }
      }
    } else {
      const activeProviderId = state[`active_${activeTool}` as keyof AiProvidersState] as string | null;
      if (activeProviderId) {
        const provider = state.providers.find(p => p.id === activeProviderId && p.tool === activeTool);
        if (provider) {
          setOpenIds(new Set([activeProviderId]));
          setEditingProvider(provider);
          setOriginalProvider(provider);
          const json = getOpenCodeJson(provider);
          setRawJson(json);
          setOriginalJson(json);
        }
      }
    }
  }, [activeTool, state.providers, claudeProfiles]);

  // 当 state.providers 更新后（loadProviders 完成），如果当前工具还没加载数据，自动填充
  useEffect(() => {
    if (activeTool === 'claude') return;
    // 如果 editingProvider 已经有 id（已加载过），跳过
    if (editingProvider.id) return;
    const activeProviderId = state[`active_${activeTool}` as keyof AiProvidersState] as string | null;
    if (!activeProviderId) return;
    const provider = state.providers.find(p => p.id === activeProviderId && p.tool === activeTool);
    if (!provider) return;
    setOpenIds(new Set([activeProviderId]));
    setEditingProvider(provider);
    setOriginalProvider(provider);
    const json = getOpenCodeJson(provider);
    setRawJson(json);
    setOriginalJson(json);
  }, [state.providers]);

  useEffect(() => {
    const p = currentProviderId
      ? state.providers.find(item => item.id === currentProviderId && item.tool === activeTool)
      : null;
    if (p) {
      setEditingProvider(p);
      setOriginalProvider(p);
      const json = getOpenCodeJson(p);
      setRawJson(json);
      setOriginalJson(json);
    } else if (!currentProviderId) {
      // 只有在没有选中 provider 时才清空
      // 如果 editingProvider 已经有 id（工具切换 effect 已填充），不要覆盖
      if (!editingProvider.id) {
        const empty = { name: '', api_key: '', base_url: '', model: '' };
        setEditingProvider(empty);
        setOriginalProvider(empty);
        setRawJson('{}');
        setOriginalJson('{}');
      }
    }
    setShowHistory(false);
  }, [currentProviderId, state.providers]);

  // 同步表单字段到 JSON 编辑器 (仅在 OpenCode 工具下)
  useEffect(() => {
    if (activeTool === 'opencode' && editingProvider) {
      try {
        const currentJson = JSON.parse(rawJson || '{}');
        let changed = false;
        
        if (editingProvider.name !== currentJson.name) {
          currentJson.name = editingProvider.name;
          changed = true;
        }
        
        if (changed) {
          setRawJson(JSON.stringify(currentJson, null, 2));
        }
      } catch (e) {}
    }
  }, [editingProvider.name, activeTool]);

  const handleFormatJson = () => {
    try {
      const parsed = JSON.parse(rawJson);
      setRawJson(JSON.stringify(parsed, null, 2));
    } catch (e) {
      setMessage({ type: 'error', text: t('invalidJson', 'Invalid JSON syntax') });
    }
  };

  const activateProvider = async (tool: string, providerId: string) => {
    try {
      setLoading(true);
      setMessage({ type: '', text: '' });

      await invoke('providers_set_active', { tool, providerId });
      await loadProviders(true);
      await invoke('projection_apply', { tool, providerId });

      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return true;
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      return false;
    } finally {
      setLoading(false);
    }
  };

  const handleSavePreset = async (
    options: { showSavedMessage?: boolean } = {}
  ): Promise<SavePresetResult> => {
    const { showSavedMessage = true } = options;
    if (!editingProvider.name) {
      setMessage({ type: 'error', text: t('providePresetName', 'Please provide a preset name') });
      return { ok: false };
    }

    const newId = editingProvider.id || `custom-${Date.now()}`;
    const wasActiveBeforeSave =
      activeTool !== 'opencode' &&
      ((state as any)[`active_${activeTool}`] as string | null) === newId;
    
    let baseProvider: any = { ...editingProvider };
    let currentHistory = baseProvider.history || [];
    
    // If opencode, sync from JSON box
    if (activeTool === 'opencode') {
      try {
        const parsed = JSON.parse(rawJson);
        
        // Add PREVIOUS content to history (not the new one)
        if (rawJson !== originalJson) {
          currentHistory = [
            { timestamp: Date.now(), content: originalJson },
            ...currentHistory
          ].slice(0, 50); // Keep last 50 entries
        }

        baseProvider = {
          ...parsed,
          id: baseProvider.id,
          tool: baseProvider.tool,
          is_enabled: true,
          provider_key: baseProvider.provider_key,
          history: currentHistory,
          // Preserve global config fields from editingProvider (they are not in JSON)
          opencode_default_model: editingProvider.opencode_default_model,
          opencode_default_agent: editingProvider.opencode_default_agent,
          opencode_sessions_dir: editingProvider.opencode_sessions_dir,
        };
        
        // 同步核心字段以便表单回显
        if (parsed.name) baseProvider.name = parsed.name;
        if (parsed.options) {
          if (parsed.options.apiKey) baseProvider.api_key = parsed.options.apiKey;
          if (parsed.options.baseURL) baseProvider.base_url = parsed.options.baseURL;
        }
        if (parsed.models) {
          const firstModel = Object.keys(parsed.models)[0];
          if (firstModel) baseProvider.model = firstModel;
        }
      } catch (e) {
        setMessage({ type: 'error', text: t('invalidJson', 'Invalid JSON syntax') });
        return { ok: false };
      }
    }

    const finalProvider: AiProvider = {
      ...baseProvider,
      id: newId,
      name: baseProvider.name || 'Unnamed',
      provider_key: baseProvider.provider_key,
      tool: activeTool,
      api_key: baseProvider.api_key || '',
      is_enabled: activeTool === 'opencode' ? true : (baseProvider.is_enabled ?? true),
      env_managed: activeTool !== 'opencode' ? (baseProvider.env_managed ?? true) : undefined,
      history: currentHistory,
    };

    try {
      setSaving(true);
      await invoke('providers_upsert', { provider: finalProvider });
      await loadProviders(true);
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.delete(newId);
        return next;
      });
      setCurrentProviderId(newId);
      
      // Update counts in sidebar
      emit('refresh-counts');

      // Update originals to disable save button after success
      setOriginalProvider(finalProvider);
      setIsRollbackMode(false);
      
      if (activeTool === 'opencode') {
        setOriginalJson(rawJson);
      }

      // Only apply projection for the currently active provider or opencode
      // (opencode writes all providers to its config file)
      if (wasActiveBeforeSave || activeTool === 'opencode') {
        await invoke('projection_apply', { tool: finalProvider.tool, providerId: finalProvider.id });
      }

      if (showSavedMessage) {
        setMessage({ type: 'success', text: t('presetSaved', 'Preset saved successfully') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
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
    } finally {
      setSaving(false);
    }
  };

  const handleSavePresetWithActivationPrompt = async () => {
    const result = await handleSavePreset();
    if (!result.ok || activeTool === 'opencode' || !result.providerId || result.wasActiveBeforeSave) return;

    const canActivate =
      !!result.provider?.api_key &&
      !(isManagedTool(activeTool) && result.provider?.env_managed === false);
    if (!canActivate) return;

    const confirmed = await confirmDialog(
      t('confirmActivateAfterSave', 'Preset saved. Activate this environment now?'),
      {
        okLabel: t('applyToCli'),
        cancelLabel: t('cancel')
      }
    );
    if (!confirmed) return;

    await activateProvider(activeTool, result.providerId);
  };

  const handleApply = async () => {
    const saveResult = await handleSavePreset({ showSavedMessage: false });
    if (!saveResult.ok || activeTool === 'opencode' || !saveResult.providerId) return;
    await activateProvider(activeTool, saveResult.providerId);
  };

  const handleRollback = (entry: HistoryEntry) => {
    try {
      JSON.parse(entry.content); // Verify syntax
      setRawJson(entry.content);
      setIsRollbackMode(true);
      // We don't save immediately, let the user review then click save
      setShowHistory(false);
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
      name: `${t('newPreset', 'New Preset')} (${toolName})`,
      tool: toolName,
      api_key: '',
      base_url: '',
      model: '',
      code: toolName === 'claude' ? 'new-profile' : undefined,
      env_managed: toolName !== 'opencode' ? true : undefined,
      provider_key: toolName === 'opencode' ? `provider_${Date.now()}` : undefined,
      is_enabled: true,
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
    // Preserve existing openIds and add the new one — never replace
    setOpenIds(prev => {
      const next = new Set(prev);
      next.add(newId);
      return next;
    });
  };

  const handleDelete = async (providerId?: string) => {
    const targetId = providerId || currentProviderId;
    if (!targetId) return;
    const isUnsavedNewProvider = unsavedNewProviderIds.has(targetId);
    const providerToDelete = state.providers.find(p => p.id === targetId);
    if (!providerToDelete) return;
    const activeProviderIdForTool = (state as any)[`active_${activeTool}`] as string | null;
    const isDefaultImportedForTool =
      isManagedTool(activeTool) && providerToDelete.id === `default-${activeTool}`;
    const isDeletingActiveDefaultImported =
      isDefaultImportedForTool && activeProviderIdForTool === providerToDelete.id;
    const isDeletingInactiveDefaultImported =
      isDefaultImportedForTool && activeProviderIdForTool !== providerToDelete.id;
    if (isDeletingActiveDefaultImported) return;
    if (
      !isUnsavedNewProvider &&
      !isDeletingInactiveDefaultImported &&
      state.providers.filter(p => p.tool === activeTool).length <= 1
    ) {
      return;
    }

    const confirmMsg = activeTool === 'opencode'
      ? t('confirmDelete', { name: providerToDelete.name })
      : t('confirmDelete', { name: providerToDelete.name });

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
      await invoke('providers_delete', { providerId: targetId });
      await loadProviders(true);
      if (currentProviderId === targetId) {
        setCurrentProviderId(null);
      }
      emit('refresh-counts');
      setMessage({ type: 'success', text: t('deleteSuccess', 'Preset deleted successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    }
  };

  const handleToggleEnvManaged = async (enabled: boolean) => {
    if (!isManagedTool(activeTool)) return;
    const activeProviderId = (state as any)[`active_${activeTool}`] as string | null;
    const provider = activeProviderId
      ? state.providers.find(p => p.id === activeProviderId && p.tool === activeTool) || null
      : null;
    const providerId = provider?.id || null;
    if (!providerId) {
      setMessage({ type: 'error', text: t('noManagedProvider') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }
    const confirmText = enabled ? t('confirmEnableManaged') : t('confirmDisableManaged');
    const confirmed = await confirmDialog(confirmText, {
      okLabel: t('ok'),
      cancelLabel: t('cancel')
    });
    if (!confirmed) return;
    try {
      setLoading(true);
      await invoke('providers_set_env_managed', {
        tool: activeTool,
        providerId,
        enabled
      });

      // Optimistically update local state so card/button status changes immediately.
      setState(prev => ({
        ...prev,
        providers: prev.providers.map(p =>
          p.id === providerId ? { ...p, env_managed: enabled } : p
        )
      }));
      if (currentProviderId === providerId) {
        setEditingProvider(prev => ({ ...prev, env_managed: enabled }));
        setOriginalProvider(prev => ({ ...prev, env_managed: enabled }));
      }

      let projectionError: string | null = null;
      if (enabled) {
        // Re-write active provider config to target CLI files when managed mode is enabled.
        try {
          await invoke('projection_apply', { tool: activeTool, providerId });
        } catch (e: any) {
          projectionError = e.toString();
        }
      }
      await loadProviders(true);

      if (projectionError) {
        setMessage({ type: 'error', text: projectionError });
      } else {
        setMessage({
          type: 'success',
          text: enabled ? t('envManagedEnabled') : t('envManagedDisabled')
        });
      }
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const selectedProvider = currentProviderId
    ? state.providers.find(p => p.id === currentProviderId && p.tool === activeTool) || null
    : null;
  const showingProviderDetails = !!selectedProvider;
  const isDefaultPreset = showingProviderDetails && currentProviderId?.startsWith('default-');
  const isCurrentProviderActive =
    activeTool !== 'opencode' &&
    !!selectedProvider &&
    state[`active_${activeTool}` as keyof AiProvidersState] === selectedProvider.id;
  const activeManagedProviderId = isManagedTool(activeTool)
    ? ((state as any)[`active_${activeTool}`] as string | null)
    : null;
  const managedProvider = isManagedTool(activeTool) && activeManagedProviderId
    ? state.providers.find(p => p.id === activeManagedProviderId && p.tool === activeTool) || null
    : null;
  const envManagedState = getManagedStateForTool(activeTool as CliTool);
  const envManagedEnabled = envManagedState === 'enabled';
  const isSelectedDefaultImportedProvider =
    !!selectedProvider &&
    isManagedTool(activeTool) &&
    selectedProvider.id === `default-${activeTool}`;
  const defaultImportMissingFieldLabels = isSelectedDefaultImportedProvider
    ? [
        ...(editingProvider.api_key?.trim() ? [] : [t('apiKey', 'API Key')]),
        ...(editingProvider.base_url?.trim() ? [] : [t('baseUrl', 'Base URL')])
      ]
    : [];
  const showDefaultImportInactiveNotice =
    isSelectedDefaultImportedProvider &&
    !isCurrentProviderActive &&
    defaultImportMissingFieldLabels.length > 0;
  const canDeleteSelectedProvider =
    !!selectedProvider &&
    (!isDefaultPreset || (isSelectedDefaultImportedProvider && !isCurrentProviderActive));
  const defaultImportInactiveNoticeText = showDefaultImportInactiveNotice
    ? t('autoImportedButInactiveMissingFields', {
        fields: defaultImportMissingFieldLabels.join(' + ')
      })
    : '';

  const getToolDescription = (tool: string) => {
    switch (tool.toLowerCase()) {
      case 'claude': return t('configureClaude');
      case 'codex': return t('configureCodex');
      case 'gemini': return t('configureGemini');
      case 'opencode': return t('configureOpenCode');
      default: return t('configureAiEndpoint');
    }
  };

  const handleCopyInstallCommand = async (command: string, key: string) => {
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(command);
      } else {
        const input = document.createElement('textarea');
        input.value = command;
        input.setAttribute('readonly', 'true');
        input.style.position = 'fixed';
        input.style.left = '-9999px';
        document.body.appendChild(input);
        input.select();
        const copied = document.execCommand('copy');
        document.body.removeChild(input);
        if (!copied) throw new Error('copy_failed');
      }
      setCopiedInstallCommandKey(key);
      window.setTimeout(() => {
        setCopiedInstallCommandKey(prev => (prev === key ? null : prev));
      }, 1500);
    } catch (e: any) {
      setMessage({ type: 'error', text: t('copyCommandFailed', 'Failed to copy command. Please copy manually.') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
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

  const handleSkipClaudeOnboardingLogin = async () => {
    if (!isTauri) return;
    try {
      setSkippingClaudeOnboarding(true);
      await invoke('skip_claude_onboarding_login');
      const successText = t(
        'skipClaudeOnboardingLoginSuccess',
        '已经跳过引导页的登录，请重启claude终端'
      );
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(successText, {
        title: t('aiEnvironments', 'AI Environments'),
        kind: 'info'
      });
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: `${t('skipClaudeOnboardingLoginFailed', 'Failed to skip onboarding login')}: ${e.toString()}`
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setSkippingClaudeOnboarding(false);
    }
  };

  const handleClaudeSetDefault = async (profileId: string) => {
    if (!isTauri) return;
    try {
      await invoke('claude_profile_set_default', { profileId });
      await loadClaudeProfiles();
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    }
  };

  const handleClaudeCopyCommand = async (configDir: string) => {
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
      setCopiedClaudeProfileId(configDir);
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

  const handleClaudeMaterialize = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setSaving(true);
      // Save editing provider changes first (without triggering projection_apply)
      let materializeId = profileId;
      if (hasChanges && editingProvider.name) {
        const newId = editingProvider.id || `custom-${Date.now()}`;
        const finalProvider: AiProvider = {
          ...editingProvider,
          id: newId,
          name: editingProvider.name || 'Unnamed',
          tool: 'claude',
          api_key: editingProvider.api_key || '',
          is_enabled: editingProvider.is_enabled ?? true,
          env_managed: editingProvider.env_managed ?? true,
        };
        await invoke('providers_upsert', { provider: finalProvider });
        await loadProviders(true);
        setUnsavedNewProviderIds(prev => {
          const next = new Set(prev);
          next.delete(newId);
          return next;
        });
        setCurrentProviderId(newId);
        setOriginalProvider(finalProvider);
        setIsRollbackMode(false);
        emit('refresh-counts');
        materializeId = newId;
      }
      await invoke('claude_profile_materialize', { providerId: materializeId });
      setMessage({ type: 'success', text: t('presetSaved', 'Preset saved successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setSaving(false);
    }
  };

  const handleClaudeApplyGlobal = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setApplyingGlobal(true);
      await invoke('projection_apply', { tool: 'claude', providerId: profileId });
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setApplyingGlobal(false);
    }
  };

  const handleClaudeLaunch = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setLoading(true);
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
      setLoading(false);
    }
  };

  const handleActivateSyncedProvider = async (deviceId: string, provider: SyncedDeviceProvider) => {
    const apiKey = String(provider.api_key || '').trim();
    if (!apiKey) {
      setMessage({
        type: 'error',
        text: t(
          'syncedProviderMissingApiKey',
          '该环境缺少可解密的 API Key，无法直接激活。'
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
        text: t('syncedProviderActivated', '已导入并激活该设备环境。'),
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
        defaultPath: `onespace-ai-environments-${stamp}.json`,
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
        defaultValue: 'Exported {{count}} environment(s) to {{path}}',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(successText, {
        title: t('aiEnvironments', 'AI Environments'),
        kind: 'info',
      });
    } catch (e: any) {
      const errorText = t('providersExportFailed', {
        error: String(e),
        defaultValue: 'Failed to export environments: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(errorText, {
        title: t('aiEnvironments', 'AI Environments'),
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
        const emptyText = t('providersImportEmpty', 'No environments found in the selected file.');
        setMessage({ type: 'error', text: emptyText });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        await message(emptyText, {
          title: t('aiEnvironments', 'AI Environments'),
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
        title: t('aiEnvironments', 'AI Environments'),
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
          'Imported {{imported}} environment(s): {{overwritten}} overwritten, {{created}} created, {{activeRestored}} active binding(s) restored.',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(successText, {
        title: t('aiEnvironments', 'AI Environments'),
        kind: 'info',
      });
    } catch (e: any) {
      const errorText = t('providersImportApplyFailed', {
        error: String(e),
        defaultValue: 'Failed to import environments: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      await message(errorText, {
        title: t('aiEnvironments', 'AI Environments'),
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

  const claudeFilterCounts = useMemo(() => {
    const all = claudeProfiles.length;
    const apiKey = claudeProfiles.filter(p => p.auth_type !== 'oauth').length;
    const oauth = claudeProfiles.filter(p => p.auth_type === 'oauth').length;
    const defaultMode = claudeProfiles.filter(p => p.tool_config?.permission_mode === 'default' || !p.tool_config?.permission_mode).length;
    const acceptEdits = claudeProfiles.filter(p => p.tool_config?.permission_mode === 'accept_edits' || p.tool_config?.dangerously_skip_permissions).length;
    return { all, apiKey, oauth, defaultMode, acceptEdits };
  }, [claudeProfiles]);

  const importConflictItems = importPreview?.items.filter(item => item.conflict) || [];
  const importNewItems = importPreview?.items.filter(item => !item.conflict) || [];

  const handleToggleOpen = (id: string) => {
    setOpenIds(prev => {
      const willOpen = !prev.has(id);
      const next = new Set(prev);
      if (willOpen) {
        next.add(id);
        // 展开 Claude Profile 时，将其配置加载到 editingProvider 中
        if (activeTool === 'claude') {
          const profile = claudeProfiles.find(p => p.id === id);
          if (profile) {
            setEditingProvider({
              id: profile.id,
              name: profile.name,
              code: profile.code,
              api_key: profile.raw_api_key || '',
              base_url: profile.raw_base_url || '',
              model: profile.model || undefined,
              dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
              enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
              enable_mcp: profile.tool_config?.enable_mcp || false,
              allowed_tools: profile.tool_config?.allowed_tools || [],
              blocked_tools: profile.tool_config?.blocked_tools || [],
              max_session_turns: profile.tool_config?.max_session_turns,
              claude_reasoning_model: profile.tool_config?.claude_reasoning_model,
              claude_haiku_model: profile.tool_config?.claude_haiku_model,
              claude_sonnet_model: profile.tool_config?.claude_sonnet_model,
              claude_opus_model: profile.tool_config?.claude_opus_model,
              claude_default_model: profile.tool_config?.claude_default_model,
              claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
            });
            setOriginalProvider({
              id: profile.id,
              name: profile.name,
              code: profile.code,
              api_key: profile.raw_api_key || '',
              base_url: profile.raw_base_url || '',
              model: profile.model || undefined,
              dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
              enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
              enable_mcp: profile.tool_config?.enable_mcp || false,
              allowed_tools: profile.tool_config?.allowed_tools || [],
              blocked_tools: profile.tool_config?.blocked_tools || [],
              max_session_turns: profile.tool_config?.max_session_turns,
              claude_reasoning_model: profile.tool_config?.claude_reasoning_model,
              claude_haiku_model: profile.tool_config?.claude_haiku_model,
              claude_sonnet_model: profile.tool_config?.claude_sonnet_model,
              claude_opus_model: profile.tool_config?.claude_opus_model,
              claude_default_model: profile.tool_config?.claude_default_model,
              claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
            });
          }
        } else {
          // 展开非 Claude 工具（Codex/Gemini/OpenCode）时，加载 provider 数据
          const provider = state.providers.find(p => p.id === id && p.tool === activeTool);
          if (provider) {
            setEditingProvider(provider);
            setOriginalProvider(provider);
            const json = getOpenCodeJson(provider);
            setRawJson(json);
            setOriginalJson(json);
          }
        }
      } else {
        next.delete(id);
      }
      return next;
    });
  };

  const handleFilterChange = (filter: string) => {
    setActiveFilters(prev => {
      const next = new Set(prev);
      if (next.has(filter)) {
        next.delete(filter);
      } else {
        next.add(filter);
      }
      return next;
    });
  };

  const matchesFilter = (provider: AiProvider | ClaudeProfileSummary, tool: string) => {
    if (activeFilters.size === 0) return true;
    if (activeFilters.has('all')) return true;
    // Claude Profile specific filters
    if (tool === 'claude') {
      const profile = provider as ClaudeProfileSummary;
      if (activeFilters.has('api_key') && profile.auth_type !== 'oauth') return true;
      if (activeFilters.has('oauth') && profile.auth_type === 'oauth') return true;
      const permissionMode = profile.tool_config?.dangerously_skip_permissions
        ? 'accept_edits'
        : profile.tool_config?.permission_mode || 'default';
      if (activeFilters.has('default') && permissionMode === 'default') return true;
      if (activeFilters.has('accept_edits') && permissionMode === 'accept_edits') return true;
      // Fall back to active/inactive for Claude (all Claude profiles are considered 'active')
      if (activeFilters.has('active') || activeFilters.has('inactive')) return true;
      return false;
    }
    // Provider filters
    const isActive = tool === 'opencode'
      ? true
      : (state as any)[`active_${tool}`] === (provider as AiProvider).id;
    if (activeFilters.has('active') && isActive) return true;
    if (activeFilters.has('inactive') && !isActive) return true;
    return false;
  };

  const matchesSearch = (query: string, name: string, extra?: string) => {
    if (!query) return true;
    const q = query.toLowerCase();
    return name.toLowerCase().includes(q) || (extra && extra.toLowerCase().includes(q));
  };

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('aiEnvironments')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('aiEnvironmentsDesc')}</p>
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
            onAdd={() => { handleAddCustom(activeTool); }}
            activeFilters={activeFilters}
            onFilterChange={handleFilterChange}
            loading={loading}
            previewingImport={previewingImport}
            applyingImport={applyingImport}
            exportingProviders={exportingProviders}
            t={t}
          />
        </div>

        {/* Scrollable accordion area */}
        <div className="flex-1 overflow-y-auto">
          {activeTool === 'claude' ? (
            <div>
              {claudeProfileLoading ? (
                <div className="flex items-center justify-center py-8 text-sm text-muted-foreground">
                  <Loader2 className="w-4 h-4 animate-spin mr-2" />
                  {t('loading', 'Loading...')}
                </div>
              ) : claudeProfiles.length === 0 ? (
                <div className="py-6 text-center text-sm text-muted-foreground">
                  {t('noProfiles', 'No profiles configured')}
                </div>
              ) : (
                claudeProfiles
                  .filter(profile => matchesSearch(searchQuery, profile.name, profile.code || ''))
                  .filter(profile => matchesFilter(profile, 'claude'))
                  .map(profile => {
                    const isOpen = openIds.has(profile.id);
                    const permissionMode = profile.tool_config?.dangerously_skip_permissions
                      ? 'acceptEdits'
                      : profile.tool_config?.permission_mode || 'default';
                    const displayModel = profile.model || profile.tool_config?.claude_default_model || '';
                    const authBadge = profile.auth_type === 'oauth' ? 'OAuth' : 'API Key';
                    const tildeDir = profile.tilde_config_dir || profile.config_dir;

                    const isMissingKey = profile.auth_type === 'oauth' && !profile.raw_api_key;

                    return (
                      <AccordionItem
                        key={profile.id}
                        id={profile.id}
                        isOpen={isOpen}
                        onToggle={handleToggleOpen}
                        compact
                        avatar={
                          <div className={`acc-avatar ${isMissingKey ? 'warn' : ''}`}>
                            {(profile.name || '?')[0].toUpperCase()}
                            {(profile.is_default || state.active_claude === profile.id) && (
                              <span className="running-dot" />
                            )}
                          </div>
                        }
                        nameRow={
                          <div className="flex items-center gap-2">
                            <span className="acc-name">{profile.name}</span>
                            {profile.is_default ? (
                              <span className="badge-pill bg-green-600/10 text-green-600">
                                <svg width="8" height="8" viewBox="0 0 24 24" fill="currentColor"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
                                default
                              </span>
                            ) : (
                              <Star
                                className="w-3 h-3 text-muted-foreground hover:text-green-600 cursor-pointer shrink-0"
                                onClick={(e) => { e.stopPropagation(); void handleClaudeSetDefault(profile.id); }}
                              />
                            )}
                            {isMissingKey && (
                              <span className="badge-pill bg-red-500/10 text-red-600">
                                <svg width="8" height="8" viewBox="0 0 24 24" fill="currentColor"><path d="M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z"/></svg>
                                缺少 API Key
                              </span>
                            )}
                          </div>
                        }
                        badges={
                          <>
                            <span className={`badge-pill ${
                              profile.auth_type === 'oauth'
                                ? 'bg-green-500/10 text-green-700'
                                : 'bg-green-500/10 text-green-700'
                            }`}>
                              <span className="badge-dot" />
                              {authBadge}
                            </span>
                            {displayModel && (
                              <span className="badge-pill border">{displayModel}</span>
                            )}
                          </>
                        }
                        meta={
                          <>
                            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
                            <span className="group inline-flex items-center gap-1 cursor-pointer"
                              onClick={(e) => {
                                e.stopPropagation();
                                void navigator.clipboard.writeText(tildeDir).then(() => {
                                  setCopiedProfileDir(profile.id);
                                  window.setTimeout(() => setCopiedProfileDir(null), 2000);
                                });
                              }}
                            >
                              <span className="group-hover:opacity-0 group-has-[:focus-visible]:opacity-0 transition-opacity">{tildeDir}</span>
                              {copiedProfileDir === profile.id ? (
                                <Check className="w-3 h-3 text-green-600 shrink-0" />
                              ) : (
                                <svg className="w-3 h-3 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                              )}
                            </span>
                          </>
                        }
                        actions={
                          <div className="acc-actions">
                            <button
                              type="button"
                              className="acc-btn acc-btn-launch"
                              onClick={(e) => { e.stopPropagation(); void handleClaudeLaunch(profile.id); }}
                              disabled={loading}
                            >
                              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                              {t('claudeProfileLaunch', '启动')}
                            </button>
                            <button
                              type="button"
                              className="acc-btn"
                              title="复制命令"
                              onClick={(e) => { e.stopPropagation(); void handleClaudeCopyCommand(profile.config_dir); }}
                            >
                              {copiedClaudeProfileId === profile.config_dir
                                ? <Check className="w-3.5 h-3.5 text-green-600" />
                                : <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                              }
                            </button>
                            <button
                              type="button"
                              className="acc-btn"
                              title="打开目录"
                              onClick={(e) => { e.stopPropagation(); void handleClaudeOpenDir(profile.id); }}
                            >
                              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
                            </button>
                          </div>
                        }
                        panel={
                          <div className="space-y-5">
                            {/* Imported-but-Inactive notice */}
                            {showDefaultImportInactiveNotice && (
                              <div className="rounded-lg border-2 border-amber-500/70 bg-amber-100/80 px-4 py-3 shadow-sm">
                                <div className="flex items-start gap-2.5">
                                  <AlertTriangle className="w-5 h-5 mt-0.5 text-amber-800 shrink-0" />
                                  <div>
                                    <p className="text-sm font-extrabold tracking-wide uppercase text-amber-900">
                                      {t('importedButInactiveTitle')}
                                    </p>
                                    <p className="text-sm font-medium text-amber-900/90 mt-1">
                                      {defaultImportInactiveNoticeText}
                                    </p>
                                  </div>
                                </div>
                              </div>
                            )}

                            {/* 基本信息 */}
                            <div className="space-y-4 max-w-4xl">
                              <div className="flex items-center gap-2 border-b pb-2">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4 text-primary"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
                                <h3 className="font-semibold text-sm">{t('basicInfo', '基本信息')}</h3>
                              </div>
                              <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('profileName', '名称')}</label>
                                  <input value={editingProvider.name || ''} onChange={e => setEditingProvider({...editingProvider, name: e.target.value})}
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('profileCode', 'Profile Code')}</label>
                                  <input value={editingProvider.code || ''} onChange={e => { const val = e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, ''); setEditingProvider({...editingProvider, code: val}); }}
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                              </div>
                              <div className="space-y-2">
                                <label className="text-sm font-medium text-foreground">{t('configDirectory', '配置目录')}</label>
                                <input value={profile.config_dir} disabled
                                  className="w-full bg-muted/40 border rounded-lg px-3 py-2.5 text-sm text-muted-foreground cursor-not-allowed focus:outline-none transition-all"
                                />
                              </div>
                            </div>

                            {/* 认证 & 端点 */}
                            <div className="space-y-4 max-w-4xl">
                              <div className="flex items-center gap-2 border-b pb-2">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4 text-primary"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                                <h3 className="font-semibold text-sm">{t('authAndEndpoint', '认证 & 端点')}</h3>
                              </div>
                              <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('authMethod', '认证方式')}</label>
                                  <select value={profile.auth_type} disabled
                                    className="w-full bg-muted/40 border rounded-md px-3 py-2 text-sm text-muted-foreground cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-primary/50"
                                  >
                                    <option value="api_key">API Key</option>
                                    <option value="oauth">OAuth</option>
                                  </select>
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('baseUrl', 'Base URL')}</label>
                                  <input placeholder="留空使用默认端点" value={editingProvider.base_url || ''} onChange={e => setEditingProvider({...editingProvider, base_url: e.target.value})}
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                              </div>
                              <div className="space-y-2">
                                <label className="text-sm font-medium text-foreground">{t('apiKey', 'API Key')}</label>
                                <input type="text" value={editingProvider.api_key || ''} onChange={e => setEditingProvider({...editingProvider, api_key: e.target.value})}
                                  className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                />
                              </div>
                            </div>

                            {/* 模型路由 */}
                            <div className="space-y-4 max-w-4xl">
                              <div className="flex items-center gap-2 border-b pb-2">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4 text-primary"><circle cx="12" cy="12" r="3"/><path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83"/></svg>
                                <h3 className="font-semibold text-sm">{t('modelRouting', '模型路由')}</h3>
                              </div>
                              <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('sonnetModel', 'Sonnet 模型')}</label>
                                  <input value={editingProvider.claude_sonnet_model || ''} onChange={e => setEditingProvider({...editingProvider, claude_sonnet_model: e.target.value})} placeholder="默认日常任务"
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('defaultModel', '默认模型')}</label>
                                  <input value={editingProvider.claude_default_model || ''} onChange={e => setEditingProvider({...editingProvider, claude_default_model: e.target.value})} placeholder="claude-sonnet-4-20250514"
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('reasoningModel', 'Reasoning 模型')}</label>
                                  <input value={editingProvider.claude_reasoning_model || ''} onChange={e => setEditingProvider({...editingProvider, claude_reasoning_model: e.target.value})} placeholder="用于深度推理任务"
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('fastModel', 'Haiku 模型')}</label>
                                  <input value={editingProvider.claude_haiku_model || ''} onChange={e => setEditingProvider({...editingProvider, claude_haiku_model: e.target.value})} placeholder="轻量快速任务"
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('powerfulModel', 'Opus 模型')}</label>
                                  <input value={editingProvider.claude_opus_model || ''} onChange={e => setEditingProvider({...editingProvider, claude_opus_model: e.target.value})} placeholder="最复杂任务"
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('reasoningEffort', '推理努力强度')}</label>
                                  <select value={editingProvider.claude_reasoning_effort || ''} onChange={e => setEditingProvider({...editingProvider, claude_reasoning_effort: e.target.value || undefined})}
                                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                                  >
                                    <option value="">{t('reasoningEffortDefault', '默认')}</option>
                                    <option value="minimal">极小（minimal）</option>
                                    <option value="low">低（low）</option>
                                    <option value="medium">中（medium）</option>
                                    <option value="high">高（high）</option>
                                    <option value="xhigh">超高（xhigh）</option>
                                  </select>
                                </div>
                              </div>
                              <p className="text-xs text-muted-foreground">模型路由根据任务复杂度自动选择最合适的模型，降低不必要的高成本调用。</p>
                            </div>

                            {/* 权限 & 高级配置 */}
                            <div className="space-y-4 max-w-4xl">
                              <div className="flex items-center gap-2 border-b pb-2">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4 text-primary"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                                <h3 className="font-semibold text-sm">{t('permissions', '权限 & 高级配置')}</h3>
                              </div>
                              <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('permissionMode', '权限模式')}</label>
                                  <select value={permissionMode} onChange={e => setEditingProvider({...editingProvider, dangerously_skip_permissions: e.target.value === 'acceptEdits'})}
                                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                                  >
                                    <option value="default">default</option>
                                    <option value="acceptEdits">acceptEdits（跳过确认）</option>
                                  </select>
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('maxSessionTurns', '最大会话轮次')}</label>
                                  <input type="number" value={editingProvider.max_session_turns || ''} onChange={e => setEditingProvider({...editingProvider, max_session_turns: e.target.value ? parseInt(e.target.value) : undefined})} placeholder="0 = 不限制"
                                    className="w-full bg-background border rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                                  />
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('enableMcp', 'MCP 服务')}</label>
                                  <select value={editingProvider.enable_mcp ? '1' : '0'} onChange={e => setEditingProvider({...editingProvider, enable_mcp: e.target.value === '1'})}
                                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                                  >
                                    <option value="1">启用</option>
                                    <option value="0">禁用</option>
                                  </select>
                                </div>
                                <div className="space-y-2">
                                  <label className="text-sm font-medium text-foreground">{t('enableAllMemoryFeatures', '记忆功能')}</label>
                                  <select value={editingProvider.enable_all_memory_features ? '1' : '0'} onChange={e => setEditingProvider({...editingProvider, enable_all_memory_features: e.target.value === '1'})}
                                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                                  >
                                    <option value="1">启用</option>
                                    <option value="0">禁用</option>
                                  </select>
                                </div>
                              </div>
                            </div>

                            {/* 工作空间隔离 */}
                            <div className="space-y-4 max-w-4xl">
                              <div className="flex items-center gap-2 border-b pb-2">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4 text-primary"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/></svg>
                                <h3 className="font-semibold text-sm">{t('isolation', '工作空间隔离')}</h3>
                              </div>
                              <p className="text-sm text-muted-foreground">
                                通过注入 <code className="font-mono text-xs bg-muted/50 px-1.5 py-0.5 rounded">CLAUDE_CONFIG_DIR</code> 实现隔离启动。不会重写 <code className="font-mono text-xs bg-muted/50 px-1.5 py-0.5 rounded">~/.claude</code>，多个终端窗口可同时运行不同 Profile。
                              </p>
                            </div>

                            {/* 托管配置管理 */}
                            {isManagedTool('claude') && (
                              <div className="space-y-4 max-w-4xl">
                                <div className="flex items-center gap-2 border-b pb-2">
                                  <ShieldAlert className="w-4 h-4 text-primary" />
                                  <h3 className="font-semibold text-sm">{t('managedConfig', '托管配置管理')}</h3>
                                </div>
                                <div className="flex items-start gap-3 bg-primary/5 p-4 rounded-md border border-primary/20">
                                  <input
                                    type="checkbox"
                                    id={`claude-env-managed-${profile.id}`}
                                    checked={editingProvider.env_managed !== false}
                                    onChange={e => {
                                      const enabled = e.target.checked;
                                      setEditingProvider(prev => ({ ...prev, env_managed: enabled }));
                                    }}
                                    className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-primary"
                                  />
                                  <div className="space-y-1">
                                    <label htmlFor={`claude-env-managed-${profile.id}`} className="text-sm font-medium cursor-pointer">
                                      {t('envManagedToggle', '启用托管配置')}
                                    </label>
                                    <p className="text-xs text-muted-foreground">
                                      {editingProvider.env_managed !== false
                                        ? t('envManagedEnabledDesc', '应用时将自动更新 CLI 配置文件。关闭后，应用按钮将被禁用，CLI 配置不会被自动覆盖。')
                                        : t('envManagedDisabledDesc', '托管配置已禁用。CLI 配置文件不会被自动覆盖，需要手动管理。')}
                                    </p>
                                  </div>
                                </div>
                              </div>
                            )}

                            {/* Action buttons */}
                            <div className="flex items-center gap-3 justify-between pt-3 border-t">
                              <div className="flex items-center gap-3">
                                <button
                                  type="button"
                                  onClick={() => {
                                    setMessage({ type: 'warning', text: t('claudeProfileDeleteNotSupported', 'Profile deletion is not yet supported') });
                                    setTimeout(() => setMessage({ type: '', text: '' }), 3000);
                                  }}
                                  title={t('claudeProfileDeleteNotSupported', 'Profile deletion is not yet supported')}
                                  className="px-4 py-2 text-sm border bg-background hover:bg-destructive/10 text-destructive rounded-md flex items-center gap-2 transition-colors"
                                >
                                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                                  {t('deletePreset', '删除')}
                                </button>
                              </div>
                              <div className="flex items-center gap-3">
                                <button
                                  type="button"
                                  onClick={() => { void handleClaudeMaterialize(profile.id); }}
                                  disabled={saving}
                                  className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md flex items-center gap-2 transition-colors disabled:opacity-50"
                                >
                                  {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>}
                                  {t('save', '保存')}
                                </button>
                                <button
                                  type="button"
                                  onClick={() => { void handleClaudeApplyGlobal(profile.id); }}
                                  disabled={applyingGlobal}
                                  className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors shadow-sm"
                                >
                                  {applyingGlobal ? <Loader2 className="w-4 h-4 animate-spin" /> : <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="w-4 h-4"><polyline points="20 6 9 17 4 12"/></svg>}
                                  {t('claudeProfileApplyGlobal', '应用并生效')}
                                </button>
                              </div>
                            </div>
                          </div>
                        }
                      />
                    );
                  })
              )}
            </div>
          ) : (
            <div>
              {(() => {
                const toolProviders = state.providers.filter(p => p.tool === activeTool);
                const activeProviderId = state[`active_${activeTool}` as keyof AiProvidersState] as string | null;
                return toolProviders.length === 0 ? (
                  <div className="py-6 text-center text-sm text-muted-foreground">
                    {t('noProvidersGuide', { tool: activeTool, defaultValue: `No ${activeTool} providers configured` })}
                  </div>
                ) : (
                  toolProviders
                    .filter(p => matchesSearch(searchQuery, p.name, p.model || ''))
                    .filter(p => matchesFilter(p, activeTool))
                    .map(p => {
                      const isOpen = openIds.has(p.id);
                      const isActive = activeTool === 'opencode' || activeProviderId === p.id;
                      return (
                        <AccordionItem
                          key={p.id}
                          id={p.id}
                          isOpen={isOpen}
                          onToggle={handleToggleOpen}
                          avatar={
                            <div className={`w-2 h-2 rounded-full ${isActive ? 'bg-green-500' : 'bg-muted-foreground/40'}`} />
                          }
                          nameRow={
                            <span className="text-sm font-medium truncate">{p.name}</span>
                          }
                          badges={
                            p.model ? (
                              <span className="badge-pill bg-blue-500/10 text-blue-700">
                                {p.model}
                              </span>
                            ) : undefined
                          }
                          meta={
                            activeTool === 'opencode' ? undefined : (
                              <span className={`badge-pill ${isActive ? 'bg-green-500/10 text-green-700' : 'bg-muted/50 text-muted-foreground'}`}>
                                {isActive ? t('active', 'Active') : t('inactive', 'Inactive')}
                              </span>
                            )
                          }
                          actions={
                            activeTool === 'opencode' ? (
                              <div className="flex gap-1">
                                {canDeleteSelectedProvider && currentProviderId === p.id && (
                                  <button
                                    type="button"
                                    onClick={(e) => { e.stopPropagation(); void handleDelete(p.id); }}
                                    className="px-2 py-1 text-[10px] font-medium rounded-md border hover:bg-destructive/10 text-destructive transition-colors"
                                  >
                                    <Trash2 className="w-3 h-3" />
                                  </button>
                                )}
                              </div>
                            ) : undefined
                          }
                          panel={
                            <div className="space-y-5">
                              {/* Imported-but-Inactive notice */}
                              {showDefaultImportInactiveNotice && (
                                <div className="rounded-lg border-2 border-amber-500/70 bg-amber-100/80 px-4 py-3 shadow-sm">
                                  <div className="flex items-start gap-2.5">
                                    <AlertTriangle className="w-5 h-5 mt-0.5 text-amber-800 shrink-0" />
                                    <div>
                                      <p className="text-sm font-extrabold tracking-wide uppercase text-amber-900">
                                        {t('importedButInactiveTitle')}
                                      </p>
                                      <p className="text-sm font-medium text-amber-900/90 mt-1">
                                        {defaultImportInactiveNoticeText}
                                      </p>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Name + Tool */}
                              <div>
                                <div className="acc-section-head">
                                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
                                  <h5>{t('basicInfo', '基本信息')}</h5>
                                </div>
                                <div className="field-grid col-2">
                                  <div className="field">
                                    <label>{activeTool === 'opencode' ? t('providerName') : t('presetName')}</label>
                                    <input type="text" value={editingProvider.name || ''} onChange={e => setEditingProvider({...editingProvider, name: e.target.value})}
                                      className="w-full"
                                    />
                                  </div>
                                  <div className="field">
                                    <label>{activeTool === 'opencode' ? t('providerIdentifier') : t('targetCliTool')}</label>
                                    {activeTool === 'opencode' ? (
                                      <input type="text" value={editingProvider.provider_key || ''} onChange={e => setEditingProvider({...editingProvider, provider_key: e.target.value.replace(/[^a-zA-Z]/g, '')})}
                                        placeholder="e.g. MyOpenAI" className="font-mono"
                                      />
                                    ) : (
                                      <input value={editingProvider.tool || activeTool} disabled className="capitalize" />
                                    )}
                                  </div>
                                </div>
                              </div>
                              {/* Auth & Endpoint */}
                              {activeTool !== 'opencode' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                                    <h5>{t('authAndEndpoint')}</h5>
                                  </div>
                                  <div className="field-grid col-2">
                                    <div className="field full-span">
                                      <label>{t('apiKey')}</label>
                                      <input type="password" placeholder="sk-..." value={editingProvider.api_key || ''} onChange={e => setEditingProvider({...editingProvider, api_key: e.target.value})}
                                        className="font-mono"
                                      />
                                    </div>
                                    <div className="field full-span">
                                      <label>{t('baseUrl')}</label>
                                      <input type="url" placeholder="https://api.your-proxy.com" value={editingProvider.base_url || ''} onChange={e => setEditingProvider({...editingProvider, base_url: e.target.value})}
                                      />
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Model Config */}
                              {activeTool !== 'opencode' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83"/></svg>
                                    <h5>{t('modelConfig')}</h5>
                                  </div>
                                  <div className="field-grid">
                                    <div className="field">
                                      <label>{t('primaryModel')}</label>
                                      <input type="text" placeholder={activeTool === 'gemini' ? "gemini-2.5-flash" : "gpt-4o"} value={editingProvider.model || ''} onChange={e => setEditingProvider({...editingProvider, model: e.target.value})}
                                      />
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Codex Advanced Options */}
                              {activeTool === 'codex' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
                                    <h5>{t('advancedOptions', 'Advanced Options')}</h5>
                                  </div>
                                  <div className="checkbox-row info">
                                    <input type="checkbox" id={`drs-${p.id}`} checked={editingProvider.disable_response_storage || false} onChange={e => setEditingProvider({...editingProvider, disable_response_storage: e.target.checked})}
                                    />
                                    <div>
                                      <div className="label">{t('disableResponseStorage', 'Disable Response Storage')}</div>
                                      <div className="desc">{t('disableResponseStorageDesc', 'Do not store responses locally for privacy.')}</div>
                                    </div>
                                  </div>
                                  <div className="field-grid col-2">
                                    <div className="field">
                                      <label>{t('personality', 'Personality')}</label>
                                      <select value={editingProvider.personality || ''} onChange={e => setEditingProvider({...editingProvider, personality: e.target.value || undefined})}>
                                        <option value="">{t('personalityDefault', 'Default')}</option>
                                        <option value="pragmatic">{t('personalityPragmatic', 'Pragmatic')}</option>
                                        <option value="chatty">{t('personalityChatty', 'Chatty')}</option>
                                      </select>
                                      <div className="field-hint">{t('personalityDesc', 'Controls the AI response style.')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('wireApi', 'Wire API Format')}</label>
                                      <select value={editingProvider.wire_api || ''} onChange={e => setEditingProvider({...editingProvider, wire_api: e.target.value || undefined})}>
                                        <option value="">{t('wireApiDefault', 'Default')}</option>
                                        <option value="chat">{t('wireApiChat', 'Chat (Legacy)')}</option>
                                        <option value="responses">{t('wireApiResponses', 'Responses (New)')}</option>
                                      </select>
                                      <div className="field-hint">{t('wireApiDesc', 'API format for model providers.')}</div>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Codex Reasoning Config */}
                              {activeTool === 'codex' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 2a10 10 0 1 0 10 10H12V2z"/><path d="M12 2a10 10 0 0 1 10 10"/></svg>
                                    <h5>{t('reasoningConfig')}</h5>
                                  </div>
                                  <div className="field-grid col-2">
                                    <div className="field">
                                      <label>{t('reasoningEffort')}</label>
                                      <select value={editingProvider.model_reasoning_effort || ''} onChange={e => setEditingProvider({...editingProvider, model_reasoning_effort: e.target.value || undefined})}>
                                        <option value="">{t('reasoningEffortDefault')}</option>
                                        <option value="minimal">{t('reasoningEffortMinimal')}（minimal）</option>
                                        <option value="low">{t('reasoningEffortLow')}（low）</option>
                                        <option value="medium">{t('reasoningEffortMedium')}（medium）</option>
                                        <option value="high">{t('reasoningEffortHigh')}（high）</option>
                                        <option value="xhigh">{t('reasoningEffortXHigh')}（xhigh）</option>
                                      </select>
                                      <div className="field-hint">{t('reasoningEffortDesc')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('reasoningSummary')}</label>
                                      <select value={editingProvider.model_reasoning_summary || ''} onChange={e => setEditingProvider({...editingProvider, model_reasoning_summary: e.target.value || undefined})}>
                                        <option value="">{t('reasoningSummaryAuto')}</option>
                                        <option value="concise">{t('reasoningSummaryConcise')}</option>
                                        <option value="detailed">{t('reasoningSummaryDetailed')}</option>
                                        <option value="none">{t('reasoningSummaryNone')}</option>
                                      </select>
                                      <div className="field-hint">{t('reasoningSummaryDesc')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('approvalPolicy')}</label>
                                      <select value={editingProvider.approval_policy || ''} onChange={e => setEditingProvider({...editingProvider, approval_policy: e.target.value || undefined})}>
                                        <option value="">{t('approvalPolicyDefault')}</option>
                                        <option value="untrusted">{t('approvalPolicyUntrusted')}</option>
                                        <option value="on-failure">{t('approvalPolicyOnFailure')}</option>
                                        <option value="on-request">{t('approvalPolicyOnRequest')}</option>
                                        <option value="never">{t('approvalPolicyNever')}</option>
                                      </select>
                                      <div className="field-hint">{t('approvalPolicyDesc')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('sandboxMode')}</label>
                                      <select value={editingProvider.sandbox_mode || ''} onChange={e => setEditingProvider({...editingProvider, sandbox_mode: e.target.value || undefined})}>
                                        <option value="">{t('sandboxModeDefault')}</option>
                                        <option value="read-only">{t('sandboxModeReadOnly')}</option>
                                        <option value="workspace-write">{t('sandboxModeWorkspaceWrite')}</option>
                                      </select>
                                      <div className="field-hint">{t('sandboxModeDesc')}</div>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Gemini Auth Method */}
                              {activeTool === 'gemini' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                                    <h5>{t('authMethod')}</h5>
                                  </div>
                                  <div className="field-grid">
                                    <div className="field">
                                      <label>{t('geminiAuthType')}</label>
                                      <select value={editingProvider.gemini_auth_type || ''} onChange={e => setEditingProvider({...editingProvider, gemini_auth_type: e.target.value || undefined})}>
                                        <option value="">{t('geminiAuthDefault')}</option>
                                        <option value="gemini-api-key">{t('geminiAuthApiKey')}</option>
                                        <option value="oauth-personal">{t('geminiAuthOAuth')}</option>
                                      </select>
                                      <div className="field-hint">{t('geminiAuthTypeDesc')}</div>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Gemini Behavior Config */}
                              {activeTool === 'gemini' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
                                    <h5>{t('behaviorConfig')}</h5>
                                  </div>
                                  <div className="field-grid col-2">
                                    <div className="field">
                                      <label>{t('theme')}</label>
                                      <select value={editingProvider.theme || ''} onChange={e => setEditingProvider({...editingProvider, theme: e.target.value || undefined})}>
                                        <option value="">{t('themeDefault')}</option>
                                        <option value="Default">{t('themeDefault')}</option>
                                        <option value="GitHub Dark">{t('themeGitHubDark')}</option>
                                        <option value="Light">{t('themeLight')}</option>
                                      </select>
                                      <div className="field-hint">{t('themeDesc')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('defaultApprovalMode')}</label>
                                      <select value={editingProvider.default_approval_mode || ''} onChange={e => setEditingProvider({...editingProvider, default_approval_mode: e.target.value || undefined})}>
                                        <option value="">{t('defaultApprovalModeDefault')}</option>
                                        <option value="auto_edit">{t('defaultApprovalModeAutoEdit')}</option>
                                        <option value="plan">{t('defaultApprovalModePlan')}</option>
                                      </select>
                                      <div className="field-hint">{t('defaultApprovalModeDesc')}</div>
                                    </div>
                                  </div>
                                  <div className="checkbox-row info">
                                    <input type="checkbox" id={`vim-${p.id}`} checked={editingProvider.vim_mode || false} onChange={e => setEditingProvider({...editingProvider, vim_mode: e.target.checked})} />
                                    <div>
                                      <div className="label">{t('vimMode')}</div>
                                      <div className="desc">{t('vimModeDesc')}</div>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* OpenCode Global Config */}
                              {activeTool === 'opencode' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="10"/><path d="M12 16v-4M12 8h.01"/></svg>
                                    <h5>{t('globalConfig', 'Global Configuration')}</h5>
                                  </div>
                                  <div className="field-grid col-4">
                                    <div className="field">
                                      <label>{t('defaultModel', 'Default Model')}</label>
                                      <input type="text" placeholder="anthropic/claude-3-7-sonnet-20250219" value={editingProvider.opencode_default_model || ''} onChange={e => setEditingProvider({...editingProvider, opencode_default_model: e.target.value})}
                                      />
                                      <div className="field-hint">{t('defaultModelDesc', 'Default model for all OpenCode sessions.')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('defaultAgent', 'Default Agent')}</label>
                                      <input type="text" placeholder="coder" value={editingProvider.opencode_default_agent || ''} onChange={e => setEditingProvider({...editingProvider, opencode_default_agent: e.target.value})}
                                      />
                                      <div className="field-hint">{t('defaultAgentDesc', 'Default agent type (e.g., coder, architect, reviewer).')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('sessionsDir', 'Sessions Directory')}</label>
                                      <input type="text" placeholder=".opencode/sessions" value={editingProvider.opencode_sessions_dir || ''} onChange={e => setEditingProvider({...editingProvider, opencode_sessions_dir: e.target.value})}
                                      />
                                      <div className="field-hint">{t('sessionsDirDesc', 'Directory to store session history.')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('smallModel')}</label>
                                      <input type="text" placeholder={t('smallModelPlaceholder')} value={editingProvider.small_model || ''} onChange={e => setEditingProvider({...editingProvider, small_model: e.target.value})}
                                      />
                                      <div className="field-hint">{t('smallModelDesc')}</div>
                                    </div>
                                  </div>
                                  <div className="field-hint" style={{ marginTop: '8px' }}>{t('globalConfigHint', '全局配置应用于所有 OpenCode 会话，不按 Provider 隔离。')}</div>
                                </div>
                              )}
                              {/* OpenCode Advanced Config */}
                              {activeTool === 'opencode' && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
                                    <h5>{t('advancedConfig', 'Advanced Configuration')}</h5>
                                  </div>
                                  <div className="field-grid col-2">
                                    <div className="field">
                                      <label>{t('requestTimeout')}</label>
                                      <input type="number" placeholder="60000" value={editingProvider.timeout || ''} onChange={e => setEditingProvider({...editingProvider, timeout: e.target.value ? parseInt(e.target.value) : undefined})}
                                      />
                                      <div className="field-hint">{t('requestTimeoutDesc')}</div>
                                    </div>
                                    <div className="field">
                                      <label>{t('shareMode')}</label>
                                      <select value={editingProvider.share_mode || ''} onChange={e => setEditingProvider({...editingProvider, share_mode: e.target.value || undefined})}>
                                        <option value="">{t('shareModeManual')}</option>
                                        <option value="manual">{t('shareModeManual')}</option>
                                        <option value="auto">{t('shareModeAuto')}</option>
                                        <option value="disabled">{t('shareModeDisabled')}</option>
                                      </select>
                                      <div className="field-hint">{t('shareModeDesc')}</div>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* OpenCode JSON Editor */}
                              {activeTool === 'opencode' && (
                                <div>
                                  <div className="acc-section-head">
                                    <div className="flex items-center gap-2">
                                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
                                      <h5>{t('jsonConfig')}</h5>
                                    </div>
                                    <div className="flex items-center gap-2">
                                      <div className="relative" ref={historyRef}>
                                        <button onClick={() => setShowHistory(!showHistory)} className="acc-btn">
                                          <History className="w-3 h-3" /> {t('aiHistory')}
                                        </button>
                                        {showHistory && (
                                          <div className="absolute right-0 top-full mt-2 w-80 max-h-96 bg-popover border shadow-xl rounded-lg overflow-hidden z-50 animate-in fade-in slide-in-from-top-2">
                                            <div className="p-3 border-b flex items-center justify-between bg-muted/30">
                                              <span className="text-xs font-bold uppercase tracking-wider">{t('aiHistory')}</span>
                                              <button onClick={() => setShowHistory(false)}><X className="w-4 h-4" /></button>
                                            </div>
                                            <div className="overflow-y-auto max-h-[300px] p-1">
                                              {(!editingProvider.history || editingProvider.history.length === 0) ? (
                                                <div className="p-8 text-center text-xs text-muted-foreground">{t('noHistory')}</div>
                                              ) : (
                                                editingProvider.history.map((entry, i) => (
                                                  <div key={i} className="p-2 hover:bg-muted/50 rounded-md border border-transparent hover:border-border transition-all mb-1 group">
                                                    <div className="flex items-center justify-between mb-1">
                                                      <span className="text-[10px] font-mono text-muted-foreground">{new Date(entry.timestamp).toLocaleString()}</span>
                                                      <button onClick={() => handleRollback(entry)} className="text-[10px] text-primary hover:underline flex items-center gap-1">
                                                        <RotateCcw className="w-2.5 h-2.5" /> {t('rollback')}
                                                      </button>
                                                    </div>
                                                    <div className="bg-background/50 p-1.5 rounded text-[10px] font-mono truncate text-muted-foreground border border-border/50">
                                                      {entry.content.substring(0, 100)}...
                                                    </div>
                                                  </div>
                                                ))
                                              )}
                                            </div>
                                          </div>
                                        )}
                                      </div>
                                      <button onClick={handleFormatJson} className="acc-btn">
                                        <Eraser className="w-3 h-3" /> {t('format')}
                                      </button>
                                    </div>
                                  </div>
                                  {isRollbackMode && (
                                    <div className="bg-amber-50 border border-amber-200 p-3 rounded-md flex items-start gap-3 animate-in fade-in slide-in-from-top-1">
                                      <RotateCcw className="w-4 h-4 text-amber-600 mt-0.5" />
                                      <div className="space-y-1">
                                        <p className="text-sm font-semibold text-amber-800">{t('rollbackModeTitle')}</p>
                                        <p className="text-xs text-amber-700">{t('rollbackModeDesc')}</p>
                                      </div>
                                      <button
                                        onClick={() => {
                                          setRawJson(originalJson);
                                          setIsRollbackMode(false);
                                        }}
                                        className="ml-auto text-xs font-medium text-amber-800 hover:underline"
                                      >
                                        {t('cancel')}
                                      </button>
                                    </div>
                                  )}
                                  <div className="field full-span">
                                    <div className={`border rounded-md bg-white overflow-hidden font-mono text-sm shadow-inner transition-colors ${isRollbackMode ? 'ring-2 ring-amber-500 border-amber-500' : ''}`}>
                                      <Editor value={rawJson} onValueChange={code => {
                                        setRawJson(code);
                                        if (isRollbackMode) setIsRollbackMode(false);
                                      }} highlight={code => highlight(code, languages.json, 'json')} padding={16}
                                        style={{ fontFamily: '"Fira code", "Fira Mono", monospace', minHeight: '200px', backgroundColor: 'white', color: '#1a1a1a' }}
                                        className="focus:outline-none"
                                      />
                                    </div>
                                    <div className="field-hint">{t('jsonEditHint')}</div>
                                  </div>
                                </div>
                              )}
                              {/* 托管配置管理 (Codex/Gemini/OpenCode) */}
                              {activeTool !== 'claude' && isManagedTool(activeTool) && (
                                <div>
                                  <div className="acc-section-head">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                                    <h5>{t('managedConfig', '托管配置管理')}</h5>
                                  </div>
                                  <div className="checkbox-row info">
                                    <input
                                      type="checkbox"
                                      id={`${activeTool}-env-managed-${p.id}`}
                                      checked={editingProvider.env_managed !== false}
                                      onChange={e => {
                                        const enabled = e.target.checked;
                                        setEditingProvider(prev => ({ ...prev, env_managed: enabled }));
                                      }}
                                    />
                                    <div>
                                      <div className="label">{t('envManagedToggle', '启用托管配置')}</div>
                                      <div className="desc">
                                        {editingProvider.env_managed !== false
                                          ? t('envManagedEnabledDesc', '应用时将自动更新 CLI 配置文件。关闭后，应用按钮将被禁用，CLI 配置不会被自动覆盖。')
                                          : t('envManagedDisabledDesc', '托管配置已禁用。CLI 配置文件不会被自动覆盖，需要手动管理。')}
                                      </div>
                                    </div>
                                  </div>
                                </div>
                              )}
                              {/* Action buttons */}
                              <div className="acc-panel-footer">
                                <div className="left">
                                  {canDeleteSelectedProvider && currentProviderId === p.id && (
                                    <button onClick={() => void handleDelete(p.id)} className="acc-panel-btn danger">
                                      <Trash2 className="w-4 h-4" /> {activeTool === 'opencode' ? t('deleteProvider') : t('deletePreset')}
                                    </button>
                                  )}
                                </div>
                                <div className="right">
                                  {activeTool !== 'opencode' && !isCurrentProviderActive && (
                                    <button
                                      onClick={() => void activateProvider(activeTool, p.id)}
                                      disabled={loading}
                                      className="acc-panel-btn primary"
                                    >
                                      <Play className="w-4 h-4" /> {t('applyToCli')}
                                    </button>
                                  )}
                                  <button onClick={handleSavePresetWithActivationPrompt} disabled={saving} className="acc-panel-btn">
                                    {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />} {t('save')}
                                  </button>
                                </div>
                              </div>
                            </div>
                          }
                        />
                      );
                    })
                );
              })()}
              {/* Synced devices */}
              <SyncedDevices
                syncedOtherDeviceProviders={syncedOtherDeviceProviders}
                activeTool={activeTool}
                onActivate={handleActivateSyncedProvider}
                loading={loading}
                activatingSyncedKey={activatingSyncedKey}
                t={t}
              />
            </div>
          )}
        </div>
      </div>

    {importPreview && (
      <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
        <div className="w-full max-w-4xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
          <div className="p-5 border-b flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h3 className="text-lg font-semibold">
                {t('providersImportReviewTitle', 'Review environment import')}
              </h3>
              <p className="text-sm text-muted-foreground mt-1">
                {t('providersImportReviewDesc', {
                  total: importPreview.total || 0,
                  conflicts: importPreview.conflicts || 0,
                  defaultValue:
                    'Found {{total}} environment(s), including {{conflicts}} conflict(s). Choose how to handle conflicts before importing.',
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
                      'Overwrite will update the existing environment. Create new will keep both versions.',
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
                  '{{count}} conflict(s) require a choice. Non-conflicting environments will be imported directly.',
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
