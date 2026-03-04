import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Terminal, Plus, FolderOpen, Play, Trash2, Loader2, AlertCircle, Settings2, Edit2, Check, X, Copy } from 'lucide-react';
import type { AiProvidersState } from './AiEnvironments';
import { ToolIcon } from './AiEnvironments';

interface AiSession {
  id: string;
  name: string;
  working_dir: string;
  model_type: string;
  tool_session_id: string;
  created_at: number;
}

interface AiCommand {
  id: string;
  name: string;
  command: string;
}

interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

const DEFAULT_COMMANDS: AiCommand[] = [
  { id: 'claude', name: 'Claude Code', command: 'claude code' },
  { id: 'gemini', name: 'Gemini', command: 'gemini -y' },
  { id: 'codex', name: 'Codex', command: 'codex' },
  { id: 'opencode', name: 'OpenCode', command: 'opencode' },
  { id: 'bash', name: 'Bash (Empty Terminal)', command: '' }
];

export function AiSessions({ onNavigate }: { onNavigate?: (tab: string, hash?: string) => void }) {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<AiSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cliInstalled, setCliInstalled] = useState(true);
  
  // Custom Commands State
  const [aiCommands, setAiCommands] = useState<AiCommand[]>(DEFAULT_COMMANDS);
  const [isManagingCommands, setIsManagingCommands] = useState(false);
  const [newCmdName, setNewCmdName] = useState('');
  const [newCmdValue, setNewCmdValue] = useState('');

  // New session modal state
  const [isCreating, setIsCreating] = useState(false);
  const [newSessionName, setNewSessionName] = useState('');
  const [selectedCommandId, setSelectedCommandId] = useState('claude');

  const [newSessionDir, setNewSessionDir] = useState('');
  
  // Custom states
  const [editingSession, setEditingSession] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [sessionToDelete, setSessionToDelete] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Active environments state
  const [providersState, setProvidersState] = useState<AiProvidersState | null>(null);

  const isTauri = '__TAURI_INTERNALS__' in window;


  const checkCli = async () => {
    if (!isTauri) return;
    try {
      const installed = await invoke<boolean>('check_cli_installed');
      setCliInstalled(installed);
    } catch (e) {
      console.error("Failed to check CLI", e);
    }
  };

  const loadDefaultDir = async () => {
    if (!isTauri) return;
    try {
      const cfg: any = await invoke('get_storage_config');
      if (cfg.default_ai_dir) {
        setNewSessionDir(cfg.default_ai_dir);
      }
    } catch (e) {
      console.error("Failed to load default dir", e);
    }
  };

  const loadProvidersState = async () => {
    if (!isTauri) return;
    try {
      const res: ApiResp<AiProvidersState> = await invoke('providers_list');
      setProvidersState(res.data);
    } catch (e) {
      console.error(e);
    }
  };

  // Load custom commands from local storage on mount
  useEffect(() => {
    const savedCommands = localStorage.getItem('onespace_ai_commands');
    if (savedCommands) {
      try {
        const parsed: AiCommand[] = JSON.parse(savedCommands);
        // Ensure new default commands (like codex) are added if missing
        const merged = [...parsed];
        DEFAULT_COMMANDS.forEach(def => {
          if (!merged.some(m => m.id === def.id || m.command === def.command)) {
            merged.push(def);
          }
        });
        setAiCommands(merged);
        if (merged.length !== parsed.length) {
          localStorage.setItem('onespace_ai_commands', JSON.stringify(merged));
        }
      } catch (e) {
        console.error('Failed to parse saved commands', e);
      }
    }
    loadProvidersState();
    checkCli();
    loadDefaultDir();
  }, []);

  const saveCommands = (cmds: AiCommand[]) => {
    setAiCommands(cmds);
    localStorage.setItem('onespace_ai_commands', JSON.stringify(cmds));
  };

  const handleAddCommand = () => {
    if (!newCmdName) return;
    const newId = Date.now().toString();
    const newCmd = { id: newId, name: newCmdName, command: newCmdValue };
    saveCommands([...aiCommands, newCmd]);
    setNewCmdName('');
    setNewCmdValue('');
    setSelectedCommandId(newId);
  };


  const handleDeleteCommand = (id: string) => {
    const updated = aiCommands.filter(c => c.id !== id);
    saveCommands(updated);
    if (selectedCommandId === id && updated.length > 0) {
      setSelectedCommandId(updated[0].id);
    }
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
      const res: ApiResp<AiSession[]> = await invoke('sessions_list');
      setSessions(res.data);
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSessions();
    
    let unlisten: (() => void) | undefined;
    const setupListener = async () => {
      unlisten = await listen('refresh-counts', () => {
        loadSessions();
      });
    };
    setupListener();
    
    return () => {
      if (unlisten) unlisten();
    };
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

    // Capture model type
    const cmd = aiCommands.find(c => c.id === selectedCommandId);
    const modelType = getCommandToolType(cmd?.command || '', cmd?.id) || 'bash';
    
    // Generate/Request tool session ID
    // For now, generate a UUID for Claude/Codex, others might need manual capture
    const toolSessionId = (modelType === 'claude' || modelType === 'codex') 
      ? crypto.randomUUID() 
      : `session_${Date.now()}`;

    try {
      setLoading(true);
      await invoke('sessions_create', {
        session: {
          name: newSessionName,
          working_dir: newSessionDir,
          tool: modelType,
          tool_session_id: toolSessionId,
          status: 'active'
        }
      });
      
      emit('refresh-counts').catch(console.error);
      
      setIsCreating(false);
      setNewSessionName('');
      setNewSessionDir('');
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const handleLaunch = async (session: AiSession) => {
    if (!isTauri) return;
    try {
      await invoke('sessions_launch', { sessionId: session.id });
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    }
  };

  const handleDeleteRequest = (id: string) => {
    setSessionToDelete(id);
  };

  const confirmDelete = async () => {
    if (!isTauri || !sessionToDelete) return;
    try {
      setLoading(true);
      await invoke('sessions_delete', { sessionId: sessionToDelete });
      emit('refresh-counts').catch(console.error);
      setSessionToDelete(null);
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };


  const handleCopyId = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    await navigator.clipboard.writeText(id);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const handleStartRename = (session: AiSession) => {
    setEditingSession(session.id);
    setEditName(session.name);
  };

  const handleSaveRename = async (session: AiSession) => {
    if (!isTauri) return;
    if (!editName || editName === session.name) {
      setEditingSession(null);
      return;
    }
    
    try {
      setLoading(true);
      const updatedSession = { ...session, name: editName };
      await invoke('sessions_update', {
        session: {
          id: updatedSession.id,
          name: updatedSession.name,
          working_dir: updatedSession.working_dir,
          tool: updatedSession.model_type,
          tool_session_id: updatedSession.tool_session_id,
          status: 'active'
        }
      });
      setEditingSession(null);
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const handleInstallCli = async () => {
    if (onNavigate) {
      onNavigate('documentation', 'cli');
    }
  };

  const formatTime = (ts: number) => {
    return new Date(ts * 1000).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
    });
  };

  const handleNewSession = async () => {
    await Promise.all([
      loadDefaultDir(),
      loadProvidersState()
    ]);
    setIsCreating(true);
  };

  const getCommandToolType = (cmd: string, cmdId?: string) => {
    // Priority 1: Check by ID (for built-in commands)
    if (cmdId === 'claude') return 'claude';
    if (cmdId === 'gemini') return 'gemini';
    if (cmdId === 'codex') return 'codex';
    if (cmdId === 'opencode') return 'opencode';

    // Priority 2: Check command string content
    const c = (cmd || '').toLowerCase();
    if (c.includes('claude')) return 'claude';
    if (c.includes('gemini')) return 'gemini';
    if (c.includes('codex') || c.includes('openai')) return 'codex';
    if (c.includes('opencode')) return 'opencode';
    return null;
  };

  const renderActiveProvider = () => {
    if (!providersState || !selectedCommandId) return null;
    
    const cmd = aiCommands.find(c => c.id === selectedCommandId);
    if (!cmd) return null;

    const toolType = getCommandToolType(cmd.command, cmd.id);
    if (!toolType) return null;

    const activeId = (providersState as any)[`active_${toolType}`];
    if (!activeId) return null;

    const provider = providersState.providers.find(p => p.id === activeId);
    if (!provider) return null;

    return (
      <div className="pt-1">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground bg-muted/40 p-1.5 rounded border animate-in fade-in slide-in-from-top-1 duration-200">
          <ToolIcon tool={toolType} className="w-3.5 h-3.5 text-primary" />
          <span>{t('toolEnvironment', { tool: toolType.charAt(0).toUpperCase() + toolType.slice(1) })}: <span className="font-medium text-foreground">{provider.name}</span></span>
        </div>
      </div>
    );
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
            onClick={handleNewSession}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
        >
          <Plus className="w-4 h-4" />
          {t('newSession')}
        </button>
        </div>
      </div>

      {!cliInstalled && (
        <div className="bg-primary/5 border border-primary/20 p-4 rounded-xl flex flex-col sm:flex-row items-center justify-between gap-4 animate-in fade-in slide-in-from-top-2">
          <div className="flex items-start gap-3">
            <div className="bg-primary/10 p-2 rounded-full mt-0.5">
              <Terminal className="w-4 h-4 text-primary" />
            </div>
            <div className="space-y-1">
              <p className="text-sm font-medium leading-none">{t('cliNotInstalled')}</p>
              <p className="text-xs text-muted-foreground leading-relaxed">
                {t('cliNotInstalledDesc')}
              </p>
            </div>
          </div>
          <button 
            onClick={handleInstallCli}
            className="whitespace-nowrap px-4 py-2 bg-primary text-primary-foreground rounded-lg text-xs font-semibold hover:bg-primary/90 transition-all shadow-sm"
          >
            {t('goToDocs')}
          </button>
        </div>
      )}

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
                  value={selectedCommandId}
                  onChange={(e) => {
                    const id = e.target.value;
                    setSelectedCommandId(id);
                  }}
                  className="flex flex-1 h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {aiCommands.map(cmd => (
                    <option key={cmd.id} value={cmd.id}>
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
              {renderActiveProvider()}
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
                          title={t('delete', 'Delete')}
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
          <div className="divide-y divide-border">
            {sessions.map((session) => (
              <div key={session.id} className="p-4 hover:bg-muted/30 transition-colors group/copy">
                <div className="flex items-center gap-3">
                  <div className="w-1.5 h-1.5 rounded-full shrink-0 bg-muted-foreground/40" />
                  <div className="flex-1 min-w-0">
                    {editingSession === session.id ? (
                      <div className="flex items-center gap-2 mb-3">
                        <input
                          type="text"
                          autoFocus
                          value={editName}
                          onChange={(e) => setEditName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handleSaveRename(session);
                            if (e.key === 'Escape') setEditingSession(null);
                          }}
                          className="flex h-7 rounded-md border border-input bg-background px-2 py-1 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring w-64"
                        />
                        <button 
                          onClick={() => handleSaveRename(session)}
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
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2 group/title">
                          <ToolIcon tool={session.model_type || 'terminal'} className="w-4 h-4 text-muted-foreground shrink-0" />
                          <span className="font-semibold text-base truncate max-w-md">{session.name}</span>
                          <button
                            onClick={() => handleStartRename(session)}
                            className="opacity-0 group-hover/title:opacity-100 text-muted-foreground hover:text-foreground p-0.5 rounded transition-all shrink-0"
                            title={t('edit', 'Rename')}
                          >
                            <Edit2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                        <div className="flex items-center gap-2">
                          <button
                            onClick={() => handleLaunch(session)}
                            className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                          >
                            <Play className="w-3.5 h-3.5" />
                            {t('continue', 'Continue')}
                          </button>
                          <button
                            onClick={() => handleDeleteRequest(session.id)}
                            className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                            {t('delete', 'Delete')}
                          </button>
                        </div>
                      </div>
                    )}

                    {editingSession !== session.id && (
                      <div className="flex items-center gap-4 text-sm text-muted-foreground">
                        <div className="flex items-center gap-1.5 font-mono text-xs shrink-0 group/copybtn">
                          <span className="truncate max-w-[320px]">{session.tool_session_id}</span>
                          {copiedId === session.tool_session_id ? (
                            <Check className="w-3.5 h-3.5 text-green-500 shrink-0" />
                          ) : (
                            <button
                              onClick={(e) => handleCopyId(session.tool_session_id, e)}
                              className="opacity-0 group-hover/copy:opacity-100 hover:text-foreground p-0.5 rounded transition-all shrink-0"
                              title={t('copy', 'Copy ID')}
                            >
                              <Copy className="w-3.5 h-3.5" />
                            </button>
                          )}
                        </div>
                        <div className="flex items-center gap-1.5 min-w-0 flex-1">
                          <FolderOpen className="w-3 h-3 shrink-0" />
                          <span className="truncate">{session.working_dir}</span>
                        </div>
                        <span className="text-xs font-normal tabular-nums shrink-0">
                          {formatTime(session.created_at)}
                        </span>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Delete Confirmation Modal */}
      {sessionToDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            <div className="p-5">
              <div className="flex items-center gap-3 text-destructive mb-3">
                <div className="bg-destructive/10 p-2 rounded-full">
                  <AlertCircle className="w-5 h-5" />
                </div>
                <h3 className="font-semibold">{t('removeSession', 'Delete Session')}</h3>
              </div>
              <p className="text-sm text-muted-foreground">
                {t('confirmRemove')}
              </p>
            </div>
            <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
              <button
                onClick={() => setSessionToDelete(null)}
                disabled={loading}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors disabled:opacity-50"
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                onClick={confirmDelete}
                disabled={loading}
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors disabled:opacity-50"
              >
                {loading && <Loader2 className="w-4 h-4 animate-spin" />}
                {t('delete', 'Delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
