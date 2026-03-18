import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import {
  ArrowLeft,
  Bot,
  CheckCircle2,
  Copy,
  FolderOpen,
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
import { AiSessionsList, type AiSessionListItem } from '../AiSessionsList';
import { Skills } from '../Skills';
import { Subagents } from '../Subagents';
import { useConfirmDialog } from '../ConfirmDialogProvider';
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
type WorkspaceMcpViewMode = 'repository' | 'installed';

type WorkspaceFormState = {
  id?: string;
  name: string;
  root_path: string;
  description: string;
  tags: string;
};

type CopyableSkill = InstalledSkill & { selection_key: string };
type CopyableSubagent = InstalledSubagent & { selection_key: string };
type WorkspaceMcpEntry = { server: MCPServer; binding: WorkspaceMcpBinding | null };

const TOOL_OPTIONS: Array<{ id: ModelId; label: string }> = [
  { id: 'claude', label: 'Claude Code' },
  { id: 'gemini', label: 'Gemini' },
  { id: 'codex', label: 'Codex' },
  { id: 'opencode', label: 'OpenCode' },
];

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

function getMcpConnectionText(server: MCPServer) {
  const command = normalizeText(server.command).trim();
  if (command) {
    const args = Array.isArray(server.args) ? server.args.join(' ') : '';
    return `${command}${args ? ` ${args}` : ''}`;
  }
  return normalizeText(server.http_url || server.url, '-');
}

export function Workspaces({ isVisible = false }: { isVisible?: boolean }) {
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
  const [activeTab, setActiveTab] = useState<WorkspaceTab>('sessions');
  const [mcpViewMode, setMcpViewMode] = useState<WorkspaceMcpViewMode>('installed');
  const [activeMcpModel, setActiveMcpModel] = useState<ModelId>('claude');
  const [mcpServers, setMcpServers] = useState<MCPServer[]>([]);
  const [mcpDialogServer, setMcpDialogServer] = useState<MCPServer | null>(null);
  const [mcpDialogModels, setMcpDialogModels] = useState<ModelId[]>([]);
  const [mcpDialogSubmitting, setMcpDialogSubmitting] = useState(false);
  const [mcpDialogError, setMcpDialogError] = useState('');
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogMode, setDialogMode] = useState<DialogMode>('create');
  const [formSubmitting, setFormSubmitting] = useState(false);
  const [formError, setFormError] = useState('');
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

  const activeWorkspace = activeDetail?.workspace.workspace || null;

  const loadMcpServers = useCallback(async () => {
    if (!isTauri) return;
    if (mcpServers.length > 0) return;
    try {
      const resp = await invoke<MCPStateResp>('get_mcp_servers');
      setMcpServers(
        Array.isArray(resp?.servers) ? resp.servers.map((server) => normalizeMcpServer(server)) : [],
      );
    } catch (e) {
      console.error('Failed to load MCP servers', e);
    }
  }, [isTauri, mcpServers.length]);

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

  const loadWorkspaceDetail = useCallback(async (workspaceId: string) => {
    if (!isTauri) return;
    try {
      setLoading(true);
      const [detailResp, sessionsResp] = await Promise.all([
        invoke<ApiResp<WorkspaceDetail>>('workspace_get', { workspaceId }),
        invoke<ApiResp<AiSessionListItem[]>>('workspace_sessions_list', { workspaceId }),
      ]);
      setActiveWorkspaceId(workspaceId);
      setActiveDetail(normalizeWorkspaceDetail(detailResp.data));
      setActiveSessions(Array.isArray(sessionsResp.data) ? sessionsResp.data : []);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceDetailLoadFailed', 'Failed to load workspace detail: {{message}}', {
          message: String(e),
        }),
      });
    } finally {
      setLoading(false);
    }
  }, [isTauri, t]);

  const refreshActiveWorkspace = useCallback(async () => {
    if (!activeWorkspaceId) return;
    await loadWorkspaceDetail(activeWorkspaceId);
  }, [activeWorkspaceId, loadWorkspaceDetail]);

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
          project_root: workspace.root_path,
        }),
        invoke<ApiResp<InstalledSubagent[]>>('subagents_list_installed', {
          model: null,
          scope: 'project',
          project_root: workspace.root_path,
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
  }, [isVisible, loadWorkspaces]);

  useEffect(() => {
    if (!isVisible || !activeWorkspace || activeTab !== 'mcp') return;
    void loadMcpServers();
  }, [activeTab, activeWorkspace, isVisible, loadMcpServers]);

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

  const workspaceRepositoryMcpEntries = useMemo<WorkspaceMcpEntry[]>(() => {
    return [...mcpServers]
      .sort(sortMcpServersByName)
      .map((server) => ({
        server,
        binding:
          (activeDetail?.mcp_bindings || []).find((binding) => binding.server_id === server.id) || null,
      }));
  }, [activeDetail, mcpServers]);

  const workspaceInstalledMcpEntries = useMemo<WorkspaceMcpEntry[]>(() => {
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
      }))
      .sort((a, b) => sortMcpServersByName(a.server, b.server));
  }, [activeDetail, mcpServers]);

  const workspaceInstalledCountsByModel = useMemo<Record<ModelId, number>>(() => {
    const counts: Record<ModelId, number> = {
      claude: 0,
      gemini: 0,
      codex: 0,
      opencode: 0,
    };
    workspaceInstalledMcpEntries.forEach((entry) => {
      (entry.binding?.enabled_models || []).forEach((model) => {
        if (model in counts) {
          counts[model as ModelId] += 1;
        }
      });
    });
    return counts;
  }, [workspaceInstalledMcpEntries]);

  const workspaceInstalledCards = useMemo(
    () =>
      workspaceInstalledMcpEntries.filter((entry) =>
        (entry.binding?.enabled_models || []).includes(activeMcpModel),
      ),
    [activeMcpModel, workspaceInstalledMcpEntries],
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

  const activeMcpModelLabel = useMemo(
    () => TOOL_OPTIONS.find((item) => item.id === activeMcpModel)?.label || activeMcpModel,
    [activeMcpModel],
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
      await loadWorkspaceDetail(resp.data.workspace.workspace.id);
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
    try {
      await invoke('sessions_launch', { sessionId: session.id });
      await refreshActiveWorkspace();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('workspaceSessionLaunchFailed', 'Failed to launch session: {{message}}', {
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
      setMcpViewMode('installed');
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
      <div className="flex items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('workspaces', 'Workspaces')}</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            {activeWorkspace
              ? t('workspaceDetailDesc', 'Manage the current workspace directory and its related sessions.')
              : t('workspaceListDesc', 'A workspace is a local folder with project-scoped MCP, Skills, Subagents, and sessions.')}
          </p>
        </div>
        <div className="flex items-center gap-2">
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
                return (
                  <div
                    key={workspace.id}
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      setActiveTab('sessions');
                      void loadWorkspaceDetail(workspace.id);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault();
                        setActiveTab('sessions');
                        void loadWorkspaceDetail(workspace.id);
                      }
                    }}
                    className="group flex h-full flex-col rounded-xl border bg-card p-4 text-left transition-all hover:-translate-y-0.5 hover:border-primary/35 hover:shadow-sm"
                  >
                    <div className="flex items-start gap-2.5">
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-start gap-1.5">
                          <span className="min-w-0 flex-1 truncate text-base font-semibold leading-tight">
                            {workspace.name}
                          </span>
                          <span className="shrink-0 rounded-full border px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide text-muted-foreground">
                            {getSourceBadgeLabel(workspace.source)}
                          </span>
                        </div>
                        <div
                          className="mt-2 rounded-md bg-muted/40 px-2.5 py-1.5 text-[11px] leading-4 text-muted-foreground"
                          title={workspace.root_path}
                        >
                          {workspace.root_path}
                        </div>
                      </div>
                      <div className="flex items-center gap-1 self-start">
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
                          className="rounded-md p-1.5 text-muted-foreground/80 transition-colors hover:bg-muted hover:text-foreground"
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
                          className="rounded-md p-1.5 text-muted-foreground/80 transition-colors hover:bg-destructive/10 hover:text-destructive"
                          title={t('delete', 'Delete')}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
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
                        className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-secondary px-2.5 text-xs font-medium text-secondary-foreground hover:bg-secondary/80"
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
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border px-2.5 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                    >
                      <ArrowLeft className="h-3.5 w-3.5" />
                      {t('back', 'Back')}
                    </button>
                    <h3 className="min-w-0 truncate text-lg font-semibold tracking-tight">
                      {activeWorkspace.name}
                    </h3>
                    <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                      {getSourceBadgeLabel(activeWorkspace.source)}
                    </span>
                    <div
                      className="min-w-0 max-w-full truncate text-xs text-muted-foreground md:max-w-[40rem]"
                      title={activeWorkspace.root_path}
                    >
                      {activeWorkspace.root_path}
                    </div>
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
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border px-2.5 text-xs hover:bg-muted"
                  >
                    <Copy className="h-3.5 w-3.5" />
                    {t('workspaceCopyAction', 'Copy Config')}
                  </button>
                  <button
                    type="button"
                    onClick={() => openLaunchDialog(activeWorkspace)}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                  >
                    <Play className="h-3.5 w-3.5" />
                    {t('workspaceQuickLaunch', 'New AI Session')}
                  </button>
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
                onClick={() => setActiveTab(item.id)}
                className={`rounded-md px-3 py-1.5 text-sm ${
                  activeTab === item.id ? 'bg-black text-white' : 'bg-white text-black'
                }`}
              >
                {item.label}
              </button>
            ))}
          </div>

          {activeTab === 'sessions' && (
            <div className="space-y-4">
              <div className="rounded-xl border bg-card p-4">
                <div className="flex flex-col gap-3">
                  <div>
                    <h3 className="text-lg font-semibold tracking-tight">
                      {t('terminalSessions', 'Terminal Sessions')}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {t(
                        'workspaceSessionsSectionDesc',
                        'View AI terminal sessions related to this workspace with the same actions and display as the sessions page.',
                      )}
                    </p>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <span className="rounded-full border px-2.5 py-1 text-[11px] text-muted-foreground">
                      {t('workspaceSessionsCount', '{{count}} sessions', {
                        count: activeSessions.length,
                      })}
                    </span>
                  </div>
                </div>
              </div>
              <AiSessionsList
                sessions={activeSessions}
                loading={loading}
                onLaunch={handleWorkspaceSessionLaunch}
                onDelete={handleWorkspaceSessionDelete}
                onRename={handleWorkspaceSessionRename}
              />
            </div>
          )}

          {activeTab === 'mcp' && (
            <div className="space-y-4">
              <div className="space-y-3">
                <div className="rounded-xl border bg-card p-4">
                  <div className="flex flex-col gap-3">
                  <div>
                    <h3 className="text-lg font-semibold tracking-tight">
                      {t('mcpServers', 'MCP Servers')}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                        {t(
                          'workspaceMcpSectionDesc',
                          'Manage MCP servers available to this workspace from repository and installed views.',
                        )}
                      </p>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <span className="rounded-full border px-2.5 py-1 text-[11px] text-muted-foreground">
                        {t('workspaceMcpRepositoryCount', 'Repository {{count}}', {
                          count: workspaceRepositoryMcpEntries.length,
                        })}
                      </span>
                      <span className="rounded-full border px-2.5 py-1 text-[11px] text-muted-foreground">
                        {t('workspaceMcpInstalledCount', 'Installed {{count}}', {
                          count: workspaceInstalledMcpEntries.length,
                        })}
                      </span>
                      {mcpViewMode === 'installed' && (
                        <span className="rounded-full border px-2.5 py-1 text-[11px] text-muted-foreground">
                          {t('workspaceMcpCurrentModel', 'Current model {{model}}', {
                            model: activeMcpModelLabel,
                          })}
                        </span>
                      )}
                    </div>
                  </div>
                </div>

                <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
                  <button
                    type="button"
                    onClick={() => setMcpViewMode('repository')}
                    className={`rounded-md px-3 py-1.5 text-sm ${
                      mcpViewMode === 'repository' ? 'bg-black text-white' : 'bg-white text-black'
                    }`}
                  >
                    {t('mcpViewByServer', 'Repository')}
                  </button>
                  <button
                    type="button"
                    onClick={() => setMcpViewMode('installed')}
                    className={`rounded-md px-3 py-1.5 text-sm ${
                      mcpViewMode === 'installed' ? 'bg-black text-white' : 'bg-white text-black'
                    }`}
                  >
                    {t('mcpViewByModel', 'Installed')}
                  </button>
                </div>

                {mcpViewMode === 'installed' && (
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
                )}
              </div>

              {mcpViewMode === 'repository' ? (
                workspaceRepositoryMcpEntries.length === 0 ? (
                  <div className="rounded-xl border bg-card p-6 text-sm text-muted-foreground">
                    {t('workspaceMcpEmpty', 'No MCP servers available yet. Add global MCP servers first, then bind them to this workspace.')}
                  </div>
                ) : (
                  <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                    {workspaceRepositoryMcpEntries.map(({ server, binding }) => (
                      <div
                        key={`workspace-mcp-repo-${server.id}`}
                        className="group border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className={`p-2 rounded-md ${binding ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground'}`}>
                            <Server className="w-4 h-4" />
                          </div>
                          <div className="flex items-center gap-2">
                            {binding && (
                              <span className="inline-flex items-center gap-1 rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[10px] text-primary">
                                <CheckCircle2 className="h-3 w-3" />
                                {t('workspaceMcpInstalledForWorkspace', 'Installed')}
                              </span>
                            )}
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
                          {t('workspaceMcpEnabledModels', 'Enabled models')}: {formatEnabledModels(binding?.enabled_models || [])}
                        </div>

                        <div className="mt-3 flex items-center justify-end">
                          <button
                            type="button"
                            onClick={() => {
                              void openMcpInstallDialog(server);
                            }}
                            className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs font-medium hover:bg-muted"
                          >
                            {binding ? <Settings2 className="h-3.5 w-3.5" /> : <Plus className="h-3.5 w-3.5" />}
                            {binding
                              ? t('workspaceMcpManageModels', 'Manage Models')
                              : t('workspaceMcpInstallToWorkspace', 'Install to Workspace')}
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )
              ) : workspaceInstalledCards.length === 0 ? (
                <div className="text-center py-12">
                  <Server className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
                  <h3 className="text-lg font-semibold mb-2">
                    {t('mcpNoServersForModelTitle', 'No enabled MCP for this model')}
                  </h3>
                  <p className="text-muted-foreground mb-4">
                    {t('workspaceMcpNoInstalledForModelDesc', 'Switch to Repository and install MCP into this workspace first.')}
                  </p>
                  <button
                    type="button"
                    onClick={() => setMcpViewMode('repository')}
                    className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
                  >
                    {t('mcpViewByServer', 'Repository')}
                  </button>
                </div>
              ) : (
                <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                  {workspaceInstalledCards.map(({ server, binding }) => (
                    <div
                      key={`workspace-mcp-installed-${activeMcpModel}-${server.id}`}
                      className="group border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="p-2 rounded-md bg-primary/10 text-primary">
                          <Server className="w-4 h-4" />
                        </div>
                        <span className="text-[10px] text-muted-foreground uppercase">
                          {server.transport || 'stdio'}
                        </span>
                      </div>

                      <h4 className="mt-3 font-semibold text-sm line-clamp-1">{server.name}</h4>
                      <p className="mt-1 text-xs text-muted-foreground line-clamp-2">
                        {server.description?.trim() || t('workspaceMcpNoDescription', 'No description')}
                      </p>

                      <div className="mt-3 text-[11px] text-muted-foreground font-mono line-clamp-1">
                        {getMcpConnectionText(server)}
                      </div>
                      <div className="mt-2 text-[11px] text-muted-foreground">
                        {t('workspaceMcpEnabledModels', 'Enabled models')}: {formatEnabledModels(binding?.enabled_models || [])}
                      </div>

                      <div className="mt-3 flex items-center justify-between gap-2">
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
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {activeTab === 'skills' && (
            <Skills
              isVisible={isVisible && activeTab === 'skills'}
              lockedProjectRoot={activeWorkspace.root_path}
            />
          )}

          {activeTab === 'subagents' && (
            <Subagents
              isVisible={isVisible && activeTab === 'subagents'}
              lockedProjectRoot={activeWorkspace.root_path}
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
                    className="h-10 w-full rounded-md border px-3 text-sm"
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
              <DialogTitle>{t('workspaceMcpInstallDialogTitle', 'Install MCP to workspace')}</DialogTitle>
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
    </div>
  );
}
