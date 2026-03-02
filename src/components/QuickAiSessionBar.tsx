import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
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

    // Close on blur (optional, user might want to keep it)
    const win = getCurrentWindow();
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && !expanded) {
        // win.hide(); // Uncomment if you want it to hide automatically
      }
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  const handleLaunch = async () => {
    if (!name || !path) return;
    try {
      setLoading(true);
      const cmd = models.find(m => m.id === model)?.cmd || 'claude code';
      await invoke('create_tmux_session', {
        sessionName: name.replace(/\s+/g, '_'),
        workingDir: path,
        command: cmd
      });
      await invoke('attach_tmux_session', { sessionName: name.replace(/\s+/g, '_') });
      const win = getCurrentWindow();
      await win.hide();
      setName('');
    } catch (e) {
      console.error(e);
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
    <div className="w-full h-full bg-background/95 backdrop-blur-xl border-none shadow-2xl rounded-xl overflow-hidden flex flex-col">
      <div className="flex items-center h-[70px] px-4 gap-3 bg-card/50">
        <div className="bg-primary/10 p-2 rounded-lg">
          <Terminal className="w-6 h-6 text-primary" />
        </div>
        
        <input
          ref={inputRef}
          type="text"
          placeholder={t('quickSessionPlaceholder', 'AI Session Name...')}
          value={name}
          onChange={e => setName(e.target.value)}
          onKeyDown={async e => {
            if (e.key === 'Enter') handleLaunch();
            if (e.key === 'Escape') {
              const win = getCurrentWindow();
              await win.hide();
            }
          }}
          className="flex-1 bg-transparent border-none text-xl font-medium focus:ring-0 placeholder:text-muted-foreground/50"
        />

        <div className="flex items-center gap-2 border-l pl-4">
          <div className="relative flex items-center gap-2 bg-muted/50 rounded-md px-2 py-1.5 border hover:bg-muted transition-colors cursor-pointer group">
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
            disabled={!name || !path || loading}
            title={t('launchSession')}
            className="p-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 shadow-sm transition-all"
          >
            <Send className="w-5 h-5" />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="p-4 border-t bg-muted/20 space-y-4 animate-in slide-in-from-top-2 duration-300">
          <div className="space-y-2">
            <label className="text-xs font-bold uppercase tracking-widest text-muted-foreground">{t('workingDirectory')}</label>
            <div className="flex gap-2">
              <div className="flex-1 bg-background border rounded-md px-3 py-2 text-sm text-muted-foreground truncate flex items-center gap-2 font-mono">
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
