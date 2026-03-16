import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { Terminal, Box, ChevronDown, ChevronUp, FolderOpen, Send } from 'lucide-react';
import { ToolIcon } from './AiEnvironments';
import { open } from '@tauri-apps/plugin-dialog';
import { workflowsLaunchPreset, workflowsListPresets, type WorkflowPreset } from '@/lib/workflows';

const QUICK_MODELS = [
  { id: 'claude', name: 'Claude Code', cmd: 'claude code' },
  { id: 'gemini', name: 'Gemini', cmd: 'gemini -y' },
  { id: 'codex', name: 'Codex', cmd: 'codex' },
  { id: 'opencode', name: 'OpenCode', cmd: 'opencode' }
] as const;

const QUICK_MODEL_IDS = new Set(QUICK_MODELS.map(m => m.id));

interface StorageConfig {
  default_ai_dir?: string;
  default_ai_model?: 'claude' | 'codex' | 'gemini' | 'opencode';
}

export function QuickAiSessionBar() {
  const { t } = useTranslation();
  const isTauri = '__TAURI_INTERNALS__' in window;
  const [model, setModel] = useState('claude');
  const [path, setPath] = useState('');
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [workflowPresets, setWorkflowPresets] = useState<WorkflowPreset[]>([]);
  const [selectedWorkflowPresetId, setSelectedWorkflowPresetId] = useState('');
  const launchingRef = useRef(false);

  const handleLaunch = useCallback(async ({ closeImmediately = false }: { closeImmediately?: boolean } = {}) => {
    if (launchingRef.current) return;

    launchingRef.current = true;
    if (closeImmediately) {
      invoke('hide_quick_ai_window').catch(err =>
        console.error('Hide quick-ai window failed:', err)
      );
    }
    setLoading(true);
    try {
      let targetPath = path.trim();
      if (!targetPath) {
        try {
          const cfg = await invoke<StorageConfig>('get_storage_config');
          targetPath = cfg.default_ai_dir?.trim() || '';
          if (targetPath) {
            setPath(targetPath);
          }
        } catch (e) {
          console.error(e);
        }
      }
      if (!targetPath) {
        targetPath = './';
      }

      if (selectedWorkflowPresetId) {
        await workflowsLaunchPreset({
          preset_id: selectedWorkflowPresetId,
          override_working_dir: targetPath || undefined,
        });
      } else {
        await invoke('sessions_create', {
          session: {
            name: '',
            working_dir: targetPath,
            tool: model,
            status: 'active'
          }
        });
      }
      
      // Emit events and clear state
      emit('refresh-counts').catch(console.error);
      emit('sessions-updated').catch(console.error);

      if (!closeImmediately) {
        // Delay hiding the window for slow-starting models (Gemini, Opencode)
        // to prevent users from accidentally triggering duplicate launches
        const shouldDelayHide = model === 'gemini' || model === 'opencode';
        const hideDelay = shouldDelayHide ? 2000 : 0;

        setTimeout(async () => {
          await invoke('hide_quick_ai_window').catch(err => console.error('Hide quick-ai window failed:', err));
        }, hideDelay);
      }
    } catch (e) {
      console.error('Failed to launch AI session:', e);
    } finally {
      launchingRef.current = false;
      setLoading(false);
    }
  }, [path, model, selectedWorkflowPresetId]);

  const applyQuickDefaults = useCallback(async () => {
    try {
      const cfg = await invoke<StorageConfig>('get_storage_config');
      if (cfg.default_ai_model && QUICK_MODEL_IDS.has(cfg.default_ai_model)) {
        setModel(cfg.default_ai_model);
      }
      setPath(cfg.default_ai_dir || '');
    } catch (e) {
      console.error(e);
    }
  }, []);

  const loadWorkflowPresets = useCallback(async () => {
    try {
      const resp = await workflowsListPresets();
      setWorkflowPresets(resp.data || []);
    } catch (e) {
      console.error('Failed to load workflow presets in quick bar', e);
    }
  }, []);

  useEffect(() => {
    // Load default model/path on initial open
    applyQuickDefaults();
    loadWorkflowPresets();
  }, [applyQuickDefaults, loadWorkflowPresets]);

  useEffect(() => {
    // Re-apply default model/path each time quick window becomes visible
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        applyQuickDefaults();
        loadWorkflowPresets();
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [applyQuickDefaults, loadWorkflowPresets]);

  useEffect(() => {
    // Global key listener
    const handleGlobalKeys = async (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const isEditableTarget = Boolean(
        target &&
        (target.closest('input,textarea,select,[contenteditable="true"]') ||
          target.isContentEditable)
      );
      if (isEditableTarget) return;
      if (e.key === 'Escape') {
        await invoke('hide_quick_ai_window').catch(() => {});
      } else if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
        if (!loading) {
          await handleLaunch({ closeImmediately: true });
        }
      }
    };
    window.addEventListener('keydown', handleGlobalKeys);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeys);
    };
  }, [loading, handleLaunch]);

  useEffect(() => {
    // Sync window size when expanded state changes
    const syncWindowSize = async () => {
      try {
        const height = expanded ? 260 : 70;
        await invoke('resize_window', { height });
      } catch (err) {
        console.error('Failed to resize window:', err);
      }
    };
    syncWindowSize();
  }, [expanded]);

  const handleSelectDir = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setPath(selected);
      }
    } catch (err: unknown) {
      console.error(err);
    }
  }, []);

  const handleSelectWorkflowPreset = (presetId: string) => {
    setSelectedWorkflowPresetId(presetId);
    if (!presetId) return;
    const preset = workflowPresets.find((item) => item.id === presetId);
    if (!preset) return;
    if (QUICK_MODEL_IDS.has(preset.tool)) {
      setModel(preset.tool);
    }
    if (preset.working_dir?.trim()) {
      setPath(preset.working_dir.trim());
    }
  };

  const handleDragMouseDown = (e: React.MouseEvent<HTMLElement>) => {
    const target = e.target as HTMLElement;
    if (target.closest('button,input,select,textarea,a,[role="button"],[data-no-drag]')) {
      return;
    }
    if (!isTauri) return;
    getCurrentWindow().startDragging().catch(() => {});
  };

  return (
    <div className="w-full h-full bg-background/95 backdrop-blur-xl border-none shadow-2xl rounded-xl flex flex-col overflow-hidden">
      <div className="flex items-center h-[70px] px-4 gap-3 bg-card/50" data-tauri-drag-region onMouseDown={handleDragMouseDown}>
        <div className="bg-primary/10 p-2 rounded-lg pointer-events-none">
          <Terminal className="w-6 h-6 text-primary" />
        </div>
        
        <div className="flex-1 min-w-0">
          <div className="text-lg font-medium truncate">
            {selectedWorkflowPresetId
              ? workflowPresets.find((preset) => preset.id === selectedWorkflowPresetId)?.name || t('workflowPreset', 'Workflow Preset')
              : t('quickSessionPlaceholder', 'Syncing title from history')}
          </div>
          <div className="text-xs text-muted-foreground truncate">
            {path || t('noPathSelected', 'Choose a directory...')}
          </div>
        </div>

        <div className="flex items-center gap-2 pl-4">
          <div className="relative flex items-center gap-2 bg-muted/50 rounded-md px-2 py-1.5 hover:bg-muted transition-colors cursor-pointer group">
            <ToolIcon tool={model} className="w-4 h-4" />
            <select 
              value={model}
              onChange={e => {
                setModel(e.target.value);
              }}
              className="bg-transparent text-sm font-medium pr-6 focus:ring-0 cursor-pointer appearance-none outline-none"
            >
              {QUICK_MODELS.map(m => (
                <option key={m.id} value={m.id}>{m.name}</option>
              ))}
            </select>
            <ChevronDown className="w-3.5 h-3.5 absolute right-2 pointer-events-none text-muted-foreground" />
          </div>

          <button 
            onClick={() => {
              setExpanded(!expanded);
            }}
            title={expanded ? t('collapseOptions') : t('expandOptions')}
            className={`p-2 rounded-md transition-colors ${expanded ? 'bg-primary/10 text-primary' : 'hover:bg-muted text-muted-foreground'}`}
          >
            {expanded ? <ChevronUp className="w-5 h-5" /> : <Box className="w-5 h-5" />}
          </button>

          <button 
            onClick={() => {
              void handleLaunch();
            }}
            disabled={loading}
            title={t('launchSession')}
            className="p-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 shadow-sm transition-all"
          >
            <Send className="w-5 h-5" />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="p-4 bg-muted/20 space-y-4 animate-in slide-in-from-top-2 duration-300">
          <div className="space-y-2">
            <label className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
              {t('workflowPreset', 'Workflow Preset')}
            </label>
            <select
              value={selectedWorkflowPresetId}
              onChange={(e) => {
                handleSelectWorkflowPreset(e.target.value);
              }}
              className="w-full h-10 rounded-md border bg-background px-3 py-2 text-sm"
            >
              <option value="">{t('workflowPresetNoManual', 'No preset (manual)')}</option>
              {workflowPresets.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.name} ({preset.tool}/{preset.launch_scope || 'shared'})
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2">
            <label className="text-xs font-bold uppercase tracking-widest text-muted-foreground">{t('workingDirectory')}</label>
            <div className="flex gap-2">
              <div className="flex-1 bg-background rounded-md px-3 py-2 text-sm text-muted-foreground truncate flex items-center gap-2 font-mono ring-1 ring-border/10">
                <FolderOpen className="w-3.5 h-3.5" />
                {path || t('noPathSelected', 'Choose a directory...')}
              </div>
              <button 
                onClick={handleSelectDir}
                className="px-3 py-2 bg-secondary text-secondary-foreground rounded-md text-sm font-medium hover:bg-secondary/80 transition-colors"
              >
                {t('browse')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
