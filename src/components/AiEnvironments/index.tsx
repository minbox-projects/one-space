import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Plus, Save, Play, Trash2, CheckCircle2, ShieldAlert, KeyRound, Globe, Zap, Brain, Sparkles, Box, TerminalSquare, Code2, Eraser, History, RotateCcw, X } from 'lucide-react';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './icons';
import Editor from 'react-simple-code-editor';
import { highlight, languages } from 'prismjs';
import 'prismjs/components/prism-json';
import 'prismjs/themes/prism-tomorrow.css';

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
  claude_reasoning_model?: string;
  claude_haiku_model?: string;
  claude_sonnet_model?: string;
  claude_opus_model?: string;
  dangerously_skip_permissions?: boolean;
  is_enabled?: boolean;
  provider_key?: string;
  history?: HistoryEntry[];
  [key: string]: any; // Allow extra fields from JSON
}

export interface AiProvidersState {
  active_claude: string | null;
  active_codex: string | null;
  active_gemini: string | null;
  active_opencode: string | null;
  providers: AiProvider[];
}

const DEFAULT_STATE: AiProvidersState = {
  active_claude: null,
  active_codex: null,
  active_gemini: null,
  active_opencode: null,
  providers: []
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

export function AiEnvironments() {
  const { t } = useTranslation();
  const [state, setState] = useState<AiProvidersState>(DEFAULT_STATE);
  const [activeTool, setActiveTool] = useState('claude');
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  
  const [editingProvider, setEditingProvider] = useState<Partial<AiProvider>>({});
  const [originalProvider, setOriginalProvider] = useState<Partial<AiProvider>>({});
  const [rawJson, setRawJson] = useState('');
  const [originalJson, setOriginalJson] = useState('');
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  const [showHistory, setShowHistory] = useState(false);
  const [isRollbackMode, setIsRollbackMode] = useState(false);
  const historyRef = useRef<HTMLDivElement>(null);

  const isTauri = '__TAURI_INTERNALS__' in window;

  const getOpenCodeJson = (provider: Partial<AiProvider>) => {
    const internalFields = [
      'id', 'tool', 'is_enabled', 'provider_key', 'api_key', 'base_url', 'model',
      'claude_reasoning_model', 'claude_haiku_model', 'claude_sonnet_model', 
      'claude_opus_model', 'dangerously_skip_permissions', 'history'
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
      return rawJson !== originalJson || editingProvider.name !== originalProvider.name || editingProvider.provider_key !== originalProvider.provider_key;
    }
    
    // For other tools, compare essential fields
    return JSON.stringify(editingProvider) !== JSON.stringify(originalProvider);
  })();

  const loadProviders = async () => {
    if (!isTauri) return;
    try {
      const res: AiProvidersState = await invoke('get_ai_providers');
      if (res.providers.length === 0) {
        setState(DEFAULT_STATE);
        await invoke('save_ai_providers', { state: DEFAULT_STATE });
      } else {
        setState(res);
      }
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    loadProviders();
  }, []);

  useEffect(() => {
    let activeId = null;
    if (activeTool === 'claude') activeId = state.active_claude;
    if (activeTool === 'codex') activeId = state.active_codex;
    if (activeTool === 'gemini') activeId = state.active_gemini;
    if (activeTool === 'opencode') activeId = state.active_opencode;

    // Only auto-select if no selection exists OR current selection tool doesn't match activeTool
    const current = state.providers.find(p => p.id === currentProviderId);
    if (!currentProviderId || (current && current.tool !== activeTool)) {
      if (!activeId) {
        const first = state.providers.find(p => p.tool === activeTool);
        if (first) activeId = first.id;
      }
      setCurrentProviderId(activeId);
    }
  }, [activeTool, state.active_claude, state.active_codex, state.active_gemini, state.active_opencode]);

  useEffect(() => {
    if (currentProviderId) {
      const p = state.providers.find(p => p.id === currentProviderId);
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
    }
  }, [currentProviderId, state.providers]);

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

    const isNew = !state.providers.some(p => p.id === editingProvider.id);
    let newProviders = [...state.providers];
    let newId = editingProvider.id || `custom-${Date.now()}`;
    
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
          ...parsed
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
      history: currentHistory,
    };

    if (isNew) {
      newProviders.push(finalProvider);
    } else {
      newProviders = newProviders.map(p => p.id === newId ? finalProvider : p);
    }

    const newState = { ...state, providers: newProviders };

    if (activeTool === 'claude') newState.active_claude = newId;
    if (activeTool === 'codex') newState.active_codex = newId;
    if (activeTool === 'gemini') newState.active_gemini = newId;
    if (activeTool === 'opencode') newState.active_opencode = newId;

    try {
      setLoading(true);
      await invoke('save_ai_providers', { state: newState });
      setState(newState);
      setCurrentProviderId(newId);
      
      // Update originals to disable save button after success
      setOriginalProvider(finalProvider);
      setIsRollbackMode(false);
      if (activeTool === 'opencode') {
        setOriginalJson(rawJson);
        await invoke('apply_ai_environment', { provider: finalProvider });
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
    await handleSavePreset();
    if (activeTool === 'opencode') return; // OpenCode is handled in handleSavePreset

    try {
      setLoading(true);
      setMessage({ type: '', text: '' });
      const targetProvider = state.providers.find(p => p.id === currentProviderId) || editingProvider;
      await invoke('apply_ai_environment', { provider: targetProvider });
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment applied successfully to CLI!') });
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
      setMessage({ type: 'error', text: 'Failed to parse history entry' });
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

    if (toolName === 'claude') newState.active_claude = newId;
    if (toolName === 'codex') newState.active_codex = newId;
    if (toolName === 'gemini') newState.active_gemini = newId;
    if (toolName === 'opencode') newState.active_opencode = newId;
    
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
    
    if (!window.confirm(confirmMsg)) return;
    
    const newProviders = state.providers.filter(p => p.id !== currentProviderId);
    const newState = { ...state, providers: newProviders };
    if (activeTool === 'claude' && state.active_claude === currentProviderId) newState.active_claude = null;
    if (activeTool === 'codex' && state.active_codex === currentProviderId) newState.active_codex = null;
    if (activeTool === 'gemini' && state.active_gemini === currentProviderId) newState.active_gemini = null;
    if (activeTool === 'opencode' && state.active_opencode === currentProviderId) newState.active_opencode = null;
    try {
      await invoke('save_ai_providers', { state: newState });
      setState(newState);
    } catch (e) {
      console.error(e);
    }
  };

  const isDefaultPreset = currentProviderId?.startsWith('default-');

  return (
    <div className="flex h-full border rounded-xl overflow-hidden bg-background">
      <div className="w-64 border-r flex flex-col shrink-0 bg-muted/20">
        <div className="p-4 border-b flex items-center justify-between bg-card shrink-0">
          <h2 className="font-semibold">{t('environments', 'Environments')}</h2>
          <div className="relative group">
            <button className="p-1.5 hover:bg-muted rounded-md transition-colors text-muted-foreground">
              <Plus className="w-4 h-4" />
            </button>
            <div className="absolute left-0 top-full mt-1 w-40 bg-popover border shadow-md rounded-md py-1 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-opacity z-10">
              {['claude', 'codex', 'gemini', 'opencode'].map(toolName => (
                <button key={toolName} onClick={() => handleAddCustom(toolName)} className="w-full text-left px-3 py-1.5 text-sm hover:bg-muted capitalize">
                  {toolName === 'opencode' ? t('opencodeProvider', 'OpenCode Provider') : toolName}
                </button>
              ))}
            </div>
          </div>
        </div>
        
        <div className="flex-1 overflow-y-auto p-2 space-y-4">
          {['claude', 'codex', 'gemini', 'opencode'].map(tool => {
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
                    <div className={`w-2 h-2 rounded-full shrink-0 ${tool === 'opencode' ? (p.is_enabled ? 'bg-green-500' : 'bg-amber-500') : (activeId === p.id ? 'bg-green-500' : 'bg-transparent border border-muted-foreground/30')}`} />
                    <span className="truncate flex-1 text-left">{p.name}</span>
                  </button>
                ))}
              </div>
            );
          })}
        </div>
      </div>

      <div className="flex-1 flex flex-col h-full bg-card overflow-hidden">
        <div className="p-4 md:p-6 border-b shrink-0 flex items-center justify-between">
          <div>
            <h2 className="text-xl font-bold tracking-tight">{t('providerDetails', 'Provider Details')}</h2>
            <p className="text-sm text-muted-foreground">{t('configureAiEndpoint', 'Configure authentication and model routing for this CLI engine.')}</p>
          </div>
          {message.text && (
            <div className={`px-3 py-1.5 rounded-md text-sm flex items-center gap-2 animate-in fade-in slide-in-from-top-2 ${message.type === 'error' ? 'bg-destructive/10 text-destructive' : 'bg-primary/10 text-primary'}`}>
              {message.type === 'success' && <CheckCircle2 className="w-4 h-4" />}
              {message.text}
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto p-4 md:p-6 space-y-8">
          {activeTool === 'opencode' && (
            <div className="max-w-2xl bg-muted/30 p-4 rounded-lg border flex items-center justify-between">
              <div className="space-y-0.5">
                <div className="font-semibold flex items-center gap-2">
                  {editingProvider.is_enabled ? (
                    <span className="flex items-center gap-1.5 text-green-600 dark:text-green-500">
                      <CheckCircle2 className="w-4 h-4" /> {t('enabledInOpenCode', 'Enabled (Synced to OpenCode CLI)')}
                    </span>
                  ) : (
                    <span className="flex items-center gap-1.5 text-amber-600 dark:text-amber-500">
                      <Box className="w-4 h-4" /> {t('pausedInOneSpace', 'Paused (Stored in OneSpace Only)')}
                    </span>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">
                  {editingProvider.is_enabled ? t('enabledDesc', "This provider's configuration is currently active in your opencode.json file.") : t('pausedDesc', "This provider is safely stored in OneSpace but won't be seen by the OpenCode CLI tool.")}
                </p>
              </div>
              <button onClick={() => setEditingProvider({...editingProvider, is_enabled: !editingProvider.is_enabled})}
                className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${editingProvider.is_enabled ? 'bg-amber-100 text-amber-700 hover:bg-amber-200 border border-amber-200' : 'bg-green-100 text-green-700 hover:bg-green-200 border border-green-200'}`}
              >
                {editingProvider.is_enabled ? t('pauseCliSync', "Pause CLI Sync") : t('enableCliSync', "Enable CLI Sync")}
              </button>
            </div>
          )}

          <div className="space-y-4 max-w-2xl">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{activeTool === 'opencode' ? t('providerName', 'Provider Name') : t('presetName', 'Preset Name')}</label>
                <input type="text" value={editingProvider.name || ''} onChange={e => setEditingProvider({...editingProvider, name: e.target.value})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{activeTool === 'opencode' ? t('providerIdentifier', 'Provider Identifier') : t('targetCliTool', 'Target CLI Tool')}</label>
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

          {activeTool !== 'opencode' && (
            <div className="space-y-4 max-w-2xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <KeyRound className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{t('authAndEndpoint', 'Authentication & Endpoint')}</h3>
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('apiKey', 'API Key')}</label>
                <input type="password" placeholder="sk-..." value={editingProvider.api_key || ''} onChange={e => setEditingProvider({...editingProvider, api_key: e.target.value})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 font-mono"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('baseUrl', 'Base URL (Custom Endpoint)')}</label>
                <div className="relative">
                  <Globe className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                  <input type="url" placeholder="https://api.your-proxy.com" value={editingProvider.base_url || ''} onChange={e => setEditingProvider({...editingProvider, base_url: e.target.value})}
                    className="w-full bg-background border rounded-md pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
              </div>
            </div>
          )}

          {activeTool !== 'opencode' && (
            <div className="space-y-4 max-w-2xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <Box className="w-4 h-4 text-primary" />
                <h3 className="font-semibold">{activeTool === 'claude' ? t('modelRouting', 'Model Routing (Claude Specific)') : t('modelConfig', 'Model Configuration')}</h3>
              </div>
              {activeTool === 'claude' ? (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {[
                    { label: t('defaultModel', 'Default Model (Sonnet)'), icon: Brain, key: 'claude_sonnet_model', placeholder: 'claude-3-7-sonnet-20250219' },
                    { label: t('fastModel', 'Fast Model (Haiku)'), icon: Zap, key: 'claude_haiku_model', placeholder: 'claude-3-5-haiku-20241022' },
                    { label: t('powerfulModel', 'Powerful Model (Opus)'), icon: Sparkles, key: 'claude_opus_model', placeholder: 'claude-3-opus-20240229' },
                    { label: t('thinkingModel', 'Thinking Model (Reasoning)'), icon: Brain, key: 'claude_reasoning_model', placeholder: 'claude-3-7-sonnet-20250219' }
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
                  <label className="text-sm font-medium text-foreground">{t('primaryModel', 'Primary Model')}</label>
                  <input type="text" placeholder={activeTool === 'gemini' ? "gemini-2.5-flash" : "gpt-4o"} value={editingProvider.model || ''} onChange={e => setEditingProvider({...editingProvider, model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
              )}
            </div>
          )}

          {activeTool === 'claude' && (
            <div className="space-y-4 max-w-2xl">
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
            </div>
          )}

          {activeTool === 'opencode' && (
            <div className="space-y-4 max-w-2xl">
              <div className="flex items-center justify-between border-b pb-2">
                <div className="flex items-center gap-2">
                  <Code2 className="w-4 h-4 text-primary" />
                  <h3 className="font-semibold">{t('jsonConfig', 'JSON Configuration')}</h3>
                </div>
                <div className="flex items-center gap-2">
                  <div className="relative" ref={historyRef}>
                    <button onClick={() => setShowHistory(!showHistory)} className="text-xs flex items-center gap-1 px-2 py-1 bg-secondary hover:bg-secondary/80 rounded transition-colors">
                      <History className="w-3 h-3" /> {t('aiHistory', 'History')}
                    </button>
                    
                    {showHistory && (
                      <div className="absolute right-0 top-full mt-2 w-80 max-h-96 bg-popover border shadow-xl rounded-lg overflow-hidden z-50 animate-in fade-in slide-in-from-top-2">
                        <div className="p-3 border-b flex items-center justify-between bg-muted/30">
                          <span className="text-xs font-bold uppercase tracking-wider">{t('aiHistory', 'History')}</span>
                          <button onClick={() => setShowHistory(false)}><X className="w-4 h-4" /></button>
                        </div>
                        <div className="overflow-y-auto max-h-[300px] p-1">
                          {(!editingProvider.history || editingProvider.history.length === 0) ? (
                            <div className="p-8 text-center text-xs text-muted-foreground">{t('noHistory', 'No history records.')}</div>
                          ) : (
                            editingProvider.history.map((entry, i) => (
                              <div key={i} className="p-2 hover:bg-muted/50 rounded-md border border-transparent hover:border-border transition-all mb-1 group">
                                <div className="flex items-center justify-between mb-1">
                                  <span className="text-[10px] font-mono text-muted-foreground">{new Date(entry.timestamp).toLocaleString()}</span>
                                  <button onClick={() => handleRollback(entry)} className="text-[10px] text-primary hover:underline flex items-center gap-1">
                                    <RotateCcw className="w-2.5 h-2.5" /> {t('rollback', 'Restore')}
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
                    <Eraser className="w-3 h-3" /> {t('format', 'Format')}
                  </button>
                </div>
              </div>
              
              {isRollbackMode && (
                <div className="bg-amber-50 border border-amber-200 p-3 rounded-md flex items-start gap-3 animate-in fade-in slide-in-from-top-1">
                  <RotateCcw className="w-4 h-4 text-amber-600 mt-0.5" />
                  <div className="space-y-1">
                    <p className="text-sm font-semibold text-amber-800">{t('rollbackModeTitle', 'History Version Loaded')}</p>
                    <p className="text-xs text-amber-700">{t('rollbackModeDesc', 'You are currently viewing a historical version.')}</p>
                  </div>
                  <button 
                    onClick={() => {
                      setRawJson(originalJson);
                      setIsRollbackMode(false);
                    }}
                    className="ml-auto text-xs font-medium text-amber-800 hover:underline"
                  >
                    {t('cancel', 'Cancel')}
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
              <p className="text-xs text-muted-foreground">{t('jsonEditHint', 'Manual JSON edits will overwrite form fields above upon saving.')}</p>
            </div>
          )}
        </div>

        <div className="p-4 border-t bg-muted/10 shrink-0 flex items-center justify-between">
          <div className="text-sm text-muted-foreground flex items-center gap-2">
            {activeTool !== 'opencode' && state[`active_${activeTool}` as keyof AiProvidersState] === currentProviderId && (
              <span className="flex items-center gap-1 text-green-600 dark:text-green-500 font-medium bg-green-500/10 px-2 py-1 rounded">
                <CheckCircle2 className="w-4 h-4" /> {t('currentlyActive', 'Currently Active')}
              </span>
            )}
          </div>
          <div className="flex items-center gap-3">
            {!isDefaultPreset && (
              <button onClick={handleDelete} className="px-4 py-2 text-sm border bg-background hover:bg-destructive/10 text-destructive rounded-md flex items-center gap-2 transition-colors">
                <Trash2 className="w-4 h-4" /> {activeTool === 'opencode' ? t('deleteProvider', 'Delete Provider') : t('deletePreset', 'Delete Preset')}
              </button>
            )}
            <button onClick={handleSavePreset} disabled={loading || !hasChanges} className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md flex items-center gap-2 transition-colors disabled:opacity-50">
              <Save className="w-4 h-4" /> {t('save', 'Save')}
            </button>
            {activeTool !== 'opencode' && (
              <button onClick={handleApply} disabled={loading || !editingProvider.api_key} className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors shadow-sm">
                <Play className="w-4 h-4" /> {t('applyToCli', 'Apply to CLI')}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
