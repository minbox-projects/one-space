import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Terminal, Plus, FolderOpen, Play, Trash2, Loader2, AlertCircle, Settings2, Edit2, Check, X, Copy } from 'lucide-react';
import type { AiProvidersState } from './AiEnvironments';
import { ToolIcon } from './AiEnvironments';
import { WorkflowPresetsPanel } from './WorkflowPresetsPanel';
import { RecentWorkflowRuns } from './RecentWorkflowRuns';
import {
  workflowsApplyDependencies,
  workflowsCheckDependencies,
  workflowsLaunchPreset,
  workflowsListPresets,
  type WorkflowDependencyState,
  type WorkflowPreset,
} from '@/lib/workflows';

type MCPServerLite = { id: string; name: string };
type SkillsCatalogLite = { id: string; source_id: string; rel_path: string; name: string };
type SkillsRepoLite = {
  repo_key: string;
  skill_id: string;
  name: string;
  source_id: string;
  source_rel_path: string;
  models: string[];
};
type MCPStateResp = { servers?: Array<{ id?: string; name?: string }> };
type SkillsCatalogResp = { data?: Array<{ id?: string; name?: string; source_id?: string; rel_path?: string }> };
type SkillsRepoListResp = {
  data?: Array<{
    repo_key?: string;
    skill_id?: string;
    name?: string;
    source_id?: string;
    source_rel_path?: string;
    models?: string[];
  }>;
};

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
}

interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

type AiModelId = 'claude' | 'gemini' | 'codex' | 'opencode';

type AiModelLaunchCommands = Record<AiModelId, string>;

interface SessionStorageConfig {
  default_ai_dir?: string;
  ai_model_launch_commands?: Partial<AiModelLaunchCommands>;
}

const AI_MODEL_OPTIONS: Array<{ id: AiModelId; name: string }> = [
  { id: 'claude', name: 'Claude Code' },
  { id: 'gemini', name: 'Gemini' },
  { id: 'codex', name: 'Codex' },
  { id: 'opencode', name: 'OpenCode' },
];

const DEFAULT_AI_MODEL_LAUNCH_COMMANDS: AiModelLaunchCommands = {
  claude: 'claude --session-id {session_id}',
  gemini: 'gemini',
  codex: 'codex',
  opencode: 'opencode',
};

function normalizeAiModelLaunchCommands(
  source?: Partial<AiModelLaunchCommands>,
): AiModelLaunchCommands {
  return {
    claude: typeof source?.claude === 'string' ? source.claude : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.claude,
    gemini: typeof source?.gemini === 'string' ? source.gemini : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.gemini,
    codex: typeof source?.codex === 'string' ? source.codex : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.codex,
    opencode: typeof source?.opencode === 'string' ? source.opencode : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.opencode,
  };
}

function encodeCatalogSkillValue(sourceId: string, relPath: string): string {
  return `catalog::${sourceId}::${relPath}`;
}

function encodeRepoSkillValue(repoKey: string): string {
  return `repo::${repoKey}`;
}

export function AiSessions({
  onNavigate,
  isVisible = false,
}: {
  onNavigate?: (tab: string, hash?: string) => void;
  isVisible?: boolean;
}) {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<AiSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cliInstalled, setCliInstalled] = useState(true);

  // New session modal state
  const [isCreating, setIsCreating] = useState(false);
  const [selectedCommandId, setSelectedCommandId] = useState<AiModelId>('claude');
  const [aiModelLaunchCommands, setAiModelLaunchCommands] = useState<AiModelLaunchCommands>(
    DEFAULT_AI_MODEL_LAUNCH_COMMANDS,
  );

  const [newSessionDir, setNewSessionDir] = useState('');
  
  // Custom states
  const [editingSession, setEditingSession] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [sessionToDelete, setSessionToDelete] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Active environments state
  const [providersState, setProvidersState] = useState<AiProvidersState | null>(null);
  const [workflowPresets, setWorkflowPresets] = useState<WorkflowPreset[]>([]);
  const [showWorkflowPresetsPanel, setShowWorkflowPresetsPanel] = useState(false);
  const [selectedWorkflowPresetId, setSelectedWorkflowPresetId] = useState<string>('');
  const [selectedWorkflowDeps, setSelectedWorkflowDeps] = useState<WorkflowDependencyState | null>(null);
  const [workflowMcpNameMap, setWorkflowMcpNameMap] = useState<Record<string, string>>({});
  const [workflowSkillNameMap, setWorkflowSkillNameMap] = useState<Record<string, string>>({});
  const [checkingWorkflowDeps, setCheckingWorkflowDeps] = useState(false);
  const [applyingWorkflowDeps, setApplyingWorkflowDeps] = useState(false);
  const [activeContentTab, setActiveContentTab] = useState<'sessions' | 'runs'>('sessions');
  const [toolFilter, setToolFilter] = useState<string>('all');
  const [modelFilter, setModelFilter] = useState<string>('all');
  const [nameFilter, setNameFilter] = useState('');
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
      const cfg = await invoke<SessionStorageConfig>('get_storage_config');
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
      const res: ApiResp<AiProvidersState> = await invoke('providers_list');
      setProvidersState(res.data);
    } catch (e) {
      console.error(e);
    }
  }, [isTauri]);

  const loadWorkflowPresets = useCallback(async () => {
    if (!isTauri) return;
    try {
      const resp = await workflowsListPresets();
      setWorkflowPresets(resp.data || []);
    } catch (e) {
      console.error('Failed to load workflow presets', e);
    }
  }, [isTauri]);

  const loadSessions = useCallback(async ({ silent = false }: { silent?: boolean } = {}) => {
    if (!isTauri) {
      setError(t('notInTauri'));
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
      setError(err.toString());
    } finally {
      sessionsLoadingRef.current = false;
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
      if (selectedWorkflowPresetId) {
        await workflowsLaunchPreset({
          preset_id: selectedWorkflowPresetId,
          override_working_dir: newSessionDir || undefined,
        });
      } else {
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
      }
      
      emit('refresh-counts').catch(console.error);
      
      setIsCreating(false);
      setNewSessionDir('');
      setSelectedWorkflowPresetId('');
      setSelectedWorkflowDeps(null);
      await loadSessions();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      creatingRef.current = false;
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
          tool: updatedSession.model_type
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
    try {
      setLoading(true);
      await invoke('install_cli');
      checkCli();
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

  const handleNewSession = async () => {
    await Promise.all([
      loadAiSessionConfig(),
      loadProvidersState(),
      loadWorkflowPresets()
    ]);
    setSelectedWorkflowPresetId('');
    setSelectedWorkflowDeps(null);
    setIsCreating(true);
  };

  const commandIdFromTool = (tool: string) => {
    const normalized = tool.toLowerCase();
    if (normalized === 'claude') return 'claude';
    if (normalized === 'gemini') return 'gemini';
    if (normalized === 'codex') return 'codex';
    if (normalized === 'opencode') return 'opencode';
    return selectedCommandId;
  };

  const handleOpenAiSessionSettings = () => {
    const win = window as typeof window & { setSettingsTab?: (tab: string) => void };
    win.setSettingsTab?.('ai');
    onNavigate?.('settings');
  };

  const applyWorkflowPresetToForm = async (presetId: string) => {
    setSelectedWorkflowPresetId(presetId);
    if (!presetId) {
      setSelectedWorkflowDeps(null);
      setWorkflowMcpNameMap({});
      setWorkflowSkillNameMap({});
      return;
    }
    const preset = workflowPresets.find((item) => item.id === presetId);
    if (!preset) return;
    if (preset.working_dir?.trim()) {
      setNewSessionDir(preset.working_dir);
    }
    setSelectedCommandId(commandIdFromTool(preset.tool));
    setCheckingWorkflowDeps(true);
    try {
      const [depsResp, mcpResp, catalogResp, repoResp] = await Promise.all([
        workflowsCheckDependencies(presetId),
        invoke('get_mcp_servers') as Promise<MCPStateResp>,
        invoke('skills_list_catalog', { model: preset.tool }) as Promise<SkillsCatalogResp>,
        invoke('skills_repo_list') as Promise<SkillsRepoListResp>,
      ]);
      setSelectedWorkflowDeps(depsResp.data);

      const mcpList: MCPServerLite[] = (Array.isArray(mcpResp?.servers) ? mcpResp.servers : [])
        .map((server) => ({
          id: String(server?.id || '').trim(),
          name: String(server?.name || '').trim(),
        }))
        .filter((server) => Boolean(server.id && server.name));
      setWorkflowMcpNameMap(
        mcpList.reduce<Record<string, string>>((acc, server) => {
          acc[server.id] = server.name;
          return acc;
        }, {})
      );

      const catalog: SkillsCatalogLite[] = (Array.isArray(catalogResp?.data) ? catalogResp.data : [])
        .map((item) => ({
          id: String(item?.id || '').trim(),
          source_id: String(item?.source_id || '').trim(),
          rel_path: String(item?.rel_path || item?.id || '').trim(),
          name: String(item?.name || '').trim(),
        }))
        .filter((item) => Boolean(item.source_id && item.rel_path && item.name));
      const recommendedSourceRef = new Set(catalog.map((item) => `${item.source_id}::${item.rel_path}`));
      const repo: SkillsRepoLite[] = (Array.isArray(repoResp?.data) ? repoResp.data : [])
        .map((item) => ({
          repo_key: String(item?.repo_key || '').trim(),
          skill_id: String(item?.skill_id || '').trim(),
          name: String(item?.name || '').trim(),
          source_id: String(item?.source_id || '').trim(),
          source_rel_path: String(item?.source_rel_path || '').trim(),
          models: Array.isArray(item?.models)
            ? item.models.map((model) => String(model || '').toLowerCase().trim()).filter(Boolean)
            : [],
        }))
        .filter((item) => Boolean(item.repo_key && item.name));

      const nameMap: Record<string, string> = {};
      const setSkillName = (keys: string[], name: string) => {
        keys.forEach((key) => {
          const normalized = key.trim();
          if (!normalized || nameMap[normalized]) return;
          nameMap[normalized] = name;
        });
      };

      catalog.forEach((item) => {
        setSkillName(
          [
            encodeCatalogSkillValue(item.source_id, item.rel_path),
            `${item.source_id}::${item.rel_path}`,
            item.rel_path,
            item.id,
            encodeCatalogSkillValue(item.source_id, item.id),
          ],
          item.name,
        );
      });
      repo.forEach((item) => {
        if (item.models.length > 0 && !item.models.includes(String(preset.tool).toLowerCase())) return;
        if (item.source_id && item.source_rel_path && recommendedSourceRef.has(`${item.source_id}::${item.source_rel_path}`)) {
          return;
        }
        setSkillName(
          [
            encodeRepoSkillValue(item.repo_key),
            item.repo_key,
            item.skill_id,
            `${item.source_id}::${item.source_rel_path}`,
            item.source_rel_path,
            encodeCatalogSkillValue(item.source_id, item.source_rel_path),
          ],
          item.name,
        );
      });
      setWorkflowSkillNameMap(nameMap);
    } catch (e) {
      console.error(e);
      setSelectedWorkflowDeps(null);
      setWorkflowMcpNameMap({});
      setWorkflowSkillNameMap({});
    } finally {
      setCheckingWorkflowDeps(false);
    }
  };

  const formatWorkflowMcpList = (ids: string[]) =>
    ids.map((id) => workflowMcpNameMap[id] || id).join(', ') || '-';
  const formatWorkflowSkillList = (ids: string[]) =>
    ids.map((id) => workflowSkillNameMap[id] || id).join(', ') || '-';
  const formatWorkflowMcpDeps = (ids: string[], names?: string[]) =>
    (names && names.length > 0 ? names.join(', ') : formatWorkflowMcpList(ids)) || '-';
  const formatWorkflowSkillDeps = (ids: string[], names?: string[]) =>
    (names && names.length > 0 ? names.join(', ') : formatWorkflowSkillList(ids)) || '-';

  const sessionToolOptions = useMemo(() => Array.from(
    new Set(
      sessions
        .map((session) => session.model_type?.trim().toLowerCase())
        .filter((tool): tool is string => Boolean(tool))
    )
  ).sort(), [sessions]);

  const toolFilteredSessions = useMemo(() => (
    toolFilter === 'all'
      ? sessions
      : sessions.filter((session) => (session.model_type?.trim().toLowerCase() || '') === toolFilter)
  ), [sessions, toolFilter]);

  const sessionModelOptions = useMemo(() => Array.from(
    new Set(
      toolFilteredSessions
        .map((session) => session.model_name?.trim())
        .filter((model): model is string => Boolean(model))
    )
  ).sort((a, b) => a.localeCompare(b)), [toolFilteredSessions]);

  useEffect(() => {
    if (modelFilter !== 'all' && !sessionModelOptions.includes(modelFilter)) {
      setModelFilter('all');
    }
  }, [modelFilter, sessionModelOptions]);

  useEffect(() => {
    if (!isVisible) return;
    if (activeContentTab !== 'runs' && !showWorkflowPresetsPanel) return;
    void loadWorkflowPresets();
  }, [isVisible, activeContentTab, showWorkflowPresetsPanel, loadWorkflowPresets]);

  const getSessionDisplayName = useCallback((session: AiSession) => {
    if (session.name?.trim()) {
      return session.name;
    }
    if (session.tool_session_id?.trim()) {
      return session.tool_session_id;
    }
    return t('sessionTitleSyncingFromHistory', 'Syncing from history');
  }, [t]);

  const filteredSessions = useMemo(() => sessions.filter((session) => {
    const normalizedTool = session.model_type?.trim().toLowerCase() || '';
    const normalizedModel = session.model_name?.trim() || '';
    const displayName = getSessionDisplayName(session);
    const normalizedName = displayName.toLowerCase();
    const normalizedQuery = nameFilter.trim().toLowerCase();

    if (toolFilter !== 'all' && normalizedTool !== toolFilter) {
      return false;
    }
    if (modelFilter !== 'all' && normalizedModel !== modelFilter) {
      return false;
    }
    if (normalizedQuery && !normalizedName.includes(normalizedQuery)) {
      return false;
    }
    return true;
  }), [sessions, toolFilter, modelFilter, nameFilter, getSessionDisplayName]);

  const handleApplyWorkflowDependencies = async () => {
    if (!selectedWorkflowPresetId) return;
    setApplyingWorkflowDeps(true);
    try {
      const result = await workflowsApplyDependencies(selectedWorkflowPresetId);
      setSelectedWorkflowDeps(result.data.dependencies_after);
      await Promise.all([loadProvidersState(), loadWorkflowPresets()]);
      alert(t('workflowPresetAppliedSummary', {
        defaultValue: 'Dependencies applied: MCP linked {{linked}}, MCP enabled {{enabled}}, Skills installed {{installed}}',
        linked: result.data.linked_mcp_count,
        enabled: result.data.enabled_mcp_switch_count,
        installed: result.data.installed_skill_count,
      }));
    } catch (e: any) {
      setError(e.toString());
    } finally {
      setApplyingWorkflowDeps(false);
    }
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
            onClick={() => setShowWorkflowPresetsPanel((prev) => !prev)}
            className={`px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors ${
              showWorkflowPresetsPanel
                ? 'bg-secondary text-secondary-foreground'
                : 'bg-muted text-muted-foreground hover:bg-muted/80'
            }`}
          >
            <Settings2 className="w-4 h-4" />
            {t('workflowPresets', 'Workflow Presets')}
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

      {showWorkflowPresetsPanel && (
        <WorkflowPresetsPanel
          onChanged={(presets) => setWorkflowPresets(presets)}
          onSelectPreset={(presetId) => {
            if (presetId) {
              void applyWorkflowPresetToForm(presetId);
            }
          }}
        />
      )}

      {isCreating && (
        <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Terminal className="w-4 h-4 text-primary" />
            {t('createNewAiSession')}
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2 md:col-span-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t('workflowPreset', 'Workflow Preset')}
              </label>
              <div className="flex gap-2">
                <select
                  value={selectedWorkflowPresetId}
                  onChange={(e) => {
                    void applyWorkflowPresetToForm(e.target.value);
                  }}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                >
                  <option value="">{t('workflowPresetNoManual', 'No preset (manual)')}</option>
                  {workflowPresets.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {preset.name} ({preset.tool}/{preset.launch_scope || 'shared'})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={() => setShowWorkflowPresetsPanel(true)}
                  className="px-3 rounded-md border transition-colors bg-background hover:bg-muted text-muted-foreground"
                  title={t('workflowPresetOpenEditor', 'Open preset editor')}
                >
                  <Settings2 className="w-4 h-4" />
                </button>
              </div>
              {selectedWorkflowPresetId && (
                <div className="text-xs rounded-md border bg-muted/20 px-2.5 py-2 space-y-1">
                  {checkingWorkflowDeps ? (
                    <div className="text-muted-foreground flex items-center gap-1.5">
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      {t('workflowPresetCheckingDeps', 'Checking preset dependencies...')}
                    </div>
                  ) : selectedWorkflowDeps ? (
                    <>
                      <div className="text-muted-foreground">
                        {t('workflowPresetMissingMcp', 'Missing MCP')}:{' '}
                        <span className="font-mono">
                          {formatWorkflowMcpDeps(
                            selectedWorkflowDeps.missing_mcp_server_ids,
                            selectedWorkflowDeps.missing_mcp_names,
                          )}
                        </span>
                      </div>
                      <div className="text-muted-foreground">
                        {t('workflowPresetInactiveMcp', 'Inactive MCP Link')}:{' '}
                        <span className="font-mono">
                          {formatWorkflowMcpDeps(
                            selectedWorkflowDeps.inactive_mcp_server_ids,
                            selectedWorkflowDeps.inactive_mcp_names,
                          )}
                        </span>
                      </div>
                      <div className="text-muted-foreground">
                        {t('workflowPresetMissingSkills', 'Missing Skills')}:{' '}
                        <span className="font-mono">
                          {formatWorkflowSkillDeps(
                            selectedWorkflowDeps.missing_skill_ids,
                            selectedWorkflowDeps.missing_skill_names,
                          )}
                        </span>
                      </div>
                      {(selectedWorkflowDeps.inactive_mcp_server_ids.length > 0 ||
                        selectedWorkflowDeps.installable_skill_ids.length > 0) && (
                        <button
                          type="button"
                          onClick={() => void handleApplyWorkflowDependencies()}
                          disabled={applyingWorkflowDeps}
                          className="mt-1 px-2 py-1 rounded border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                        >
                          {applyingWorkflowDeps ? (
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <Check className="w-3.5 h-3.5" />
                          )}
                          {t('workflowPresetFixDeps', 'One-click fix dependencies')}
                        </button>
                      )}
                    </>
                  ) : (
                    <div className="text-muted-foreground">
                      {t('workflowPresetNoDepsData', 'Dependency data unavailable.')}
                    </div>
                  )}
                </div>
              )}
            </div>
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
              disabled={loading || (!selectedWorkflowPresetId && !newSessionDir)}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
            >
              {loading && <Loader2 className="w-4 h-4 animate-spin" />}
              {t('launch')}
            </button>
          </div>
        </div>
      )}

      <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
        <button
          type="button"
          onClick={() => setActiveContentTab('sessions')}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeContentTab === 'sessions'
              ? 'bg-black text-white'
              : 'bg-white text-black'
          }`}
        >
          {t('terminalSessions', 'Terminal Sessions')}
        </button>
        <button
          type="button"
          onClick={() => setActiveContentTab('runs')}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeContentTab === 'runs'
              ? 'bg-black text-white'
              : 'bg-white text-black'
          }`}
        >
          {t('workflowTab', 'Workflow')}
        </button>
      </div>

      {activeContentTab === 'sessions' ? (
        <div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
          <div className="grid gap-3 md:grid-cols-3">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t('filterByTool', 'Tool')}
              </label>
              <div className="flex h-10 w-full items-center gap-2 rounded-md border border-input bg-background px-3">
                <ToolIcon
                  tool={toolFilter === 'all' ? 'terminal' : toolFilter}
                  className="w-4 h-4 text-muted-foreground shrink-0"
                />
                <select
                  value={toolFilter}
                  onChange={(e) => setToolFilter(e.target.value)}
                  className="h-full w-full bg-transparent text-sm outline-none"
                >
                  <option value="all">{t('allTools', 'All tools')}</option>
                  {sessionToolOptions.map((tool) => (
                    <option key={tool} value={tool}>
                      {AI_MODEL_OPTIONS.find((item) => item.id === tool)?.name || tool}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t('filterByModel', 'Model')}
              </label>
              <select
                value={modelFilter}
                onChange={(e) => setModelFilter(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              >
                <option value="all">{t('allModels', 'All models')}</option>
                {sessionModelOptions.map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
            </div>

            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {t('filterByName', 'Name')}
              </label>
              <input
                type="text"
                value={nameFilter}
                onChange={(e) => setNameFilter(e.target.value)}
                placeholder={t('filterSessionsByName', 'Filter sessions by name...')}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              />
            </div>
          </div>

          <div className="text-xs text-muted-foreground">
            {t('sessionFilterSummary', {
              defaultValue: 'Showing {{visible}} of {{total}} sessions',
              visible: filteredSessions.length,
              total: sessions.length,
            })}
          </div>
        </div>
      ) : null}

      {activeContentTab === 'sessions' ? (
        <div className="flex-1 overflow-auto rounded-xl border bg-card text-card-foreground shadow-sm">
          {sessions.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
              <Terminal className="w-10 h-10 mb-3 opacity-20" />
              <p>{t('noActiveSessions')}</p>
              <p className="text-sm mt-1">{t('createOneToGetStarted')}</p>
            </div>
          ) : filteredSessions.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
              <Terminal className="w-10 h-10 mb-3 opacity-20" />
              <p>{t('noMatchingSessions', 'No matching sessions')}</p>
              <p className="text-sm mt-1">{t('adjustSessionFilters', 'Try adjusting the tool, model, or name filters.')}</p>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {filteredSessions.map((session) => {
                const canResume = Boolean(
                  session.tool_session_id &&
                  session.status !== 'unbound' &&
                  session.status !== 'pending_bind'
                );
                const isPendingBind = session.status === 'pending_bind';
                const isUnbound = session.status === 'unbound';
                const displayName = getSessionDisplayName(session);
                const displayModelName = session.model_name?.trim()
                  ? session.model_name
                  : null;
                const displaySessionId = session.tool_session_id ||
                  (isPendingBind
                    ? t('sessionIdPendingBind', 'Syncing from history')
                    : t('sessionIdUnavailable', 'ID unavailable'));
                return (
                <div
                  key={session.id}
                  className="p-4 hover:bg-muted/30 transition-colors group/copy"
                  style={{ contentVisibility: 'auto', containIntrinsicSize: '108px' }}
                >
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
                            <span className="font-semibold text-base truncate max-w-md">{displayName}</span>
                            {displayModelName ? (
                              <span className="px-2 py-0.5 rounded-full bg-muted text-muted-foreground text-[11px] font-mono shrink-0">
                                {displayModelName}
                              </span>
                            ) : null}
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
                              onClick={() => canResume && handleLaunch(session)}
                              disabled={!canResume}
                              className={`px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors ${
                                canResume
                                  ? 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                                  : 'bg-muted text-muted-foreground cursor-not-allowed'
                              }`}
                            >
                              <Play className="w-3.5 h-3.5" />
                              {canResume ? t('continue', 'Continue') : t('unavailable', 'Unavailable')}
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
                            <span className="truncate max-w-[320px]">{displaySessionId}</span>
                            {session.tool_session_id && copiedId === session.tool_session_id ? (
                              <Check className="w-3.5 h-3.5 text-green-500 shrink-0" />
                            ) : session.tool_session_id ? (
                              <button
                                onClick={(e) => handleCopyId(session.tool_session_id, e)}
                                className="opacity-0 group-hover/copy:opacity-100 hover:text-foreground p-0.5 rounded transition-all shrink-0"
                                title={t('copy', 'Copy ID')}
                              >
                                <Copy className="w-3.5 h-3.5" />
                              </button>
                            ) : (
                              <span className="text-[11px] text-muted-foreground/80">
                                {isPendingBind
                                  ? t('sessionBinding', 'syncing from history')
                                  : isUnbound
                                    ? t('sessionUnbound', 'unbound')
                                    : t('sessionIdUnavailable', 'unavailable')}
                              </span>
                            )}
                          </div>
                          <div className="flex items-center gap-1.5 min-w-0 flex-1">
                            <FolderOpen className="w-3 h-3 shrink-0" />
                            <span className="truncate">{session.working_dir}</span>
                          </div>
                          <span className="text-xs font-normal tabular-nums shrink-0">
                            {formatTime(session.last_used_at || session.created_at)}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
                );
              })}
            </div>
          )}
        </div>
      ) : (
        <RecentWorkflowRuns presets={workflowPresets} />
      )}

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
