import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { Terminal, Box, ChevronDown, ChevronUp, FolderOpen, Send } from 'lucide-react';
import { ToolIcon } from './AiEnvironments';
import { open } from '@tauri-apps/plugin-dialog';

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
  const [name, setName] = useState('');
  const [model, setModel] = useState('claude');
  const [path, setPath] = useState('');
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleLaunch = useCallback(async () => {
    if (!name) {
      await invoke('hide_window').catch(() => {});
      return;
    }

    setLoading(true);
    try {
      // Hide the window immediately via backend command for maximum reliability
      await invoke('hide_window').catch(err => console.error('Hide window failed:', err));
      
      const targetPath = path || './'; 
      const toolSessionId = crypto.randomUUID();

      await invoke('sessions_create', {
        session: {
          name: name,
          working_dir: targetPath,
          tool: model,
          tool_session_id: toolSessionId,
          status: 'active'
        }
      });
      
      emit('refresh-counts').catch(console.error);
      
      setName('');
    } catch (e) {
      console.error('Failed to launch AI session:', e);
    } finally {
      setLoading(false);
    }
  }, [name, path, model]);

  const applyDefaultModel = useCallback(async () => {
    try {
      const cfg = await invoke<StorageConfig>('get_storage_config');
      if (cfg.default_ai_model && QUICK_MODEL_IDS.has(cfg.default_ai_model)) {
        setModel(cfg.default_ai_model);
      }
    } catch (e) {
      console.error(e);
    }
  }, []);

  useEffect(() => {
    // Initial focus
    inputRef.current?.focus();

    // Load default path once and apply default model on initial open
    const loadDefaultPath = async () => {
      try {
        const cfg = await invoke<StorageConfig>('get_storage_config');
        if (cfg.default_ai_dir) {
          setPath(cfg.default_ai_dir);
        }
      } catch (e) {
        console.error(e);
      }
    };
    loadDefaultPath();
    applyDefaultModel();
  }, [applyDefaultModel]);

  useEffect(() => {
    // Re-apply default model each time quick window becomes visible
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        applyDefaultModel();
      }
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [applyDefaultModel]);

  useEffect(() => {
    // Global key listener
    const handleGlobalKeys = async (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        await invoke('hide_window').catch(() => {});
      } else if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
        if (name && !loading) {
          await handleLaunch();
        } else if (!name) {
          await invoke('hide_window').catch(() => {});
        }
      }
    };
    window.addEventListener('keydown', handleGlobalKeys);

    // Focus when window might have been shown (polling as a fallback or just use the event if available)
    const focusInterval = setInterval(() => {
      if (document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'SELECT') {
        inputRef.current?.focus();
      }
    }, 500);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeys);
      clearInterval(focusInterval);
    };
  }, [name, loading, handleLaunch]);

  useEffect(() => {
    // Sync window size when expanded state changes
    const syncWindowSize = async () => {
      try {
        const height = expanded ? 180 : 70;
        await invoke('resize_window', { height });
      } catch (err) {
        console.error('Failed to resize window:', err);
      }
    };
    syncWindowSize();
  }, [expanded]);

  const handleSelectDir = async () => {
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
        
        <input
          ref={inputRef}
          type="text"
          placeholder={t('quickSessionPlaceholder', 'AI Session Name...')}
          value={name}
          onChange={e => setName(e.target.value)}
          onKeyDown={async e => {
            if (e.key === 'Enter') {
              e.preventDefault();
              await handleLaunch();
            }
            if (e.key === 'Escape') {
              await invoke('hide_window').catch(() => {});
            }
          }}
          className="flex-1 bg-transparent border-none text-xl font-medium focus:ring-0 placeholder:text-muted-foreground/50"
        />

        <div className="flex items-center gap-2 pl-4">
          <div className="relative flex items-center gap-2 bg-muted/50 rounded-md px-2 py-1.5 hover:bg-muted transition-colors cursor-pointer group">
            <ToolIcon tool={model} className="w-4 h-4" />
            <select 
              value={model}
              onChange={e => setModel(e.target.value)}
              className="bg-transparent text-sm font-medium pr-6 focus:ring-0 cursor-pointer appearance-none outline-none"
            >
              {QUICK_MODELS.map(m => (
                <option key={m.id} value={m.id}>{m.name}</option>
              ))}
            </select>
            <ChevronDown className="w-3.5 h-3.5 absolute right-2 pointer-events-none text-muted-foreground" />
          </div>

          <button 
            onClick={() => setExpanded(!expanded)}
            title={expanded ? t('collapseOptions') : t('expandOptions')}
            className={`p-2 rounded-md transition-colors ${expanded ? 'bg-primary/10 text-primary' : 'hover:bg-muted text-muted-foreground'}`}
          >
            {expanded ? <ChevronUp className="w-5 h-5" /> : <Box className="w-5 h-5" />}
          </button>

          <button 
            onClick={handleLaunch}
            disabled={!name || loading}
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
