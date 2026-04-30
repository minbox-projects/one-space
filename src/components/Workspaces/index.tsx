import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import {
  ArrowLeft,
  Bot,
  Check,
  Copy,
  FolderOpen,
  Info,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Server,
  Settings2,
  Sparkles,
  Tag,
  Trash2,
} from 'lucide-react';
import { ToolIcon } from '../AiEnvironments';
import { AiSessionsList, type AiSessionListItem, type AiSessionsQueryState } from '../AiSessionsList';
import { TerminalPermissionConfirmDialog } from '../TerminalPermissionConfirmDialog';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { WorkspaceSkillsPanel } from './WorkspaceSkillsPanel';
import { WorkspaceSubagentsPanel } from './WorkspaceSubagentsPanel';
import {
  type AiModelId as PermAiModelId,
  type TerminalPermissionMode,
  getInvokeErrorCode,
  formatInvokeError,
} from '@/lib/terminalPermissions';
import type {
  CapabilityTargetTab,
  WorkspaceCapabilityContext,
  WorkspaceCapabilityEntry,
} from '../workspaceCapabilityContext';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';

type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
};

type WorkspaceRecord = {
  id: string;
  name: string;
  root_path: string;
  description?: string | null;
  tags: string[];
  source: string;
  created_at: number;
  updated_at: number;
  last_activity_at: number;
};

type WorkspaceView = {
  workspace: WorkspaceRecord;
  session_count: number;
};

type WorkspaceMcpBinding = {
  workspace_id: string;
  server_id: string;
  enabled_models: string[];
  created_at: number;
  updated_at: number;
};

type WorkspaceDetail = {
  workspace: WorkspaceView;
  mcp_bindings: WorkspaceMcpBinding[];
};

type MCPServer = {
  id: string;
  name: string;
  config_key?: string;
  description?: string;
  transport?: 'stdio' | 'http' | 'sse';
  command?: string;
  args?: string[];
  url?: string;
  http_url?: string;
};

type MCPStateResp = {
  servers?: MCPServer[];
};

type MCPModelSwitchState = Record<ModelId, boolean>;

type InstalledSkill = {
  id: string;
  model: ModelId;
  name: string;
  description?: string;
  source_id: string;
  source_rel_path: string;
  scope?: 'global' | 'project';
  project_root?: string | null;
};

type InstalledSubagent = {
  id: string;
  model: ModelId;
  name: string;
  description?: string;
  source_id: string;
  source_rel_path: string;
  scope?: 'global' | 'project';
  project_root?: string | null;
};

type WorkspaceTab = 'sessions' | 'mcp' | 'skills' | 'subagents';
type DialogMode = 'create' | 'edit';
type ModelId = 'claude' | 'gemini' | 'codex' | 'opencode';

type WorkspaceFormState = {
  id?: string;
  name: string;
  root_path: string;
  description: string;
  tags: string;
};

type CopyableSkill = InstalledSkill & { selection_key: string };
type CopyableSubagent = InstalledSubagent & { selection_key: string };
type WorkspaceMcpScope = 'global' | 'project';
type WorkspaceMcpEntry = {
  server: MCPServer;
  binding: WorkspaceMcpBinding | null;
  scope: WorkspaceMcpScope;
  enabled_models: ModelId[];
};
type WorkspaceMcpCatalogEntry = WorkspaceMcpEntry & {
  status: 'enabled_for_model' | 'enabled_user_level' | 'bound_other_models' | 'not_bound';
};
type WorkspaceSessionsListData = {
  items: AiSessionListItem[];
  total: number;
  tool_options: string[];
  model_options: string[];
};

const TOOL_OPTIONS: Array<{ id: ModelId; label: string }> = [
  { id: 'claude', label: 'Claude Code' },
  { id: 'gemini', label: 'Gemini' },
  { id: 'codex', label: 'Codex' },
  { id: 'opencode', label: 'OpenCode' },
];
const DEFAULT_MCP_MODEL_SWITCH_STATE: MCPModelSwitchState = {
  claude: false,
  gemini: false,
  codex: false,
  opencode: false,
};
const DEFAULT_WORKSPACE_SESSIONS_QUERY: AiSessionsQueryState = {
  toolFilter: 'all',
  modelFilter: 'all',
  nameFilter: '',
};
const TAB_LOADING_MIN_MS = 200;

function formatTs(ts?: number) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString();
}

function parseTagsInput(value: string) {
  return Array.from(
    new Set(
      value
        .split(/[,\n]/)
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

function buildSkillSelectionKey(item: InstalledSkill) {
  return `${item.model}::${item.source_id}::${item.source_rel_path}`;
}

function buildSubagentSelectionKey(item: InstalledSubagent) {
  return `${item.model}::${item.source_id}::${item.source_rel_path}`;
}

function getSourceBadgeLabel(source: string) {
  const normalized = String(source || '').trim().toLowerCase();
  if (normalized === 'session_auto') return 'Auto';
  if (normalized === 'copy_target') return 'Copied';
  return 'Manual';
}

function getSourceBadgeDescription(source: string) {
  const normalized = String(source || '').trim().toLowerCase();
  if (normalized === 'session_auto') {
    return 'Created automatically from an existing AI session working directory.';
  }
  if (normalized === 'copy_target') {
    return 'Created as the target workspace when copying configuration from another workspace.';
  }
  return 'Created manually from the workspace manager.';
}

function getSourceBadgeClassName(source: string) {
  const normalized = String(source || '').trim().toLowerCase();
  if (normalized === 'session_auto') {
    return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:border-emerald-400/30 dark:bg-emerald-400/10 dark:text-emerald-300';
  }
  return 'border-border text-muted-foreground';
}

function getScopeBadgeClassName(scope?: WorkspaceMcpScope) {
  return scope === 'global'
    ? 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300'
    : 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300';
}

function getSourceBadgeTranslationKeys(source: string) {
  const normalized = String(source || '').trim().toLowerCase();
  if (normalized === 'session_auto') {
    return {
      label: 'workspaceSourceAuto',
      description: 'workspaceSourceAutoDesc',
    };
  }
  if (normalized === 'copy_target') {
    return {
      label: 'workspaceSourceCopied',
      description: 'workspaceSourceCopiedDesc',
    };
  }
  return {
    label: 'workspaceSourceManual',
    description: 'workspaceSourceManualDesc',
  };
}

function compactWorkspaceRootPath(path: string) {
  const trimmed = normalizeText(path).trim();
  if (!trimmed) return '';

  const homePrefix =
    trimmed.match(/^\/Users\/[^/]+(?=\/|$)/)?.[0] ?? trimmed.match(/^\/home\/[^/]+(?=\/|$)/)?.[0];

  return homePrefix ? `~${trimmed.slice(homePrefix.length)}` : trimmed;
}

function normalizeText(value: unknown, fallback = '') {
  if (typeof value === 'string') return value;
  if (value == null) return fallback;
  return String(value);
}

function normalizeOptionalText(value: unknown) {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed ? value : null;
}

function normalizeStringArray(value: unknown) {
  const source = Array.isArray(value) ? value : typeof value === 'string' ? [value] : [];
  return Array.from(
    new Set(
      source
        .map((item) => normalizeText(item).trim())
        .filter(Boolean),
    ),
  );
}

function normalizeWorkspaceRecord(raw: any): WorkspaceRecord {
  return {
    id: normalizeText(raw?.id),
    name: normalizeText(raw?.name),
    root_path: normalizeText(raw?.root_path),
    description: normalizeOptionalText(raw?.description),
    tags: normalizeStringArray(raw?.tags),
    source: normalizeText(raw?.source),
    created_at: Number(raw?.created_at) || 0,
    updated_at: Number(raw?.updated_at) || 0,
    last_activity_at: Number(raw?.last_activity_at) || 0,
  };
}

function normalizeWorkspaceView(raw: any): WorkspaceView {
  return {
    workspace: normalizeWorkspaceRecord(raw?.workspace ?? raw),
    session_count: Number(raw?.session_count) || 0,
  };
}

function normalizeWorkspaceDetail(raw: any): WorkspaceDetail {
  const bindings = Array.isArray(raw?.mcp_bindings)
    ? raw.mcp_bindings.map((binding: any) => ({
        workspace_id: normalizeText(binding?.workspace_id),
        server_id: normalizeText(binding?.server_id),
        enabled_models: normalizeStringArray(binding?.enabled_models),
        created_at: Number(binding?.created_at) || 0,
        updated_at: Number(binding?.updated_at) || 0,
      }))
    : [];

  return {
    workspace: normalizeWorkspaceView(raw?.workspace),
    mcp_bindings: bindings,
  };
}

function createOptimisticWorkspaceDetail(
  view: WorkspaceView,
  previous: WorkspaceDetail | null,
): WorkspaceDetail {
  const previousWorkspaceId = previous?.workspace.workspace.id;
  return {
    workspace: view,
    mcp_bindings: previousWorkspaceId === view.workspace.id ? previous?.mcp_bindings || [] : [],
  };
}

function normalizeMcpServer(raw: any): MCPServer {
  const transport = normalizeText(raw?.transport, 'stdio').trim().toLowerCase();
  return {
    id: normalizeText(raw?.id),
    name: normalizeText(raw?.name, normalizeText(raw?.id)),
    config_key: normalizeOptionalText(raw?.config_key) || undefined,
    description: normalizeOptionalText(raw?.description) || undefined,
    transport:
      transport === 'http' || transport === 'sse' || transport === 'stdio'
        ? transport
        : 'stdio',
    command: normalizeOptionalText(raw?.command) || undefined,
    args: Array.isArray(raw?.args)
      ? raw.args.map((item: unknown) => normalizeText(item)).filter(Boolean)
      : undefined,
    url: normalizeOptionalText(raw?.url) || undefined,
    http_url: normalizeOptionalText(raw?.http_url) || undefined,
  };
}

function sortMcpServersByName(a: MCPServer, b: MCPServer) {
  return normalizeText(a?.name).localeCompare(normalizeText(b?.name), undefined, {
    sensitivity: 'base',
  });
}

function normalizeMcpModelSwitchState(raw: any): MCPModelSwitchState {
  return {
    claude: Boolean(raw?.claude),
    gemini: Boolean(raw?.gemini),
    codex: Boolean(raw?.codex),
    opencode: Boolean(raw?.opencode),
  };
}

function getMcpMergeKey(server: MCPServer) {
  return normalizeText(server.config_key || server.name || server.id)
    .trim()
    .toLowerCase();
}

function getMcpEnabledModelsFromSwitch(state: MCPModelSwitchState | undefined) {
  const normalized = state || DEFAULT_MCP_MODEL_SWITCH_STATE;
  return TOOL_OPTIONS.flatMap((tool) => (normalized[tool.id] ? [tool.id] : []));
}

function getMcpConnectionText(server: MCPServer) {
  const command = normalizeText(server.command).trim();
  if (command) {
    const args = Array.isArray(server.args) ? server.args.join(' ') : '';
    return `${command}${args ? ` ${args}` : ''}`;
  }
  return normalizeText(server.http_url || server.url, '-');
}

function createWorkspaceCapabilityContext(
  workspace: WorkspaceRecord,
  entry: WorkspaceCapabilityEntry,
): WorkspaceCapabilityContext {
  return {
    workspaceId: workspace.id,
    workspaceName: workspace.name,
    rootPath: workspace.root_path,
    persistence: 'one_shot',
    entry,
  };
}

export function Workspaces({
  isVisible = false,
  onNavigateToCapability,
}: {
  isVisible?: boolean;
  onNavigateToCapability?: (
    targetTab: CapabilityTargetTab,
    context: WorkspaceCapabilityContext,
  ) => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const isTauri = '__TAURI_INTERNALS__' in window;

  const [loading, setLoading] = useState(false);
  const [workspacesInitialized, setWorkspacesInitialized] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceView[]>([]);
  const [allTags, setAllTags] = useState<string[]>([]);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [activeDetail, setActiveDetail] = useState<WorkspaceDetail | null>(null);
  const [activeSessions, setActiveSessions] = useState<AiSessionListItem[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const [sessionsInitialized, setSessionsInitialized] = useState(false);
  const [sessionsTotal, setSessionsTotal] = useState(0);
  const [sessionToolOptions, setSessionToolOptions] = useState<string[]>([]);
  const [sessionModelOptions, setSessionModelOptions] = useState<string[]>([]);
  const [sessionQuery, setSessionQuery] = useState<AiSessionsQueryState>(DEFAULT_WORKSPACE_SESSIONS_QUERY);
  const [activeTab, setActiveTab] = useState<WorkspaceTab>('sessions');
  const [activeMcpModel, setActiveMcpModel] = useState<ModelId>('claude');
  const [mcpServers, setMcpServers] = useState<MCPServer[]>([]);
  const [mcpModelSwitchStates, setMcpModelSwitchStates] = useState<Record<string, MCPModelSwitchState>>({});
  const [mcpLoading, setMcpLoading] = useState(false);
  const [mcpInitialized, setMcpInitialized] = useState(false);
  const [mcpDialogServer, setMcpDialogServer] = useState<MCPServer | null>(null);
  const [mcpDialogModels, setMcpDialogModels] = useState<ModelId[]>([]);
  const [mcpDialogSubmitting, setMcpDialogSubmitting] = useState(false);
  const [mcpDialogError, setMcpDialogError] = useState('');
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogMode, setDialogMode] = useState<DialogMode>('create');
  const [formSubmitting, setFormSubmitting] = useState(false);
  const [formError, setFormError] = useState('');

  // Permission confirmation state
  const [permissionDialogOpen, setPermissionDialogOpen] = useState(false);
  const [permissionDialogSession, setPermissionDialogSession] = useState<AiSessionListItem | null>(null);

  const [formState, setFormState] = useState<WorkspaceFormState>({
    name: '',
    root_path: '',
    description: '',
    tags: '',
  });
  const [launchWorkspace, setLaunchWorkspace] = useState<WorkspaceRecord | null>(null);
  const [launchModel, setLaunchModel] = useState<ModelId>('claude');
  const [launchSubmitting, setLaunchSubmitting] = useState(false);
  const [copyWorkspace, setCopyWorkspace] = useState<WorkspaceRecord | null>(null);
  const [copyDetail, setCopyDetail] = useState<WorkspaceDetail | null>(null);
  const [copySkills, setCopySkills] = useState<CopyableSkill[]>([]);
  const [copySubagents, setCopySubagents] = useState<CopyableSubagent[]>([]);
  const [copyTargetRoot, setCopyTargetRoot] = useState('');
  const [copySelectedMcpIds, setCopySelectedMcpIds] = useState<string[]>([]);
  const [copySelectedSkills, setCopySelectedSkills] = useState<string[]>([]);
  const [copySelectedSubagents, setCopySelectedSubagents] = useState<string[]>([]);
  const [copySubmitting, setCopySubmitting] = useState(false);
  const [copyError, setCopyError] = useState('');
  const [copyLoading, setCopyLoading] = useState(false);
  const [copiedRootPath, setCopiedRootPath] = useState(false);
  const sessionsRequestSeqRef = useRef(0);
  const detailRequestSeqRef = useRef(0);
  const copiedRootPathTimeoutRef = useRef<number | null>(null);

  const activeWorkspace = activeDetail?.workspace.workspace || null;
  const navigateToCapability = useCallback(
    (targetTab: CapabilityTargetTab, workspace: WorkspaceRecord, entry: WorkspaceCapabilityEntry) => {
      onNavigateToCapability?.(targetTab, createWorkspaceCapabilityContext(workspace, entry));
    },
    [onNavigateToCapability],
  );
  const [debouncedSessionNameFilter, setDebouncedSessionNameFilter] = useState('');
  const requestedSessionQuery = useMemo<AiSessionsQueryState>(
    () => ({
      ...sessionQuery,
      nameFilter: debouncedSessionNameFilter,
    }),
    [debouncedSessionNameFilter, sessionQuery],
  );

  const ensureMinimumLoadingDuration = useCallback(async (startedAt: number) => {
    const elapsed = Date.now() - startedAt;
    if (elapsed >= TAB_LOADING_MIN_MS) return;
    await new Promise((resolve) => window.setTimeout(resolve, TAB_LOADING_MIN_MS - elapsed));
  }, []);

  useEffect(() => {
    return () => {
      if (copiedRootPathTimeoutRef.current !== null) {
        window.clearTimeout(copiedRootPathTimeoutRef.current);
      }
    };
  }, []);

  useEffect(() => {
    setCopiedRootPath(false);
    if (copiedRootPathTimeoutRef.current !== null) {
      window.clearTimeout(copiedRootPathTimeoutRef.current);
      copiedRootPathTimeoutRef.current = null;
    }
  }, [activeWorkspace?.root_path]);

  const loadMcpServers = useCallback(async (force = false) => {
    if (!isTauri) return;
    if (!force && mcpServers.length > 0) {
      setMcpInitialized(true);
      setMcpLoading(false);
      return;
    }
    const startedAt = Date.now();
    try {
      setMcpLoading(true);
      const resp = await invoke<MCPStateResp>('get_mcp_servers');
      const nextServers = Array.isArray(resp?.servers)
        ? resp.servers.map((server) => normalizeMcpServer(server))
        : [];
      setMcpServers(nextServers);
      const defaultSwitches = nextServers.reduce<Record<string, MCPModelSwitchState>>((acc, server) => {
        acc[server.id] = { ...DEFAULT_MCP_MODEL_SWITCH_STATE };
        return acc;
      }, {});
      if (nextServers.length > 0) {
        try {
          const switches = await invoke<Record<string, MCPModelSwitchState>>('get_mcp_model_switch_states');
          const normalizedSwitches = Object.entries(switches || {}).reduce<Record<string, MCPModelSwitchState>>(
            (acc, [serverId, state]) => {
              acc[serverId] = normalizeMcpModelSwitchState(state);
              return acc;
            },
            {},
          );
          setMcpModelSwitchStates({ ...defaultSwitches, ...normalizedSwitches });
        } catch (e) {
          console.error('Failed to load MCP model switches', e);
          setMcpModelSwitchStates(defaultSwitches);
        }
      } else {
        setMcpModelSwitchStates({});
      }
      setMcpInitialized(true);
    } catch (e) {
      console.error('Failed to load MCP servers', e);
      setMcpInitialized(true);
    } finally {
      await ensureMinimumLoadingDuration(startedAt);
      setMcpLoading(false);
    }
  }, [ensureMinimumLoadingDuration, isTauri, mcpServers.length]);

  const loadWorkspaces = useCallback(async () => {
    if (!isTauri) return;
    try {
      setLoading(true);
      const resp = await invoke<ApiResp<WorkspaceView[]>>('workspaces_list');
      const allWorkspaces = Array.isArray(resp.data)
        ? resp.data.map((item) => normalizeWorkspaceView(item))
        : [];
      setWorkspaces(allWorkspaces);
      const nextTags = Array.from(
        new Set(
          allWorkspaces.flatMap((item) => item.workspace.tags || []).filter(Boolean),
        ),
      ).sort((a, b) => a.localeCompare(b));
      setAllTags(nextTags);
      if (activeWorkspaceId && !allWorkspaces.some((item) => item.workspace.id === activeWorkspaceId)) {
        setActiveWorkspaceId(null);
        setActiveDetail(null);
        setActiveSessions([]);
      }
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceLoadFailed', 'Failed to load workspaces: {{message}}', { message: String(e) }),
      });
    } finally {
      setWorkspacesInitialized(true);
      setLoading(false);
    }
  }, [activeWorkspaceId, isTauri, t]);

  const loadWorkspaceDetail = useCallback(async (
    workspaceId: string,
    optimisticView?: WorkspaceView,
  ) => {
    if (!isTauri) return;
    const requestId = detailRequestSeqRef.current + 1;
    detailRequestSeqRef.current = requestId;
    try {
      setLoading(true);
      const switchingWorkspace = workspaceId !== activeWorkspaceId;
      setActiveWorkspaceId(workspaceId);
      if (optimisticView) {
        setActiveDetail((prev) => createOptimisticWorkspaceDetail(optimisticView, prev));
      } else if (switchingWorkspace) {
        setActiveDetail(null);
      }
      if (switchingWorkspace) {
        sessionsRequestSeqRef.current += 1;
        setActiveSessions([]);
        setSessionsTotal(0);
        setSessionToolOptions([]);
        setSessionModelOptions([]);
        setSessionsInitialized(false);
        setSessionsLoading(false);
        setSessionQuery(DEFAULT_WORKSPACE_SESSIONS_QUERY);
        setDebouncedSessionNameFilter('');
      }
      const detailResp = await invoke<ApiResp<WorkspaceDetail>>('workspace_get', { workspaceId });
      if (requestId !== detailRequestSeqRef.current) {
        return;
      }
      setActiveWorkspaceId(workspaceId);
      setActiveDetail(normalizeWorkspaceDetail(detailResp.data));
    } catch (e: any) {
      if (requestId === detailRequestSeqRef.current) {
        setMessage({
          type: 'error',
          text: t('workspaceDetailLoadFailed', 'Failed to load workspace detail: {{message}}', {
            message: String(e),
          }),
        });
      }
    } finally {
      if (requestId === detailRequestSeqRef.current) {
        setLoading(false);
      }
    }
  }, [activeWorkspaceId, isTauri, t]);

  const loadPermissionConfig = useCallback(async () => {
    if (!isTauri) return;
    try {
      await invoke<Record<string, unknown>>('get_storage_config');
      // Config loaded for backend-side enforcement; no local state needed
    } catch (e) {
      console.error('Failed to load permission config', e);
    }
  }, [isTauri]);

  const loadWorkspaceSessions = useCallback(async (
    workspaceId: string,
    query: AiSessionsQueryState,
    { silent = false }: { silent?: boolean } = {},
  ) => {
    if (!isTauri) return;
    const requestId = sessionsRequestSeqRef.current + 1;
    sessionsRequestSeqRef.current = requestId;
    const startedAt = Date.now();
    try {
      if (!silent) {
        setSessionsLoading(true);
      }
      const resp = await invoke<ApiResp<WorkspaceSessionsListData>>('workspace_sessions_list', {
        workspaceId,
        tool: query.toolFilter === 'all' ? null : query.toolFilter,
        modelName: query.modelFilter === 'all' ? null : query.modelFilter,
        query: query.nameFilter.trim() ? query.nameFilter.trim() : null,
      });
      if (requestId !== sessionsRequestSeqRef.current) return;
      const nextData = resp.data;
      setActiveSessions(Array.isArray(nextData?.items) ? nextData.items : []);
      setSessionsTotal(Number(nextData?.total) || 0);
      setSessionToolOptions(Array.isArray(nextData?.tool_options) ? nextData.tool_options : []);
      setSessionModelOptions(Array.isArray(nextData?.model_options) ? nextData.model_options : []);
      setSessionsInitialized(true);
    } catch (e: any) {
      if (requestId !== sessionsRequestSeqRef.current) return;
      setMessage({
        type: 'error',
        text: t('workspaceSessionsLoadFailed', 'Failed to load workspace sessions: {{message}}', {
          message: String(e),
        }),
      });
      setSessionsInitialized(true);
    } finally {
      if (requestId === sessionsRequestSeqRef.current) {
        if (!silent) {
          await ensureMinimumLoadingDuration(startedAt);
        }
        setSessionsLoading(false);
      }
    }
  }, [ensureMinimumLoadingDuration, isTauri, t]);

  const refreshActiveWorkspace = useCallback(async () => {
    if (!activeWorkspaceId) return;
    await loadWorkspaceDetail(activeWorkspaceId);
    if (activeTab === 'sessions') {
      await loadWorkspaceSessions(activeWorkspaceId, requestedSessionQuery, { silent: true });
    }
  }, [activeTab, activeWorkspaceId, loadWorkspaceDetail, loadWorkspaceSessions, requestedSessionQuery]);

  const loadCopySources = useCallback(async (workspace: WorkspaceRecord) => {
    if (!isTauri) return;
    setCopyLoading(true);
    setCopyError('');
    try {
      const [, detailResp, skillsResp, subagentsResp] = await Promise.all([
        loadMcpServers().catch(() => {}),
        invoke<ApiResp<WorkspaceDetail>>('workspace_get', { workspaceId: workspace.id }),
        invoke<ApiResp<InstalledSkill[]>>('skills_list_installed', {
          model: null,
          scope: 'project',
          projectRoot: workspace.root_path,
        }),
        invoke<ApiResp<InstalledSubagent[]>>('subagents_list_installed', {
          model: null,
          scope: 'project',
          projectRoot: workspace.root_path,
        }),
      ]);
      const detailData = normalizeWorkspaceDetail(detailResp.data);
      const nextSkills = (skillsResp.data || []).map((item) => ({
        ...item,
        selection_key: buildSkillSelectionKey(item),
      }));
      const nextSubagents = (subagentsResp.data || []).map((item) => ({
        ...item,
        selection_key: buildSubagentSelectionKey(item),
      }));
      setCopyWorkspace(workspace);
      setCopyDetail(detailData);
      setCopySkills(nextSkills);
      setCopySubagents(nextSubagents);
      setCopySelectedMcpIds((detailData.mcp_bindings || []).map((item) => item.server_id));
      setCopySelectedSkills(nextSkills.map((item) => item.selection_key));
      setCopySelectedSubagents(nextSubagents.map((item) => item.selection_key));
      setCopyTargetRoot('');
    } catch (e: any) {
      setCopyError(
        t('workspaceCopyLoadFailed', 'Failed to load copyable content: {{message}}', {
          message: String(e),
        }),
      );
    } finally {
      setCopyLoading(false);
    }
  }, [isTauri, loadMcpServers, t]);

  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => setMessage(null), 3000);
    return () => window.clearTimeout(timer);
  }, [message]);

  useEffect(() => {
    if (!isVisible) return;
    void loadWorkspaces();
    void loadPermissionConfig();
  }, [isVisible, loadWorkspaces, loadPermissionConfig]);

  useEffect(() => {
    if (!isVisible || !activeWorkspace || activeTab !== 'mcp') return;
    void loadMcpServers(true);
  }, [activeTab, activeWorkspace, isVisible, loadMcpServers]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setDebouncedSessionNameFilter(sessionQuery.nameFilter);
    }, 250);
    return () => window.clearTimeout(timer);
  }, [sessionQuery.nameFilter]);

  useEffect(() => {
    if (sessionQuery.modelFilter === 'all') return;
    if (sessionModelOptions.includes(sessionQuery.modelFilter)) return;
    setSessionQuery((prev) => ({ ...prev, modelFilter: 'all' }));
  }, [sessionModelOptions, sessionQuery.modelFilter]);

  useEffect(() => {
    if (!isVisible || activeTab !== 'sessions' || !activeWorkspaceId) return;
    void loadWorkspaceSessions(activeWorkspaceId, requestedSessionQuery);
  }, [activeTab, activeWorkspaceId, isVisible, loadWorkspaceSessions, requestedSessionQuery]);

  const handleSelectTab = useCallback((nextTab: WorkspaceTab) => {
    if (nextTab === 'sessions') {
      setSessionsLoading(true);
    }
    if (nextTab === 'mcp') {
      setMcpLoading(true);
    }
    setActiveTab(nextTab);
  }, []);

  useEffect(() => {
    if (!isVisible) return;
    let unlistenRefresh: (() => void) | undefined;
    let unlistenSessions: (() => void) | undefined;
    let unlistenWorkspaces: (() => void) | undefined;

    const register = async () => {
      unlistenRefresh = await listen('refresh-counts', () => {
        void loadWorkspaces();
        void refreshActiveWorkspace();
      });
      unlistenSessions = await listen('sessions-updated', () => {
        void loadWorkspaces();
        void refreshActiveWorkspace();
      });
      unlistenWorkspaces = await listen('workspaces-updated', () => {
        void loadWorkspaces();
        void refreshActiveWorkspace();
      });
    };

    void register();

    return () => {
      unlistenRefresh?.();
      unlistenSessions?.();
      unlistenWorkspaces?.();
    };
  }, [isVisible, loadWorkspaces, refreshActiveWorkspace]);

  const mcpBindingMap = useMemo(() => {
    const next = new Map<string, string[]>();
    (activeDetail?.mcp_bindings || []).forEach((binding) => {
      next.set(binding.server_id, binding.enabled_models || []);
    });
    return next;
  }, [activeDetail]);

  const workspaceProjectMcpEntries = useMemo<WorkspaceMcpEntry[]>(() => {
    const serverMap = new Map(mcpServers.map((server) => [server.id, server]));
    return (activeDetail?.mcp_bindings || [])
      .map((binding) => ({
        server:
          serverMap.get(binding.server_id) || {
            id: binding.server_id,
            name: binding.server_id,
            transport: 'stdio',
          },
        binding,
        scope: 'project' as const,
        enabled_models: (binding.enabled_models || []).filter((model): model is ModelId =>
          TOOL_OPTIONS.some((tool) => tool.id === model),
        ),
      }))
      .sort((a, b) => sortMcpServersByName(a.server, b.server));
  }, [activeDetail, mcpServers]);

  const workspaceGlobalMcpEntries = useMemo<WorkspaceMcpEntry[]>(
    () =>
      mcpServers
        .map((server) => ({
          server,
          binding: null,
          scope: 'global' as const,
          enabled_models: getMcpEnabledModelsFromSwitch(mcpModelSwitchStates[server.id]),
        }))
        .filter((entry) => entry.enabled_models.length > 0)
        .sort((a, b) => sortMcpServersByName(a.server, b.server)),
    [mcpModelSwitchStates, mcpServers],
  );

  const workspaceEffectiveMcpEntriesByModel = useMemo<Record<ModelId, WorkspaceMcpEntry[]>>(() => {
    const next: Record<ModelId, WorkspaceMcpEntry[]> = {
      claude: [],
      gemini: [],
      codex: [],
      opencode: [],
    };

    TOOL_OPTIONS.forEach((tool) => {
      const byKey = new Map<string, WorkspaceMcpEntry>();
      workspaceGlobalMcpEntries.forEach((entry) => {
        if (!entry.enabled_models.includes(tool.id)) return;
        const key = getMcpMergeKey(entry.server);
        if (key) {
          byKey.set(key, entry);
        }
      });
      workspaceProjectMcpEntries.forEach((entry) => {
        if (!entry.enabled_models.includes(tool.id)) return;
        const key = getMcpMergeKey(entry.server);
        if (key) {
          byKey.set(key, entry);
        }
      });
      next[tool.id] = Array.from(byKey.values()).sort((a, b) => sortMcpServersByName(a.server, b.server));
    });

    return next;
  }, [workspaceGlobalMcpEntries, workspaceProjectMcpEntries]);

  const workspaceInstalledCountsByModel = useMemo<Record<ModelId, number>>(
    () => ({
      claude: workspaceEffectiveMcpEntriesByModel.claude.length,
      gemini: workspaceEffectiveMcpEntriesByModel.gemini.length,
      codex: workspaceEffectiveMcpEntriesByModel.codex.length,
      opencode: workspaceEffectiveMcpEntriesByModel.opencode.length,
    }),
    [workspaceEffectiveMcpEntriesByModel],
  );

  const workspaceInstalledCards = useMemo(
    () => workspaceEffectiveMcpEntriesByModel[activeMcpModel] || [],
    [activeMcpModel, workspaceEffectiveMcpEntriesByModel],
  );

  const activeMcpLoadRule = useMemo(() => {
    switch (activeMcpModel) {
      case 'claude':
        return t(
          'workspaceMcpLoadRuleClaude',
          'Claude Code merges MCP by scope. Same-name servers resolve as local > project > user > plugin/connectors; different names are kept side by side.',
        );
      case 'gemini':
        return t(
          'workspaceMcpLoadRuleGemini',
          'Gemini merges mcpServers from system, workspace, and user settings. Same-name servers resolve as system > workspace > user.',
        );
      case 'codex':
        return t(
          'workspaceMcpLoadRuleCodex',
          'Codex reads user config plus trusted project .codex/config.toml files. Same-name MCP keys from the closest project config override user config.',
        );
      case 'opencode':
      default:
        return t(
          'workspaceMcpLoadRuleOpenCode',
          'OpenCode merges config files instead of replacing them. Project opencode.json overrides global MCP keys with the same name; non-conflicting keys remain.',
        );
    }
  }, [activeMcpModel, t]);

  const workspaceAvailableMcpEntries = useMemo<WorkspaceMcpCatalogEntry[]>(
    () =>
      [...mcpServers]
        .sort(sortMcpServersByName)
        .map((server) => {
          const binding = (activeDetail?.mcp_bindings || []).find((item) => item.server_id === server.id) || null;
          const enabledModels = (binding?.enabled_models || []).filter((model): model is ModelId =>
            TOOL_OPTIONS.some((tool) => tool.id === model),
          );
          const globalEnabledModels = getMcpEnabledModelsFromSwitch(mcpModelSwitchStates[server.id]);
          const status: WorkspaceMcpCatalogEntry['status'] = enabledModels.includes(activeMcpModel)
            ? 'enabled_for_model'
            : globalEnabledModels.includes(activeMcpModel)
              ? 'enabled_user_level'
              : enabledModels.length > 0
                ? 'bound_other_models'
                : 'not_bound';
          return {
            server,
            binding,
            scope: 'global' as const,
            enabled_models: enabledModels.length > 0 ? enabledModels : globalEnabledModels,
            status,
          };
        }),
    [activeDetail, activeMcpModel, mcpModelSwitchStates, mcpServers],
  );

  const formatEnabledModels = useCallback(
    (models: string[]) => {
      if (models.length === 0) {
        return t('workspaceMcpNoEnabledModels', 'No models enabled');
      }
      return models
        .map((model) => TOOL_OPTIONS.find((item) => item.id === model)?.label || model)
        .join(' · ');
    },
    [t],
  );

  const getWorkspaceMcpStatusMeta = useCallback(
    (status: WorkspaceMcpCatalogEntry['status']) => {
      if (status === 'enabled_for_model') {
        return {
          label: t('workspaceMcpStatusEnabledForModel', 'Enabled for current model'),
          className: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700',
        };
      }
      if (status === 'bound_other_models') {
        return {
          label: t('workspaceMcpStatusBoundOtherModels', 'Enabled for other models'),
          className: 'border-amber-500/30 bg-amber-500/10 text-amber-700',
        };
      }
      if (status === 'enabled_user_level') {
        return {
          label: t('workspaceMcpStatusEnabledUserLevel', 'Enabled at user level'),
          className: 'border-blue-500/30 bg-blue-500/10 text-blue-700',
        };
      }
      return {
        label: t('workspaceMcpStatusNotBound', 'Not enabled yet'),
        className: 'border-border bg-muted/30 text-muted-foreground',
      };
    },
    [t],
  );

  const visibleWorkspaces = useMemo(() => {
    if (selectedTags.length === 0) {
      return workspaces;
    }
    const selected = new Set(selectedTags.map((item) => item.trim().toLowerCase()).filter(Boolean));
    return workspaces.filter((item) =>
      (item.workspace.tags || []).some((tag) => selected.has(tag.trim().toLowerCase())),
    );
  }, [selectedTags, workspaces]);

  const selectedWorkspaceTags = useMemo(
    () => new Set(selectedTags),
    [selectedTags],
  );

  const formTitle = dialogMode === 'create'
    ? t('workspaceCreateTitle', 'Create Workspace')
    : t('workspaceEditTitle', 'Edit Workspace');
  const activeWorkspaceSourceBadgeKeys = activeWorkspace
    ? getSourceBadgeTranslationKeys(activeWorkspace.source)
    : null;
  const activeWorkspaceSourceBadgeLabel = activeWorkspace && activeWorkspaceSourceBadgeKeys
    ? t(activeWorkspaceSourceBadgeKeys.label, getSourceBadgeLabel(activeWorkspace.source))
    : '';
  const activeWorkspaceSourceBadgeDescription = activeWorkspace && activeWorkspaceSourceBadgeKeys
    ? t(
        activeWorkspaceSourceBadgeKeys.description,
        getSourceBadgeDescription(activeWorkspace.source),
      )
    : '';

  const openCreateDialog = () => {
    setDialogMode('create');
    setFormState({
      name: '',
      root_path: '',
      description: '',
      tags: '',
    });
    setFormError('');
    setDialogOpen(true);
  };

  const openEditDialog = (workspace: WorkspaceRecord) => {
    setDialogMode('edit');
    setFormState({
      id: workspace.id,
      name: workspace.name,
      root_path: workspace.root_path,
      description: workspace.description || '',
      tags: (workspace.tags || []).join(', '),
    });
    setFormError('');
    setDialogOpen(true);
  };

  const handleWorkspaceSubmit = async () => {
    if (!isTauri) return;
    const name = formState.name.trim();
    const rootPath = formState.root_path.trim();
    if (!name) {
      setFormError(t('workspaceNameRequired', 'Workspace name is required.'));
      return;
    }
    if (dialogMode === 'create' && !rootPath) {
      setFormError(t('workspaceRootRequired', 'Workspace directory is required.'));
      return;
    }
    try {
      setFormSubmitting(true);
      const input = {
        id: formState.id,
        name,
        root_path: rootPath,
        description: formState.description.trim() || null,
        tags: parseTagsInput(formState.tags),
      };
      const resp = dialogMode === 'create'
        ? await invoke<ApiResp<WorkspaceDetail>>('workspace_create', { input })
        : await invoke<ApiResp<WorkspaceDetail>>('workspace_update_meta', { input });
      setDialogOpen(false);
      setFormError('');
      setMessage({
        type: 'success',
        text:
          dialogMode === 'create'
            ? t('workspaceCreated', 'Workspace created')
            : t('workspaceUpdated', 'Workspace updated'),
      });
      emit('refresh-counts').catch(() => {});
      await loadWorkspaces();
      await loadWorkspaceDetail(
        resp.data.workspace.workspace.id,
        normalizeWorkspaceView(resp.data.workspace),
      );
    } catch (e: any) {
      setFormError(String(e));
    } finally {
      setFormSubmitting(false);
    }
  };

  const handleDeleteWorkspace = async (workspace: WorkspaceRecord) => {
    const ok = await confirmDialog(
      t('workspaceDeleteConfirm', 'Delete workspace "{{name}}"?', { name: workspace.name }),
      {
        title: t('workspaceDeleteTitle', 'Delete Workspace'),
        okLabel: t('delete', 'Delete'),
        cancelLabel: t('cancel', 'Cancel'),
      },
    );
    if (!ok || !isTauri) return;
    try {
      setLoading(true);
      await invoke('workspace_delete', { workspaceId: workspace.id });
      emit('refresh-counts').catch(() => {});
      setMessage({ type: 'success', text: t('workspaceDeleted', 'Workspace deleted') });
      if (activeWorkspaceId === workspace.id) {
        setActiveWorkspaceId(null);
        setActiveDetail(null);
        setActiveSessions([]);
      }
      await loadWorkspaces();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceDeleteFailed', 'Failed to delete workspace: {{message}}', {
          message: String(e),
        }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleWorkspaceSessionDelete = async (sessionId: string) => {
    if (!isTauri) return;
    try {
      await invoke('sessions_delete', { sessionId });
      emit('refresh-counts').catch(() => {});
      await Promise.all([loadWorkspaces(), refreshActiveWorkspace()]);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceSessionDeleteFailed', 'Failed to delete session: {{message}}', {
          message: String(e),
        }),
      });
    }
  };

  const handleWorkspaceSessionRename = async (session: AiSessionListItem, nextName: string) => {
    if (!isTauri) return;
    try {
      await invoke('sessions_update', {
        session: {
          id: session.id,
          name: nextName.trim(),
          working_dir: session.working_dir,
          tool: session.model_type,
        },
      });
      await Promise.all([loadWorkspaces(), refreshActiveWorkspace()]);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceSessionRenameFailed', 'Failed to rename session: {{message}}', {
          message: String(e),
        }),
      });
    }
  };

  const handleWorkspaceSessionLaunch = async (session: AiSessionListItem) => {
    if (!isTauri) return;
    // Always call without permissionMode first; backend will enforce confirmation if needed
    try {
      await invoke('sessions_launch', { sessionId: session.id });
      await refreshActiveWorkspace();
    } catch (e: unknown) {
      const code = getInvokeErrorCode(e);
      if (code === 'PERMISSION_CONFIRMATION_REQUIRED') {
        setPermissionDialogSession(session);
        setPermissionDialogOpen(true);
      } else {
        setMessage({
          type: 'error',
          text: t('workspaceSessionLaunchFailed', 'Failed to launch session: {{message}}', {
            message: formatInvokeError(e),
          }),
        });
      }
    }
  };

  const handleWorkspacePermissionConfirm = async (mode: TerminalPermissionMode) => {
    if (!permissionDialogSession) return;
    setPermissionDialogOpen(false);
    const session = permissionDialogSession;
    setPermissionDialogSession(null);
    try {
      await invoke('sessions_launch', { sessionId: session.id, permissionMode: mode });
      await refreshActiveWorkspace();
    } catch (e: unknown) {
      setMessage({
        type: 'error',
        text: t('workspaceSessionLaunchFailed', 'Failed to launch session: {{message}}', {
          message: formatInvokeError(e),
        }),
      });
    }
  };

  const handleWorkspacePermissionCancel = () => {
    setPermissionDialogOpen(false);
    setPermissionDialogSession(null);
  };

  const handleWorkspaceSessionFavoriteChange = async (session: AiSessionListItem, favorite: boolean) => {
    if (!isTauri) return;
    try {
      await invoke('sessions_set_favorite', { sessionId: session.id, favorite });
      await Promise.all([loadWorkspaces(), refreshActiveWorkspace()]);
    } catch (e: unknown) {
      setMessage({
        type: 'error',
        text: t('workspaceSessionFavoriteFailed', 'Failed to update favorite: {{message}}', {
          message: String(e),
        }),
      });
    }
  };

  const toggleTagFilter = (tag: string) => {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((item) => item !== tag) : [...prev, tag],
    );
  };

  const saveWorkspaceMcpBinding = useCallback(async (serverId: string, nextModels: ModelId[]) => {
    if (!activeWorkspaceId || !isTauri) return null;
    try {
      const resp = await invoke<ApiResp<WorkspaceDetail>>('workspace_mcp_binding_upsert', {
        input: {
          workspace_id: activeWorkspaceId,
          server_id: serverId,
          enabled_models: nextModels,
        },
      });
      const detail = normalizeWorkspaceDetail(resp.data);
      setActiveDetail(detail);
      emit('refresh-counts').catch(() => {});
      await loadWorkspaces();
      return detail;
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceMcpUpdateFailed', 'Failed to update MCP binding: {{message}}', {
          message: String(e),
        }),
      });
      return null;
    }
  }, [activeWorkspaceId, isTauri, loadWorkspaces, t]);

  const openMcpInstallDialog = async (server: MCPServer) => {
    if (!isTauri) return;
    await loadMcpServers();
    const currentModels = (mcpBindingMap.get(server.id) || []).filter((model): model is ModelId =>
      TOOL_OPTIONS.some((item) => item.id === model),
    );
    setMcpDialogServer(server);
    setMcpDialogModels(currentModels.length > 0 ? currentModels : [activeMcpModel]);
    setMcpDialogError('');
  };

  const toggleMcpDialogModel = (model: ModelId) => {
    setMcpDialogModels((prev) =>
      prev.includes(model) ? prev.filter((item) => item !== model) : [...prev, model],
    );
    if (mcpDialogError) {
      setMcpDialogError('');
    }
  };

  const handleSaveMcpDialog = async () => {
    if (!mcpDialogServer) return;
    if (mcpDialogModels.length === 0) {
      setMcpDialogError(t('workspaceMcpInstallModelsRequired', 'Choose at least one model.'));
      return;
    }
    try {
      setMcpDialogSubmitting(true);
      const nextModels = TOOL_OPTIONS
        .map((item) => item.id)
        .filter((item) => mcpDialogModels.includes(item));
      const detail = await saveWorkspaceMcpBinding(mcpDialogServer.id, nextModels);
      if (!detail) return;
      setMcpDialogServer(null);
      setMcpDialogModels([]);
      setMcpDialogError('');
      setActiveMcpModel(nextModels[0] || 'claude');
    } finally {
      setMcpDialogSubmitting(false);
    }
  };

  const handleUninstallWorkspaceMcpForModel = async (serverId: string, model: ModelId) => {
    const currentModels = new Set(mcpBindingMap.get(serverId) || []);
    currentModels.delete(model);
    const nextModels = TOOL_OPTIONS.map((item) => item.id).filter((item) => currentModels.has(item));
    await saveWorkspaceMcpBinding(serverId, nextModels);
  };

  const handleEnableWorkspaceMcpForActiveModel = async (server: MCPServer) => {
    const currentModels = new Set(
      (mcpBindingMap.get(server.id) || []).filter((model): model is ModelId =>
        TOOL_OPTIONS.some((item) => item.id === model),
      ),
    );
    currentModels.add(activeMcpModel);
    const nextModels = TOOL_OPTIONS.map((item) => item.id).filter((item) => currentModels.has(item));
    await saveWorkspaceMcpBinding(server.id, nextModels);
  };

  const openLaunchDialog = (workspace: WorkspaceRecord) => {
    setLaunchWorkspace(workspace);
    setLaunchModel('claude');
  };

  const handleLaunchWorkspaceSession = async () => {
    if (!launchWorkspace || !isTauri) return;
    try {
      setLaunchSubmitting(true);
      await invoke('workspace_launch_session', {
        workspaceId: launchWorkspace.id,
        tool: launchModel,
      });
      emit('refresh-counts').catch(() => {});
      setMessage({
        type: 'success',
        text: t('workspaceLaunchSuccess', 'New terminal session started'),
      });
      setLaunchWorkspace(null);
      await Promise.all([loadWorkspaces(), refreshActiveWorkspace()]);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceLaunchFailed', 'Failed to start session: {{message}}', {
          message: String(e),
        }),
      });
    } finally {
      setLaunchSubmitting(false);
    }
  };

  const openCopyDialog = async (workspace: WorkspaceRecord) => {
    await loadCopySources(workspace);
  };

  const handleCopyWorkspace = async () => {
    if (!copyWorkspace || !isTauri) return;
    if (!copyTargetRoot.trim()) {
      setCopyError(t('workspaceCopyTargetRequired', 'Target directory is required.'));
      return;
    }
    try {
      setCopySubmitting(true);
      setCopyError('');
      await invoke<ApiResp<WorkspaceDetail>>('workspace_copy', {
        input: {
          source_workspace_id: copyWorkspace.id,
          target_root_path: copyTargetRoot.trim(),
          selected_mcp_server_ids: copySelectedMcpIds,
          selected_skills: copySkills
            .filter((item) => copySelectedSkills.includes(item.selection_key))
            .map((item) => ({
              model: item.model,
              source_id: item.source_id,
              source_rel_path: item.source_rel_path,
            })),
          selected_subagents: copySubagents
            .filter((item) => copySelectedSubagents.includes(item.selection_key))
            .map((item) => ({
              model: item.model,
              source_id: item.source_id,
              source_rel_path: item.source_rel_path,
            })),
        },
      });
      emit('refresh-counts').catch(() => {});
      setMessage({
        type: 'success',
        text: t('workspaceCopySuccess', 'Workspace configuration copied'),
      });
      setCopyWorkspace(null);
      setCopyDetail(null);
      setCopySkills([]);
      setCopySubagents([]);
      setCopyTargetRoot('');
      await loadWorkspaces();
    } catch (e: any) {
      setCopyError(String(e));
    } finally {
      setCopySubmitting(false);
    }
  };

  const handleCopyActiveRootPath = async () => {
    if (!activeWorkspace?.root_path) return;
    try {
      await navigator.clipboard.writeText(activeWorkspace.root_path);
      setCopiedRootPath(true);
      if (copiedRootPathTimeoutRef.current !== null) {
        window.clearTimeout(copiedRootPathTimeoutRef.current);
      }
      copiedRootPathTimeoutRef.current = window.setTimeout(() => {
        setCopiedRootPath(false);
        copiedRootPathTimeoutRef.current = null;
      }, 2000);
    } catch (error) {
      console.error('failed to copy workspace root path', error);
      setMessage({
        type: 'error',
        text: t('copyPathFailed', 'Failed to copy path. Please copy manually.'),
      });
    }
  };

  const toggleCopySelection = (
    kind: 'mcp' | 'skills' | 'subagents',
    key: string,
  ) => {
    const updater = (prev: string[]) =>
      prev.includes(key) ? prev.filter((item) => item !== key) : [...prev, key];
    if (kind === 'mcp') {
      setCopySelectedMcpIds(updater);
      return;
    }
    if (kind === 'skills') {
      setCopySelectedSkills(updater);
      return;
    }
    setCopySelectedSubagents(updater);
  };

  const setAllCopySelections = (kind: 'mcp' | 'skills' | 'subagents', enabled: boolean) => {
    if (kind === 'mcp') {
      setCopySelectedMcpIds(enabled ? (copyDetail?.mcp_bindings || []).map((item) => item.server_id) : []);
      return;
    }
    if (kind === 'skills') {
      setCopySelectedSkills(enabled ? copySkills.map((item) => item.selection_key) : []);
      return;
    }
    setCopySelectedSubagents(enabled ? copySubagents.map((item) => item.selection_key) : []);
  };

  if (!isTauri) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t('notInTauri', 'This feature is only available in the desktop app.')}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-bold tracking-tight">{t('workspaces', 'Workspaces')}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {activeWorkspace
              ? t(
                  'workspaceDetailDesc',
                  'Review {{name}} directory, metadata, terminal sessions, and installed project capabilities in one place.',
                  { name: activeWorkspace.name },
                )
              : t(
                  'workspaceListDesc',
                  'Use workspaces to organize each local project folder together with its sessions, MCP, Skills, and Subagents.',
                )}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2 md:justify-end">
          {message && (
            <div
              className={`rounded-md border px-2.5 py-1.5 text-xs ${
                message.type === 'error'
                  ? 'border-destructive/20 bg-destructive/10 text-destructive'
                  : 'border-green-500/20 bg-green-500/10 text-green-700'
              }`}
            >
              {message.text}
            </div>
          )}
          <button
            type="button"
            onClick={() => {
              void Promise.all([loadWorkspaces(), activeWorkspaceId ? refreshActiveWorkspace() : Promise.resolve()]);
            }}
            className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted"
          >
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
            {t('refresh', 'Refresh')}
          </button>
          <button
            type="button"
            onClick={openCreateDialog}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
            {t('workspaceCreate', 'New Workspace')}
          </button>
        </div>
      </div>

      {!activeWorkspace ? (
        <>
          <div className="rounded-xl border bg-card p-4">
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => setSelectedTags([])}
                className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                  selectedTags.length === 0
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'hover:bg-muted'
                }`}
              >
                {t('all', 'All')}
              </button>
              {allTags.map((tag) => (
                <button
                  key={tag}
                  type="button"
                  onClick={() => toggleTagFilter(tag)}
                  className={`inline-flex items-center gap-1 rounded-full border px-3 py-1 text-xs transition-colors ${
                    selectedWorkspaceTags.has(tag)
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'hover:bg-muted'
                  }`}
                >
                  <Tag className="h-3 w-3" />
                  {tag}
                </button>
              ))}
            </div>
            <div className="mt-3 text-xs text-muted-foreground">
              {t('workspaceCountSummary', 'Showing {{count}} workspaces', { count: visibleWorkspaces.length })}
            </div>
          </div>

          {!workspacesInitialized || (loading && workspaces.length === 0) ? (
            <div className="flex flex-1 flex-col items-center justify-center rounded-xl border bg-card p-8 text-center text-muted-foreground">
              <Loader2 className="mb-4 h-10 w-10 animate-spin opacity-70" />
              <p className="text-sm">{t('loading', 'Loading...')}</p>
            </div>
          ) : visibleWorkspaces.length === 0 ? (
            <div className="flex flex-1 flex-col items-center justify-center rounded-xl border bg-card p-8 text-center text-muted-foreground">
              <FolderOpen className="mb-4 h-12 w-12 opacity-30" />
              <p className="text-base font-medium text-foreground">
                {t('workspaceEmptyTitle', 'No workspaces yet')}
              </p>
              <p className="mt-2 max-w-xl text-sm">
                {t('workspaceEmptyDesc', 'Create a workspace manually, or let AI terminal session sync create them automatically from working directories.')}
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
              {visibleWorkspaces.map((item) => {
                const workspace = item.workspace;
                const lastActiveText = formatTs(workspace.last_activity_at);
                const compactRootPath = compactWorkspaceRootPath(workspace.root_path);
                const sourceBadgeKeys = getSourceBadgeTranslationKeys(workspace.source);
                const sourceBadgeLabel = t(sourceBadgeKeys.label, getSourceBadgeLabel(workspace.source));
                const sourceBadgeDescription = t(
                  sourceBadgeKeys.description,
                  getSourceBadgeDescription(workspace.source),
                );
                return (
                  <div
                    key={workspace.id}
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      setActiveTab('sessions');
                      void loadWorkspaceDetail(workspace.id, item);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault();
                        setActiveTab('sessions');
                        void loadWorkspaceDetail(workspace.id, item);
                      }
                    }}
                    className="group flex h-full flex-col rounded-xl border bg-card p-4 text-left transition-all hover:-translate-y-0.5 hover:border-primary/35 hover:shadow-sm"
                  >
                    <div className="flex items-start justify-between gap-2.5">
                      <div className="min-w-0 flex-1">
                        <div className="inline-flex max-w-full items-start gap-1.5">
                          <span className="min-w-0 shrink truncate text-base font-semibold leading-tight">
                            {workspace.name}
                          </span>
                          <span
                            className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide ${getSourceBadgeClassName(workspace.source)}`}
                            title={`${sourceBadgeLabel}: ${sourceBadgeDescription}`}
                          >
                            {sourceBadgeLabel}
                          </span>
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-1 self-start">
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            openEditDialog(workspace);
                          }}
                          className="rounded-md p-1.5 text-muted-foreground/80 transition-colors hover:bg-muted hover:text-foreground"
                          title={t('edit', 'Edit')}
                        >
                          <Pencil className="h-3.5 w-3.5" />
                        </button>
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            void openCopyDialog(workspace);
                          }}
                          className="rounded-md p-1.5 text-amber-600/90 transition-colors hover:bg-amber-500/10 hover:text-amber-700 dark:text-amber-300 dark:hover:text-amber-200"
                          title={t('copy', 'Copy')}
                        >
                          <Copy className="h-3.5 w-3.5" />
                        </button>
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleDeleteWorkspace(workspace);
                          }}
                          className="rounded-md p-1.5 text-destructive/80 transition-colors hover:bg-destructive/10 hover:text-destructive"
                          title={t('delete', 'Delete')}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </div>

                    <div
                      className="mt-2 w-full truncate rounded-md bg-muted/40 px-2.5 py-1.5 text-[11px] leading-4 text-muted-foreground"
                      title={workspace.root_path}
                    >
                      {compactRootPath}
                    </div>

                    <p className="mt-3 line-clamp-2 min-h-[36px] text-[13px] leading-5 text-muted-foreground">
                      {workspace.description?.trim() || t('workspaceNoDescription', 'No description yet.')}
                    </p>

                    <div className="mt-3 flex flex-wrap gap-1.5">
                      {(workspace.tags || []).length > 0 ? (
                        workspace.tags.map((tag) => (
                          <span
                            key={`${workspace.id}-${tag}`}
                            className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] leading-4 text-muted-foreground"
                          >
                            <Tag className="h-2.5 w-2.5" />
                            {tag}
                          </span>
                        ))
                      ) : (
                        <span className="rounded-full border border-dashed px-2 py-0.5 text-[10px] leading-4 text-muted-foreground">
                          {t('workspaceNoTags', 'No tags')}
                        </span>
                      )}
                    </div>

                    <div className="mt-3 flex items-end justify-between gap-3">
                      <div className="grid min-w-0 flex-1 grid-cols-2 gap-1.5 rounded-lg bg-muted/35 p-2">
                        <div className="min-w-0">
                          <div className="text-[10px] uppercase tracking-wide text-muted-foreground/80">
                            {t('sessions', 'Sessions')}
                          </div>
                          <div className="mt-0.5 truncate text-[11px] font-medium text-foreground">
                            {t('workspaceSessionsCount', '{{count}} sessions', { count: item.session_count })}
                          </div>
                        </div>
                        <div className="min-w-0">
                          <div className="text-[10px] uppercase tracking-wide text-muted-foreground/80">
                            {t('workspaceLastActive', 'Last active')}
                          </div>
                          <div className="mt-0.5 truncate text-[11px] font-medium text-foreground" title={lastActiveText}>
                            {lastActiveText}
                          </div>
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          openLaunchDialog(workspace);
                        }}
                        className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                      >
                        <Play className="h-3.5 w-3.5" />
                        {t('workspaceQuickLaunch', 'New AI Session')}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      ) : (
        <>
          <div className="rounded-2xl border bg-card p-3.5">
            <div className="flex flex-col gap-3">
              <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2 md:gap-3">
                    <button
                      type="button"
                      onClick={() => {
                        setActiveWorkspaceId(null);
                        setActiveDetail(null);
                        setActiveSessions([]);
                      }}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border border-primary/30 bg-primary/10 px-2.5 text-xs font-medium text-primary transition-colors hover:border-primary/40 hover:bg-primary/15"
                    >
                      <ArrowLeft className="h-3.5 w-3.5" />
                      {t('back', 'Back')}
                    </button>
                    <h3 className="min-w-0 truncate text-lg font-semibold tracking-tight">
                      {activeWorkspace.name}
                    </h3>
                    <span
                      className={`rounded-full border px-2 py-0.5 text-[10px] ${getSourceBadgeClassName(activeWorkspace.source)}`}
                      title={`${activeWorkspaceSourceBadgeLabel}: ${activeWorkspaceSourceBadgeDescription}`}
                    >
                      {activeWorkspaceSourceBadgeLabel}
                    </span>
                  </div>
                </div>
                <div className="flex flex-wrap items-center gap-2 md:justify-end">
                  <button
                    type="button"
                    onClick={() => openEditDialog(activeWorkspace)}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border px-2.5 text-xs hover:bg-muted"
                  >
                    <Pencil className="h-3.5 w-3.5" />
                    {t('edit', 'Edit')}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      void openCopyDialog(activeWorkspace);
                    }}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 text-xs font-medium text-amber-700 transition-colors hover:bg-amber-500/15 dark:border-amber-400/30 dark:bg-amber-400/10 dark:text-amber-300 dark:hover:bg-amber-400/15"
                  >
                    <Copy className="h-3.5 w-3.5" />
                    {t('workspaceCopyAction', 'Copy Config')}
                  </button>
                </div>
              </div>
              <div className="group/rootpath flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                <FolderOpen className="h-3.5 w-3.5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="inline-flex max-w-full items-center gap-1 overflow-hidden align-middle">
                    <div className="truncate" title={activeWorkspace.root_path}>
                      {activeWorkspace.root_path}
                    </div>
                    {copiedRootPath ? (
                      <Check
                        className="h-3.5 w-3.5 shrink-0 text-green-600"
                        aria-label={t('copied', 'Copied!')}
                      />
                    ) : (
                      <button
                        type="button"
                        onClick={() => {
                          void handleCopyActiveRootPath();
                        }}
                        className="shrink-0 rounded-md p-0.5 text-muted-foreground opacity-0 transition-all hover:bg-muted hover:text-foreground focus:opacity-100 group-hover/rootpath:opacity-100"
                        title={t('copyPath', 'Copy path')}
                        aria-label={t('copyPath', 'Copy path')}
                      >
                        <Copy className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </div>
                </div>
              </div>

              {(activeWorkspace.description || (activeWorkspace.tags || []).length > 0 || activeDetail) && (
                <div className="space-y-2">
                  {activeWorkspace.description && (
                    <p className="line-clamp-1 text-xs text-muted-foreground">
                      {activeWorkspace.description}
                    </p>
                  )}
                  <div className="flex flex-wrap items-center gap-1.5">
                    {(activeWorkspace.tags || []).length > 0 ? (
                      (activeWorkspace.tags || []).map((tag) => (
                        <span
                          key={`detail-tag-${tag}`}
                          className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground"
                        >
                          <Tag className="h-3 w-3" />
                          {tag}
                        </span>
                      ))
                    ) : (
                      <span className="rounded-full border border-dashed px-2 py-0.5 text-[10px] text-muted-foreground">
                        {t('workspaceNoTags', 'No tags')}
                      </span>
                    )}
                    <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                      {t('workspaceSessionsCount', '{{count}} sessions', {
                        count: activeDetail?.workspace.session_count || 0,
                      })}
                    </span>
                    <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                      {t('workspaceCreatedAt', 'Created')}: {formatTs(activeWorkspace.created_at)}
                    </span>
                    <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                      {t('workspaceLastActive', 'Last active')}: {formatTs(activeWorkspace.last_activity_at)}
                    </span>
                  </div>
                </div>
              )}
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
              {[
                { id: 'sessions' as const, label: t('terminalSessions', 'Terminal Sessions') },
                { id: 'mcp' as const, label: 'MCP' },
                { id: 'skills' as const, label: t('skills', 'Skills') },
                { id: 'subagents' as const, label: t('subagents', 'Subagents') },
              ].map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => handleSelectTab(item.id)}
                  className={`rounded-md px-3 py-1.5 text-sm ${
                    activeTab === item.id ? 'bg-black text-white' : 'bg-white text-black'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>

          {activeTab === 'sessions' && (
            <div className="space-y-4 pb-24">
              <div className="rounded-xl border bg-card p-4">
                <div className="flex flex-col gap-3">
                  <div>
                    <h3 className="text-lg font-semibold tracking-tight">
                      {t('terminalSessions', 'Terminal Sessions')}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {t(
                        'workspaceSessionsSectionDesc',
                        'Filter terminal sessions for this workspace by tool, model, or name, then continue, rename, or remove them without leaving the current context.',
                      )}
                    </p>
                  </div>
                </div>
              </div>
              <AiSessionsList
                sessions={activeSessions}
                loading={sessionsLoading || !sessionsInitialized}
                queryState={sessionQuery}
                onQueryChange={setSessionQuery}
                serverFiltered
                totalSessions={sessionsTotal}
                availableToolOptions={sessionToolOptions}
                availableModelOptions={sessionModelOptions}
                onLaunch={handleWorkspaceSessionLaunch}
                onDelete={handleWorkspaceSessionDelete}
                onRename={handleWorkspaceSessionRename}
                onFavoriteChange={handleWorkspaceSessionFavoriteChange}
              />
            </div>
          )}

          {activeTab === 'sessions' && (
            <button
              type="button"
              onClick={() => openLaunchDialog(activeWorkspace)}
              className="fixed bottom-4 right-4 z-40 inline-flex h-12 items-center gap-2 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground shadow-lg shadow-primary/20 transition-all hover:-translate-y-0.5 hover:bg-primary/90 hover:shadow-xl sm:bottom-6 sm:right-6"
              title={t('workspaceQuickLaunch', 'New AI Session')}
            >
              <Play className="h-4 w-4" />
              {t('workspaceQuickLaunch', 'New AI Session')}
            </button>
          )}

          {activeTab === 'mcp' && (
            <div className="space-y-4">
              {mcpLoading && !mcpInitialized ? (
                <div className="flex min-h-[16rem] flex-col items-center justify-center rounded-xl border bg-card text-muted-foreground">
                  <Loader2 className="mb-3 h-8 w-8 animate-spin" />
                  <p>{t('loading', 'Loading...')}</p>
                </div>
              ) : (
                <>
              <div className="space-y-3">
                <div className="rounded-xl border bg-card p-4">
                  <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                    <div className="min-w-0">
                      <h3 className="text-lg font-semibold tracking-tight">
                        {t('mcpServers', 'MCP Servers')}
                      </h3>
                      <p className="mt-1 text-sm text-muted-foreground">
                        {t(
                          'workspaceMcpSectionDesc',
                          'Review MCP servers already enabled in this workspace by model, and adjust which models can use each server.',
                        )}
                      </p>
                    </div>
                    <div className="flex flex-wrap items-center gap-2 lg:justify-end">
                      {mcpLoading && mcpInitialized && (
                        <div className="inline-flex w-fit items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs text-muted-foreground">
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          {t('loading', 'Loading...')}
                        </div>
                      )}
                      <button
                        type="button"
                        onClick={() => {
                          navigateToCapability('mcp-servers', activeWorkspace, 'recommended');
                        }}
                        className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
                      >
                        <Settings2 className="h-4 w-4" />
                        {t('workspaceMcpManageGlobalServers', 'Manage Global Servers')}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="border rounded-xl bg-card p-3">
                  <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                    {TOOL_OPTIONS.map((tool) => (
                      <button
                        key={`workspace-mcp-model-${tool.id}`}
                        type="button"
                        onClick={() => setActiveMcpModel(tool.id)}
                        className={`rounded-lg border px-4 py-3 text-left transition-all ${
                          activeMcpModel === tool.id
                            ? 'border-primary bg-primary/5'
                            : 'hover:bg-muted/40 hover:-translate-y-0.5'
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <ToolIcon tool={tool.id} className="h-5 w-5" />
                          <span className="text-sm font-semibold">{tool.label}</span>
                        </div>
                        <div className="mt-2.5 text-sm leading-none text-muted-foreground">
                          {t('mcpInstalledCountForModel', 'Enabled {{count}} MCP servers', {
                            count: workspaceInstalledCountsByModel[tool.id],
                          })}
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
                <div className="flex items-start gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
                  <Info className="mt-0.5 h-4 w-4 shrink-0" />
                  <p>
                    <span className="font-medium text-foreground">
                      {t('workspaceEffectiveLoadRule', 'Effective load rule')}:
                    </span>{' '}
                    {activeMcpLoadRule}
                  </p>
                </div>
              </div>

              {workspaceInstalledCards.length === 0 ? (
                <div className="text-center py-12">
                  <Server className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
                  <h3 className="text-lg font-semibold mb-2">
                    {t('mcpNoServersForModelTitle', 'No enabled MCP for this model')}
                  </h3>
                  <p className="text-muted-foreground">
                    {t('workspaceMcpNoInstalledForModelDesc', 'This workspace has not enabled any MCP servers for the selected model yet.')}
                  </p>
                  <div className="mt-4 flex flex-wrap justify-center gap-2">
                    <button
                      type="button"
                      onClick={() => {
                        navigateToCapability('mcp-servers', activeWorkspace, 'recommended');
                      }}
                      className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                    >
                      <Settings2 className="h-4 w-4" />
                      {t('workspaceMcpBrowseGlobalServers', 'Browse Global Servers')}
                    </button>
                  </div>
                </div>
              ) : (
                <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                  {workspaceInstalledCards.map(({ server, scope, enabled_models }) => (
                    <div
                      key={`workspace-mcp-installed-${activeMcpModel}-${scope}-${server.id}`}
                      className="group border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="p-2 rounded-md bg-primary/10 text-primary">
                          <Server className="w-4 h-4" />
                        </div>
                        <div className="flex flex-col items-end gap-1">
                          <span
                            className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${getScopeBadgeClassName(scope)}`}
                          >
                            {scope === 'global'
                              ? t('workspaceScopeUser', 'User-level')
                              : t('workspaceScopeDirectory', 'Directory-level')}
                          </span>
                          <span className="text-[10px] text-muted-foreground uppercase">
                            {server.transport || 'stdio'}
                          </span>
                        </div>
                      </div>

                      <h4 className="mt-3 font-semibold text-sm line-clamp-1">{server.name}</h4>
                      <p className="mt-1 text-xs text-muted-foreground line-clamp-2">
                        {server.description?.trim() || t('workspaceMcpNoDescription', 'No description')}
                      </p>

                      <div className="mt-3 text-[11px] text-muted-foreground font-mono line-clamp-1">
                        {getMcpConnectionText(server)}
                      </div>
                      <div className="mt-2 text-[11px] text-muted-foreground">
                        {t('workspaceMcpEnabledModels', 'Enabled models')}: {formatEnabledModels(enabled_models)}
                      </div>

                      <div className="mt-3 flex items-center justify-between gap-2">
                        {scope === 'global' ? (
                          <button
                            type="button"
                            onClick={() => {
                              navigateToCapability('mcp-servers', activeWorkspace, 'installed');
                            }}
                            className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs font-medium hover:bg-muted"
                          >
                            <Settings2 className="h-3.5 w-3.5" />
                            {t('workspaceManageUserLevel', 'Manage User-level')}
                          </button>
                        ) : (
                          <>
                            <button
                              type="button"
                              onClick={() => {
                                void openMcpInstallDialog(server);
                              }}
                              className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs font-medium hover:bg-muted"
                            >
                              <Settings2 className="h-3.5 w-3.5" />
                              {t('workspaceMcpManageModels', 'Manage Models')}
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                void handleUninstallWorkspaceMcpForModel(server.id, activeMcpModel);
                              }}
                              className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1.5 text-xs font-medium text-destructive hover:bg-destructive/10"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                              {t('workspaceMcpDisableCurrentModel', 'Disable current model')}
                            </button>
                          </>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div className="rounded-xl border bg-card p-4">
                <div className="flex flex-col gap-1">
                  <h3 className="text-base font-semibold tracking-tight">
                    {t('workspaceMcpAvailableSectionTitle', 'Add MCP to This Workspace')}
                  </h3>
                  <p className="text-sm text-muted-foreground">
                    {t(
                      'workspaceMcpAvailableSectionDesc',
                      'Choose from global MCP server definitions and enable them for the current workspace model.',
                    )}
                  </p>
                </div>
              </div>

              {workspaceAvailableMcpEntries.length === 0 ? (
                <div className="text-center py-10">
                  <Server className="mx-auto mb-4 h-14 w-14 text-muted-foreground" />
                  <p className="text-sm text-muted-foreground">
                    {t('workspaceMcpEmpty', 'No MCP servers available yet. Add global MCP servers first, then bind them to this workspace.')}
                  </p>
                </div>
              ) : (
                <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                  {workspaceAvailableMcpEntries.map(({ server, enabled_models, status }) => {
                    const statusMeta = getWorkspaceMcpStatusMeta(status);
                    const enabledForCurrentModel = status === 'enabled_for_model';
                    const enabledAtUserLevel = status === 'enabled_user_level';
                    const enabledForOtherModels = status === 'bound_other_models';
                    return (
                      <div
                        key={`workspace-mcp-catalog-${activeMcpModel}-${server.id}`}
                        className="group rounded-xl border bg-card p-4 transition-all duration-200 hover:-translate-y-0.5 hover:border-primary/30 hover:shadow-md"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="rounded-md bg-primary/10 p-2 text-primary">
                            <Server className="h-4 w-4" />
                          </div>
                          <div className="flex flex-col items-end gap-1">
                            <span className={`rounded-full border px-2 py-0.5 text-[10px] ${statusMeta.className}`}>
                              {statusMeta.label}
                            </span>
                            <span
                              className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${getScopeBadgeClassName('global')}`}
                            >
                              {t('workspaceScopeUser', 'User-level')}
                            </span>
                          </div>
                        </div>

                        <h4 className="mt-3 line-clamp-1 text-sm font-semibold">{server.name}</h4>
                        <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                          {server.description?.trim() || t('workspaceMcpNoDescription', 'No description')}
                        </p>

                        <div className="mt-3 line-clamp-1 font-mono text-[11px] text-muted-foreground">
                          {getMcpConnectionText(server)}
                        </div>
                        <div className="mt-2 text-[11px] text-muted-foreground">
                          {t('workspaceMcpEnabledModels', 'Enabled models')}: {formatEnabledModels(enabled_models)}
                        </div>

                        <div className="mt-3 flex items-center justify-between gap-2">
                          <button
                            type="button"
                            onClick={() => {
                              if (enabledForCurrentModel) {
                                void openMcpInstallDialog(server);
                                return;
                              }
                              void handleEnableWorkspaceMcpForActiveModel(server);
                            }}
                            className={`inline-flex items-center gap-2 rounded-md px-3 py-1.5 text-xs font-medium ${
                              enabledForCurrentModel
                                ? 'border hover:bg-muted'
                                : 'bg-primary text-primary-foreground hover:bg-primary/90'
                            }`}
                          >
                            <Settings2 className="h-3.5 w-3.5" />
                            {enabledForCurrentModel
                              ? t('workspaceMcpManageModels', 'Manage Models')
                              : enabledAtUserLevel
                                ? t('workspaceMcpPromoteToDirectoryLevel', 'Enable Directory-level')
                                : t('workspaceMcpEnableCurrentModel', 'Enable Current Model')}
                          </button>
                          {enabledForOtherModels && (
                            <button
                              type="button"
                              onClick={() => {
                                void openMcpInstallDialog(server);
                              }}
                              className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1.5 text-xs font-medium hover:bg-muted"
                            >
                              <Settings2 className="h-3.5 w-3.5" />
                              {t('workspaceMcpManageModels', 'Manage Models')}
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
                </>
              )}
            </div>
          )}

          {activeTab === 'skills' && (
            <WorkspaceSkillsPanel
              isVisible={isVisible && activeTab === 'skills'}
              rootPath={activeWorkspace.root_path}
              onNavigateToGlobalPage={(entry) => {
                navigateToCapability('skills', activeWorkspace, entry);
              }}
            />
          )}

          {activeTab === 'subagents' && (
            <WorkspaceSubagentsPanel
              isVisible={isVisible && activeTab === 'subagents'}
              rootPath={activeWorkspace.root_path}
              onNavigateToGlobalPage={(entry) => {
                navigateToCapability('subagents', activeWorkspace, entry);
              }}
            />
          )}
        </>
      )}

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        {dialogOpen && (
          <DialogContent className="max-w-xl">
            <DialogHeader>
              <DialogTitle>{formTitle}</DialogTitle>
              <DialogDescription>
                {dialogMode === 'create'
                  ? t('workspaceCreateDesc', 'Name and directory are required. Description and tags are optional.')
                  : t('workspaceEditDesc', 'Only name, description, and tags can be updated. Directory is read-only.')}
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('name', 'Name')}
                </label>
                <input
                  value={formState.name}
                  onChange={(event) => {
                    setFormState((prev) => ({ ...prev, name: event.target.value }));
                    if (formError) setFormError('');
                  }}
                  className="h-10 w-full rounded-md border px-3 text-sm"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('workingDirectory', 'Directory')}
                </label>
                <div className="flex gap-2">
                  <input
                    value={formState.root_path}
                    onChange={(event) => {
                      if (dialogMode === 'edit') return;
                      setFormState((prev) => ({ ...prev, root_path: event.target.value }));
                      if (formError) setFormError('');
                    }}
                    readOnly={dialogMode === 'edit'}
                    className={`h-10 w-full rounded-md border px-3 text-sm ${
                      dialogMode === 'edit' ? 'bg-muted/60 text-muted-foreground' : ''
                    }`}
                  />
                  {dialogMode === 'create' && (
                    <button
                      type="button"
                      onClick={async () => {
                        const selected = await open({ directory: true, multiple: false });
                        if (selected && typeof selected === 'string') {
                          setFormState((prev) => ({ ...prev, root_path: selected }));
                          if (formError) setFormError('');
                        }
                      }}
                      className="inline-flex items-center justify-center rounded-md border px-3 hover:bg-muted"
                    >
                      <FolderOpen className="h-4 w-4" />
                    </button>
                  )}
                </div>
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('description', 'Description')}
                </label>
                <textarea
                  value={formState.description}
                  onChange={(event) => setFormState((prev) => ({ ...prev, description: event.target.value }))}
                  rows={3}
                  className="w-full rounded-md border px-3 py-2 text-sm"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('tags', 'Tags')}
                </label>
                <input
                  value={formState.tags}
                  onChange={(event) => setFormState((prev) => ({ ...prev, tags: event.target.value }))}
                  placeholder={t('workspaceTagsPlaceholder', 'frontend, work, personal')}
                  className="h-10 w-full rounded-md border px-3 text-sm"
                />
              </div>
              {formError && <p className="text-sm text-destructive">{formError}</p>}
            </div>
            <DialogFooter>
              <button
                type="button"
                onClick={() => setDialogOpen(false)}
                className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
                disabled={formSubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleWorkspaceSubmit();
                }}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                disabled={formSubmitting}
              >
                {formSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
                {dialogMode === 'create' ? t('create', 'Create') : t('save', 'Save')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={Boolean(launchWorkspace)} onOpenChange={(open) => !open && setLaunchWorkspace(null)}>
        {launchWorkspace && (
          <DialogContent className="max-w-lg">
            <DialogHeader>
              <DialogTitle>{t('workspaceLaunchDialogTitle', 'Choose a model')}</DialogTitle>
              <DialogDescription>
                {t('workspaceLaunchDialogDesc', 'Start a new AI terminal session in {{name}}', {
                  name: launchWorkspace.name,
                })}
              </DialogDescription>
            </DialogHeader>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {TOOL_OPTIONS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setLaunchModel(item.id)}
                  className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
                    launchModel === item.id
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'hover:bg-muted'
                  }`}
                >
                  <ToolIcon tool={item.id} className="h-5 w-5" />
                  <span className="font-medium">{item.label}</span>
                </button>
              ))}
            </div>
            <DialogFooter>
              <button
                type="button"
                onClick={() => setLaunchWorkspace(null)}
                className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
                disabled={launchSubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleLaunchWorkspaceSession();
                }}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                disabled={launchSubmitting}
              >
                {launchSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('launch', 'Launch')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog
        open={Boolean(mcpDialogServer)}
        onOpenChange={(open) => {
          if (!open && !mcpDialogSubmitting) {
            setMcpDialogServer(null);
            setMcpDialogModels([]);
            setMcpDialogError('');
          }
        }}
      >
        {mcpDialogServer && (
          <DialogContent className="max-w-lg">
            <DialogHeader>
              <DialogTitle>{t('workspaceMcpInstallDialogTitle', 'Manage workspace MCP models')}</DialogTitle>
              <DialogDescription>
                {t(
                  'workspaceMcpInstallDialogDesc',
                  'Choose which models in {{name}} should enable {{server}}.',
                  {
                    name: activeWorkspace?.name || '',
                    server: mcpDialogServer.name,
                  },
                )}
              </DialogDescription>
            </DialogHeader>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {TOOL_OPTIONS.map((tool) => {
                const selected = mcpDialogModels.includes(tool.id);
                return (
                  <button
                    key={`workspace-mcp-dialog-${tool.id}`}
                    type="button"
                    onClick={() => toggleMcpDialogModel(tool.id)}
                    className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
                      selected ? 'border-primary bg-primary/10 text-primary' : 'hover:bg-muted'
                    }`}
                  >
                    <ToolIcon tool={tool.id} className="h-5 w-5" />
                    <div className="min-w-0 flex-1">
                      <div className="font-medium">{tool.label}</div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        {selected ? t('selected', 'Selected') : t('clickToSelect', 'Click to select')}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
            {mcpDialogError && <p className="text-sm text-destructive">{mcpDialogError}</p>}
            <DialogFooter>
              <button
                type="button"
                onClick={() => {
                  setMcpDialogServer(null);
                  setMcpDialogModels([]);
                  setMcpDialogError('');
                }}
                className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
                disabled={mcpDialogSubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleSaveMcpDialog();
                }}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                disabled={mcpDialogSubmitting}
              >
                {mcpDialogSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('save', 'Save')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog
        open={Boolean(copyWorkspace)}
        onOpenChange={(open) => {
          if (!open && !copySubmitting) {
            setCopyWorkspace(null);
            setCopyDetail(null);
            setCopySkills([]);
            setCopySubagents([]);
            setCopyTargetRoot('');
            setCopyError('');
          }
        }}
      >
        {copyWorkspace && (
          <DialogContent className="max-w-3xl max-h-[85vh] overflow-hidden">
            <DialogHeader>
              <DialogTitle>{t('workspaceCopyTitle', 'Copy Workspace Config')}</DialogTitle>
              <DialogDescription>
                {t('workspaceCopyDesc', 'Choose what to copy from {{name}} and where to create or update the target workspace.', {
                  name: copyWorkspace.name,
                })}
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4 overflow-auto pr-1">
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('workspaceCopyTarget', 'Target Directory')}
                </label>
                <div className="flex gap-2">
                  <input
                    value={copyTargetRoot}
                    onChange={(event) => {
                      setCopyTargetRoot(event.target.value);
                      if (copyError) setCopyError('');
                    }}
                    className="h-10 w-full rounded-md border px-3 text-sm"
                    placeholder={t('workspaceCopyTargetPlaceholder', 'Choose a target folder')}
                  />
                  <button
                    type="button"
                    onClick={async () => {
                      const selected = await open({ directory: true, multiple: false });
                      if (selected && typeof selected === 'string') {
                        setCopyTargetRoot(selected);
                        if (copyError) setCopyError('');
                      }
                    }}
                    className="inline-flex items-center justify-center rounded-md border px-3 hover:bg-muted"
                  >
                    <FolderOpen className="h-4 w-4" />
                  </button>
                </div>
              </div>

              {copyLoading ? (
                <div className="flex items-center gap-2 rounded-xl border bg-card p-4 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('loading', 'Loading...')}
                </div>
              ) : (
                <>
                  <div className="rounded-xl border bg-card p-4">
                    <div className="mb-3 flex items-center justify-between">
                      <div className="inline-flex items-center gap-2 text-sm font-medium">
                        <Server className="h-4 w-4" />
                        MCP
                      </div>
                      <div className="flex items-center gap-2 text-xs">
                        <button type="button" onClick={() => setAllCopySelections('mcp', true)} className="hover:text-foreground">
                          {t('selectAll', 'Select All')}
                        </button>
                        <button type="button" onClick={() => setAllCopySelections('mcp', false)} className="hover:text-foreground">
                          {t('clear', 'Clear')}
                        </button>
                      </div>
                    </div>
                    <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                      {(copyDetail?.mcp_bindings || []).length === 0 ? (
                        <div className="text-sm text-muted-foreground">{t('workspaceCopyEmptyMcp', 'No workspace MCP bindings')}</div>
                      ) : (
                        (copyDetail?.mcp_bindings || []).map((binding) => {
                          const server = mcpServers.find((item) => item.id === binding.server_id);
                          const selected = copySelectedMcpIds.includes(binding.server_id);
                          return (
                            <button
                              key={binding.server_id}
                              type="button"
                              onClick={() => toggleCopySelection('mcp', binding.server_id)}
                              className={`rounded-lg border p-3 text-left ${
                                selected ? 'border-primary bg-primary/10' : 'hover:bg-muted'
                              }`}
                            >
                              <div className="font-medium">{server?.name || binding.server_id}</div>
                              <div className="mt-1 text-xs text-muted-foreground">
                                {(binding.enabled_models || []).join(', ') || '-'}
                              </div>
                            </button>
                          );
                        })
                      )}
                    </div>
                  </div>

                  <div className="rounded-xl border bg-card p-4">
                    <div className="mb-3 flex items-center justify-between">
                      <div className="inline-flex items-center gap-2 text-sm font-medium">
                        <Sparkles className="h-4 w-4" />
                        {t('skills', 'Skills')}
                      </div>
                      <div className="flex items-center gap-2 text-xs">
                        <button type="button" onClick={() => setAllCopySelections('skills', true)} className="hover:text-foreground">
                          {t('selectAll', 'Select All')}
                        </button>
                        <button type="button" onClick={() => setAllCopySelections('skills', false)} className="hover:text-foreground">
                          {t('clear', 'Clear')}
                        </button>
                      </div>
                    </div>
                    <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                      {copySkills.length === 0 ? (
                        <div className="text-sm text-muted-foreground">{t('workspaceCopyEmptySkills', 'No project skills')}</div>
                      ) : (
                        copySkills.map((item) => {
                          const selected = copySelectedSkills.includes(item.selection_key);
                          return (
                            <button
                              key={item.selection_key}
                              type="button"
                              onClick={() => toggleCopySelection('skills', item.selection_key)}
                              className={`rounded-lg border p-3 text-left ${
                                selected ? 'border-primary bg-primary/10' : 'hover:bg-muted'
                              }`}
                            >
                              <div className="font-medium">{item.name}</div>
                              <div className="mt-1 text-xs text-muted-foreground">
                                {item.model} · {item.source_rel_path}
                              </div>
                            </button>
                          );
                        })
                      )}
                    </div>
                  </div>

                  <div className="rounded-xl border bg-card p-4">
                    <div className="mb-3 flex items-center justify-between">
                      <div className="inline-flex items-center gap-2 text-sm font-medium">
                        <Bot className="h-4 w-4" />
                        {t('subagents', 'Subagents')}
                      </div>
                      <div className="flex items-center gap-2 text-xs">
                        <button type="button" onClick={() => setAllCopySelections('subagents', true)} className="hover:text-foreground">
                          {t('selectAll', 'Select All')}
                        </button>
                        <button type="button" onClick={() => setAllCopySelections('subagents', false)} className="hover:text-foreground">
                          {t('clear', 'Clear')}
                        </button>
                      </div>
                    </div>
                    <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                      {copySubagents.length === 0 ? (
                        <div className="text-sm text-muted-foreground">{t('workspaceCopyEmptySubagents', 'No project subagents')}</div>
                      ) : (
                        copySubagents.map((item) => {
                          const selected = copySelectedSubagents.includes(item.selection_key);
                          return (
                            <button
                              key={item.selection_key}
                              type="button"
                              onClick={() => toggleCopySelection('subagents', item.selection_key)}
                              className={`rounded-lg border p-3 text-left ${
                                selected ? 'border-primary bg-primary/10' : 'hover:bg-muted'
                              }`}
                            >
                              <div className="font-medium">{item.name}</div>
                              <div className="mt-1 text-xs text-muted-foreground">
                                {item.model} · {item.source_rel_path}
                              </div>
                            </button>
                          );
                        })
                      )}
                    </div>
                  </div>
                </>
              )}

              {copyError && <p className="text-sm text-destructive">{copyError}</p>}
            </div>
            <DialogFooter>
              <button
                type="button"
                onClick={() => setCopyWorkspace(null)}
                className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
                disabled={copySubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleCopyWorkspace();
                }}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                disabled={copySubmitting || copyLoading}
              >
                {copySubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('copy', 'Copy')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      {/* Permission confirmation dialog */}
      {permissionDialogSession && (
        <TerminalPermissionConfirmDialog
          open={permissionDialogOpen}
          toolId={permissionDialogSession.model_type.toLowerCase() as PermAiModelId}
          toolLabel={permissionDialogSession.model_type}
          onConfirm={handleWorkspacePermissionConfirm}
          onCancel={handleWorkspacePermissionCancel}
        />
      )}
    </div>
  );
}
