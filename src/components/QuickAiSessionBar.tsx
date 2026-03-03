import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Terminal, Box, ChevronDown, ChevronUp, FolderOpen, Send } from 'lucide-react';
import { ToolIcon } from './AiEnvironments';
import { open } from '@tauri-apps/plugin-dialog';

export function QuickAiSessionBar() {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [model, setModel] = useState('claude');
  const [path, setPath] = useState('');
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const models = [
    { id: 'claude', name: 'Claude Code', cmd: 'claude code' },
    { id: 'gemini', name: 'Gemini', cmd: 'gemini -y' },
    { id: 'codex', name: 'Codex', cmd: 'codex' },
    { id: 'opencode', name: 'OpenCode', cmd: 'opencode' }
  ];

  useEffect(() => {
    // Initial focus
    inputRef.current?.focus();

    // Global key listener
    const handleGlobalKeys = async (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        await invoke('hide_window').catch(() => {});
      } else if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
        // If we're not already loading and have a name, launch!
        // We use a small delay to ensure state is synced if needed
        if (name && !loading) {
          handleLaunch();
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

    // Load default path
    const loadDefaultPath = async () => {
      try {
        const cfg: any = await invoke('get_storage_config');
        if (cfg.default_ai_dir) {
          setPath(cfg.default_ai_dir);
        }
      } catch (e) {
        console.error(e);
      }
    };
    loadDefaultPath();

    return () => {
      window.removeEventListener('keydown', handleGlobalKeys);
      clearInterval(focusInterval);
    };
  }, [name, loading, path, model]); // Add dependencies to ensure listener uses latest state

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

  const handleLaunch = async () => {
    if (!name) {
      await invoke('hide_window').catch(() => {});
      return;
    }

    setLoading(true);
    try {
      // Hide the window immediately via backend command for maximum reliability
      await invoke('hide_window').catch(err => console.error('Hide window failed:', err));
      
      const cmd = models.find(m => m.id === model)?.cmd || 'claude code';
      const targetPath = path || './'; 

      await invoke('create_tmux_session', {
        sessionName: name.replace(/[.\s]+/g, '_'),
        workingDir: targetPath,
        command: cmd
      });
      
      await invoke('attach_tmux_session', { sessionName: name.replace(/[.\s]+/g, '_') });
      
      setName('');
    } catch (e) {
      console.error('Failed to launch AI session:', e);
    } finally {
      setLoading(false);
    }
  };

  const handleSelectDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setPath(selected);
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  return (
    <div className="w-full h-full bg-background/95 backdrop-blur-xl border-none shadow-2xl rounded-xl flex flex-col overflow-hidden">
      <div className="flex items-center h-[70px] px-4 gap-3 bg-card/50" data-tauri-drag-region>
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
              {models.map(m => (
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
