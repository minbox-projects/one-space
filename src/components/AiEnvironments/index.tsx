import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Plus, Save, Play, Trash2, CheckCircle2, Cpu, ShieldAlert, KeyRound, Globe, Zap, Brain, Sparkles, Box } from 'lucide-react';

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

export function AiEnvironments() {
  const { t } = useTranslation();
  const [state, setState] = useState<AiProvidersState>(DEFAULT_STATE);
  const [activeTool, setActiveTool] = useState('claude');
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  
  const [editingProvider, setEditingProvider] = useState<Partial<AiProvider>>({});
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  
  const isTauri = '__TAURI_INTERNALS__' in window;

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

  // Set current provider ID when tool or state changes
  useEffect(() => {
    let activeId = null;
    if (activeTool === 'claude') activeId = state.active_claude;
    if (activeTool === 'codex') activeId = state.active_codex;
    if (activeTool === 'gemini') activeId = state.active_gemini;
    if (activeTool === 'opencode') activeId = state.active_opencode;

    if (!activeId) {
      const first = state.providers.find(p => p.tool === activeTool);
      if (first) activeId = first.id;
    }

    setCurrentProviderId(activeId);
  }, [activeTool, state]);

  // Sync editing form
  useEffect(() => {
    if (currentProviderId) {
      const p = state.providers.find(p => p.id === currentProviderId);
      if (p) {
        setEditingProvider(p);
      } else {
        setEditingProvider({ name: '', api_key: '', base_url: '', model: '' });
      }
    }
  }, [currentProviderId, state.providers]);

  const handleSavePreset = async () => {
    if (!editingProvider.name) {
      setMessage({ type: 'error', text: t('providePresetName', 'Please provide a preset name') });
      return;
    }

    const isNew = !state.providers.some(p => p.id === editingProvider.id);
    let newProviders = [...state.providers];
    let newId = editingProvider.id || `custom-${Date.now()}`;
    
    const finalProvider: AiProvider = {
      id: newId,
      name: editingProvider.name || 'Unnamed',
      tool: activeTool,
      api_key: editingProvider.api_key || '',
      base_url: editingProvider.base_url || '',
      model: editingProvider.model || '',
      claude_reasoning_model: editingProvider.claude_reasoning_model || '',
      claude_haiku_model: editingProvider.claude_haiku_model || '',
      claude_sonnet_model: editingProvider.claude_sonnet_model || '',
      claude_opus_model: editingProvider.claude_opus_model || '',
      dangerously_skip_permissions: editingProvider.dangerously_skip_permissions || false,
    };

    if (isNew) {
      newProviders.push(finalProvider);
    } else {
      newProviders = newProviders.map(p => p.id === newId ? finalProvider : p);
    }

    const newState = {
      ...state,
      providers: newProviders
    };

    if (activeTool === 'claude') newState.active_claude = newId;
    if (activeTool === 'codex') newState.active_codex = newId;
    if (activeTool === 'gemini') newState.active_gemini = newId;
    if (activeTool === 'opencode') newState.active_opencode = newId;

    try {
      setLoading(true);
      await invoke('save_ai_providers', { state: newState });
      setState(newState);
      setCurrentProviderId(newId);
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
    try {
      setLoading(true);
      setMessage({ type: '', text: '' });
      await invoke('apply_ai_environment', { provider: editingProvider });
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment applied successfully to CLI!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleAddCustom = (tool: string) => {
    setActiveTool(tool);
    const newId = `custom-${Date.now()}`;
    const newProvider: AiProvider = {
      id: newId,
      name: `New ${tool} Preset`,
      tool: tool,
      api_key: '',
      base_url: '',
      model: ''
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
      {/* Left Sidebar - Providers List */}
      <div className="w-64 border-r flex flex-col shrink-0 bg-muted/20">
        <div className="p-4 border-b flex items-center justify-between bg-card shrink-0">
          <h2 className="font-semibold">{t('environments', 'Environments')}</h2>
          <div className="relative group">
            <button className="p-1.5 hover:bg-muted rounded-md transition-colors text-muted-foreground">
              <Plus className="w-4 h-4" />
            </button>
            <div className="absolute left-0 top-full mt-1 w-32 bg-popover border shadow-md rounded-md py-1 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-opacity z-10">
              {['claude', 'codex', 'gemini', 'opencode'].map(t => (
                <button key={t} onClick={() => handleAddCustom(t)} className="w-full text-left px-3 py-1.5 text-sm hover:bg-muted capitalize">
                  {t}
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
                  <Cpu className="w-3.5 h-3.5" />
                  {tool} ({toolProviders.length})
                </div>
                {toolProviders.map(p => (
                  <button
                    key={p.id}
                    onClick={() => {
                      setActiveTool(tool);
                      setCurrentProviderId(p.id);
                    }}
                    className={`w-full flex items-center gap-2 px-3 py-2 text-sm rounded-md transition-colors ${
                      currentProviderId === p.id 
                        ? 'bg-primary/10 text-primary font-medium' 
                        : 'hover:bg-muted text-foreground'
                    }`}
                  >
                    <div className={`w-2 h-2 rounded-full shrink-0 ${activeId === p.id ? 'bg-green-500' : 'bg-transparent border border-muted-foreground/30'}`} />
                    <span className="truncate flex-1 text-left">{p.name}</span>
                  </button>
                ))}
              </div>
            );
          })}
        </div>
      </div>

      {/* Right Content - Provider Details Form */}
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
          {/* General Section */}
          <div className="space-y-4 max-w-2xl">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('presetName', 'Preset Name')}</label>
                <input 
                  type="text" 
                  value={editingProvider.name || ''}
                  onChange={e => setEditingProvider({...editingProvider, name: e.target.value})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('targetCliTool', 'Target CLI Tool')}</label>
                <div className="w-full bg-muted/50 border rounded-md px-3 py-2 text-sm text-muted-foreground capitalize cursor-not-allowed">
                  {editingProvider.tool || activeTool}
                </div>
              </div>
            </div>
          </div>

          {/* Authentication & Endpoint Section */}
          <div className="space-y-4 max-w-2xl">
            <div className="flex items-center gap-2 border-b pb-2">
              <KeyRound className="w-4 h-4 text-primary" />
              <h3 className="font-semibold">{t('authAndEndpoint', 'Authentication & Endpoint')}</h3>
            </div>
            
            <div className="space-y-2">
              <label className="text-sm font-medium text-foreground">{t('apiKey', 'API Key')}</label>
              <input 
                type="password" 
                placeholder="sk-..."
                value={editingProvider.api_key || ''}
                onChange={e => setEditingProvider({...editingProvider, api_key: e.target.value})}
                className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 font-mono"
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium text-foreground">{t('baseUrl', 'Base URL (Custom Endpoint)')}</label>
              <div className="relative">
                <Globe className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                <input 
                  type="url" 
                  placeholder="https://api.your-proxy.com"
                  value={editingProvider.base_url || ''}
                  onChange={e => setEditingProvider({...editingProvider, base_url: e.target.value})}
                  className="w-full bg-background border rounded-md pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
              </div>
            </div>
          </div>

          {/* Model Configuration Section */}
          <div className="space-y-4 max-w-2xl">
            <div className="flex items-center gap-2 border-b pb-2">
              <Box className="w-4 h-4 text-primary" />
              <h3 className="font-semibold">{activeTool === 'claude' ? t('modelRouting', 'Model Routing (Claude Specific)') : t('modelConfig', 'Model Configuration')}</h3>
            </div>

            {activeTool === 'claude' ? (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground flex items-center gap-1.5"><Brain className="w-3.5 h-3.5"/> {t('defaultModel', 'Default Model (Sonnet)')}</label>
                  <input 
                    type="text" 
                    placeholder="claude-3-7-sonnet-20250219"
                    value={editingProvider.claude_sonnet_model || ''}
                    onChange={e => setEditingProvider({...editingProvider, claude_sonnet_model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground flex items-center gap-1.5"><Zap className="w-3.5 h-3.5"/> {t('fastModel', 'Fast Model (Haiku)')}</label>
                  <input 
                    type="text" 
                    placeholder="claude-3-5-haiku-20241022"
                    value={editingProvider.claude_haiku_model || ''}
                    onChange={e => setEditingProvider({...editingProvider, claude_haiku_model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground flex items-center gap-1.5"><Sparkles className="w-3.5 h-3.5"/> {t('powerfulModel', 'Powerful Model (Opus)')}</label>
                  <input 
                    type="text" 
                    placeholder="claude-3-opus-20240229"
                    value={editingProvider.claude_opus_model || ''}
                    onChange={e => setEditingProvider({...editingProvider, claude_opus_model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground flex items-center gap-1.5"><Brain className="w-3.5 h-3.5"/> {t('thinkingModel', 'Thinking Model (Reasoning)')}</label>
                  <input 
                    type="text" 
                    placeholder="claude-3-7-sonnet-20250219"
                    value={editingProvider.claude_reasoning_model || ''}
                    onChange={e => setEditingProvider({...editingProvider, claude_reasoning_model: e.target.value})}
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                  />
                </div>
              </div>
            ) : (
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">{t('primaryModel', 'Primary Model')}</label>
                <input 
                  type="text" 
                  placeholder={activeTool === 'gemini' ? "gemini-2.5-flash" : "gpt-4o"}
                  value={editingProvider.model || ''}
                  onChange={e => setEditingProvider({...editingProvider, model: e.target.value})}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
              </div>
            )}
          </div>

          {/* Advanced Section */}
          {activeTool === 'claude' && (
            <div className="space-y-4 max-w-2xl">
              <div className="flex items-center gap-2 border-b pb-2">
                <ShieldAlert className="w-4 h-4 text-destructive" />
                <h3 className="font-semibold text-destructive">{t('advancedOptions', 'Advanced Options')}</h3>
              </div>
              
              <div className="flex items-start gap-3 bg-destructive/5 p-4 rounded-md border border-destructive/20">
                <input
                  type="checkbox"
                  id="dangerouslySkipPermissions"
                  checked={editingProvider.dangerously_skip_permissions || false}
                  onChange={e => setEditingProvider({...editingProvider, dangerously_skip_permissions: e.target.checked})}
                  className="mt-1 shrink-0 cursor-pointer w-4 h-4 accent-destructive"
                />
                <div className="space-y-1">
                  <label htmlFor="dangerouslySkipPermissions" className="text-sm font-medium cursor-pointer flex items-center gap-2">
                    {t('dangerouslySkipPermissions', 'Dangerously Skip Permissions')}
                  </label>
                  <p className="text-xs text-muted-foreground">
                    {t('dangerouslySkipPermissionsDesc', 'Auto-approve all terminal commands executed by Claude Code (use with extreme caution).')}
                  </p>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Footer actions */}
        <div className="p-4 border-t bg-muted/10 shrink-0 flex items-center justify-between">
          <div className="text-sm text-muted-foreground flex items-center gap-2">
            {state[`active_${activeTool}` as keyof AiProvidersState] === currentProviderId && (
              <span className="flex items-center gap-1 text-green-600 dark:text-green-500 font-medium bg-green-500/10 px-2 py-1 rounded">
                <CheckCircle2 className="w-4 h-4" /> {t('currentlyActive', 'Currently Active')}
              </span>
            )}
          </div>
          <div className="flex items-center gap-3">
            {!isDefaultPreset && (
              <button 
                onClick={handleDelete}
                className="px-4 py-2 text-sm border bg-background hover:bg-destructive/10 text-destructive rounded-md flex items-center gap-2 transition-colors"
              >
                <Trash2 className="w-4 h-4" />
                {t('deletePreset', 'Delete Preset')}
              </button>
            )}
            <button 
              onClick={handleSavePreset}
              disabled={loading}
              className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md flex items-center gap-2 transition-colors disabled:opacity-50"
            >
              <Save className="w-4 h-4" />
              {t('save', 'Save')}
            </button>
            <button 
              onClick={handleApply}
              disabled={loading || !editingProvider.api_key}
              className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors shadow-sm"
            >
              <Play className="w-4 h-4" />
              {t('applyToCli', 'Apply to CLI')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}