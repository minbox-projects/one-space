import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Terminal, Plus, FolderOpen, Play, Trash2, Loader2, AlertCircle, Settings2, Edit2, Check, X, Download, Shield } from 'lucide-react';
import type { AiProvidersState } from './AiEnvironments';

interface TmuxSession {
  name: String;
  created: number;
  attached: boolean;
  path: string;
}

interface AiCommand {
  id: string;
  name: string;
  command: string;
}

const DEFAULT_COMMANDS: AiCommand[] = [
  { id: 'claude', name: 'Claude Code', command: 'claude code' },
  { id: 'gemini', name: 'Gemini', command: 'gemini -y' },
  { id: 'opencode', name: 'OpenCode', command: 'opencode' },
  { id: 'bash', name: 'Bash (Empty Terminal)', command: '' }
];

export function AiSessions() {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<TmuxSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // Custom Commands State
  const [aiCommands, setAiCommands] = useState<AiCommand[]>(DEFAULT_COMMANDS);
  const [isManagingCommands, setIsManagingCommands] = useState(false);
  const [newCmdName, setNewCmdName] = useState('');
  const [newCmdValue, setNewCmdValue] = useState('');

  // New session modal state
  const [isCreating, setIsCreating] = useState(false);
  const [newSessionName, setNewSessionName] = useState('');
  const [newSessionCommand, setNewSessionCommand] = useState('claude code');

  const [newSessionDir, setNewSessionDir] = useState('');
  
  // Custom states
  const [editingSession, setEditingSession] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [sessionToKill, setSessionToKill] = useState<string | null>(null);

  // Active environments state
  const [providersState, setProvidersState] = useState<AiProvidersState | null>(null);

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadProvidersState = async () => {
    if (!isTauri) return;
    try {
      const res: AiProvidersState = await invoke('get_ai_providers');
      setProvidersState(res);
    } catch (e) {
      console.error(e);
    }
  };

  // Load custom commands from local storage on mount
  useEffect(() => {
    const savedCommands = localStorage.getItem('onespace_ai_commands');
    if (savedCommands) {
      try {
        setAiCommands(JSON.parse(savedCommands));
      } catch (e) {
        console.error('Failed to parse saved commands', e);
      }
    }
    loadProvidersState();
  }, []);

  const saveCommands = (cmds: AiCommand[]) => {
    setAiCommands(cmds);
    localStorage.setItem('onespace_ai_commands', JSON.stringify(cmds));
  };

  const handleAddCommand = () => {
    if (!newCmdName) return;
    const newCmd = { id: Date.now().toString(), name: newCmdName, command: newCmdValue };
    saveCommands([...aiCommands, newCmd]);
    setNewCmdName('');
    setNewCmdValue('');
    setNewSessionCommand(newCmd.command);
  };

  const handleDeleteCommand = (id: string) => {
    saveCommands(aiCommands.filter(c => c.id !== id));
  };

  const handleUpdateCommand = (id: string, newCmdValue: string) => {
    saveCommands(aiCommands.map(c => c.id === id ? { ...c, command: newCmdValue } : c));
  };

  const handleRestoreDefaults = () => {
    saveCommands(DEFAULT_COMMANDS);
  };

  const loadSessions = async () => {
    if (!isTauri) {
      setError(t('notInTauri'));
      return;
    }
    
    try {
      setLoading(true);
      setError(null);
      const res: TmuxSession[] = await invoke('get_tmux_sessions');
      setSessions(res);
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSessions();
    if (!isTauri) return;
    
    const interval = setInterval(loadSessions, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleSelectDir = async () => {
    if (!isTauri) {
      setError(t('notInTauri'));
      return;
    }

    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setNewSessionDir(selected);
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  const handleCreate = async () => {
    if (!isTauri) {
      setError(t('notInTauri'));
      return;
    }

    if (!newSessionName || !newSessionDir) {
      setError(t('provideNameAndDir'));
      return;
    }
    try {
      setLoading(true);
      await invoke('create_tmux_session', {
        sessionName: newSessionName.replace(/\s+/g, '_'),
        workingDir: newSessionDir,
        command: newSessionCommand
      });
      
      await handleAttach(newSessionName.replace(/\s+/g, '_'));
      
      setIsCreating(false);
      setNewSessionName('');
      setNewSessionDir('');
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
      setLoading(false);
    }
  };

  const handleAttach = async (sessionName: string) => {
    if (!isTauri) return;
    try {
      await invoke('attach_tmux_session', { sessionName });
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    }
  };



  const handleKillRequest = (sessionName: string) => {
    setSessionToKill(sessionName);
  };

  const confirmKill = async () => {
    if (!isTauri || !sessionToKill) return;
    try {
      setLoading(true);
      await invoke('kill_tmux_session', { sessionName: sessionToKill });
      setSessionToKill(null);
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
      setLoading(false);
    }
  };


  const handleStartRename = (sessionName: string) => {
    setEditingSession(sessionName);
    setEditName(sessionName);
  };

  const handleSaveRename = async (oldName: string) => {
    if (!isTauri) return;
    if (!editName || editName === oldName) {
      setEditingSession(null);
      return;
    }
    
    // Replace invalid characters similar to how the bash script does it
    const sanitizedName = editName.replace(/[.\s]/g, '_');
    
    try {
      setLoading(true);
      await invoke('rename_tmux_session', { 
        oldName: oldName,
        newName: sanitizedName 
      });
      setEditingSession(null);
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
      setLoading(false);
    }
  };

  const handleInstallCli = async () => {
    if (!isTauri) return;
    try {
      setLoading(true);
      await invoke('install_cli');
      alert(t('cliInstalled', 'CLI tool installed to ~/.local/bin/onespace'));
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };


  const formatTime = (ts: number) => {
    return new Date(ts * 1000).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
    });
  };

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('aiSessions')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('manageAiAssistants')}</p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={handleInstallCli}
            disabled={loading}
            className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
            title={t('installCliTitle', 'Install CLI tool to ~/.local/bin')}
          >
            <Download className="w-4 h-4" />
            <span className="hidden sm:inline">{t('installCli', 'Install CLI')}</span>
          </button>
          <button
            onClick={() => setIsCreating(true)}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
        >
          <Plus className="w-4 h-4" />
          {t('newSession')}
        </button>
        </div>
      </div>

      {error && (
        <div className="bg-destructive/15 text-destructive text-sm p-4 rounded-md flex items-start gap-3">
          <AlertCircle className="w-5 h-5 shrink-0 mt-0.5" />
          <div>{error}</div>
        </div>
      )}

      {isCreating && (
        <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Terminal className="w-4 h-4 text-primary" />
            {t('createNewAiSession')}
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('sessionName')}</label>
              <input 
                type="text" 
                placeholder={t('sessionNamePlaceholder', 'e.g. project_x_claude')} 
                value={newSessionName}
                onChange={(e) => setNewSessionName(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('aiCommand')}</label>
              <div className="flex gap-2">
                <select 
                  value={newSessionCommand}
                  onChange={(e) => setNewSessionCommand(e.target.value)}
                  className="flex flex-1 h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {aiCommands.map(cmd => (
                    <option key={cmd.id} value={cmd.command}>
                      {cmd.name}
                    </option>
                  ))}
                </select>
                <button
                  onClick={() => setIsManagingCommands(!isManagingCommands)}
                  className={`px-3 rounded-md border transition-colors ${isManagingCommands ? 'bg-secondary text-secondary-foreground' : 'bg-background hover:bg-muted text-muted-foreground'}`}
                  title={t('manageCommands')}
                >
                  <Settings2 className="w-4 h-4" />
                </button>
              </div>
              
              {/* Active Provider Indicator */}
              {providersState && newSessionCommand && (
                <div className="pt-1">
                  {newSessionCommand.includes('claude') && providersState.active_claude && (
                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground bg-muted/40 p-1.5 rounded border">
                      <Shield className="w-3.5 h-3.5 text-primary" />
                      <span>Claude Environment: <span className="font-medium text-foreground">{providersState.providers.find((p: any) => p.id === providersState.active_claude)?.name || 'Default'}</span></span>
                    </div>
                  )}
                  {newSessionCommand.includes('gemini') && providersState.active_gemini && (
                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground bg-muted/40 p-1.5 rounded border">
                      <Shield className="w-3.5 h-3.5 text-primary" />
                      <span>Gemini Environment: <span className="font-medium text-foreground">{providersState.providers.find((p: any) => p.id === providersState.active_gemini)?.name || 'Default'}</span></span>
                    </div>
                  )}
                  {newSessionCommand.includes('opencode') && providersState.active_opencode && (
                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground bg-muted/40 p-1.5 rounded border">
                      <Shield className="w-3.5 h-3.5 text-primary" />
                      <span>OpenCode Environment: <span className="font-medium text-foreground">{providersState.providers.find((p: any) => p.id === providersState.active_opencode)?.name || 'Default'}</span></span>
                    </div>
                  )}
                </div>
              )}
            </div>

            {isManagingCommands && (
              <div className="md:col-span-2 mt-2 p-4 bg-muted/30 border border-dashed rounded-lg space-y-4">
                <div className="flex justify-between items-center">
                  <h4 className="text-xs font-bold uppercase tracking-wider">{t('manageCommands')}</h4>
                  <button onClick={handleRestoreDefaults} className="text-xs text-muted-foreground hover:text-foreground transition-colors">
                    {t('restoreDefaults')}
                  </button>
                </div>
                
                <div className="space-y-2 max-h-48 overflow-y-auto pr-2">
                  {aiCommands.map(cmd => (
                    <div key={cmd.id} className="flex flex-col sm:flex-row sm:items-center gap-2 text-sm group bg-background border px-3 py-2.5 rounded-md hover:border-primary/50 transition-colors">
                      <div className="font-medium w-32 shrink-0">{cmd.name}</div>
                      <div className="flex-1 flex gap-2 items-center">
                        <input 
                          type="text"
                          value={cmd.command}
                          onChange={(e) => handleUpdateCommand(cmd.id, e.target.value)}
                          className="flex-1 bg-transparent border-0 border-b border-transparent hover:border-border focus:border-primary focus:ring-0 focus:outline-none px-1 py-0.5 font-mono text-xs text-muted-foreground focus:text-foreground transition-colors"
                          placeholder={t('emptyTerminalPlaceholder', '(empty terminal)')}
                        />
                        <button 
                          onClick={() => handleDeleteCommand(cmd.id)} 
                          className="text-muted-foreground hover:text-destructive p-1.5 rounded-md hover:bg-destructive/10 transition-colors"
                          title="Delete command"
                        >
                          <Trash2 className="w-4 h-4"/>
                        </button>
                      </div>
                    </div>
                  ))}
                </div>

                <div className="flex flex-col sm:flex-row gap-3 pt-4 border-t border-border/50">
                  <div className="w-full sm:w-1/3">
                    <input 
                      placeholder={t('commandName')} 
                      value={newCmdName} 
                      onChange={e=>setNewCmdName(e.target.value)} 
                      className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2" 
                    />
                  </div>
                  <div className="flex-1 flex gap-2">
                    <input 
                      placeholder={t('commandValue')} 
                      value={newCmdValue} 
                      onChange={e=>setNewCmdValue(e.target.value)} 
                      className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background font-mono placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2" 
                      onKeyDown={(e) => e.key === 'Enter' && handleAddCommand()}
                    />
                    <button 
                      onClick={handleAddCommand} 
                      disabled={!newCmdName}
                      className="h-10 bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-1.5 shrink-0"
                    >
                      <Plus className="w-4 h-4" />
                      {t('add')}
                    </button>
                  </div>
                </div>
              </div>
            )}

            <div className="space-y-2 md:col-span-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('workingDirectory')}</label>
              <div className="flex gap-2">
                <input 
                  type="text" 
                  readOnly
                  placeholder={t('selectProjectDir')}
                  value={newSessionDir}
                  className="flex h-10 w-full rounded-md border border-input bg-muted/50 px-3 py-2 text-sm ring-offset-background cursor-not-allowed"
                />
                <button 
                  onClick={handleSelectDir}
                  className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
                >
                  <FolderOpen className="w-4 h-4" />
                  {t('browse')}
                </button>
              </div>
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button 
              onClick={() => setIsCreating(false)}
              className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
            >
              {t('cancel')}
            </button>
            <button 
              onClick={handleCreate}
              disabled={loading || !newSessionName || !newSessionDir}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
            >
              {loading && <Loader2 className="w-4 h-4 animate-spin" />}
              {t('launch')}
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-auto rounded-xl border bg-card text-card-foreground shadow-sm">
        {sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
            <Terminal className="w-10 h-10 mb-3 opacity-20" />
            <p>{t('noActiveSessions')}</p>
            <p className="text-sm mt-1">{t('createOneToGetStarted')}</p>
          </div>
        ) : (
          <div className="divide-y">
            {sessions.map((session, idx) => (
              <div key={idx} className="p-4 flex flex-col md:flex-row md:items-center justify-between gap-4 hover:bg-muted/30 transition-colors">
                <div className="flex items-start gap-4">
                  <div className={`mt-1 w-2 h-2 rounded-full shrink-0 ${session.attached ? 'bg-amber-500' : 'bg-green-500'}`} 
                       title={session.attached ? t('attachedElsewhere') : t('runningInBackground')}></div>

                  <div>
                    {editingSession === session.name ? (
                      <div className="flex items-center gap-2 mb-1">
                        <input
                          type="text"
                          autoFocus
                          value={editName}
                          onChange={(e) => setEditName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handleSaveRename(session.name as string);
                            if (e.key === 'Escape') setEditingSession(null);
                          }}
                          className="flex h-7 rounded-md border border-input bg-background px-2 py-1 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring w-48"
                        />
                        <button 
                          onClick={() => handleSaveRename(session.name as string)}
                          className="text-green-500 hover:bg-green-500/10 p-1 rounded transition-colors"
                        >
                          <Check className="w-4 h-4" />
                        </button>
                        <button 
                          onClick={() => setEditingSession(null)}
                          className="text-muted-foreground hover:bg-muted p-1 rounded transition-colors"
                        >
                          <X className="w-4 h-4" />
                        </button>
                      </div>
                    ) : (
                      <h4 className="font-semibold text-base flex items-center gap-2 group/title">
                        {session.name}
                        <button
                          onClick={() => handleStartRename(session.name as string)}
                          className="opacity-0 group-hover/title:opacity-100 text-muted-foreground hover:text-foreground p-0.5 rounded transition-all"
                          title="Rename session"
                        >
                          <Edit2 className="w-3.5 h-3.5" />
                        </button>
                        <span className="text-xs text-muted-foreground font-normal ml-2">
                          {formatTime(session.created)}
                        </span>
                        {session.attached && <span className="text-[10px] uppercase bg-amber-500/20 text-amber-500 px-2 py-0.5 rounded-full font-bold tracking-wider">{t('attached')}</span>}
                      </h4>
                    )}

                    <p className="text-xs text-muted-foreground mt-1 flex items-center gap-1.5 truncate max-w-[300px] sm:max-w-md">
                      <FolderOpen className="w-3 h-3" />
                      {session.path}
                    </p>
                  </div>
                </div>
                
                <div className="flex items-center gap-2 shrink-0">
                  <button
                    onClick={() => handleAttach(session.name as string)}
                    className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                  >
                    <Play className="w-3.5 h-3.5" />
                    {t('attach')}
                  </button>
                  <button
                    onClick={() => handleKillRequest(session.name as string)}
                    className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                    {t('kill')}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Kill Confirmation Modal */}
      {sessionToKill && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            <div className="p-5">
              <div className="flex items-center gap-3 text-destructive mb-3">
                <div className="bg-destructive/10 p-2 rounded-full">
                  <AlertCircle className="w-5 h-5" />
                </div>
                <h3 className="font-semibold">{t('terminateSession', 'Terminate Session')}</h3>
              </div>
              <p className="text-sm text-muted-foreground">
                {t('confirmKill', { name: sessionToKill })}
              </p>
              <p className="text-xs text-muted-foreground mt-2 bg-muted/50 p-2 rounded border">
                {t('terminateWarning', 'This will immediately stop the AI process and you will lose any unsaved conversation context in the terminal.')}
              </p>
            </div>
            <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
              <button
                onClick={() => setSessionToKill(null)}
                disabled={loading}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors disabled:opacity-50"
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                onClick={confirmKill}
                disabled={loading}
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors disabled:opacity-50"
              >
                {loading && <Loader2 className="w-4 h-4 animate-spin" />}
                {t('terminate', 'Terminate')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
