import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Plus, Save, Play, Trash2, CheckCircle2 } from 'lucide-react';

export interface AiProvider {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
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
  providers: [
    {
      id: 'default-claude',
      name: 'Anthropic Official',
      tool: 'claude',
      api_key: '',
    },
    {
      id: 'default-codex',
      name: 'OpenAI Official',
      tool: 'codex',
      api_key: '',
    },
    {
      id: 'default-gemini',
      name: 'Google Official',
      tool: 'gemini',
      api_key: '',
    },
    {
      id: 'default-opencode',
      name: 'OpenCode Official',
      tool: 'opencode',
      api_key: '',
    }
  ]
};

export function AiEnvironmentsTab() {
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

  // Update current provider when tool changes or state loads
  useEffect(() => {
    let activeId = null;
    if (activeTool === 'claude') activeId = state.active_claude;
    if (activeTool === 'codex') activeId = state.active_codex;
    if (activeTool === 'gemini') activeId = state.active_gemini;
    if (activeTool === 'opencode') activeId = state.active_opencode;

    // If no active provider set, fallback to the first one for this tool
    if (!activeId) {
      const first = state.providers.find(p => p.tool === activeTool);
      if (first) activeId = first.id;
    }

    setCurrentProviderId(activeId);
  }, [activeTool, state]);

  // Sync editing form when provider selection changes
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
      model: editingProvider.model || ''
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

    // Update active state if we're saving the currently selected one
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
    // Save first to make sure it's up to date
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

  const handleAddCustom = () => {
    setCurrentProviderId(`custom-${Date.now()}`);
    setEditingProvider({
      id: `custom-${Date.now()}`,
      name: `New ${activeTool} Preset`,
      tool: activeTool,
      api_key: '',
      base_url: '',
      model: ''
    });
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

  const toolProviders = state.providers.filter(p => p.tool === activeTool);
  const isDefaultPreset = currentProviderId?.startsWith('default-');

  return (
    <div className="flex flex-col h-full space-y-4">
      <div>
        <h3 className="font-semibold text-lg">{t('aiEnvironments', 'AI Environments')}</h3>
        <p className="text-sm text-muted-foreground mt-1">
          {t('aiEnvironmentsDesc', 'Manage API keys, endpoints, and models for your CLI tools.')}
        </p>
      </div>

      {message.text && (
        <div className={`p-3 rounded-md text-sm flex items-center gap-2 ${message.type === 'error' ? 'bg-destructive/10 text-destructive border border-destructive/20' : 'bg-primary/10 text-primary border border-primary/20'}`}>
          {message.type === 'success' && <CheckCircle2 className="w-4 h-4" />}
          {message.text}
        </div>
      )}

      {/* Tool Selector */}
      <div className="flex p-1 bg-muted/50 rounded-lg w-full max-w-md">
        {['claude', 'codex', 'gemini', 'opencode'].map(tool => (
          <button
            key={tool}
            onClick={() => setActiveTool(tool)}
            className={`flex-1 py-1.5 text-sm font-medium rounded-md capitalize transition-colors ${
              activeTool === tool 
                ? 'bg-background shadow-sm text-foreground' 
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            {tool}
          </button>
        ))}
      </div>

      {/* Preset Selector */}
      <div className="space-y-2">
        <label className="text-sm font-medium text-muted-foreground">{t('activePreset', 'Active Preset')}</label>
        <div className="flex gap-2">
          <select 
            value={currentProviderId || ''}
            onChange={(e) => setCurrentProviderId(e.target.value)}
            className="flex-1 bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            {toolProviders.map(p => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
          <button
            onClick={handleAddCustom}
            className="px-3 py-2 border bg-background hover:bg-muted rounded-md text-sm transition-colors"
            title={t('newPreset', 'New Preset')}
          >
            <Plus className="w-4 h-4" />
          </button>
          {!isDefaultPreset && (
            <button
              onClick={handleDelete}
              className="px-3 py-2 border bg-background hover:bg-destructive/10 text-destructive rounded-md text-sm transition-colors"
              title={t('deletePreset', 'Delete Preset')}
            >
              <Trash2 className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>

      <div className="space-y-4 pt-4 border-t border-border/50">
        <div className="space-y-2">
          <label className="text-sm font-medium text-muted-foreground">{t('presetName', 'Preset Name')}</label>
          <input 
            type="text" 
            value={editingProvider.name || ''}
            onChange={e => setEditingProvider({...editingProvider, name: e.target.value})}
            className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium text-muted-foreground">{t('apiKey', 'API Key')}</label>
          <input 
            type="password" 
            placeholder={`sk-...`}
            value={editingProvider.api_key || ''}
            onChange={e => setEditingProvider({...editingProvider, api_key: e.target.value})}
            className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 font-mono"
          />
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium text-muted-foreground">{t('baseUrl', 'Base URL (Custom Endpoint)')}</label>
          <input 
            type="url" 
            placeholder="e.g. https://api.anthropic.com"
            value={editingProvider.base_url || ''}
            onChange={e => setEditingProvider({...editingProvider, base_url: e.target.value})}
            className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
        </div>

        {(activeTool === 'codex' || activeTool === 'gemini' || activeTool === 'opencode') && (
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">{t('primaryModel', 'Primary Model')}</label>
            <input 
              type="text" 
              placeholder={activeTool === 'gemini' ? "e.g. gemini-2.5-flash" : "e.g. gpt-4o"}
              value={editingProvider.model || ''}
              onChange={e => setEditingProvider({...editingProvider, model: e.target.value})}
              className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        )}
      </div>

      <div className="flex-1" />

      <div className="flex justify-end gap-3 pt-4 border-t border-border/50 mt-auto">
        <button 
          onClick={handleSavePreset}
          disabled={loading}
          className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md flex items-center gap-2 transition-colors disabled:opacity-50"
        >
          <Save className="w-4 h-4" />
          {t('save', 'Save Preset')}
        </button>
        <button 
          onClick={handleApply}
          disabled={loading || !editingProvider.api_key}
          className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors"
        >
          <Play className="w-4 h-4" />
          {t('applyToCli', 'Apply to CLI')}
        </button>
      </div>
    </div>
  );
}
