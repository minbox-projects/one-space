import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Terminal, Plus, FolderOpen, Loader2, AlertCircle, Settings2 } from 'lucide-react';
import type { AiProvidersState } from './AiEnvironments';
import { ToolIcon } from './AiEnvironments';
import { AiSessionsList } from './AiSessionsList';
import { TerminalPermissionConfirmDialog } from './TerminalPermissionConfirmDialog';
import { useToast } from './ToastProvider';
import {
  type TerminalPermissionMode,
} from '@/lib/terminalPermissions';

interface AiSession {
  id: string;
  name: string;
  working_dir: string;
  model_type: string;
  model_name?: string | null;
  tool_session_id: string;
  status?: string;
  created_at: number;
  last_used_at?: number;
  favorited_at?: number | null;
}

interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

type AiModelId = 'claude' | 'antigravity' | 'codex' | 'opencode';

type AiModelLaunchCommands = Record<AiModelId, string>;

interface SessionStorageConfig {
  default_ai_dir?: string;
  ai_model_launch_commands?: Partial<AiModelLaunchCommands>;
}

const AI_MODEL_OPTIONS: Array<{ id: AiModelId; name: string }> = [
  { id: 'claude', name: 'Claude Code' },
  { id: 'antigravity', name: 'Antigravity' },
  { id: 'codex', name: 'Codex' },
  { id: 'opencode', name: 'OpenCode' },
];

const DEFAULT_AI_MODEL_LAUNCH_COMMANDS: AiModelLaunchCommands = {
  claude: 'claude --session-id {session_id}',
  antigravity: 'agy',
  codex: 'codex',
  opencode: 'opencode',
};

function normalizeAiModelLaunchCommands(
  source?: Partial<AiModelLaunchCommands>,
): AiModelLaunchCommands {
  return {
    claude: typeof source?.claude === 'string' ? source.claude : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.claude,
    antigravity: typeof source?.antigravity === 'string' ? source.antigravity : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.antigravity,
    codex: typeof source?.codex === 'string' ? source.codex : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.codex,
    opencode: typeof source?.opencode === 'string' ? source.opencode : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.opencode,
  };
}

/**
 * Extract a human-readable error string from a Tauri invoke error.
 * Tauri v2 commands that return Rust `ApiErr` serialize as:
 *   { ok: false, code: "...", message: "..." }
 */
function formatInvokeError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const maybe = err as { code?: unknown; message?: unknown; error?: unknown };
    // Prefer `message` (human-readable) but prepend `code` if present for machine-readable checks
    const code = typeof maybe.code === "string" ? maybe.code : null;
    const msg = typeof maybe.message === "string" ? maybe.message : null;
    const errMsg = typeof maybe.error === "string" ? maybe.error : null;
    if (msg) return code ? `[${code}] ${msg}` : msg;
    if (errMsg) return code ? `[${code}] ${errMsg}` : errMsg;
    if (code) return `[${code}]`;
    try {
      return JSON.stringify(err);
    } catch (_e) {
      return String(err);
    }
  }
  return String(err);
}

/** Extract the error code from a Tauri invoke error, if present. */
function getErrorCode(err: unknown): string | null {
  if (err && typeof err === "object") {
    const maybe = err as { code?: unknown };
    if (typeof maybe.code === "string") return maybe.code;
  }
  return null;
}

export function AiSessions({
  onNavigate,
  isVisible = false,
}: {
  onNavigate?: (tab: string, hash?: string) => void;
  isVisible?: boolean;
}) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [sessions, setSessions] = useState<AiSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [sessionsInitialized, setSessionsInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cliInstalled, setCliInstalled] = useState(true);

  // New session modal state
  const [isCreating, setIsCreating] = useState(false);
  const [selectedCommandId, setSelectedCommandId] = useState<AiModelId>('claude');
  const [aiModelLaunchCommands, setAiModelLaunchCommands] = useState<AiModelLaunchCommands>(
    DEFAULT_AI_MODEL_LAUNCH_COMMANDS,
  );

  const [newSessionDir, setNewSessionDir] = useState('');

  // Active environments state
  const [providersState, setProvidersState] = useState<AiProvidersState | null>(null);

  // Permission confirmation state
  const [permissionDialogOpen, setPermissionDialogOpen] = useState(false);
  const [permissionDialogSession, setPermissionDialogSession] = useState<AiSession | null>(null);
  const creatingRef = useRef(false);
  const isVisibleRef = useRef(isVisible);
  const sessionsLoadedRef = useRef(false);
  const sessionsLoadingRef = useRef(false);
  const pendingRefreshRef = useRef(false);
  const refreshTimerRef = useRef<number | null>(null);
  const sessionBootstrapLoadedRef = useRef(false);

  const isTauri = '__TAURI_INTERNALS__' in window;


  useEffect(() => {
    isVisibleRef.current = isVisible;
  }, [isVisible]);

  const checkCli = useCallback(async () => {
    if (!isTauri) return;
    try {
      const installed = await invoke<boolean>('check_cli_installed');
      setCliInstalled(installed);
    } catch (e) {
      console.error("Failed to check CLI", e);
    }
  }, [isTauri]);

  const loadAiSessionConfig = useCallback(async () => {
    if (!isTauri) return;
    try {
      const cfg = await invoke<SessionStorageConfig & { ai_model_permission_modes?: Record<string, string> }>('get_storage_config');
      if (cfg.default_ai_dir) {
        setNewSessionDir(cfg.default_ai_dir);
      }
      setAiModelLaunchCommands(normalizeAiModelLaunchCommands(cfg.ai_model_launch_commands));
    } catch (e) {
      console.error("Failed to load AI session config", e);
    }
  }, [isTauri]);

  const loadProvidersState = useCallback(async () => {
    if (!isTauri) return;
    try {
      const res: ApiResp<AiProvidersState> = await invoke('service_providers_list');
      setProvidersState(res.data);
    } catch (e) {
      console.error(e);
    }
  }, [isTauri]);

  const loadSessions = useCallback(async ({ silent = false }: { silent?: boolean } = {}) => {
    if (!isTauri) {
      setError(t('notInTauri'));
      setSessionsInitialized(true);
      return;
    }
    if (sessionsLoadingRef.current) {
      pendingRefreshRef.current = true;
      return;
    }

    try {
      sessionsLoadingRef.current = true;
      if (!silent) {
        setLoading(true);
      }
      setError(null);
      const res: ApiResp<AiSession[]> = await invoke('sessions_list');
      setSessions(res.data);
      sessionsLoadedRef.current = true;
      pendingRefreshRef.current = false;
    } catch (err: any) {
      setError(formatInvokeError(err));
    } finally {
      sessionsLoadingRef.current = false;
      setSessionsInitialized(true);
      if (!silent) {
        setLoading(false);
      }
    }
  }, [isTauri, t]);

  useEffect(() => {
    if (!isVisible) {
      return;
    }

    if (!sessionBootstrapLoadedRef.current) {
      sessionBootstrapLoadedRef.current = true;
      void Promise.all([checkCli(), loadAiSessionConfig()]);
    }

    if (!sessionsLoadedRef.current || pendingRefreshRef.current) {
      void loadSessions({ silent: sessionsLoadedRef.current });
    }
  }, [isVisible, checkCli, loadAiSessionConfig, loadSessions]);

  const scheduleSessionsRefresh = useCallback((silent = true) => {
    if (refreshTimerRef.current !== null) {
      return;
    }
    refreshTimerRef.current = window.setTimeout(() => {
      refreshTimerRef.current = null;
      if (!isVisibleRef.current) {
        pendingRefreshRef.current = true;
        return;
      }
      void loadSessions({ silent });
    }, 80);
  }, [loadSessions]);

  useEffect(() => {
    const handleFocus = () => {
      if (!isVisibleRef.current) return;
      scheduleSessionsRefresh(true);
    };
    window.addEventListener('focus', handleFocus);

    let unlistenCounts: (() => void) | undefined;
    let unlistenSessions: (() => void) | undefined;

    const initListeners = async () => {
      unlistenCounts = await listen('refresh-counts', () => {
        scheduleSessionsRefresh(true);
      });
      unlistenSessions = await listen('sessions-updated', () => {
        scheduleSessionsRefresh(true);
      });
    };
    initListeners();

    return () => {
      window.removeEventListener('focus', handleFocus);
      if (refreshTimerRef.current !== null) {
        window.clearTimeout(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
      if (unlistenCounts) unlistenCounts();
      if (unlistenSessions) unlistenSessions();
    };
  }, [scheduleSessionsRefresh]);
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
    if (creatingRef.current) return;
    if (!isTauri) {
      setError(t('notInTauri'));
      return;
    }

    try {
      creatingRef.current = true;
      setLoading(true);
      if (!newSessionDir) {
        setError(t('provideDirOnly', 'Please provide a working directory.'));
        return;
      }
      await invoke('sessions_create', {
        session: {
          name: '',
          working_dir: newSessionDir,
          tool: selectedCommandId,
          status: 'active'
        }
      });
      
      emit('refresh-counts').catch(console.error);
      
      setIsCreating(false);
      setNewSessionDir('');
      await loadSessions();
    } catch (err: any) {
      setError(formatInvokeError(err));
    } finally {
      creatingRef.current = false;
      setLoading(false);
    }
  };

  const handleLaunch = async (session: AiSession) => {
    if (!isTauri) return;
    // Always call without permissionMode first; backend will enforce confirmation if needed
    try {
      await invoke('sessions_launch', { sessionId: session.id });
      await loadSessions();
    } catch (err: unknown) {
      const code = getErrorCode(err);
      if (code === 'PERMISSION_CONFIRMATION_REQUIRED') {
        setPermissionDialogSession(session);
        setPermissionDialogOpen(true);
      } else {
        setError(formatInvokeError(err));
      }
    }
  };

  const handlePermissionConfirm = async (mode: TerminalPermissionMode) => {
    if (!permissionDialogSession) return;
    setPermissionDialogOpen(false);
    const session = permissionDialogSession;
    setPermissionDialogSession(null);
    try {
      await invoke('sessions_launch', { sessionId: session.id, permissionMode: mode });
      await loadSessions();
    } catch (err: any) {
      setError(formatInvokeError(err));
    }
  };

  const handlePermissionCancel = () => {
    setPermissionDialogOpen(false);
    setPermissionDialogSession(null);
  };

  const handleDelete = async (sessionId: string) => {
    if (!isTauri) return;
    try {
      setLoading(true);
      await invoke('sessions_delete', { sessionId });
      emit('refresh-counts').catch(console.error);
      await loadSessions();
    } catch (err: any) {
      setError(formatInvokeError(err));
    } finally {
      setLoading(false);
    }
  };

  const handleRename = async (session: AiSession, nextName: string) => {
    if (!isTauri) return;
    const normalizedName = nextName.trim();
    if (!normalizedName || normalizedName === session.name) {
      return;
    }

    try {
      setLoading(true);
      await invoke('sessions_update', {
        session: {
          id: session.id,
          name: normalizedName,
          working_dir: session.working_dir,
          tool: session.model_type,
        },
      });
      await loadSessions();
    } catch (err: any) {
      setError(formatInvokeError(err));
    } finally {
      setLoading(false);
    }
  };

  const handleFavoriteChange = async (session: AiSession, favorite: boolean) => {
    if (!isTauri) return;
    try {
      await invoke('sessions_set_favorite', { sessionId: session.id, favorite });
      await loadSessions({ silent: true });
    } catch (err: unknown) {
      setError(formatInvokeError(err));
    }
  };

  const handleInstallCli = async () => {
    try {
      setLoading(true);
      await invoke('install_cli');
      checkCli();
      pushToast({
        title: t('cliInstalled', 'CLI tool installed to ~/.local/bin/onespace'),
        kind: 'success',
      });
    } catch (err: any) {
      setError(formatInvokeError(err));
    } finally {
      setLoading(false);
    }
  };

  const handleNewSession = async () => {
    await Promise.all([
      loadAiSessionConfig(),
      loadProvidersState()
    ]);
    setIsCreating(true);
  };

  const handleOpenAiSessionSettings = () => {
    const win = window as typeof window & { setSettingsTab?: (tab: string) => void };
    win.setSettingsTab?.('ai');
    onNavigate?.('settings');
  };

  const renderActiveProvider = () => {
    if (!providersState || !selectedCommandId) return null;

    const toolType = selectedCommandId;

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
            onClick={handleInstallCli}
            disabled={loading}
            title={t('installCliTitle', 'Install CLI tool to ~/.local/bin')}
            className={`px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-all ${
              cliInstalled 
                ? 'bg-muted text-muted-foreground hover:bg-muted/80' 
                : 'bg-primary/10 text-primary hover:bg-primary/20 border border-primary/20'
            }`}
          >
            {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Terminal className="w-4 h-4" />}
            {cliInstalled ? t('reinstallCli', 'Update CLI') : t('installCli', 'Install CLI')}
          </button>
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
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('aiCommand')}</label>
              <div className="flex gap-2">
                <select 
                  value={selectedCommandId}
                  onChange={(e) => {
                    const id = e.target.value as AiModelId;
                    setSelectedCommandId(id);
                  }}
                  className="flex flex-1 h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {AI_MODEL_OPTIONS.map((cmd) => (
                    <option key={cmd.id} value={cmd.id}>
                      {cmd.name}
                    </option>
                  ))}
                </select>
                <button
                  onClick={handleOpenAiSessionSettings}
                  className="px-3 rounded-md border transition-colors bg-background hover:bg-muted text-muted-foreground"
                  title={t('goToAiSessionSettings', 'Configure in Settings')}
                >
                  <Settings2 className="w-4 h-4" />
                </button>
              </div>

              <div className="flex gap-2">
                <input
                  type="text"
                  readOnly
                  value={aiModelLaunchCommands[selectedCommandId] || ''}
                  className="flex h-9 w-full rounded-md border border-input bg-muted/50 px-3 py-2 text-xs font-mono text-muted-foreground cursor-default"
                />
                <button
                  type="button"
                  onClick={handleOpenAiSessionSettings}
                  className="px-3 rounded-md border bg-background hover:bg-muted text-xs text-muted-foreground transition-colors shrink-0"
                >
                  {t('goToSettings', 'Go to Settings')}
                </button>
              </div>
              
              {/* Active Provider Indicator */}
              {renderActiveProvider()}
            </div>

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
              disabled={loading || !newSessionDir}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
            >
              {loading && <Loader2 className="w-4 h-4 animate-spin" />}
              {t('launch')}
            </button>
          </div>
        </div>
      )}

      <AiSessionsList
        sessions={sessions}
        loading={loading || !sessionsInitialized}
        onLaunch={handleLaunch}
        onDelete={handleDelete}
        onRename={handleRename}
        onFavoriteChange={handleFavoriteChange}
      />

      {/* Permission confirmation dialog */}
      {permissionDialogSession && (
        <TerminalPermissionConfirmDialog
          open={permissionDialogOpen}
          toolId={permissionDialogSession.model_type.toLowerCase() as AiModelId}
          toolLabel={AI_MODEL_OPTIONS.find((o) => o.id === permissionDialogSession.model_type.toLowerCase())?.name || permissionDialogSession.model_type}
          onConfirm={handlePermissionConfirm}
          onCancel={handlePermissionCancel}
        />
      )}
    </div>
  );
}
