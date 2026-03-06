import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { confirm as tauriConfirm } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Plus, Save, Play, Trash2, CheckCircle2, ShieldAlert, KeyRound, Globe, Zap, Brain, Sparkles, Box, CircleOff, TerminalSquare, Code2, Eraser, History, RotateCcw, X, RefreshCw, Settings, AlertTriangle, Loader2 } from 'lucide-react';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './icons';
import Editor from 'react-simple-code-editor';
import { highlight, languages } from 'prismjs';
import 'prismjs/components/prism-json';
import 'prismjs/themes/prism-tomorrow.css';

const TOOLS = ['claude', 'codex', 'gemini', 'opencode'] as const;
const MANAGED_TOOLS = ['claude', 'codex', 'gemini'] as const;
type CliTool = (typeof TOOLS)[number];
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
type ApiResp<T> = { ok: boolean; data: T; meta: { schema_version: number; revision: number } };

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
  model_reasoning_effort?: string;  // "minimal" | "low" | "medium" | "high"
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
  history?: HistoryEntry[];
  [key: string]: any;
}

export interface AiProvidersState {
  active_claude: string | null;
  active_codex: string | null;
  active_gemini: string | null;
  active_opencode: string | null;
  providers: AiProvider[];
  is_encrypted?: boolean;
}

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
  const [state, setState] = useState<AiProvidersState>(DEFAULT_STATE);
  const [activeTool, setActiveTool] = useState('claude');
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  
  const [editingProvider, setEditingProvider] = useState<Partial<AiProvider>>({});
  const [originalProvider, setOriginalProvider] = useState<Partial<AiProvider>>({});
  const [rawJson, setRawJson] = useState('');
  const [originalJson, setOriginalJson] = useState('');
  const [loading, setLoading] = useState(false);
  const [_message, setMessage] = useState({ type: '', text: '' });
  const [showHistory, setShowHistory] = useState(false);
  const [isRollbackMode, setIsRollbackMode] = useState(false);
  const [cliVersions, setCliVersions] = useState<Partial<Record<CliTool, CliVersionState>>>({});
  const [checkingVersions, setCheckingVersions] = useState<Partial<Record<CliTool, boolean>>>({});
  const [checkingAllVersions, setCheckingAllVersions] = useState(false);
  const [cliProbe, setCliProbe] = useState<Partial<Record<CliTool, CliEnvProbeResult>>>({});
  const [probingTool, setProbingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const historyRef = useRef<HTMLDivElement>(null);
  const versionCheckRunIdRef = useRef(0);
  const probeRunIdRef = useRef(0);
  const isVisibleRef = useRef(isVisible);
  const cliProbeInitializedRef = useRef(false);
  const autoImportInitializedRef = useRef(false);

  const isTauri = '__TAURI_INTERNALS__' in window;
  const isManagedTool = (tool: string): tool is (typeof MANAGED_TOOLS)[number] =>
    (MANAGED_TOOLS as readonly string[]).includes(tool);

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
      'small_model', 'timeout', 'share_mode', 'env_managed'
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
    } catch (e: any) {
      console.error('Failed to load AI providers:', e);
      setMessage({ type: 'error', text: `Failed to load providers: ${e.toString()}` });
    } finally {
      if (!silent) setLoading(false);
    }
  };

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
    } finally {
      if (versionCheckRunIdRef.current === runId) {
        setCheckingVersions({});
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
            const res = await invoke<ApiResp<{ imported: boolean }>>('providers_auto_import_from_system', { tool });
            return !!res.data?.imported;
          } catch (e) {
            console.error(`Auto import failed for ${tool}:`, e);
            return false;
          }
        })
      );
      if (probeRunIdRef.current !== runId) return;
      const importedAny = autoImportResults.some(Boolean);
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
    } else {
      const empty = { name: '', api_key: '', base_url: '', model: '' };
      setEditingProvider(empty);
      setOriginalProvider(empty);
      setRawJson('{}');
      setOriginalJson('{}');
    }
    setShowHistory(false);
  }, [currentProviderId, state.providers, activeTool]);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (historyRef.current && !historyRef.current.contains(event.target as Node)) {
        setShowHistory(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [historyRef]);

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

  const handleSavePreset = async () => {
    if (!editingProvider.name) {
      setMessage({ type: 'error', text: t('providePresetName', 'Please provide a preset name') });
      return;
    }

    const newId = editingProvider.id || `custom-${Date.now()}`;
    
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
          id: baseProvider.id,
          tool: baseProvider.tool,
          is_enabled: baseProvider.is_enabled,
          provider_key: baseProvider.provider_key,
          history: currentHistory,
          ...parsed,
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
        return;
      }
    }

    const finalProvider: AiProvider = {
      ...baseProvider,
      id: newId,
      name: baseProvider.name || 'Unnamed',
      provider_key: baseProvider.provider_key,
      tool: activeTool,
      api_key: baseProvider.api_key || '',
      is_enabled: baseProvider.is_enabled ?? true,
      env_managed: activeTool !== 'opencode' ? (baseProvider.env_managed ?? true) : undefined,
      history: currentHistory,
    };

    try {
      setLoading(true);
      await invoke('providers_upsert', { provider: finalProvider });
      await loadProviders(true);
      setCurrentProviderId(newId);
      
      // Update counts in sidebar
      emit('refresh-counts');

      // Update originals to disable save button after success
      setOriginalProvider(finalProvider);
      setIsRollbackMode(false);
      
      // Environment regardless of whether active needs data sync
      if (activeTool === 'opencode') {
        setOriginalJson(rawJson);
        if (finalProvider.is_enabled) {
          await invoke('projection_apply', { tool: finalProvider.tool, providerId: finalProvider.id });
        }
      } else {
        await invoke('projection_apply', { tool: finalProvider.tool, providerId: finalProvider.id });
      }

      setMessage({ type: 'success', text: t('presetSaved', 'Preset saved successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleApply = async () => {
    // First save the current data
    await handleSavePreset();
    if (activeTool === 'opencode') return; 

    try {
      setLoading(true);
      setMessage({ type: '', text: '' });

      const providerId = currentProviderId || editingProvider.id;
      if (!providerId) return;

      await invoke('providers_set_active', { tool: activeTool, providerId });
      await loadProviders(true);

      // Actually apply it to the CLI config
      await invoke('projection_apply', { tool: activeTool, providerId });
      
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
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
    setActiveTool(toolName);
    const newId = `custom-${Date.now()}`;
    const newProvider: AiProvider = {
      id: newId,
      name: `${t('newPreset', 'New Preset')} (${toolName})`,
      tool: toolName,
      api_key: '',
      base_url: '',
      model: '',
      env_managed: toolName !== 'opencode' ? true : undefined,
      provider_key: toolName === 'opencode' ? `provider_${Date.now()}` : undefined,
      is_enabled: toolName === 'opencode' ? false : true,
      ...(toolName === 'opencode' ? {
        npm: '@ai-sdk/openai-compatible',
        options: { apiKey: '', baseURL: '' },
        models: {}
      } : {})
    };
    
    const newState = {
      ...state,
      providers: [...state.providers, newProvider]
    };
    
    setState(newState);
    setCurrentProviderId(newId);
  };

  const handleDelete = async () => {
    if (!currentProviderId || state.providers.filter(p => p.tool === activeTool).length <= 1) return;
    
    const providerToDelete = state.providers.find(p => p.id === currentProviderId);
    if (!providerToDelete) return;

    const confirmMsg = activeTool === 'opencode' 
      ? t('confirmDelete', { name: providerToDelete.name }) 
      : t('confirmDelete', { name: providerToDelete.name });
    
    const confirmed = await tauriConfirm(confirmMsg, {
      okLabel: t('ok'),
      cancelLabel: t('cancel')
    });
    if (!confirmed) return;
    
    try {
      await invoke('providers_delete', { providerId: currentProviderId });
      await loadProviders(true);
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
    const confirmed = await tauriConfirm(confirmText, {
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
  const envManagedEnabled = managedProvider?.env_managed !== false;

  const getToolDescription = (tool: string) => {
    switch (tool.toLowerCase()) {
      case 'claude': return t('configureClaude');
      case 'codex': return t('configureCodex');
      case 'gemini': return t('configureGemini');
      case 'opencode': return t('configureOpenCode');
      default: return t('configureAiEndpoint');
    }
  };

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('aiEnvironments')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('aiEnvironmentsDesc')}</p>
        </div>
      </div>

      <div className="border rounded-xl bg-card p-3 space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold tracking-wide uppercase text-muted-foreground">
            {t('cliVersion')}
          </h3>
          <button
            onClick={() => {
              void detectAllVersions();
            }}
            disabled={checkingAllVersions}
            className="p-2 hover:bg-secondary rounded-md transition-colors disabled:opacity-50"
            title={t('checkVersion')}
          >
            <RefreshCw className={`w-4 h-4 ${checkingAllVersions ? 'animate-spin' : ''}`} />
          </button>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          {TOOLS.map(tool => {
            const versionInfo = cliVersions[tool];
            const isChecking = checkingVersions[tool];
            const isInstalled = versionInfo?.isInstalled;
            const activeProviderId = state[`active_${tool}` as keyof AiProvidersState] as string | null;
            const activeProvider = activeProviderId
              ? state.providers.find(p => p.id === activeProviderId && p.tool === tool) || null
              : null;
            const envManagedState = isManagedTool(tool)
              ? (activeProvider?.env_managed !== false ? 'enabled' : 'disabled')
              : 'na';
            return (
              <button
                key={tool}
                type="button"
                onClick={() => {
                  setActiveTool(tool);
                  setCurrentProviderId(null);
                }}
                className={`rounded-lg border px-4 py-3 text-left transition-colors ${
                  activeTool === tool ? 'border-primary bg-primary/5' : 'hover:bg-muted/40'
                }`}
              >
                <div className="flex items-center gap-2">
                  <ToolIcon tool={tool} className="w-5 h-5" />
                  <span className="text-sm font-semibold capitalize">{tool}</span>
                </div>
                <div className="mt-2.5 flex items-center gap-2">
                  {isChecking ? (
                    <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
                  ) : isInstalled ? (
                    <CheckCircle2 className="w-4 h-4 text-green-600" />
                  ) : (
                    <AlertTriangle className="w-4 h-4 text-amber-600" />
                  )}
                  <span className={`text-sm leading-none ${isInstalled ? 'text-foreground' : 'text-amber-600'}`}>
                    {isInstalled ? `v${versionInfo?.version}` : t('notInstalled')}
                  </span>
                </div>
                <div className="mt-2.5 flex items-center gap-2">
                  {envManagedState === 'enabled' ? (
                    <CheckCircle2 className="w-4 h-4 text-green-600" />
                  ) : envManagedState === 'disabled' ? (
                    <ShieldAlert className="w-4 h-4 text-amber-600" />
                  ) : (
                    <CircleOff className="w-4 h-4 text-muted-foreground" />
                  )}
                  <span className="text-xs text-muted-foreground leading-none">
                    {t('envManagedTitle')}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex-1 flex border rounded-xl overflow-hidden bg-background">
        <div className="w-64 border-r flex flex-col shrink-0 bg-muted/20">
          <div className="h-16 px-4 border-b flex items-center justify-between bg-card shrink-0">
            <h2 className="font-semibold">{t('environments', 'Environments')}</h2>
            <div className="relative group">
              <button className="p-1.5 hover:bg-muted rounded-md transition-colors text-muted-foreground">
                <Plus className="w-4 h-4" />
              </button>
              <div className="absolute left-0 top-full w-44 bg-popover border shadow-md rounded-md py-1 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-all z-10 before:content-[''] before:absolute before:-top-2 before:left-0 before:w-full before:h-2">
                <div className="py-0.5">
                  {TOOLS.map(toolName => (
                    <button 
                      key={toolName} 
                      onClick={() => handleAddCustom(toolName)} 
                      className="w-full text-left px-3 py-2 text-sm hover:bg-muted capitalize flex items-center gap-2"
                    >
                      <ToolIcon tool={toolName} className="w-4 h-4" />
                      {toolName === 'opencode' ? t('opencodeProvider', 'OpenCode Provider') : toolName}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-4">
            {TOOLS.map(tool => {
              const toolProviders = state.providers.filter(p => p.tool === tool);
              const activeId = state[`active_${tool}` as keyof AiProvidersState];
              return (
                <div key={tool} className="space-y-1">
                  <div className="px-2 py-1 text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                    <ToolIcon tool={tool} className="w-4 h-4" />
                    {tool} ({toolProviders.length})
                  </div>
                  {toolProviders.map(p => (
                    <button key={p.id} onClick={() => { setActiveTool(tool); setCurrentProviderId(p.id); }}
                      className={`w-full flex items-center gap-2 px-3 py-2 text-sm rounded-md transition-colors ${currentProviderId === p.id ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-muted text-foreground'}`}
                    >
                      <div 
                        className={`w-2 h-2 rounded-full shrink-0 ${tool === 'opencode' ? (p.is_enabled ? 'bg-green-500' : 'bg-amber-500') : (activeId === p.id ? 'bg-green-500' : 'bg-transparent border border-muted-foreground/30')}`}
                        title={tool === 'opencode' ? (p.is_enabled ? t('syncedToCli') : t('pausedInOneSpaceOnly')) : ''}
                      />
                      <span className="truncate flex-1 text-left">{p.name}</span>
                    </button>
                  ))}
                </div>
              );
            })}
          </div>
        </div>

        <div className="flex-1 flex flex-col h-full bg-card overflow-hidden">
          <div className="h-16 px-4 border-b shrink-0 flex items-center justify-between bg-card">
            <div>
              <h2 className="font-semibold">{t('providerDetails', 'Provider Details')}</h2>
              <p className="text-xs text-muted-foreground mt-0.5">{getToolDescription(activeTool)}</p>
            </div>
            <div />
          </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-5">
          {!showingProviderDetails && (() => {
            const tool = activeTool as CliTool;
            const probe = cliProbe[tool];
            const versionInfo = cliVersions[tool];
            const installed = probe?.installed ?? versionInfo?.isInstalled ?? false;
            const configured = probe?.configured ?? false;
            const installGuide = probe?.install_guide;
            return (
              <div className="max-w-4xl bg-muted/30 p-4 rounded-lg border space-y-3">
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <div className="font-semibold flex items-center gap-2">
                      <ToolIcon tool={activeTool} className="w-4 h-4" />
                      <span className="capitalize">{activeTool} CLI</span>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      {installed
                        ? `${t('cliInstalledStatus')} ${probe?.version || versionInfo?.version || 'unknown'}`
                        : t('cliNotInstalledStatus')}
                    </p>
                    {installed && (
                      <p className="text-xs text-muted-foreground">
                        {configured ? t('cliConfigDetected') : t('cliConfigNotDetected')}
                      </p>
                    )}
                  </div>
                  {(probingTool[tool] || checkingVersions[tool]) && (
                    <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
                  )}
                </div>

                {!installed && installGuide && installGuide.commands.length > 0 && (
                  <div className="space-y-2">
                    <p className="text-sm font-medium text-amber-700">{t('cliInstallGuideTitle')}</p>
                    {installGuide.commands.map((item, idx) => (
                      <div key={`${item.label}-${idx}`} className="rounded-md border bg-background px-3 py-2">
                        <div className="text-xs text-muted-foreground">{item.label}</div>
                        <code className="text-xs font-mono break-all">{item.command}</code>
                      </div>
                    ))}
                    {installGuide.docs_url && (
                      <a
                        href={installGuide.docs_url}
                        target="_blank"
                        rel="noreferrer"
                        className="text-xs text-primary hover:underline"
                      >
                        {t('viewOfficialDocs')}
                      </a>
                    )}
                  </div>
                )}

                {activeTool !== 'opencode' && isManagedTool(activeTool) && (
                  <div className="pt-2 border-t space-y-2">
                    <div className="font-semibold text-sm">{t('envManagedTitle')}</div>
                    <p className="text-xs text-muted-foreground">
                      {envManagedEnabled ? t('envManagedOnDesc') : t('envManagedOffDesc')}
                    </p>
                    <button
                      onClick={() => handleToggleEnvManaged(!envManagedEnabled)}
                      disabled={!managedProvider || loading}
                      className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors disabled:opacity-50 ${
                        envManagedEnabled
                          ? 'bg-amber-100 text-amber-700 hover:bg-amber-200 border border-amber-200'
                          : 'bg-green-100 text-green-700 hover:bg-green-200 border border-green-200'
                      }`}
                    >
                      {envManagedEnabled ? t('disableEnvManaged') : t('enableEnvManaged')}
                    </button>
                    {!managedProvider && (
                      <p className="text-xs text-muted-foreground">{t('noManagedProvider')}</p>
                    )}
                  </div>
                )}
              </div>
            );
          })()}

          {showingProviderDetails && activeTool === 'opencode' && (
            <div className="max-w-4xl bg-muted/30 p-4 rounded-lg border flex items-center justify-between">
              <div className="space-y-0.5">
                <div className="font-semibold flex items-center gap-2">
                  {editingProvider.is_enabled ? (
                    <span className="flex items-center gap-1.5 text-green-600 dark:text-green-500">
                      <CheckCircle2 className="w-4 h-4" /> {t('enabledInOpenCode')}
                    </span>
                  ) : (
                    <span className="flex items-center gap-1.5 text-amber-600 dark:text-amber-500">
                      <Box className="w-4 h-4" /> {t('pausedInOneSpace')}
                    </span>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">
                  {editingProvider.is_enabled ? t('enabledDesc') : t('pausedDesc')}
                </p>
              </div>
              <button onClick={() => {
                const newVal = !editingProvider.is_enabled;
                setEditingProvider({...editingProvider, is_enabled: newVal});
                setMessage({ type: 'success', text: newVal ? t('cliSyncEnabled') : t('cliSyncPaused') });
                setTimeout(() => setMessage({ type: '', text: '' }), 3000);
              }}
                className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${editingProvider.is_enabled ? 'bg-amber-100 text-amber-700 hover:bg-amber-200 border border-amber-200' : 'bg-green-100 text-green-700 hover:bg-green-200 border border-green-200'}`}
              >
                {editingProvider.is_enabled ? t('pauseCliSync') : t('enableCliSync')}
              </button>
            </div>
          )}

          {showingProviderDetails && (
          <div className="space-y-4 max-w-4xl">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{activeTool === 'opencode' ? t('providerName') : t('presetName')}</label>
                <input type="text" value={editingProvider.name || ''} onChange={e => setEditingProvider({...editingProvider, name: e.target.value})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{activeTool === 'opencode' ? t('providerIdentifier') : t('targetCliTool')}</label>
                {activeTool === 'opencode' ? (
                  <input type="text" value={editingProvider.provider_key || ''} onChange={e => setEditingProvider({...editingProvider, provider_key: e.target.value.replace(/[^a-zA-Z]/g, '')})}
                    placeholder="e.g. MyOpenAI" className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 font-mono"
                  />
                ) : (
                  <div className="w-full bg-muted/50 border rounded-md px-3 py-2 text-sm text-muted-foreground capitalize cursor-not-allowed">
                    {editingProvider.tool || activeTool}
                  </div>
                )}
              </div>
            </div>
          </div>
          )}

          {showingProviderDetails && activeTool !== 'opencode' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <KeyRound className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{t('authAndEndpoint')}</h3>
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('apiKey')}</label>
                <input type="password" placeholder="sk-..." value={editingProvider.api_key || ''} onChange={e => setEditingProvider({...editingProvider, api_key: e.target.value})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 font-mono"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('baseUrl')}</label>
                <div className="relative">
                  <Globe className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                  <input type="url" placeholder="https://api.your-proxy.com" value={editingProvider.base_url || ''} onChange={e => setEditingProvider({...editingProvider, base_url: e.target.value})}
                    className="w-full bg-background border rounded-md pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
              </div>
            </div>
          )}

          {showingProviderDetails && activeTool !== 'opencode' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <Box className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{activeTool === 'claude' ? t('modelRouting') : t('modelConfig')}</h3>
              </div>
              {activeTool === 'claude' ? (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {[
                    { label: t('defaultModel'), icon: Brain, key: 'claude_default_model', placeholder: 'claude-sonnet-4-20250514' },
                    { label: t('sonnetModel'), icon: Brain, key: 'claude_sonnet_model', placeholder: 'claude-sonnet-4-20250514' },
                    { label: t('fastModel'), icon: Zap, key: 'claude_haiku_model', placeholder: 'claude-3-5-haiku-20241022' },
                    { label: t('powerfulModel'), icon: Sparkles, key: 'claude_opus_model', placeholder: 'claude-opus-4-20250514' },
                    { label: t('thinkingModel'), icon: Brain, key: 'claude_reasoning_model', placeholder: 'claude-3-7-sonnet-20250219' }
                  ].map(m => (
                    <div key={m.key} className="space-y-2">
                      <label className="text-sm font-medium text-foreground flex items-center gap-1.5"><m.icon className="w-3.5 h-3.5"/> {m.label}</label>
                      <input type="text" placeholder={m.placeholder} value={(editingProvider as any)[m.key] || ''} onChange={e => setEditingProvider({...editingProvider, [m.key]: e.target.value})}
                        className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                      />
                    </div>
                  ))}
                </div>
              ) : (
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('primaryModel')}</label>
                  <input type="text" placeholder={activeTool === 'gemini' ? "gemini-2.5-flash" : "gpt-4o"} value={editingProvider.model || ''} onChange={e => setEditingProvider({...editingProvider, model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
              )}
            </div>
          )}
          
          {showingProviderDetails && activeTool === 'codex' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <Code2 className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{t('advancedOptions', 'Advanced Options')}</h3>
              </div>
              
              <div className="flex items-start gap-3 bg-primary/5 p-4 rounded-md border border-primary/20">
                <input type="checkbox" id="disableResponseStorage" checked={editingProvider.disable_response_storage || false} onChange={e => setEditingProvider({...editingProvider, disable_response_storage: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-primary"
                />
                <div className="space-y-1">
                  <label htmlFor="disableResponseStorage" className="text-sm font-medium cursor-pointer flex items-center gap-2">{t('disableResponseStorage', 'Disable Response Storage')}</label>
                  <p className="text-xs text-muted-foreground">{t('disableResponseStorageDesc', 'Do not store responses locally for privacy.')}</p>
                </div>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('personality', 'Personality')}</label>
                <select value={editingProvider.personality || ''} onChange={e => setEditingProvider({...editingProvider, personality: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('personalityDefault', 'Default')}</option>
                  <option value="pragmatic">{t('personalityPragmatic', 'Pragmatic')}</option>
                  <option value="chatty">{t('personalityChatty', 'Chatty')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('personalityDesc', 'Controls the AI response style.')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('wireApi', 'Wire API Format')}</label>
                <select value={editingProvider.wire_api || ''} onChange={e => setEditingProvider({...editingProvider, wire_api: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('wireApiDefault', 'Default')}</option>
                  <option value="chat">{t('wireApiChat', 'Chat (Legacy)')}</option>
                  <option value="responses">{t('wireApiResponses', 'Responses (New)')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('wireApiDesc', 'API format for model providers.')}</p>
              </div>
            </div>
          )}
          
          {showingProviderDetails && activeTool === 'codex' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <Brain className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{t('reasoningConfig')}</h3>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('reasoningEffort')}</label>
                <select 
                  value={editingProvider.model_reasoning_effort || ''}
                  onChange={e => setEditingProvider({...editingProvider, model_reasoning_effort: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('reasoningEffortDefault')}</option>
                  <option value="minimal">{t('reasoningEffortMinimal')}</option>
                  <option value="low">{t('reasoningEffortLow')}</option>
                  <option value="medium">{t('reasoningEffortMedium')}</option>
                  <option value="high">{t('reasoningEffortHigh')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('reasoningEffortDesc')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('reasoningSummary')}</label>
                <select
                  value={editingProvider.model_reasoning_summary || ''}
                  onChange={e => setEditingProvider({...editingProvider, model_reasoning_summary: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('reasoningSummaryAuto')}</option>
                  <option value="concise">{t('reasoningSummaryConcise')}</option>
                  <option value="detailed">{t('reasoningSummaryDetailed')}</option>
                  <option value="none">{t('reasoningSummaryNone')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('reasoningSummaryDesc')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('approvalPolicy')}</label>
                <select
                  value={editingProvider.approval_policy || ''}
                  onChange={e => setEditingProvider({...editingProvider, approval_policy: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('approvalPolicyDefault')}</option>
                  <option value="untrusted">{t('approvalPolicyUntrusted')}</option>
                  <option value="on-failure">{t('approvalPolicyOnFailure')}</option>
                  <option value="on-request">{t('approvalPolicyOnRequest')}</option>
                  <option value="never">{t('approvalPolicyNever')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('approvalPolicyDesc')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('sandboxMode')}</label>
                <select
                  value={editingProvider.sandbox_mode || ''}
                  onChange={e => setEditingProvider({...editingProvider, sandbox_mode: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('sandboxModeDefault')}</option>
                  <option value="read-only">{t('sandboxModeReadOnly')}</option>
                  <option value="workspace-write">{t('sandboxModeWorkspaceWrite')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('sandboxModeDesc')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('reasoningSummary')}</label>
                <select
                  value={editingProvider.model_reasoning_summary || ''}
                  onChange={e => setEditingProvider({...editingProvider, model_reasoning_summary: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('reasoningSummaryAuto')}</option>
                  <option value="concise">{t('reasoningSummaryConcise')}</option>
                  <option value="detailed">{t('reasoningSummaryDetailed')}</option>
                  <option value="none">{t('reasoningSummaryNone')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('reasoningSummaryDesc')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('approvalPolicy')}</label>
                <select
                  value={editingProvider.approval_policy || ''}
                  onChange={e => setEditingProvider({...editingProvider, approval_policy: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('approvalPolicyDefault')}</option>
                  <option value="untrusted">{t('approvalPolicyUntrusted')}</option>
                  <option value="on-failure">{t('approvalPolicyOnFailure')}</option>
                  <option value="on-request">{t('approvalPolicyOnRequest')}</option>
                  <option value="never">{t('approvalPolicyNever')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('approvalPolicyDesc')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('sandboxMode')}</label>
                <select
                  value={editingProvider.sandbox_mode || ''}
                  onChange={e => setEditingProvider({...editingProvider, sandbox_mode: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('sandboxModeDefault')}</option>
                  <option value="read-only">{t('sandboxModeReadOnly')}</option>
                  <option value="workspace-write">{t('sandboxModeWorkspaceWrite')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('sandboxModeDesc')}</p>
              </div>
            </div>
          )}
          
          {showingProviderDetails && activeTool === 'gemini' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <KeyRound className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{t('authMethod')}</h3>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('geminiAuthType')}</label>
                <select value={editingProvider.gemini_auth_type || ''} onChange={e => setEditingProvider({...editingProvider, gemini_auth_type: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('geminiAuthDefault')}</option>
                  <option value="gemini-api-key">{t('geminiAuthApiKey')}</option>
                  <option value="oauth-personal">{t('geminiAuthOAuth')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('geminiAuthTypeDesc')}</p>
              </div>
            </div>
          )}
          
          {showingProviderDetails && activeTool === 'gemini' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <Settings className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{t('behaviorConfig')}</h3>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('theme')}</label>
                <select
                  value={editingProvider.theme || ''}
                  onChange={e => setEditingProvider({...editingProvider, theme: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('themeDefault')}</option>
                  <option value="Default">{t('themeDefault')}</option>
                  <option value="GitHub Dark">{t('themeGitHubDark')}</option>
                  <option value="Light">{t('themeLight')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('themeDesc')}</p>
              </div>
              
              <div className="flex items-start gap-3 bg-primary/5 p-4 rounded-md border border-primary/20">
                <input
                  type="checkbox"
                  id="vimMode"
                  checked={editingProvider.vim_mode || false}
                  onChange={e => setEditingProvider({...editingProvider, vim_mode: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-primary"
                />
                <div className="space-y-1">
                  <label htmlFor="vimMode" className="text-sm font-medium cursor-pointer flex items-center gap-2">{t('vimMode')}</label>
                  <p className="text-xs text-muted-foreground">{t('vimModeDesc')}</p>
                </div>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('defaultApprovalMode')}</label>
                <select
                  value={editingProvider.default_approval_mode || ''}
                  onChange={e => setEditingProvider({...editingProvider, default_approval_mode: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('defaultApprovalModeDefault')}</option>
                  <option value="auto_edit">{t('defaultApprovalModeAutoEdit')}</option>
                  <option value="plan">{t('defaultApprovalModePlan')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('defaultApprovalModeDesc')}</p>
              </div>
              
              <div className="flex items-start gap-3 bg-primary/5 p-4 rounded-md border border-primary/20">
                <input
                  type="checkbox"
                  id="vimMode"
                  checked={editingProvider.vim_mode || false}
                  onChange={e => setEditingProvider({...editingProvider, vim_mode: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-primary"
                />
                <div className="space-y-1">
                  <label htmlFor="vimMode" className="text-sm font-medium cursor-pointer flex items-center gap-2">{t('vimMode')}</label>
                  <p className="text-xs text-muted-foreground">{t('vimModeDesc')}</p>
                </div>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('defaultApprovalMode')}</label>
                <select
                  value={editingProvider.default_approval_mode || ''}
                  onChange={e => setEditingProvider({...editingProvider, default_approval_mode: e.target.value || undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                >
                  <option value="">{t('defaultApprovalModeDefault')}</option>
                  <option value="auto_edit">{t('defaultApprovalModeAutoEdit')}</option>
                  <option value="plan">{t('defaultApprovalModePlan')}</option>
                </select>
                <p className="text-xs text-muted-foreground">{t('defaultApprovalModeDesc')}</p>
              </div>
            </div>
          )}

          {showingProviderDetails && activeTool === 'claude' && (
            <div className="space-y-4 max-w-4xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <ShieldAlert className="w-4 h-4 text-destructive" />
                <h3 className="font-semibold text-destructive">{t('advancedOptions', 'Advanced Options')}</h3>
              </div>
              <div className="flex items-start gap-3 bg-destructive/5 p-4 rounded-md border border-destructive/20">
                <input type="checkbox" id="dangerouslySkipPermissions" checked={editingProvider.dangerously_skip_permissions || false} onChange={e => setEditingProvider({...editingProvider, dangerously_skip_permissions: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-destructive"
                />
                <div className="space-y-1">
                  <label htmlFor="dangerouslySkipPermissions" className="text-sm font-medium cursor-pointer flex items-center gap-2">{t('dangerouslySkipPermissions', 'Dangerously Skip Permissions')}</label>
                  <p className="text-xs text-muted-foreground">{t('dangerouslySkipPermissionsDesc', 'Auto-approve all terminal commands executed by Claude Code (use with extreme caution).')}</p>
                </div>
              </div>
              
              <div className="flex items-start gap-3 bg-primary/5 p-4 rounded-md border border-primary/20">
                <input type="checkbox" id="enableAllMemoryFeatures" checked={editingProvider.enable_all_memory_features || false} onChange={e => setEditingProvider({...editingProvider, enable_all_memory_features: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-primary"
                />
                <div className="space-y-1">
                  <label htmlFor="enableAllMemoryFeatures" className="text-sm font-medium cursor-pointer flex items-center gap-2">{t('enableAllMemoryFeatures', 'Enable All Memory Features')}</label>
                  <p className="text-xs text-muted-foreground">{t('enableAllMemoryFeaturesDesc', 'Enable Claude Memory features for long-term context retention.')}</p>
                </div>
              </div>
              
              <div className="flex items-start gap-3 bg-primary/5 p-4 rounded-md border border-primary/20">
                <input type="checkbox" id="enableMcp" checked={editingProvider.enable_mcp || false} onChange={e => setEditingProvider({...editingProvider, enable_mcp: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-primary"
                />
                <div className="space-y-1">
                  <label htmlFor="enableMcp" className="text-sm font-medium cursor-pointer flex items-center gap-2">{t('enableMcp', 'Enable MCP')}</label>
                  <p className="text-xs text-muted-foreground">{t('enableMcpDesc', 'Enable Model Context Protocol for external tool integrations.')}</p>
                </div>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('allowedTools', 'Allowed Tools')}</label>
                <input type="text" placeholder="Read,Bash,Edit (comma separated)" value={(editingProvider.allowed_tools || []).join(', ')} onChange={e => setEditingProvider({...editingProvider, allowed_tools: e.target.value.split(',').map(s => s.trim()).filter(s => s)})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
                <p className="text-xs text-muted-foreground">{t('allowedToolsDesc', 'Comma-separated list of tools Claude is allowed to use.')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('blockedTools', 'Blocked Tools')}</label>
                <input type="text" placeholder="Bash,Edit (comma separated)" value={(editingProvider.blocked_tools || []).join(', ')} onChange={e => setEditingProvider({...editingProvider, blocked_tools: e.target.value.split(',').map(s => s.trim()).filter(s => s)})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
                <p className="text-xs text-muted-foreground">{t('blockedToolsDesc', 'Comma-separated list of tools Claude is NOT allowed to use.')}</p>
              </div>
              
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('maxSessionTurns', 'Max Session Turns')}</label>
                <input type="number" placeholder="100" value={editingProvider.max_session_turns || ''} onChange={e => setEditingProvider({...editingProvider, max_session_turns: e.target.value ? parseInt(e.target.value) : undefined})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
                <p className="text-xs text-muted-foreground">{t('maxSessionTurnsDesc', 'Maximum number of conversation turns per session.')}</p>
              </div>
            </div>
          )}

          {showingProviderDetails && activeTool === 'opencode' && (
            <>
              <div className="space-y-4 max-w-4xl">
                <div className="flex items-center gap-2 border-b pb-2">
                  <Brain className="w-4 h-4 text-primary" />
                  <h3 className="font-semibold">{t('globalConfig', 'Global Configuration')}</h3>
                </div>
                
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('defaultModel', 'Default Model')}</label>
                  <input type="text" placeholder="anthropic/claude-3-7-sonnet-20250219" value={editingProvider.opencode_default_model || ''} onChange={e => setEditingProvider({...editingProvider, opencode_default_model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                  <p className="text-xs text-muted-foreground">{t('defaultModelDesc', 'Default model for all OpenCode sessions.')}</p>
                </div>
                
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('defaultAgent', 'Default Agent')}</label>
                  <input type="text" placeholder="coder" value={editingProvider.opencode_default_agent || ''} onChange={e => setEditingProvider({...editingProvider, opencode_default_agent: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                  <p className="text-xs text-muted-foreground">{t('defaultAgentDesc', 'Default agent type (e.g., coder, architect, reviewer).')}</p>
                </div>
                
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('sessionsDir', 'Sessions Directory')}</label>
                  <input type="text" placeholder=".opencode/sessions" value={editingProvider.opencode_sessions_dir || ''} onChange={e => setEditingProvider({...editingProvider, opencode_sessions_dir: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                  <p className="text-xs text-muted-foreground">{t('sessionsDirDesc', 'Directory to store session history.')}</p>
                </div>
              </div>
            
              <div className="space-y-4 max-w-4xl">
                <div className="flex items-center gap-2 border-b pb-2">
                  <Settings className="w-4 h-4 text-primary" />
                  <h3 className="font-semibold">{t('advancedConfig', 'Advanced Configuration')}</h3>
                </div>
                
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('smallModel')}</label>
                  <input
                    type="text"
                    placeholder={t('smallModelPlaceholder')}
                    value={editingProvider.small_model || ''}
                    onChange={e => setEditingProvider({...editingProvider, small_model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                  <p className="text-xs text-muted-foreground">{t('smallModelDesc')}</p>
                </div>
                
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('requestTimeout')}</label>
                  <input
                    type="number"
                    placeholder="60000"
                    value={editingProvider.timeout || ''}
                    onChange={e => setEditingProvider({...editingProvider, timeout: e.target.value ? parseInt(e.target.value) : undefined})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                  <p className="text-xs text-muted-foreground">{t('requestTimeoutDesc')}</p>
                </div>
                
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">{t('shareMode')}</label>
                  <select
                    value={editingProvider.share_mode || ''}
                    onChange={e => setEditingProvider({...editingProvider, share_mode: e.target.value || undefined})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  >
                    <option value="">{t('shareModeManual')}</option>
                    <option value="manual">{t('shareModeManual')}</option>
                    <option value="auto">{t('shareModeAuto')}</option>
                    <option value="disabled">{t('shareModeDisabled')}</option>
                  </select>
                  <p className="text-xs text-muted-foreground">{t('shareModeDesc')}</p>
                </div>
              </div>
            
              <div className="space-y-4 max-w-4xl">
                <div className="flex items-center justify-between border-b pb-2">
                  <div className="flex items-center gap-2">
                    <Code2 className="w-4 h-4 text-primary" />
                    <h3 className="font-semibold">{t('jsonConfig')}</h3>
                  </div>
                  <div className="flex items-center gap-2">
                  <div className="relative" ref={historyRef}>
                    <button onClick={() => setShowHistory(!showHistory)} className="text-xs flex items-center gap-1 px-2 py-1 bg-secondary hover:bg-secondary/80 rounded transition-colors">
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
                  <button onClick={handleFormatJson} className="text-xs flex items-center gap-1 px-2 py-1 bg-secondary hover:bg-secondary/80 rounded transition-colors">
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
              
              <div className={`border rounded-md bg-white overflow-hidden font-mono text-sm shadow-inner transition-colors ${isRollbackMode ? 'ring-2 ring-amber-500 border-amber-500' : ''}`}>
                <Editor value={rawJson} onValueChange={code => {
                  setRawJson(code);
                  if (isRollbackMode) setIsRollbackMode(false);
                }} highlight={code => highlight(code, languages.json, 'json')} padding={16}
                  style={{ fontFamily: '"Fira code", "Fira Mono", monospace', minHeight: '200px', backgroundColor: 'white', color: '#1a1a1a' }}
                  className="focus:outline-none"
                />
              </div>
               <p className="text-xs text-muted-foreground">{t('jsonEditHint')}</p>
               </div>
             </>
           )}
         </div>
 
        <div className="p-4 border-t bg-muted/10 shrink-0 flex items-center justify-between">
          <div className="text-sm text-muted-foreground flex items-center gap-2">
          </div>
          {showingProviderDetails ? (
          <div className="flex items-center gap-3">
            {!isDefaultPreset && (
              <button onClick={handleDelete} className="px-4 py-2 text-sm border bg-background hover:bg-destructive/10 text-destructive rounded-md flex items-center gap-2 transition-colors">
                <Trash2 className="w-4 h-4" /> {activeTool === 'opencode' ? t('deleteProvider') : t('deletePreset')}
              </button>
            )}
            <button onClick={handleSavePreset} disabled={loading || !hasChanges} className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md flex items-center gap-2 transition-colors disabled:opacity-50">
              <Save className="w-4 h-4" /> {t('save')}
            </button>
            {activeTool !== 'opencode' && !isCurrentProviderActive && (
              <button 
                onClick={handleApply} 
                disabled={
                  loading ||
                  !editingProvider.api_key ||
                  (isManagedTool(activeTool) && editingProvider.env_managed === false)
                } 
                className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors shadow-sm"
              >
                <Play className="w-4 h-4" /> {isManagedTool(activeTool) && editingProvider.env_managed === false ? t('envManagedDisabledButton') : t('applyToCli')}
              </button>
            )}
          </div>
          ) : (
          <div className="text-xs text-muted-foreground">
            {t('selectEnvironmentToEdit', 'Click an environment from the left list to load and edit its configuration details.')}
          </div>
          )}
        </div>
      </div>
    </div>
  </div>
);
}
