import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, Edit, Server, X, Key, Link as LinkIcon, ChevronRight, ChevronDown, Download, RefreshCw } from 'lucide-react';
import { BackupManager } from '../BackupManager';
import { MCPImportExport } from '../MCPImportExport';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { useToast } from '../ToastProvider';
import { safeRecordMessage } from '@/lib/messages';
import { DEFAULT_MCP_MODEL_SWITCH_STATE, MCP_MODEL_OPTIONS } from './constants';
import { MCPServerForm } from './MCPServerForm';
import { MCPTemplatesModal } from './MCPTemplatesModal';
import { useMcpServers } from './hooks/useMcpServers';
import type { MCPServersProps } from './types';

export function MCPServers({
  providers = [],
  onLinkedProvidersChange,
  isVisible = false,
  workspaceContext,
  onDismissWorkspaceContext,
}: MCPServersProps) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const { pushToast } = useToast();
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );
  const {
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
  } = useMcpServers({
    isVisible,
    t,
    actionContext,
    confirmDialog,
    pushToast,
    onLinkedProvidersChange,
  });

  return (
    <div className="p-6">
      {workspaceContext && (
        <div className="mb-4 flex items-start justify-between gap-3 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm">
          <div className="min-w-0">
            <p className="font-medium text-foreground">
              {t('workspaceMcpContextBannerTitle', 'Opened from workspace {{name}}', {
                name: workspaceContext.workspaceName,
              })}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t(
                'workspaceMcpContextBannerDesc',
                'Manage global MCP server definitions here. Changes can then be enabled for {{path}} from the workspace detail page.',
                { path: workspaceContext.rootPath },
              )}
            </p>
          </div>
          <button
            type="button"
            onClick={onDismissWorkspaceContext}
            className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-background/80 hover:text-foreground"
            title={t('clear', 'Clear')}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      )}
      <div className="flex justify-between items-center mb-6">
        <div>
          <h2 className="text-2xl font-bold">{t('mcpServersPageTitle', 'Global MCP Servers')}</h2>
          <p className="text-sm text-muted-foreground mt-1">
            {t(
              'mcpServersPageDesc',
              'Manage global MCP server definitions and model switches. These entries are not tied to a single workspace. Enable them from workspace pages when needed.'
            )}
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <button
            onClick={() => {
              void handleCheckUpdatesInBackground();
            }}
            disabled={triggeringUpdateCheck || updatesChecking}
            className="whitespace-nowrap px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 flex items-center gap-2 disabled:opacity-60"
          >
            <RefreshCw className={`w-4 h-4 ${(triggeringUpdateCheck || updatesChecking) ? 'animate-spin' : ''}`} />
            {updatesChecking ? t('mcpCheckingUpdates', '检查中') : t('mcpCheckUpdates', '检查更新')}
          </button>
          <button
            onClick={() => {
              void handleRefreshLocalInstallState();
            }}
            disabled={refreshingLocalInstall}
            className="whitespace-nowrap px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 flex items-center gap-2 disabled:opacity-60"
          >
            <RefreshCw className={`w-4 h-4 ${refreshingLocalInstall ? 'animate-spin' : ''}`} />
            {t('mcpLocalSync', '本地同步')}
          </button>
          <button
            onClick={() => setShowImportExport(true)}
            className="whitespace-nowrap px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 flex items-center gap-2"
          >
            <Download className="w-4 h-4" />
            {t('importExport')}
          </button>
          <button
            onClick={() => setShowTemplates(true)}
            className="whitespace-nowrap px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80"
          >
            {t('useTemplate')}
          </button>
          <button
            onClick={() => {
              setEditingServer(null);
              setShowAddModal(true);
            }}
            className="whitespace-nowrap px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm flex items-center gap-2 hover:bg-primary/90"
          >
            <Plus className="w-4 h-4" /> {t('addServer')}
          </button>
        </div>
      </div>

      {!loading && servers.length > 0 && (
        <div className="mb-4 space-y-3">
          <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
            <button
              onClick={() => setViewMode('server')}
              className={`px-3 py-1.5 rounded-md text-sm ${
                viewMode === 'server'
                  ? 'bg-black text-white'
                  : 'bg-white text-black'
              }`}
            >
              {t('mcpViewByServer', '仓库')}
            </button>
            <button
              onClick={() => setViewMode('model')}
              className={`px-3 py-1.5 rounded-md text-sm ${
                viewMode === 'model'
                  ? 'bg-black text-white'
                  : 'bg-white text-black'
              }`}
            >
              {t('mcpViewByModel', '已安装')}
            </button>
          </div>

          {viewMode === 'model' && (
            <>
              <div className="border rounded-xl bg-card p-3">
                <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                  {MCP_MODEL_OPTIONS.map((m) => {
                    const ModelIcon = m.Icon;
                    return (
                      <button
                        key={`mcp-model-tab-${m.id}`}
                        type="button"
                        onClick={() => setActiveModel(m.id)}
                        className={`rounded-lg border px-4 py-3 text-left transition-all ${
                          activeModel === m.id
                            ? 'border-primary bg-primary/5'
                            : 'hover:bg-muted/40 hover:-translate-y-0.5'
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <ModelIcon className="w-5 h-5" />
                          <span className="text-sm font-semibold">{m.label}</span>
                        </div>
                        <div className="mt-2.5">
                          <span className="text-sm leading-none text-muted-foreground">
                            {t('mcpInstalledCountForModel', '已启用 {{count}} 个 MCP', {
                              count: countEnabledByModel(m.id),
                            })}
                          </span>
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>
              <p className="text-xs text-muted-foreground">
                {t('mcpModelReenableHint', '若需重新启用，请切回 MCP 视角后展开对应服务器并开启模型开关。')}
              </p>
            </>
          )}
          <p className="text-xs text-muted-foreground">
            {updatesState?.last_checked_at
              ? t('mcpLastCheckedAt', 'Last checked at {{time}}', {
                  time: new Date(updatesState.last_checked_at * 1000).toLocaleString(),
                })
              : t('mcpNoUpdateCheckYet', 'No update check has run yet.')}
            {updatesState?.last_error
              ? ` · ${t('mcpUpdateCheckError', 'Error: {{error}}', { error: updatesState.last_error })}`
              : ''}
          </p>
        </div>
      )}

      {loading ? (
        <div className="text-center py-12 text-muted-foreground">{t('loading')}</div>
      ) : servers.length === 0 ? (
        <div className="text-center py-12">
          <Server className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">{t('mcpServerListEmpty')}</h3>
          <p className="text-muted-foreground mb-4">
            {t('mcpServerListEmptyDesc')}
          </p>
          <button
            onClick={() => setShowTemplates(true)}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
          >
            {t('browseTemplates')}
          </button>
        </div>
      ) : viewMode === 'server' ? (
        <div className="grid gap-4">
          {servers.map(server => {
            const updateInfo = updatesByServerId[server.id];
            const applyPending = updateApplyPending.has(server.id);
            return (
            <div key={server.id} className="border rounded-lg bg-card overflow-hidden">
              <div 
                className="p-4 cursor-pointer hover:bg-accent/50 flex items-center justify-between"
                onClick={() => toggleExpand(server.id)}
              >
                <div className="flex items-center gap-3">
                  {expandedServers.has(server.id) ? (
                    <ChevronDown className="w-5 h-5 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="w-5 h-5 text-muted-foreground" />
                  )}
                  <div className="flex items-center gap-2">
                    <div className={`w-2 h-2 rounded-full ${
                      server.transport === 'stdio' ? 'bg-green-500' :
                      server.transport === 'http' ? 'bg-blue-500' : 'bg-purple-500'
                    }`} />
                    <h3 className="font-semibold text-lg">{server.name}</h3>
                  </div>
                  <span className="text-xs px-2 py-1 bg-secondary rounded uppercase">
                    {server.transport}
                  </span>
                  {updateInfo && (
                    <span
                      className={`text-[10px] px-2 py-1 rounded border ${getUpdateStatusClass(updateInfo.status)}`}
                    >
                      {getUpdateStatusText(updateInfo.status)}
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-4">
                  {server.linked_provider_ids.length > 0 && (
                    <div className="text-sm text-muted-foreground flex items-center gap-1">
                      <LinkIcon className="w-3 h-3" />
                      {t('environmentCount', { count: server.linked_provider_ids.length })}
                    </div>
                  )}
                  <div className="flex gap-2">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setEditingServer(server);
                        setShowAddModal(true);
                      }}
                      className="p-2 hover:bg-secondary rounded"
                      title={t('edit')}
                    >
                      <Edit className="w-4 h-4" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(server.id);
                      }}
                      className="p-2 hover:bg-destructive/10 text-destructive rounded"
                      title={t('delete')}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </div>
              
              {expandedServers.has(server.id) && (
                <div className="px-4 pb-4 border-t bg-muted/30">
                  <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_280px]">
                    <div>
                      {server.description && (
                        <p className="text-sm text-muted-foreground">{server.description}</p>
                      )}

                      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
                        {server.command && (
                          <div>
                            <label className="text-xs font-medium text-muted-foreground">{t('command')}</label>
                            <code className="block text-sm bg-background rounded p-2 mt-1 font-mono">
                              {server.command} {server.args?.join(' ')}
                            </code>
                          </div>
                        )}
                        {server.url || server.http_url ? (
                          <div>
                            <label className="text-xs font-medium text-muted-foreground">{server.transport === 'http' ? t('httpUrl') : t('sseUrl')}</label>
                            <code className="block text-sm bg-background rounded p-2 mt-1 font-mono">
                              {server.http_url || server.url}
                            </code>
                          </div>
                        ) : null}
                        {server.timeout && (
                          <div>
                            <label className="text-xs font-medium text-muted-foreground">{t('timeout')}</label>
                            <div className="text-sm mt-1">{server.timeout}ms</div>
                          </div>
                        )}
                        {server.trust && (
                          <div>
                            <label className="text-xs font-medium text-muted-foreground">{t('trust')}</label>
                            <div className="text-sm mt-1 text-green-600">{t('trustAutoApprove')}</div>
                          </div>
                        )}
                      </div>

                      {server.env && Object.keys(server.env).length > 0 && (
                        <div className="mt-4">
                          <label className="text-xs font-medium text-muted-foreground flex items-center gap-1">
                            <Key className="w-3 h-3" /> {t('envVars')}
                          </label>
                          <div className="grid grid-cols-1 md:grid-cols-2 gap-2 mt-2">
                            {Object.entries(server.env).map(([key, value]) => (
                              <div key={key} className="text-sm bg-background rounded p-2 font-mono">
                                <span className="text-primary">{key}</span>=
                                <span className="text-muted-foreground">{value}</span>
                              </div>
                            ))}
                          </div>
                        </div>
                      )}

                      {updateInfo?.status === 'updatable' && (
                        <div className="mt-4 border rounded-md bg-background p-3">
                          <div className="flex items-center justify-between gap-3">
                            <div className="text-xs font-medium text-muted-foreground">
                              {t('mcpPackageUpdate', 'Package update')}
                            </div>
                            <span className={`text-[10px] px-2 py-0.5 rounded border ${getUpdateStatusClass(updateInfo.status)}`}>
                              {getUpdateStatusText(updateInfo.status)}
                            </span>
                          </div>
                          <div className="mt-2 text-xs text-muted-foreground">
                            {updateInfo.package_name || '-'}
                            {updateInfo.current_version ? ` @ ${updateInfo.current_version}` : ''}
                            {updateInfo.latest_version ? ` -> ${updateInfo.latest_version}` : ''}
                          </div>
                          {updateInfo.message && (
                            <p className="mt-1 text-xs text-muted-foreground">{updateInfo.message}</p>
                          )}
                          <div className="mt-3">
                            <button
                              type="button"
                              disabled={applyPending}
                              className="text-xs px-2.5 py-1 rounded-md border bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-60"
                              onClick={() => {
                                void handleApplyUpdate(server.id);
                              }}
                            >
                              {applyPending
                                ? t('mcpUpdateApplying', '升级中')
                                : t('mcpUpdateToLatest', '升级到最新')}
                            </button>
                          </div>
                        </div>
                      )}

                      <div className="mt-4">
                        <label className="text-xs font-medium text-muted-foreground flex items-center gap-1 mb-2">
                          <LinkIcon className="w-3 h-3" /> {t('linkToEnvironments')}
                        </label>
                        <div className="flex flex-wrap gap-2">
                          {providers?.map(provider => (
                            <button
                              key={provider.id}
                              onClick={() => {
                                const isLinked = server.linked_provider_ids.includes(provider.id);
                                const newIds = isLinked
                                  ? server.linked_provider_ids.filter(id => id !== provider.id)
                                  : [...server.linked_provider_ids, provider.id];
                                handleLinkProviders(server.id, newIds);
                              }}
                              className={`px-3 py-1.5 rounded text-xs border ${
                                server.linked_provider_ids.includes(provider.id)
                                  ? 'bg-primary text-primary-foreground border-primary'
                                  : 'bg-background hover:bg-accent'
                              }`}
                            >
                              {provider.name}
                            </button>
                          ))}
                        </div>
                      </div>
                    </div>

                    <div className="border rounded-md bg-background p-3 h-fit">
                      <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                        {t('mcpModelSwitches')}
                      </h4>
                      <p className="text-xs text-muted-foreground mt-1">
                        {t('mcpSyncToLocalConfig')}
                      </p>
                      <div className="mt-3 space-y-2">
                        {MCP_MODEL_OPTIONS.map(({ id, label, Icon }) => {
                          const switchState = modelSwitchStates[server.id] || DEFAULT_MCP_MODEL_SWITCH_STATE;
                          const checked = switchState[id];
                          const pendingKey = `${server.id}:${id}`;
                          const syncing = modelSyncPending.has(pendingKey);
                          return (
                            <label
                              key={`${server.id}-${id}`}
                              className={`flex items-center justify-between rounded-md border px-2.5 py-2 ${
                                syncing ? 'opacity-70' : ''
                              }`}
                            >
                              <span className="flex items-center gap-2 text-sm">
                                <Icon className="w-4 h-4 text-muted-foreground" />
                                {label}
                              </span>
                              <span className="inline-flex items-center gap-2">
                                {syncing && (
                                  <span className="text-[11px] text-muted-foreground">{t('mcpModelSyncing')}</span>
                                )}
                                <span className="relative inline-flex items-center">
                                  <input
                                    type="checkbox"
                                    className="peer sr-only"
                                    checked={checked}
                                    disabled={syncing}
                                    onChange={(e) => handleToggleModelSwitch(server.id, id, e.target.checked)}
                                  />
                                  <span className="w-10 h-5 bg-gray-200 rounded-full relative transition-colors peer-checked:bg-primary dark:bg-gray-700 peer-focus:ring-2 peer-focus:ring-primary/20 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:w-4 after:h-4 after:bg-white after:border after:rounded-full after:transition-all peer-checked:after:translate-x-5"></span>
                                </span>
                              </span>
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </div>
          );
          })}
        </div>
      ) : (
        <>
          {modelViewServers.length === 0 ? (
            <div className="text-center py-12">
              <Server className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">
                {t('mcpNoServersForModelTitle', '该模型下暂无已启用 MCP')}
              </h3>
              <p className="text-muted-foreground mb-4">
                {t('mcpNoServersForModelDesc', '请先切换到“仓库”并启用模型开关。')}
              </p>
              <button
                onClick={() => setViewMode('server')}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
              >
                {t('mcpViewByServer', '仓库')}
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {modelViewServers.map((server) => {
                const pendingKey = `${server.id}:${activeModel}`;
                const syncing = modelSyncPending.has(pendingKey);
                const updateInfo = updatesByServerId[server.id];
                const versionText = getInstalledCardVersionText(server, updateInfo);
                return (
                  <div
                    key={`model-view-${activeModel}-${server.id}`}
                    className="group border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="p-2 rounded-md bg-primary/10 text-primary">
                        <Server className="w-4 h-4" />
                      </div>
                      <span className="text-[10px] text-muted-foreground uppercase">{server.transport}</span>
                    </div>

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{server.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">
                      {server.description || t('mcpSyncToLocalConfig')}
                    </p>

                    <div className="mt-3 text-[11px] text-muted-foreground font-mono line-clamp-1">
                      {server.command
                        ? `${server.command} ${(server.args || []).join(' ')}`
                        : (server.http_url || server.url || '-')}
                    </div>
                    <div className="mt-1 text-[11px] text-muted-foreground">
                      {t('mcpCurrentVersion', 'Current version')}: <span className="font-mono">{versionText}</span>
                    </div>

                    <div className="mt-3 flex items-center justify-end">
                      <button
                        disabled={syncing}
                        className="text-xs px-2.5 py-1 rounded-md border hover:bg-destructive/10 text-destructive inline-flex items-center gap-1 disabled:opacity-50"
                        onClick={() => {
                          void handleUninstallFromModelView(server);
                        }}
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                        {syncing ? t('mcpModelSyncing') : t('uninstall', '卸载')}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}

      {showAddModal && (
        <MCPServerForm
          server={editingServer}
          onClose={() => {
            setShowAddModal(false);
            setEditingServer(null);
          }}
          onSave={handleSave}
        />
      )}

      {showTemplates && (
        <MCPTemplatesModal
          templates={templates}
          onClose={() => setShowTemplates(false)}
          onSelect={handleCreateFromTemplate}
        />
      )}
      
      {showBackupManager && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-background rounded-lg w-full max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
            <div className="h-14 border-b px-4 flex items-center justify-end">
              <button
                onClick={() => setShowBackupManager(false)}
                className="p-2 hover:bg-secondary rounded"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto">
              <BackupManager />
            </div>
          </div>
        </div>
      )}
      
      {showImportExport && (
        <MCPImportExport
          servers={servers.map(s => ({ id: s.id, name: s.name }))}
          onClose={() => setShowImportExport(false)}
          onImported={() => {
            void loadServers();
          }}
        />
      )}
    </div>
  );
}
