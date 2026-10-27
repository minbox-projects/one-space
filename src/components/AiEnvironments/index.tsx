import { useState, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Pencil, Plus, Settings2, Trash2, Upload, X } from 'lucide-react';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { CliVersionCards } from './CliVersionCards';
import { ToolSectionHeader } from './ToolSectionHeader';
import { SyncedDevices } from './SyncedDevices';
import { ServiceProviderDetail } from './ServiceProviderDetail';
import { ServiceProviderList } from './ServiceProviderList';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';
import { IconPicker } from './IconPicker';
import { ModelMappingTable } from './ModelMappingTable';
import { TerminalPermissionConfirmDialog } from '../TerminalPermissionConfirmDialog';
import { useToast } from '../ToastProvider';
import { safeRecordMessage } from '@/lib/messages';
import {
  claudeProfileMaterialize,
  getActiveProviderIds,
  projectionApply,
  serviceProviderFetchModels,
  serviceProvidersSetActive,
  type AiProvider,
  type AiProvidersState,
  type ClaudeProfileSummary,
  type CliTool,
  type SyncedDeviceProvidersView,
} from '@/lib/serviceProviders';
import {
  parseOpenCodeModelConfig,
  stringifyOpenCodeModelConfig,
  type JsonObject,
} from './opencodeModelConfig';
import {
  DEFAULT_STATE,
  errorToDisplayMessage,
  getManagedStateForTool as getManagedStateForToolFromState,
  isManagedTool,
} from './helpers/providerUtils';
import { buildPresetClaudeMappings } from './helpers/claudeProvider';
import { useOpenCodeEditorState } from './hooks/useOpenCodeEditorState';
import { useOpenCodeEditor } from './hooks/useOpenCodeEditor';
import { useCliState } from './hooks/useCliState';
import { useProviderImportExportState } from './hooks/useProviderImportExportState';
import { useProviderPresets } from './hooks/useProviderPresets';
import { useAiEnvironmentData } from './hooks/useAiEnvironmentData';
import { useCliActions } from './hooks/useCliActions';
import { useProviderActions } from './hooks/useProviderActions';
import { useClaudeProfileActions } from './hooks/useClaudeProfileActions';
import { useProviderImportExportActions } from './hooks/useProviderImportExportActions';
import { useProviderListItems } from './hooks/useProviderListItems';

export type { AiProvider, AiProvidersState, ClaudeProfileSummary, HistoryEntry } from '@/lib/serviceProviders';
export { buildSyncedProviderActivationPayload, getMissingRequiredProviderFields } from './helpers/providerUtils';

import { ToolIcon } from "./ToolIcon";

export { ToolIcon };

export function AiEnvironments({ isVisible = false }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const confirmDialog = useConfirmDialog();
  const [state, setState] = useState<AiProvidersState>(DEFAULT_STATE);
  const [activeTool, setActiveTool] = useState('claude');
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  const {
    rawJson,
    setRawJson,
    originalJson,
    setOriginalJson,
    jsonError,
    setJsonError,
    openCodeModelForm,
    setOpenCodeModelForm,
    openCodeModelErrors,
    setOpenCodeModelErrors,
    lastValidOpenCodeSnapshot,
    setLastValidOpenCodeSnapshot,
  } = useOpenCodeEditorState();

  const [applyingGlobal, setApplyingGlobal] = useState(false);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  const [savingDetail, setSavingDetail] = useState(false);
  const [isRollbackMode, setIsRollbackMode] = useState(false);
  const {
    cliVersions,
    setCliVersions,
    checkingVersions,
    setCheckingVersions,
    checkingAllVersions,
    setCheckingAllVersions,
    cliUpdates,
    setCliUpdates,
    checkingUpdates,
    setCheckingUpdates,
    updatingTool,
    setUpdatingTool,
    probingTool,
    setProbingTool,
    setAutoImportInactiveNotice,
  } = useCliState();
  const [unsavedNewProviderIds, setUnsavedNewProviderIds] = useState<Set<string>>(new Set());
  const [favoritePendingIds, setFavoritePendingIds] = useState<Set<string>>(new Set());
  const [syncedOtherDeviceProviders, setSyncedOtherDeviceProviders] = useState<SyncedDeviceProvidersView[]>([]);
  const [activatingSyncedKey, setActivatingSyncedKey] = useState<string | null>(null);
  const [claudeProfiles, setClaudeProfiles] = useState<ClaudeProfileSummary[]>([]);
  const [copiedClaudeProfileId, setCopiedClaudeProfileId] = useState<string | null>(null);
  const [claudeLaunchCommand, setClaudeLaunchCommand] = useState('claude --session-id {session_id}');
  const [launchingClaudeProfileId, setLaunchingClaudeProfileId] = useState<string | null>(null);
  const [permissionDialogClaudeProfileId, setPermissionDialogClaudeProfileId] = useState<string | null>(null);
  const [permissionDialogOpen, setPermissionDialogOpen] = useState(false);
  const [applyingClaudeProfileId, setApplyingClaudeProfileId] = useState<string | null>(null);
  const {
    exportingProviders,
    setExportingProviders,
    previewingImport,
    setPreviewingImport,
    applyingImport,
    setApplyingImport,
    importPreview,
    setImportPreview,
    importPath,
    setImportPath,
    importDecisions,
    setImportDecisions,
  } = useProviderImportExportState();
  const {
    providerPresets,
    setProviderPresets,
    presetPickerOpen,
    setPresetPickerOpen,
    presetManagerOpen,
    setPresetManagerOpen,
    editingPresetId,
    presetDraft,
    setPresetDraft,
    openPresetEditor,
    savePresetDraft,
    deleteProviderPreset,
  } = useProviderPresets({ t, setMessage });

  // Accordion state
  const [searchQuery, setSearchQuery] = useState('');

  // Service provider list/detail view mode
  const [viewMode, setViewMode] = useState<'list' | 'detail'>('list');
  const [detailProvider, setDetailProvider] = useState<any | null>(null);
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );

  const versionCheckRunIdRef = useRef(0);
  const probeRunIdRef = useRef(0);
  const isVisibleRef = useRef(isVisible);
  const cliProbeInitializedRef = useRef(false);
  const autoImportInitializedRef = useRef(false);
  const listScrollContainerRef = useRef<HTMLDivElement | null>(null);
  const savedListScrollTopRef = useRef(0);
  const pendingRestoreListScrollTopRef = useRef<number | null>(null);
  const rollbackDraftBeforeRef = useRef<{ provider: AiProvider | null; rawJson: string } | null>(null);
  const openCodeDetailRequestIdRef = useRef(0);
  const openCodeDetailTargetRef = useRef<{ id: string; providerKey: string } | null>(null);

  const isTauri = '__TAURI_INTERNALS__' in window;

  const data = useAiEnvironmentData({
    t,
    isTauri,
    isVisible,
    activeTool,
    currentProviderId,
    state,
    setState,
    setCurrentProviderId,
    setMessage,
    setLoading,
    setProviderPresets,
    setSyncedOtherDeviceProviders,
    setUnsavedNewProviderIds,
    setClaudeProfiles,
    setClaudeLaunchCommand,
    setAutoImportInactiveNotice,
    setCliVersions,
    setCheckingVersions,
    setCheckingAllVersions,
    setCliUpdates,
    setCheckingUpdates,
    setProbingTool,
    versionCheckRunIdRef,
    probeRunIdRef,
    isVisibleRef,
    cliProbeInitializedRef,
    autoImportInitializedRef,
  });

  const cliActions = useCliActions({
    t,
    actionContext,
    cliUpdates,
    setUpdatingTool,
    setMessage,
    pushToast,
  });

  const editor = useOpenCodeEditor({
    t,
    setRawJson,
    setOriginalJson,
    setJsonError,
    setOpenCodeModelForm,
    setOpenCodeModelErrors,
    lastValidOpenCodeSnapshot,
    setLastValidOpenCodeSnapshot,
    isRollbackMode,
    setIsRollbackMode,
    setDetailProvider,
    openCodeDetailRequestIdRef,
  });

  const actions = useProviderActions({
    t,
    actionContext,
    pushToast,
    confirmDialog,
    isTauri,
    activeTool,
    setActiveTool,
    state,
    setState,
    currentProviderId,
    setCurrentProviderId,
    detailProvider,
    setDetailProvider,
    rawJson,
    setRawJson,
    setOriginalJson,
    setJsonError,
    setMessage,
    setLoading,
    savingDetail,
    setSavingDetail,
    isRollbackMode,
    setIsRollbackMode,
    unsavedNewProviderIds,
    setUnsavedNewProviderIds,
    setFavoritePendingIds,
    setActivatingSyncedKey,
    claudeProfiles,
    setViewMode,
    setPresetPickerOpen,
    loadProviders: data.loadProviders,
    loadClaudeProfiles: data.loadClaudeProfiles,
    editor,
    rollbackDraftBeforeRef,
    openCodeDetailRequestIdRef,
    openCodeDetailTargetRef,
    pendingRestoreListScrollTopRef,
    savedListScrollTopRef,
  });

  const claudeActions = useClaudeProfileActions({
    t,
    actionContext,
    isTauri,
    claudeLaunchCommand,
    permissionDialogClaudeProfileId,
    setCopiedClaudeProfileId,
    setMessage,
    setApplyingGlobal,
    setApplyingClaudeProfileId,
    setLaunchingClaudeProfileId,
    setPermissionDialogClaudeProfileId,
    setPermissionDialogOpen,
    setCurrentProviderId,
    loadProviders: data.loadProviders,
  });

  const importExport = useProviderImportExportActions({
    t,
    actionContext,
    isTauri,
    pushToast,
    setMessage,
    setExportingProviders,
    setPreviewingImport,
    setApplyingImport,
    importPreview,
    setImportPreview,
    importPath,
    setImportPath,
    importDecisions,
    setImportDecisions,
    loadProviders: data.loadProviders,
  });

  const { currentToolListItems, providerCountsByTool } = useProviderListItems({
    t,
    activeTool,
    state,
    claudeProfiles,
    unsavedNewProviderIds,
    favoritePendingIds,
    launchingClaudeProfileId,
    applyingClaudeProfileId,
    copiedClaudeProfileId,
    loading,
    applyingGlobal,
    viewMode,
    syncedOtherDeviceProviders,
    listScrollContainerRef,
    pendingRestoreListScrollTopRef,
  });

  const getManagedStateForTool = (tool: CliTool) => getManagedStateForToolFromState(state, tool);

  const renderServiceProviderDetail = () => {
    if (viewMode !== 'detail' || !detailProvider) return null;
    const isDetailActive =
      getActiveProviderIds(state, detailProvider.tool).includes(detailProvider.id);

    const isManagedImportedDetail =
      !!detailProvider &&
      isManagedTool(detailProvider.tool) &&
      detailProvider.code === `default-${detailProvider.tool}` &&
      !isDetailActive;
    const detailMissingFieldLabels = isManagedImportedDetail
      ? [
          ...(detailProvider.api_key?.trim() ? [] : [t('apiKey', 'API Key')]),
          ...(detailProvider.base_url?.trim() ? [] : [t('baseUrl', 'Base URL')]),
        ]
      : [];
    const importedInactiveNotice = detailMissingFieldLabels.length > 0
      ? t('autoImportedButInactiveMissingFields', {
          fields: detailMissingFieldLabels.join(' + '),
        })
      : null;

    return (
      <div className="flex-1 min-h-0 overflow-hidden border rounded-xl bg-background">
        <ServiceProviderDetail
          provider={detailProvider}
          onChange={(changes) => {
            if (detailProvider.tool === 'opencode') {
              openCodeDetailRequestIdRef.current += 1;
              if (Object.hasOwn(changes, 'provider_key')) {
                openCodeDetailTargetRef.current = null;
              }
            }
            setDetailProvider((prev: any) => {
              if (!prev) return prev;
              const next = { ...prev, ...changes };
              if (next.tool === 'opencode' && lastValidOpenCodeSnapshot && !jsonError) {
                const snapshot = JSON.parse(JSON.stringify(lastValidOpenCodeSnapshot)) as JsonObject;
                if (Object.hasOwn(changes, 'name')) snapshot.name = String(next.name || '');
                if (Object.hasOwn(changes, 'api_key') || Object.hasOwn(changes, 'base_url')) {
                  const existingOptions = snapshot.options
                    && typeof snapshot.options === 'object'
                    && !Array.isArray(snapshot.options)
                    ? snapshot.options as JsonObject
                    : {};
                  snapshot.options = {
                    ...existingOptions,
                    apiKey: String(next.api_key || ''),
                    baseURL: String(next.base_url || ''),
                  };
                }
                setLastValidOpenCodeSnapshot(snapshot);
                setRawJson(stringifyOpenCodeModelConfig(snapshot));
              }
              return next;
            });
          }}
          onSave={async () => {
            if (!isTauri || !detailProvider || savingDetail) return;
            try {
              await actions.handleSaveDetailAndReturnToList(detailProvider);
            } catch (e: any) {
              const errorMessage = errorToDisplayMessage(e);
              setMessage({ type: 'error', text: errorMessage || t('saveFailed', 'Save failed') });
              pushToast({ title: t('saveFailed', 'Save failed'), description: errorMessage, kind: 'error' });
            }
          }}
          onActivate={async () => {
            if (!isTauri || !detailProvider || savingDetail) return;
            try {
              await serviceProvidersSetActive(detailProvider.tool, detailProvider.id);
              if (detailProvider.tool === 'claude') {
                await claudeProfileMaterialize(detailProvider.id);
              }
              await projectionApply(detailProvider.tool, detailProvider.id);
              setMessage({ type: 'success', text: t('activated', 'Activated') });
              pushToast({ title: t('activated', 'Activated'), kind: 'success' });
              await data.loadProviders(true);
            } catch (e: any) {
              const errorMessage = errorToDisplayMessage(e);
              setMessage({ type: 'error', text: errorMessage || t('activationFailed', 'Activation failed') });
              pushToast({ title: t('activationFailed', 'Activation failed'), description: errorMessage, kind: 'error' });
            }
          }}
          onDelete={async () => {
            if (!isTauri || !detailProvider || savingDetail) return;
            try {
              await actions.handleDelete(detailProvider.id, detailProvider.tool);
            } catch (e: any) {
              setMessage({ type: 'error', text: e?.message || t('deleteFailed', 'Delete failed') });
              pushToast({ title: t('deleteFailed', 'Delete failed'), description: String(e?.message || e), kind: 'error' });
            }
          }}
          onBack={() => { actions.returnToProviderList({ preserveScroll: true }); }}
          isActive={isDetailActive}
          t={(key: string, fallback: string, options?: Record<string, any>) => String(t(key, fallback, options))}
          onFetchModels={async (provider: Partial<AiProvider>) => {
            if (!isTauri) return [];
            return serviceProviderFetchModels(provider);
          }}
          jsonMode={
            detailProvider.tool === 'claude'
              ? 'claude'
              : detailProvider.tool === 'opencode'
                ? 'opencode'
                : 'generic'
          }
          jsonValue={
            detailProvider.tool === 'opencode'
              ? rawJson
              : JSON.stringify(detailProvider, null, 2)
          }
          jsonHistory={detailProvider.history || []}
          jsonError={jsonError}
          openCodeModelForm={openCodeModelForm}
          openCodeModelErrors={openCodeModelErrors}
          openCodeModelFrozen={!!jsonError}
          onOpenCodeModelFormChange={editor.handleOpenCodeModelFormChange}
          isRollbackMode={isRollbackMode}
          onJsonChange={(value) => {
            if (detailProvider.tool === 'opencode') {
              editor.handleOpenCodeJsonChange(value);
              return;
            }

            try {
              const parsed = JSON.parse(value);
              setDetailProvider((prev: any) => prev ? { ...prev, ...parsed } : prev);
              setJsonError(null);
            } catch (e: any) {
              setJsonError(e?.message || t('invalidJson', 'Invalid JSON syntax'));
            }
          }}
          onJsonError={setJsonError}
          onRollback={actions.handleRollback}
          onFormatJson={() => {
            const parsed = parseOpenCodeModelConfig(rawJson);
            if (!parsed.ok) {
              setJsonError(editor.openCodeErrorMessage(parsed.errors));
              setMessage({ type: 'error', text: editor.openCodeErrorMessage(parsed.errors) });
              return;
            }
            const formatted = stringifyOpenCodeModelConfig(parsed.snapshot);
            setRawJson(formatted);
            setLastValidOpenCodeSnapshot(parsed.snapshot);
            setOpenCodeModelForm(parsed.form);
            setOpenCodeModelErrors([]);
            setJsonError(null);
          }}
          onCancelRollback={() => {
            const rollbackDraft = rollbackDraftBeforeRef.current;
            if (rollbackDraft?.provider) {
              setDetailProvider(rollbackDraft.provider);
              if (rollbackDraft.provider.tool === 'opencode') {
                editor.initializeOpenCodeEditor(rollbackDraft.rawJson);
              } else {
                setRawJson(rollbackDraft.rawJson);
                setJsonError(null);
              }
            } else if (detailProvider.tool === 'opencode') {
              editor.initializeOpenCodeEditor(originalJson);
            } else {
              setRawJson(originalJson);
              setJsonError(null);
            }
            rollbackDraftBeforeRef.current = null;
            setIsRollbackMode(false);
          }}
          importedInactiveNotice={importedInactiveNotice}
          saving={savingDetail}
          message={message}
        />
      </div>
    );
  };

  if (viewMode === 'detail' && detailProvider) {
    return (
      <div className="flex flex-col h-full space-y-6">
        {renderServiceProviderDetail()}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('aiEnvironments')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('aiEnvironmentsDesc')}</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => { setPresetPickerOpen(true); }}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
            {t('addProvider', 'Add Service Provider')}
          </button>
          <button
            type="button"
            onClick={() => { openPresetEditor(); }}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md border bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={t('manageProviderPresets', 'Manage provider presets')}
            aria-label={t('manageProviderPresets', 'Manage provider presets')}
          >
            <Settings2 className="h-4 w-4" />
          </button>
        </div>
      </div>

      <CliVersionCards
        cliVersions={cliVersions}
        activeTool={activeTool}
        checkingVersions={checkingVersions}
        cliUpdates={cliUpdates}
        checkingAllVersions={checkingAllVersions}
        probingTool={probingTool}
        checkingUpdates={checkingUpdates}
        updatingTool={updatingTool}
        stateProviders={state.providers}
        providerCounts={providerCountsByTool}
        unsavedNewProviderIds={unsavedNewProviderIds}
        setActiveTool={setActiveTool}
        setCurrentProviderId={setCurrentProviderId}
        detectAllVersions={data.detectAllVersions}
        preloadCliMetaAndAutoImport={data.preloadCliMetaAndAutoImport}
        handleApplyCliUpdate={cliActions.handleApplyCliUpdate}
        getManagedStateForTool={getManagedStateForTool}
        t={t}
        versionCheckRunIdRef={versionCheckRunIdRef}
        probeRunIdRef={probeRunIdRef}
        cliProbeInitializedRef={cliProbeInitializedRef}
      />

      {/* Main accordion container */}
      <div className="flex-1 flex flex-col min-h-0 border rounded-xl overflow-hidden bg-background">
        {/* Tool section header */}
        <div className="shrink-0 border-b bg-card px-4 py-3">
          <ToolSectionHeader
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onImport={() => { void importExport.handleImportProviders(); }}
            onExport={() => { void importExport.handleExportProviders(); }}
            loading={loading}
            previewingImport={previewingImport}
            applyingImport={applyingImport}
            exportingProviders={exportingProviders}
            t={t}
          />
        </div>

        <div
          ref={listScrollContainerRef}
          className="flex-1 overflow-y-auto p-4"
          onScroll={(event) => {
            savedListScrollTopRef.current = event.currentTarget.scrollTop;
          }}
        >
          <ServiceProviderList
            providers={currentToolListItems}
            onProviderClick={actions.openServiceProviderDetail}
            onEdit={actions.openServiceProviderDetail}
            onDuplicate={actions.handleDuplicateProvider}
            onToggleFavorite={(id, favorite) => {
              void actions.handleToggleFavorite(id, favorite);
            }}
            onApplyGlobal={(id) => {
              if (activeTool === 'claude') {
                void claudeActions.handleClaudeApplyGlobal(id);
                return;
              }
              void actions.activateProvider(activeTool, id);
            }}
            onDelete={(id) => { void actions.handleDelete(id); }}
            onLaunch={(id) => {
              if (activeTool === 'claude') {
                void claudeActions.handleClaudeLaunch(id);
              }
            }}
            onCopyLaunchCommand={(id) => {
              const profile = claudeProfiles.find((item) => item.id === id);
              if (profile) {
                void claudeActions.handleClaudeCopyCommand(profile.id, profile.config_dir);
              }
            }}
            onOpenDirectory={(id) => {
              if (activeTool === 'claude') {
                void claudeActions.handleClaudeOpenDir(id);
              }
            }}
            onAdd={() => { setPresetPickerOpen(true); }}
            tool={activeTool}
            t={(key: string, fallback: string, options?: Record<string, any>) =>
              String(t(key, fallback, options))}
            searchTerm={searchQuery}
            loading={loading}
          />

          <SyncedDevices
            syncedOtherDeviceProviders={syncedOtherDeviceProviders}
            activeTool={activeTool}
            onActivate={actions.handleActivateSyncedProvider}
            loading={loading}
            activatingSyncedKey={activatingSyncedKey}
            t={t}
          />
        </div>
      </div>

      {presetPickerOpen && (
        <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
          <div className="w-full max-w-3xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
            <div className="p-5 border-b flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold">{t('selectProviderPreset', 'Select provider preset')}</h3>
                <p className="text-sm text-muted-foreground mt-1">
                  {t('selectProviderPresetDesc', 'Create a new service provider for the current tool from a reusable endpoint preset.')}
                </p>
              </div>
              <button
                type="button"
                onClick={() => { setPresetPickerOpen(false); }}
                className="rounded-md p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label={t('close', 'Close')}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-5 space-y-3">
              <button
                type="button"
                onClick={() => { actions.handleAddCustom(activeTool); }}
                className="w-full rounded-lg border border-dashed border-primary/40 bg-primary/5 p-4 text-left transition-colors hover:border-primary/60 hover:bg-primary/10"
              >
                <div className="flex items-start gap-3">
                  <div className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground shadow-sm">
                    <Plus className="h-5 w-5" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{t('blankProviderPreset', 'Create manually')}</div>
                    <div className="mt-1 text-sm text-muted-foreground">
                      {t('blankProviderPresetDesc', 'Skip presets and create a provider from an empty form.')}
                    </div>
                  </div>
                </div>
              </button>
              {providerPresets.map((preset) => (
                <div key={preset.id} className="rounded-lg border p-4">
                  <div className="flex items-start justify-between gap-3">
                    <button
                      type="button"
                      onClick={() => { actions.handleAddCustom(activeTool, preset); }}
                      className="min-w-0 flex flex-1 items-start gap-3 text-left"
                    >
                      <ServiceProviderAvatar
                        icon={preset.icon}
                        name={preset.name}
                        id={preset.id}
                        tool={activeTool}
                        size={40}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="font-medium">{preset.name}</div>
                        {preset.description && (
                          <div className="mt-1 text-sm text-muted-foreground">{preset.description}</div>
                        )}
                        <div className="mt-3 grid gap-1 text-xs text-muted-foreground sm:grid-cols-2">
                          <span>{t('providerPresetOpenAILabel', 'OpenAI')}: {preset.endpoints.openai_base_url || t('notSet', 'Not set')}</span>
                          <span>{t('providerPresetAnthropicLabel', 'Anthropic')}: {preset.endpoints.anthropic_base_url || t('notSet', 'Not set')}</span>
                        </div>
                      </div>
                    </button>
                    <button
                      type="button"
                      onClick={() => { openPresetEditor(preset); }}
                      className="rounded-md border p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
                      title={t('editProviderPreset', 'Edit preset')}
                      aria-label={t('editProviderPreset', 'Edit preset')}
                    >
                      <Pencil className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {presetManagerOpen && (
        <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
          <div className="w-full max-w-2xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
            <div className="p-5 border-b flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold">
                  {editingPresetId
                    ? t('editProviderPreset', 'Edit preset')
                    : t('newProviderPreset', 'New preset')}
                </h3>
                <p className="text-sm text-muted-foreground mt-1">
                  {t('providerPresetEndpointDesc', 'Store protocol-specific API URLs only. API keys and instance identifiers stay on service providers.')}
                </p>
              </div>
              <button
                type="button"
                onClick={() => { setPresetManagerOpen(false); }}
                className="rounded-md p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label={t('close', 'Close')}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-5 space-y-4">
              <label className="block text-sm font-medium">
                {t('providerPresetName', 'Preset name')}
                <input
                  value={presetDraft.name}
                  onChange={(event) => { setPresetDraft(prev => ({ ...prev, name: event.target.value })); }}
                  className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
              <label className="block text-sm font-medium">
                {t('providerPresetDescription', 'Description')}
                <input
                  value={presetDraft.description}
                  onChange={(event) => { setPresetDraft(prev => ({ ...prev, description: event.target.value })); }}
                  className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                />
              </label>
              <div className="block text-sm font-medium">
                <div className="mb-1">{t('providerPresetIcon', 'Icon')}</div>
                <IconPicker
                  value={presetDraft.icon || undefined}
                  name={presetDraft.name}
                  providerId={presetDraft.id}
                  onChange={(icon) => { setPresetDraft(prev => ({ ...prev, icon: icon || '' })); }}
                  t={(key: string, fallback: string, options?: Record<string, any>) =>
                    String(t(key, fallback, options))}
                  trigger={(
                    <div className="flex w-full items-center justify-between gap-3">
                      <div className="flex min-w-0 items-center gap-3">
                        <ServiceProviderAvatar
                          icon={presetDraft.icon || undefined}
                          name={presetDraft.name || t('newProviderPreset', 'New preset')}
                          id={presetDraft.id || 'preset'}
                          size={32}
                        />
                        <span className="truncate">
                          {presetDraft.icon || t('iconAuto', 'Auto')}
                        </span>
                      </div>
                      <Pencil className="h-4 w-4 shrink-0 text-muted-foreground" />
                    </div>
                  )}
                />
              </div>
              <div className="grid gap-4 sm:grid-cols-1">
                <label className="block text-sm font-medium">
                  {t('providerPresetOpenAIUrl', 'OpenAI-compatible API URL')}
                  <input
                    value={presetDraft.openai_base_url}
                    onChange={(event) => { setPresetDraft(prev => ({ ...prev, openai_base_url: event.target.value })); }}
                    placeholder="https://api.openai.com/v1"
                    className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                  />
                </label>
                <label className="block text-sm font-medium">
                  {t('providerPresetAnthropicUrl', 'Anthropic-compatible API URL')}
                  <input
                    value={presetDraft.anthropic_base_url}
                    onChange={(event) => { setPresetDraft(prev => ({ ...prev, anthropic_base_url: event.target.value })); }}
                    placeholder="https://api.anthropic.com"
                    className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                  />
                </label>
              </div>
              <div className="rounded-lg border border-border/70 bg-muted/20 p-4 space-y-4">
                <div>
                  <div className="text-sm font-medium">
                    {t('providerPresetClaudeSectionTitle', 'Claude-only preset fields')}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t(
                      'providerPresetClaudeSectionDesc',
                      'These fields are stored on the preset, but only applied when creating a new Claude service provider from it.',
                    )}
                  </p>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                  <label className="block text-sm font-medium">
                    {t('providerPresetClaudeDefaultModel', 'Claude default model')}
                    <input
                      value={presetDraft.claude_default_model}
                      onChange={(event) => {
                        setPresetDraft(prev => ({ ...prev, claude_default_model: event.target.value }));
                      }}
                      placeholder="claude-sonnet-4-5"
                      className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                  <label className="block text-sm font-medium">
                    {t('providerPresetClaudeReasoningEffort', 'Claude reasoning effort')}
                    <input
                      value={presetDraft.claude_reasoning_effort}
                      onChange={(event) => {
                        setPresetDraft(prev => ({ ...prev, claude_reasoning_effort: event.target.value }));
                      }}
                      placeholder="high / xhigh / max / auto / custom"
                      className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                </div>
                <div className="space-y-2">
                  <div className="text-sm font-medium">
                    {t('providerPresetClaudeMappings', 'Claude model mappings')}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t(
                      'providerPresetClaudeMappingsDesc',
                      'Configure Haiku, Sonnet, and Opus upstream models for new Claude providers. Empty rows are ignored when saving.',
                    )}
                  </p>
                  <ModelMappingTable
                    mappings={presetDraft.claude_model_mappings}
                    onChange={(mappings) => {
                      setPresetDraft(prev => ({
                        ...prev,
                        claude_model_mappings: buildPresetClaudeMappings(mappings),
                      }));
                    }}
                    t={(key: string, fallback: string) => String(t(key, fallback))}
                  />
                </div>
              </div>
            </div>
            <div className="border-t p-4 flex items-center justify-between gap-3">
              <div>
                {editingPresetId && (
                  <button
                    type="button"
                    onClick={() => { void deleteProviderPreset(editingPresetId); setPresetManagerOpen(false); }}
                    className="inline-flex items-center gap-2 rounded-md border border-destructive/40 px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                  >
                    <Trash2 className="h-4 w-4" />
                    {t('delete', 'Delete')}
                  </button>
                )}
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => { setPresetManagerOpen(false); }}
                  className="rounded-md border px-3 py-2 text-sm hover:bg-muted"
                >
                  {t('cancel', 'Cancel')}
                </button>
                <button
                  type="button"
                  onClick={() => { void savePresetDraft(); }}
                  className="rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                >
                  {t('save', 'Save')}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

    {importPreview && (
      <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4">
        <div className="w-full max-w-4xl max-h-[85vh] bg-background rounded-xl shadow-xl border overflow-hidden flex flex-col">
          <div className="p-5 border-b flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h3 className="text-lg font-semibold">
                {t('providersImportReviewTitle', 'Review Service Provider import')}
              </h3>
              <p className="text-sm text-muted-foreground mt-1">
                {t('providersImportReviewDesc', {
                  total: importPreview.total || 0,
                  conflicts: importPreview.conflicts || 0,
                  defaultValue:
                    'Found {{total}} Service Provider(s), including {{conflicts}} conflict(s). Choose how to handle conflicts before importing.',
                })}
              </p>
              {importPath && (
                <p className="text-xs text-muted-foreground mt-2 truncate">{importPath}</p>
              )}
            </div>
            <button
              type="button"
              onClick={importExport.closeImportModal}
              disabled={applyingImport}
              className="p-2 hover:bg-muted rounded-md transition-colors disabled:opacity-50"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          <div className="p-5 overflow-y-auto space-y-4">
            <div className="flex flex-wrap gap-2 text-xs">
              <span className="px-2 py-1 rounded-md border bg-muted/40">
                {t('providersImportTotalBadge', {
                  count: importPreview.total || 0,
                  defaultValue: '{{count}} total',
                })}
              </span>
              <span className="px-2 py-1 rounded-md border bg-amber-500/10 text-amber-700 border-amber-500/30">
                {t('providersImportConflictBadge', {
                  count: importPreview.conflicts || 0,
                  defaultValue: '{{count}} conflict(s)',
                })}
              </span>
              <span className="badge-pill bg-green-500/10 text-green-700">
                {t('providersImportNewBadge', {
                  count: importExport.importNewItems.length,
                  defaultValue: '{{count}} new',
                })}
              </span>
              <span className="px-2 py-1 rounded-md border bg-blue-500/10 text-blue-700 border-blue-500/30">
                {t('providersImportActiveBadge', {
                  count: Object.keys(importPreview.active || {}).length,
                  defaultValue: '{{count}} active binding(s) in file',
                })}
              </span>
            </div>

            {importExport.importConflictItems.length > 0 && (
              <div className="rounded-lg border border-amber-200 bg-amber-50/70 p-4 space-y-3">
                <div>
                  <div className="font-medium text-amber-900">
                    {t('providersImportConflictTitle', 'Conflict handling')}
                  </div>
                  <p className="text-sm text-amber-800/90 mt-1">
                    {t(
                      'providersImportConflictDesc',
                      'Overwrite will update the existing Service Provider. Create new will keep both versions.',
                    )}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => importExport.handleSetAllImportConflictActions('overwrite')}
                    disabled={applyingImport}
                    className="px-3 py-1.5 rounded-md border border-amber-300 bg-background hover:bg-amber-100 text-sm transition-colors disabled:opacity-50"
                  >
                    {t('providersImportSetAllOverwrite', 'Set all to overwrite')}
                  </button>
                  <button
                    type="button"
                    onClick={() => importExport.handleSetAllImportConflictActions('new')}
                    disabled={applyingImport}
                    className="px-3 py-1.5 rounded-md border border-amber-300 bg-background hover:bg-amber-100 text-sm transition-colors disabled:opacity-50"
                  >
                    {t('providersImportSetAllNew', 'Set all to create new')}
                  </button>
                </div>
              </div>
            )}

            <div className="space-y-3">
              {importPreview.items.map(item => {
                const selectedAction = importDecisions[item.import_key] || 'overwrite';
                const conflictText = item.conflict_reason === 'name'
                  ? t(
                      'providersImportConflictByName',
                      'Same tool and same name already exist locally: {{name}} ({{id}})',
                      { name: item.existing_name || '', id: item.existing_id || '' },
                    )
                  : t(
                      'providersImportConflictById',
                      'Same tool and same ID already exist locally: {{name}} ({{id}})',
                      { name: item.existing_name || '', id: item.existing_id || '' },
                    );

                return (
                  <div
                    key={item.import_key}
                    className={`rounded-lg border p-4 ${
                      item.conflict ? 'border-amber-200 bg-amber-50/40' : 'border-border bg-card'
                    }`}
                  >
                    <div className="flex gap-3">
                      <div className="w-10 h-10 rounded-lg border bg-muted/40 flex items-center justify-center shrink-0">
                        <ToolIcon tool={item.tool} className="w-5 h-5" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-medium">{item.name}</span>
                          <span className="text-[10px] uppercase px-1.5 py-0.5 rounded border bg-muted/50 text-muted-foreground">
                            {item.tool}
                          </span>
                          {item.model && (
                            <span className="text-[10px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-700 border-blue-500/30">
                              {item.model}
                            </span>
                          )}
                          {item.conflict ? (
                            <span className="badge-pill bg-amber-500/10 text-amber-700">
                              {t('providersImportConflict', 'Conflict')}
                            </span>
                          ) : (
                            <span className="badge-pill bg-green-500/10 text-green-700">
                              {t('providersImportWillCreate', 'Create new')}
                            </span>
                          )}
                        </div>
                        <p className="text-xs font-mono text-muted-foreground mt-1">{item.id}</p>
                        {item.conflict && (
                          <p className="text-xs text-amber-800 mt-2">{conflictText}</p>
                        )}
                      </div>
                      {item.conflict && (
                        <div className="shrink-0 flex flex-col gap-2 min-w-[188px]">
                          <button
                            type="button"
                            onClick={() => {
                              setImportDecisions(prev => ({
                                ...prev,
                                [item.import_key]: 'overwrite',
                              }));
                            }}
                            disabled={applyingImport}
                            className={`px-3 py-2 rounded-md text-sm border transition-colors ${
                              selectedAction === 'overwrite'
                                ? 'border-primary bg-primary text-primary-foreground'
                                : 'border-border bg-background hover:bg-muted'
                            }`}
                          >
                            {t('providersImportOverwrite', 'Overwrite')}
                          </button>
                          <button
                            type="button"
                            onClick={() => {
                              setImportDecisions(prev => ({
                                ...prev,
                                [item.import_key]: 'new',
                              }));
                            }}
                            disabled={applyingImport}
                            className={`px-3 py-2 rounded-md text-sm border transition-colors ${
                              selectedAction === 'new'
                                ? 'border-primary bg-primary text-primary-foreground'
                                : 'border-border bg-background hover:bg-muted'
                            }`}
                          >
                            {t('providersImportCreateNew', 'Create new')}
                          </button>
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="p-4 border-t bg-muted/10 flex items-center justify-between gap-4">
            <div className="text-xs text-muted-foreground">
              {t('providersImportFooterHint', {
                count: importExport.importConflictItems.length,
                defaultValue:
                  '{{count}} conflict(s) require a choice. Non-conflicting Service Providers will be imported directly.',
              })}
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={importExport.closeImportModal}
                disabled={applyingImport}
                className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md transition-colors disabled:opacity-50"
              >
                {t('cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void importExport.handleApplyImport();
                }}
                disabled={applyingImport}
                className="px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md flex items-center gap-2 transition-colors disabled:opacity-50"
              >
                {applyingImport ? <Loader2 className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />}
                {t('import', 'Import')}
              </button>
            </div>
          </div>
        </div>
      </div>
    )}
    {permissionDialogClaudeProfileId && (
      <TerminalPermissionConfirmDialog
        open={permissionDialogOpen}
        toolId="claude"
        toolLabel="Claude Code"
        onConfirm={claudeActions.handleClaudePermissionConfirm}
        onCancel={claudeActions.handleClaudePermissionCancel}
      />
    )}
  </div>
);
}
