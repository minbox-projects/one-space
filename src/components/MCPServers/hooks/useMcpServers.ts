import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { TFunction } from 'i18next';
import { notifyMessage } from '@/lib/messages';
import { runUserAction, type ActionContext } from '@/lib/userActions';
import {
  buildDeleteMcpServerActionDescriptor,
  buildSaveMcpServerActionDescriptor,
} from '@/lib/actionDescriptors/mcpServers';
import { DEFAULT_MCP_MODEL_SWITCH_STATE, MCP_MODEL_OPTIONS } from '../constants';
import { parseNpxSpecFromServer } from '../npmPackage';
import type {
  ApiResp,
  MCPModel,
  MCPModelSwitchState,
  MCPServer,
  MCPTemplate,
  MCPUpdateInfo,
  MCPUpdateStatus,
  MCPUpdatesState,
} from '../types';

export interface UseMcpServersOptions {
  isVisible: boolean;
  t: TFunction;
  actionContext: ActionContext;
  confirmDialog: ActionContext['confirm'];
  pushToast: ActionContext['pushToast'];
  onLinkedProvidersChange?: (serverId: string, providerIds: string[]) => void;
}

export function useMcpServers({
  isVisible,
  t,
  actionContext,
  confirmDialog,
  pushToast,
  onLinkedProvidersChange,
}: UseMcpServersOptions) {
  const [servers, setServers] = useState<MCPServer[]>([]);
  const [viewMode, setViewMode] = useState<'server' | 'model'>('server');
  const [activeModel, setActiveModel] = useState<MCPModel>('claude');
  const [modelSwitchStates, setModelSwitchStates] = useState<Record<string, MCPModelSwitchState>>({});
  const [modelSyncPending, setModelSyncPending] = useState<Set<string>>(new Set());
  const [templates, setTemplates] = useState<MCPTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAddModal, setShowAddModal] = useState(false);
  const [showTemplates, setShowTemplates] = useState(false);
  const [editingServer, setEditingServer] = useState<MCPServer | null>(null);
  const [expandedServers, setExpandedServers] = useState<Set<string>>(new Set());
  const [showBackupManager, setShowBackupManager] = useState(false);
  const [showImportExport, setShowImportExport] = useState(false);
  const [refreshingLocalInstall, setRefreshingLocalInstall] = useState(false);
  const [updatesState, setUpdatesState] = useState<MCPUpdatesState | null>(null);
  const [triggeringUpdateCheck, setTriggeringUpdateCheck] = useState(false);
  const [updateApplyPending, setUpdateApplyPending] = useState<Set<string>>(new Set());

  useEffect(() => {
    loadTemplates();
  }, []);

  useEffect(() => {
    if (!isVisible) return;
    loadServers();
    void loadUpdatesStatus();
  }, [isVisible]);

  useEffect(() => {
    if (!isVisible || updatesState?.status !== 'checking') return;
    let stopped = false;
    let pending = false;
    const poll = async () => {
      if (stopped || pending) return;
      pending = true;
      try {
        await loadUpdatesStatus();
      } finally {
        pending = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => {
      void poll();
    }, 1500);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [isVisible, updatesState?.status]);

  function createDefaultSwitchStates(serverList: MCPServer[]) {
    const defaults: Record<string, MCPModelSwitchState> = {};
    for (const server of serverList) {
      defaults[server.id] = { ...DEFAULT_MCP_MODEL_SWITCH_STATE };
    }
    return defaults;
  }

  async function loadModelSwitchStates(serverList: MCPServer[], refreshFromLocal = false) {
    const defaults = createDefaultSwitchStates(serverList);
    if (serverList.length === 0) {
      setModelSwitchStates(defaults);
      return;
    }
    try {
      const command = refreshFromLocal ? 'refresh_mcp_local_install_state' : 'get_mcp_model_switch_states';
      const result = await invoke(command) as Record<string, MCPModelSwitchState>;
      setModelSwitchStates({ ...defaults, ...result });
    } catch (e) {
      console.error('Failed to load MCP model switch states:', e);
      setModelSwitchStates(defaults);
    }
  }

  async function loadServers(refreshInstallState = true) {
    setLoading(true);
    try {
      const state = await invoke('get_mcp_servers');
      const nextServers = ((state as any).servers || []) as MCPServer[];
      setServers(nextServers);
      await loadModelSwitchStates(nextServers, refreshInstallState);
    } catch (e) {
      console.error('Failed to load MCP servers:', e);
    } finally {
      setLoading(false);
    }
  }

  async function handleRefreshLocalInstallState() {
    setRefreshingLocalInstall(true);
    try {
      await loadModelSwitchStates(servers, true);
    } finally {
      setRefreshingLocalInstall(false);
    }
  }

  async function loadTemplates() {
    try {
      const result = await invoke('list_mcp_templates');
      setTemplates(result as MCPTemplate[]);
    } catch (e) {
      console.error('Failed to load MCP templates:', e);
    }
  }

  async function loadUpdatesStatus() {
    try {
      const result = await invoke('mcp_updates_status_get') as ApiResp<MCPUpdatesState> | MCPUpdatesState;
      const next = (result as ApiResp<MCPUpdatesState>)?.data
        ? (result as ApiResp<MCPUpdatesState>).data
        : result as MCPUpdatesState;
      setUpdatesState(next);
    } catch (e) {
      console.error('Failed to load MCP updates status:', e);
    }
  }

  async function handleCheckUpdatesInBackground() {
    setTriggeringUpdateCheck(true);
    try {
      await invoke('mcp_updates_check_background');
      await loadUpdatesStatus();
    } catch (e) {
      void notifyMessage.error({
        source: 'mcp_servers',
        category: 'updates',
        title: t(
          'mcpUpdateCheckFailedMessageTitle',
          'MCP Servers update check failed',
        ),
        error: e,
        dedupeKey: 'mcp:update-check:error',
        target: { tab: 'mcp-servers' },
      });
      pushToast({ title: t('mcpUpdateCheckFailed', { error: e }), kind: 'error' });
    } finally {
      setTriggeringUpdateCheck(false);
    }
  }

  async function handleApplyUpdate(serverId: string) {
    setUpdateApplyPending(prev => new Set(prev).add(serverId));
    try {
      await invoke('mcp_update_apply', { input: { server_id: serverId } });
      await Promise.all([loadServers(false), loadUpdatesStatus()]);
    } catch (e) {
      void notifyMessage.error({
        source: 'mcp_servers',
        category: 'updates',
        title: t(
          'mcpUpdateApplyFailedMessageTitle',
          'MCP Server update apply failed',
        ),
        error: e,
        dedupeKey: `mcp:update-apply:error:${serverId}`,
        target: { tab: 'mcp-servers', entity_id: serverId },
      });
      pushToast({ title: t('mcpUpdateApplyFailed', { error: e }), kind: 'error' });
    } finally {
      setUpdateApplyPending(prev => {
        const next = new Set(prev);
        next.delete(serverId);
        return next;
      });
    }
  }

  async function handleSave(server: MCPServer) {
    try {
      const result = await runUserAction(
        actionContext,
        buildSaveMcpServerActionDescriptor(t, {
          id: server.id,
          transport: server.transport,
        }),
        () => invoke('save_mcp_server', { server }),
      );
      if (result === null) return;
      await loadServers();
      emit('refresh-counts').catch(() => {});
      setShowAddModal(false);
      setEditingServer(null);
    } catch (e) {
      console.error('Failed to save MCP server', e);
    }
  }

  async function handleDelete(id: string) {
    try {
      const result = await runUserAction(
        actionContext,
        buildDeleteMcpServerActionDescriptor(t, id),
        () => invoke('delete_mcp_server', { serverId: id }),
      );
      if (result === null) return;
      await loadServers();
      emit('refresh-counts').catch(() => {});
      setModelSwitchStates(prev => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
    } catch (e) {
      console.error('Failed to delete MCP server', e);
    }
  }

  async function handleCreateFromTemplate(template: MCPTemplate) {
    try {
      const server = await invoke('get_mcp_template', { templateId: template.id });
      setEditingServer(server as MCPServer);
      setShowAddModal(true);
      setShowTemplates(false);
    } catch (e) {
      pushToast({ title: t('loadTemplateFailed', { error: e }), kind: 'error' });
    }
  }

  async function handleLinkProviders(serverId: string, providerIds: string[]) {
    try {
      await invoke('link_mcp_to_providers', { serverId, providerIds });
      await loadServers();
      if (onLinkedProvidersChange) {
        onLinkedProvidersChange(serverId, providerIds);
      }
    } catch (e) {
      pushToast({ title: t('linkProvidersFailed', { error: e }), kind: 'error' });
    }
  }

  async function handleToggleModelSwitch(serverId: string, model: MCPModel, enabled: boolean) {
    const pendingKey = `${serverId}:${model}`;
    if (modelSyncPending.has(pendingKey)) {
      return;
    }
    const previousState = modelSwitchStates[serverId] || { ...DEFAULT_MCP_MODEL_SWITCH_STATE };
    const optimisticState = { ...previousState, [model]: enabled };
    setModelSwitchStates(prev => ({ ...prev, [serverId]: optimisticState }));
    setModelSyncPending(prev => new Set(prev).add(pendingKey));

    try {
      const latest = await invoke('set_mcp_model_switch', { serverId, model, enabled }) as MCPModelSwitchState;
      setModelSwitchStates(prev => ({ ...prev, [serverId]: latest }));
      await loadModelSwitchStates(servers, true);
      emit('refresh-counts').catch(() => {});
    } catch (e) {
      setModelSwitchStates(prev => ({ ...prev, [serverId]: previousState }));
      void notifyMessage.error({
        source: 'mcp_servers',
        category: 'model_sync',
        title: t(
          'mcpModelSwitchSyncFailedMessageTitle',
          'MCP model switch sync failed',
        ),
        summary: `${model}: ${e instanceof Error ? e.message : 'Model sync failed'}`,
        error: e,
        dedupeKey: `mcp:model-switch:error:${serverId}:${model}`,
        target: { tab: 'mcp-servers', entity_id: serverId },
        metadata: { server_id: serverId, model, enabled },
      });
      pushToast({ title: t('mcpModelSyncFailed', { error: e }), kind: 'error' });
    } finally {
      setModelSyncPending(prev => {
        const next = new Set(prev);
        next.delete(pendingKey);
        return next;
      });
    }
  }

  async function handleUninstallFromModelView(server: MCPServer) {
    const modelLabel = MCP_MODEL_OPTIONS.find((option) => option.id === activeModel)?.label || activeModel;
    const confirmed = await confirmDialog(
      t('confirmUninstallMcpForModel', {
        name: server.name,
        model: modelLabel,
      }),
      {
        okLabel: t('ok'),
        cancelLabel: t('cancel'),
      }
    );
    if (!confirmed) return;
    await handleToggleModelSwitch(server.id, activeModel, false);
  }

  function getServerSwitchState(serverId: string): MCPModelSwitchState {
    return modelSwitchStates[serverId] || DEFAULT_MCP_MODEL_SWITCH_STATE;
  }

  function countEnabledByModel(model: MCPModel): number {
    return servers.reduce((count, server) => {
      return getServerSwitchState(server.id)[model] ? count + 1 : count;
    }, 0);
  }

  function toggleExpand(serverId: string) {
    const newExpanded = new Set(expandedServers);
    if (newExpanded.has(serverId)) {
      newExpanded.delete(serverId);
    } else {
      newExpanded.add(serverId);
    }
    setExpandedServers(newExpanded);
  }

  const updatesByServerId = useMemo(() => {
    const map: Record<string, MCPUpdateInfo> = {};
    for (const item of updatesState?.items || []) {
      if (!item.server_id) continue;
      map[item.server_id] = item;
    }
    return map;
  }, [updatesState?.items]);

  function getUpdateStatusText(status: MCPUpdateStatus) {
    switch (status) {
      case 'updatable':
        return t('mcpUpdateStatusUpdatable', 'Update available');
      case 'up_to_date':
        return t('mcpUpdateStatusUpToDate', 'Up to date');
      case 'floating_latest':
        return t('mcpUpdateStatusFloating', 'latest');
      case 'check_failed':
        return t('mcpUpdateStatusCheckFailed', 'Check failed');
      default:
        return t('mcpUpdateStatusUnsupported', 'Unsupported');
    }
  }

  function getUpdateStatusClass(status: MCPUpdateStatus) {
    switch (status) {
      case 'updatable':
        return 'bg-amber-500/10 text-amber-700 border-amber-500/30';
      case 'up_to_date':
        return 'bg-emerald-500/10 text-emerald-700 border-emerald-500/30';
      case 'floating_latest':
        return 'bg-blue-500/10 text-blue-700 border-blue-500/30';
      case 'check_failed':
        return 'bg-red-500/10 text-red-700 border-red-500/30';
      default:
        return 'bg-muted/60 text-muted-foreground border-border';
    }
  }

  function getInstalledCardVersionText(server: MCPServer, updateInfo?: MCPUpdateInfo): string {
    if (updateInfo?.current_version) {
      return updateInfo.current_version;
    }
    if (updateInfo?.status === 'floating_latest' && updateInfo.latest_version) {
      return `${updateInfo.latest_version} (latest)`;
    }
    const parsed = parseNpxSpecFromServer(server);
    if (!parsed) {
      return '-';
    }
    if (parsed.version) {
      return parsed.version;
    }
    return t('mcpVersionFloating', 'latest');
  }

  const updatesChecking = updatesState?.status === 'checking';
  const modelViewServers = servers.filter((server) => getServerSwitchState(server.id)[activeModel]);

  return {
    servers,
    viewMode,
    setViewMode,
    activeModel,
    setActiveModel,
    modelSwitchStates,
    modelSyncPending,
    templates,
    loading,
    showAddModal,
    setShowAddModal,
    showTemplates,
    setShowTemplates,
    editingServer,
    setEditingServer,
    expandedServers,
    showBackupManager,
    setShowBackupManager,
    showImportExport,
    setShowImportExport,
    refreshingLocalInstall,
    updatesState,
    triggeringUpdateCheck,
    updateApplyPending,
    loadServers,
    handleRefreshLocalInstallState,
    handleCheckUpdatesInBackground,
    handleApplyUpdate,
    handleSave,
    handleDelete,
    handleCreateFromTemplate,
    handleLinkProviders,
    handleToggleModelSwitch,
    handleUninstallFromModelView,
    countEnabledByModel,
    toggleExpand,
    updatesByServerId,
    getUpdateStatusText,
    getUpdateStatusClass,
    getInstalledCardVersionText,
    updatesChecking,
    modelViewServers,
  };
}
