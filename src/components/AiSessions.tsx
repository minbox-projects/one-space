import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { Terminal, Plus, FolderOpen, Play, Trash2, Loader2, AlertCircle } from 'lucide-react';

interface TmuxSession {
  name: String;
  created: number;
  attached: boolean;
  path: string;
}

export function AiSessions() {
  const [sessions, setSessions] = useState<TmuxSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // New session modal state
  const [isCreating, setIsCreating] = useState(false);
  const [newSessionName, setNewSessionName] = useState('');
  const [newSessionCommand, setNewSessionCommand] = useState('claude code');
  const [newSessionDir, setNewSessionDir] = useState('');

  const loadSessions = async () => {
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
    // Poll every 5 seconds to get updated statuses
    const interval = setInterval(loadSessions, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleSelectDir = async () => {
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
    if (!newSessionName || !newSessionDir) {
      setError("Please provide both a session name and directory.");
      return;
    }
    try {
      setLoading(true);
      await invoke('create_tmux_session', {
        sessionName: newSessionName.replace(/\s+/g, '_'),
        workingDir: newSessionDir,
        command: newSessionCommand
      });
      
      // Auto attach to newly created session
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
    try {
      await invoke('attach_tmux_session', { sessionName });
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    }
  };

  const handleKill = async (sessionName: string) => {
    if (!confirm(`Are you sure you want to kill session ${sessionName}?`)) return;
    try {
      setLoading(true);
      await invoke('kill_tmux_session', { sessionName });
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">AI Sessions</h2>
          <p className="text-sm text-muted-foreground mt-1">Manage your terminal-based AI assistants</p>
        </div>
        <button
          onClick={() => setIsCreating(true)}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
        >
          <Plus className="w-4 h-4" />
          New Session
        </button>
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
            Create New AI Session
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Session Name</label>
              <input 
                type="text" 
                placeholder="e.g. project_x_claude" 
                value={newSessionName}
                onChange={(e) => setNewSessionName(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">AI Command</label>
              <select 
                value={newSessionCommand}
                onChange={(e) => setNewSessionCommand(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <option value="claude code">Claude Code</option>
                <option value="gemini -y">Gemini</option>
                <option value="opencode">OpenCode</option>
                <option value="bash">Bash (Empty Terminal)</option>
              </select>
            </div>
            <div className="space-y-2 md:col-span-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Working Directory</label>
              <div className="flex gap-2">
                <input 
                  type="text" 
                  readOnly
                  placeholder="Select a project directory..." 
                  value={newSessionDir}
                  className="flex h-10 w-full rounded-md border border-input bg-muted/50 px-3 py-2 text-sm ring-offset-background cursor-not-allowed"
                />
                <button 
                  onClick={handleSelectDir}
                  className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                >
                  <FolderOpen className="w-4 h-4" />
                  Browse
                </button>
              </div>
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button 
              onClick={() => setIsCreating(false)}
              className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
            >
              Cancel
            </button>
            <button 
              onClick={handleCreate}
              disabled={loading || !newSessionName || !newSessionDir}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
            >
              {loading && <Loader2 className="w-4 h-4 animate-spin" />}
              Launch
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-auto rounded-xl border bg-card text-card-foreground shadow-sm">
        {sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
            <Terminal className="w-10 h-10 mb-3 opacity-20" />
            <p>No active AI sessions found.</p>
            <p className="text-sm mt-1">Create one to get started.</p>
          </div>
        ) : (
          <div className="divide-y">
            {sessions.map((session, idx) => (
              <div key={idx} className="p-4 flex flex-col md:flex-row md:items-center justify-between gap-4 hover:bg-muted/30 transition-colors">
                <div className="flex items-start gap-4">
                  <div className={`mt-1 w-2 h-2 rounded-full shrink-0 ${session.attached ? 'bg-amber-500' : 'bg-green-500'}`} 
                       title={session.attached ? 'Attached elsewhere' : 'Running in background'}></div>
                  <div>
                    <h4 className="font-semibold text-base flex items-center gap-2">
                      {session.name}
                      <span className="text-xs text-muted-foreground font-normal ml-2">
                        {formatTime(session.created)}
                      </span>
                      {session.attached && <span className="text-[10px] uppercase bg-amber-500/20 text-amber-500 px-2 py-0.5 rounded-full font-bold tracking-wider">Attached</span>}
                    </h4>
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
                    Attach
                  </button>
                  <button
                    onClick={() => handleKill(session.name as string)}
                    className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                    Kill
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}