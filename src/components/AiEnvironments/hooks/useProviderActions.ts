import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { emit } from '@tauri-apps/api/event';
import { v4 as uuidv4 } from 'uuid';
import {
  getActiveProviderIds,
  projectionApply,
  serviceProviderReadOpenCodeConfig,
  serviceProvidersDelete,
  serviceProvidersSetActive,
  serviceProvidersSetFavorite,
  serviceProvidersUpsert,
  type AiProvider,
  type AiProvidersState,
  type ClaudeProfileSummary,
  type HistoryEntry,
  type OpenCodeProviderConfig,
  type SyncedDeviceProvider,
} from '@/lib/serviceProviders';
import type { ActionContext } from '@/lib/userActions';
import { runUserAction } from '@/lib/userActions';
import { notifyMessage } from '@/lib/messages';
import {
  applyProviderPresetToDraft,
  createProviderCopyDraft,
  type ServiceProviderPresetRecord,
} from '../providerPresets';
import {
  parseOpenCodeModelConfig,
  stringifyOpenCodeModelConfig,
  type JsonObject,
  type OpenCodeModelConfigError,
} from '../opencodeModelConfig';
import {
  getMissingRequiredProviderFields,
  isManagedTool,
  unwrapApiResp,
  buildSyncedProviderActivationPayload,
  errorToDisplayMessage,
  uniqueOpenCodeProviderKey,
  uniqueProviderCode,
  type RequiredProviderField,
  type SavePresetResult,
} from '../helpers/providerUtils';
import {
  buildClaudeProviderFromProfile,
  buildClaudeProviderFromState,
  normalizeProviderForSave,
} from '../helpers/claudeProvider';
import { getOpenCodeJson } from '../helpers/openCodeProvider';

interface OpenCodeEditorBindings {
  openCodeErrorMessage: (errors: OpenCodeModelConfigError[]) => string;
  initializeOpenCodeEditor: (json: string, updateOriginal?: boolean) => boolean;
}

interface UseProviderActionsParams {
  t: TFunction;
  actionContext: ActionContext;
  pushToast: ActionContext['pushToast'];
  confirmDialog: ActionContext['confirm'];
  isTauri: boolean;
  activeTool: string;
  setActiveTool: Dispatch<SetStateAction<string>>;
  state: AiProvidersState;
  setState: Dispatch<SetStateAction<AiProvidersState>>;
  currentProviderId: string | null;
  setCurrentProviderId: Dispatch<SetStateAction<string | null>>;
  detailProvider: AiProvider | null;
  setDetailProvider: Dispatch<SetStateAction<any | null>>;
  rawJson: string;
  setRawJson: Dispatch<SetStateAction<string>>;
  setOriginalJson: Dispatch<SetStateAction<string>>;
  setJsonError: Dispatch<SetStateAction<string | null>>;
  setMessage: Dispatch<SetStateAction<{ type: string; text: string }>>;
  setLoading: Dispatch<SetStateAction<boolean>>;
  savingDetail: boolean;
  setSavingDetail: Dispatch<SetStateAction<boolean>>;
  isRollbackMode: boolean;
  setIsRollbackMode: Dispatch<SetStateAction<boolean>>;
  unsavedNewProviderIds: Set<string>;
  setUnsavedNewProviderIds: Dispatch<SetStateAction<Set<string>>>;
  setFavoritePendingIds: Dispatch<SetStateAction<Set<string>>>;
  setActivatingSyncedKey: Dispatch<SetStateAction<string | null>>;
  claudeProfiles: ClaudeProfileSummary[];
  setViewMode: Dispatch<SetStateAction<'list' | 'detail'>>;
  setPresetPickerOpen: Dispatch<SetStateAction<boolean>>;
  loadProviders: (silent?: boolean) => Promise<void>;
  loadClaudeProfiles: () => Promise<void>;
  editor: OpenCodeEditorBindings;
  rollbackDraftBeforeRef: MutableRefObject<{ provider: AiProvider | null; rawJson: string } | null>;
  openCodeDetailRequestIdRef: MutableRefObject<number>;
  openCodeDetailTargetRef: MutableRefObject<{ id: string; providerKey: string } | null>;
  pendingRestoreListScrollTopRef: MutableRefObject<number | null>;
  savedListScrollTopRef: MutableRefObject<number>;
}

export function useProviderActions({
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
  loadProviders,
  loadClaudeProfiles,
  editor,
  rollbackDraftBeforeRef,
  openCodeDetailRequestIdRef,
  openCodeDetailTargetRef,
  pendingRestoreListScrollTopRef,
  savedListScrollTopRef,
}: UseProviderActionsParams) {
  const { openCodeErrorMessage, initializeOpenCodeEditor } = editor;

  const activateProvider = async (tool: string, providerId: string) => {
    try {
      setLoading(true);
      setMessage({ type: '', text: '' });
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'activate',
          action: 'activate-provider',
          target: { tab: 'ai-environments', entity_id: providerId },
          dedupeKey: `ai-environments:activate:${tool}:${providerId}`,
          metadata: { tool, provider_id: providerId },
          confirm: {
            message: t(
              'confirmActivateProvider',
              'Activate this Service Provider and apply it to the current environment?',
            ),
            title: t('confirmActivateProviderTitle', 'Activate Service Provider'),
            okLabel: t('activate', 'Activate'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('aiEnvironmentActivatedMessageTitle', 'Service Provider activated'),
            summary: t('appliedSuccess', 'Service Provider activated successfully!'),
          },
          error: {
            title: t('activationFailed', 'Activation failed'),
          },
        },
        async () => {
          await serviceProvidersSetActive(tool, providerId);
          await loadProviders(true);
          await projectionApply(tool, providerId);
          return true;
        },
      );
      if (result === null) return false;
      setMessage({ type: 'success', text: t('appliedSuccess', 'Service Provider activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return true;
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      return false;
    } finally {
      setLoading(false);
    }
  };

  const buildProviderForSave = (provider: Partial<AiProvider>): AiProvider => {
    const newId = provider.id || uuidv4();
    let baseProvider: Record<string, any> = { ...provider };

    if (provider.tool === 'opencode') {
      const config = parseOpenCodeModelConfig(rawJson || '{}');
      if (!config.ok) throw new Error(openCodeErrorMessage(config.errors));
      const parsed = config.snapshot as Record<string, any>;

      baseProvider = {
        ...parsed,
        id: provider.id,
        tool: 'opencode',
        icon: provider.icon,
        is_enabled: true,
        provider_key: provider.provider_key,
        history: provider.history || [],
      };

      const options = parsed.options && typeof parsed.options === 'object' ? parsed.options : {};
      baseProvider.api_key = typeof options.apiKey === 'string' ? options.apiKey : '';
      baseProvider.base_url = typeof options.baseURL === 'string' ? options.baseURL : '';

    }

    return {
      ...baseProvider,
      id: newId,
      name: String(baseProvider.name || 'Unnamed'),
      tool: String(provider.tool || activeTool),
      api_key: String(baseProvider.api_key || ''),
      provider_key: baseProvider.provider_key,
      is_enabled: provider.tool === 'opencode' ? true : (baseProvider.is_enabled ?? true),
      env_managed: provider.tool !== 'opencode' ? (baseProvider.env_managed ?? true) : undefined,
      history: Array.isArray(provider.history) ? provider.history : [],
    } as AiProvider;
  };

  const saveDetailProvider = async (
    provider: Partial<AiProvider>,
    options: { showSavedMessage?: boolean } = {},
  ): Promise<SavePresetResult> => {
    const { showSavedMessage = true } = options;
    if (!provider.name) {
      setMessage({ type: 'error', text: t('providePresetName', 'Please provide a preset name') });
      return { ok: false };
    }

    const newId = provider.id || uuidv4();

    try {
      const finalProvider = buildProviderForSave(provider);
      const missingRequiredFields = getMissingRequiredProviderFields(finalProvider);
      if (missingRequiredFields.length > 0) {
        const labelByField: Record<RequiredProviderField, string> = {
          api_key: t('apiKey', 'API Key'),
          base_url: t('baseUrl', 'Base URL'),
          provider_key: t('providerIdentifier', 'Service Provider Identifier'),
          code: t('providerIdentifier', 'Service Provider Identifier'),
        };
        const fields = missingRequiredFields.map((field) => labelByField[field]).join(' + ');
        const text = t('requiredProviderFieldsMissing', 'Please fill required fields: {{fields}}', { fields });
        setMessage({ type: 'error', text });
        pushToast({ title: t('saveFailed', 'Save failed'), description: text, kind: 'error' });
        return { ok: false };
      }
      const savedData = unwrapApiResp(
        await serviceProvidersUpsert(normalizeProviderForSave(finalProvider)),
        t('saveFailed', 'Save failed'),
      );
      const savedProvider = { ...finalProvider, ...savedData } as AiProvider;
      await loadProviders(true);
      if (savedProvider.tool === 'claude') {
        await loadClaudeProfiles();
      }
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.delete(newId);
        return next;
      });
      setCurrentProviderId(savedProvider.id);
      emit('refresh-counts');
      setDetailProvider(savedProvider);
      setIsRollbackMode(false);
      rollbackDraftBeforeRef.current = null;
      setJsonError(null);
      if (savedProvider.tool === 'opencode') {
        setOriginalJson(rawJson);
        if (state.active_opencode.includes(savedProvider.id)) {
          try {
            await projectionApply('opencode', savedProvider.id);
          } catch (e) {
            console.error('Failed to sync opencode.json after save:', e);
          }
        }
      }

      if (showSavedMessage) {
        await notifyMessage.success({
          source: 'ai_environments',
          category: 'save',
          title: t('aiEnvironmentSavedMessageTitle', 'Service Provider saved'),
          summary: t('providerSaved', 'Service Provider saved'),
          dedupeKey: `ai-environments:save:${savedProvider.tool}:${savedProvider.id}`,
          target: { tab: 'ai-environments', entity_id: savedProvider.id },
          metadata: { tool: savedProvider.tool, provider_id: savedProvider.id },
        });
        setMessage({ type: 'success', text: t('providerSaved', 'Service Provider saved') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        pushToast({ title: t('providerSaved', 'Service Provider saved'), kind: 'success' });
      }

      return {
        ok: true,
        providerId: savedProvider.id,
        provider: savedProvider,
      };
    } catch (e: any) {
      const errorMessage = errorToDisplayMessage(e);
      await notifyMessage.error({
        source: 'ai_environments',
        category: 'save',
        title: t('saveFailed', 'Save failed'),
        summary: errorMessage,
        error: e,
        dedupeKey: `ai-environments:save:${provider.tool || activeTool}:${newId}`,
        target: { tab: 'ai-environments', entity_id: newId },
        metadata: { tool: provider.tool || activeTool, provider_id: newId },
      });
      pushToast({ title: t('saveFailed', 'Save failed'), description: errorMessage, kind: 'error' });
      setMessage({ type: 'error', text: errorMessage });
      return { ok: false };
    }
  };

  const returnToProviderList = (options: { preserveScroll?: boolean } = {}) => {
    openCodeDetailRequestIdRef.current += 1;
    openCodeDetailTargetRef.current = null;
    if (options.preserveScroll) {
      pendingRestoreListScrollTopRef.current = savedListScrollTopRef.current;
    }
    setViewMode('list');
    setDetailProvider(null);
  };

  const handleSaveDetailAndReturnToList = async (provider: Partial<AiProvider>) => {
    if (savingDetail) return;
    setSavingDetail(true);
    try {
      const result = await saveDetailProvider(provider);
      if (!result.ok) return;
      returnToProviderList({ preserveScroll: true });
    } finally {
      setSavingDetail(false);
    }
  };

  const handleRollback = (entry: HistoryEntry) => {
    if (entry.snapshot && typeof entry.snapshot === 'object') {
      if (!isRollbackMode) {
        rollbackDraftBeforeRef.current = {
          provider: detailProvider ? { ...detailProvider } : null,
          rawJson,
        };
      }
      const draft = {
        ...entry.snapshot,
        history: detailProvider?.history || entry.snapshot.history || [],
      } as AiProvider;
      setDetailProvider(draft);
      if (draft.tool === 'opencode') {
        initializeOpenCodeEditor(getOpenCodeJson(draft));
      } else {
        setJsonError(null);
      }
      setIsRollbackMode(true);
      setMessage({ type: 'success', text: t('rollbackModeTitle', 'History version loaded.') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    if (entry.content) {
      try {
        const parsed = parseOpenCodeModelConfig(entry.content);
        if (!parsed.ok) throw new Error(openCodeErrorMessage(parsed.errors));
        if (!isRollbackMode) {
          rollbackDraftBeforeRef.current = {
            provider: detailProvider ? { ...detailProvider } : null,
            rawJson,
          };
        }
        initializeOpenCodeEditor(entry.content);
        setIsRollbackMode(true);
        setJsonError(null);
        setMessage({ type: 'success', text: t('rollbackModeTitle', 'History version loaded.') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      } catch {
        setMessage({ type: 'error', text: t('parseHistoryFailed') });
      }
      return;
    }

    setMessage({ type: 'error', text: t('parseHistoryFailed') });
  };

  const handleAddCustom = (toolName: string, preset?: ServiceProviderPresetRecord) => {
    const newId = uuidv4();
    const baseProvider: AiProvider = {
      id: newId,
      name: preset?.name || `${t('newPreset', 'New Service Provider')} (${toolName})`,
      tool: toolName,
      api_key: '',
      base_url: '',
      model: '',
      icon: preset?.icon,
      code: toolName !== 'opencode' ? uniqueProviderCode(state.providers, toolName, preset?.name) : undefined,
      env_managed: toolName !== 'opencode' ? true : undefined,
      provider_key: toolName === 'opencode' ? uniqueOpenCodeProviderKey(state.providers, preset?.name) : undefined,
      is_enabled: true,
      ...(toolName === 'claude' ? {
        claude_api_format: 'anthropic_messages',
        claude_auth_env_key: 'ANTHROPIC_API_KEY',
        claude_model_mappings: [
          { family: 'haiku', display_name: 'Haiku', upstream_model: '', supports_1m: false },
          { family: 'sonnet', display_name: 'Sonnet', upstream_model: '', supports_1m: false },
          { family: 'opus', display_name: 'Opus', upstream_model: '', supports_1m: false },
        ],
        claude_enable_tool_search: false,
        claude_enable_attribution: false,
        claude_auto_memory_enabled: false,
        claude_always_thinking_enabled: false,
        claude_away_summary_enabled: false,
        claude_include_git_instructions: false,
      } : {}),
      ...(toolName === 'opencode' ? {
        npm: '@ai-sdk/openai-compatible',
        options: { apiKey: '', baseURL: '' },
        models: {}
      } : {})
    };
    const newProvider = preset
      ? (applyProviderPresetToDraft(baseProvider, preset, toolName) as AiProvider)
      : baseProvider;

    // Switch tool first (if different) — this may trigger the active-provider auto-expand effect
    if (toolName !== activeTool) {
      setActiveTool(toolName);
    }

    const newState = {
      ...state,
      providers: [...state.providers, newProvider]
    };

    setState(newState);
    setCurrentProviderId(newId);
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.add(newId);
        return next;
      });
    if (toolName === 'opencode') {
      initializeOpenCodeEditor(getOpenCodeJson(newProvider), true);
    } else {
      setRawJson(JSON.stringify(newProvider, null, 2));
      setOriginalJson(JSON.stringify(newProvider, null, 2));
      setJsonError(null);
    }
    setIsRollbackMode(false);
    rollbackDraftBeforeRef.current = null;
    setDetailProvider(newProvider);
    setViewMode('detail');
    setPresetPickerOpen(false);
  };

  const handleDuplicateProvider = (providerId: string) => {
    const source = state.providers.find((provider) => provider.id === providerId);
    if (!source || source.tool !== activeTool || unsavedNewProviderIds.has(providerId)) {
      setMessage({
        type: 'error',
        text: t('duplicateProviderUnavailable', 'The saved Service Provider is no longer available to duplicate.'),
      });
      return;
    }

    const draft = {
      ...createProviderCopyDraft(source),
      api_key: '',
      history: [],
    } as AiProvider;
    setCurrentProviderId(draft.id);
    setUnsavedNewProviderIds((previous) => new Set(previous).add(draft.id));
    setDetailProvider(draft);
    if (draft.tool === 'opencode') {
      initializeOpenCodeEditor(getOpenCodeJson(draft), true);
    } else {
      const json = JSON.stringify(draft, null, 2);
      setRawJson(json);
      setOriginalJson(json);
      setJsonError(null);
    }
    setIsRollbackMode(false);
    rollbackDraftBeforeRef.current = null;
    setMessage({ type: '', text: '' });
    setViewMode('detail');
  };

  const handleDelete = async (providerId?: string, toolName?: string) => {
    const targetId = providerId || currentProviderId;
    if (!targetId) return;
    const targetTool = toolName || detailProvider?.tool || activeTool;
    const isUnsavedNewProvider = unsavedNewProviderIds.has(targetId);
    const providerToDelete = isUnsavedNewProvider && detailProvider?.id === targetId
      ? detailProvider
      :
      targetTool === 'claude'
        ? claudeProfiles.find((p) => p.id === targetId)
        : state.providers.find((p) => p.id === targetId && p.tool === targetTool);
    if (!providerToDelete) return;
    const isProviderActive = getActiveProviderIds(state, targetTool).includes(providerToDelete.id);
    const isDefaultImportedForTool =
      isManagedTool(targetTool) && providerToDelete.code === `default-${targetTool}`;
    const isDeletingActiveDefaultImported =
      isDefaultImportedForTool && isProviderActive;
    const isDeletingInactiveDefaultImported =
      isDefaultImportedForTool && !isProviderActive;
    if (isDeletingActiveDefaultImported) return;
    if (
      !isUnsavedNewProvider &&
      !isDeletingInactiveDefaultImported &&
      targetTool !== 'claude' &&
      state.providers.filter(p => p.tool === targetTool).length <= 1
    ) {
      return;
    }

    if (isUnsavedNewProvider) {
      const confirmed = await confirmDialog(t('confirmDelete', { name: providerToDelete.name }), {
        title: t('confirmDeleteTitle', 'Delete Service Provider'),
        okLabel: t('delete', 'Delete'),
        cancelLabel: t('cancel', 'Cancel'),
        kind: 'error',
      });
      if (!confirmed) return;
      setState(prev => ({
        ...prev,
        providers: prev.providers.filter(p => p.id !== targetId)
      }));
      if (currentProviderId === targetId) {
        setCurrentProviderId(null);
      }
      if (detailProvider?.id === targetId) {
        setDetailProvider(null);
        setViewMode('list');
      }
      setUnsavedNewProviderIds(prev => {
        const next = new Set(prev);
        next.delete(targetId);
        return next;
      });
      await notifyMessage.success({
        source: 'ai_environments',
        category: 'delete',
        title: t('aiEnvironmentDeletedMessageTitle', 'Service Provider deleted'),
        summary: t('deleteSuccess', 'Preset deleted successfully'),
        dedupeKey: `ai-environments:delete:${targetTool}:${targetId}`,
        target: { tab: 'ai-environments', entity_id: targetId },
        metadata: { tool: targetTool, provider_id: targetId, unsaved: true },
      });
      pushToast({ title: t('deleteSuccess', 'Preset deleted successfully'), kind: 'success' });
      setMessage({ type: 'success', text: t('deleteSuccess', 'Preset deleted successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    try {
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'delete',
          action: 'delete-provider',
          target: { tab: 'ai-environments', entity_id: targetId },
          dedupeKey: `ai-environments:delete:${targetTool}:${targetId}`,
          metadata: { tool: targetTool, provider_id: targetId },
          confirm: {
            message: t('confirmDelete', { name: providerToDelete.name }),
            title: t('confirmDeleteTitle', 'Delete Service Provider'),
            okLabel: t('delete', 'Delete'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'error',
          },
          success: {
            title: t('aiEnvironmentDeletedMessageTitle', 'Service Provider deleted'),
            summary: t('deleteSuccess', 'Preset deleted successfully'),
          },
          error: {
            title: t('deleteFailed', 'Delete failed'),
          },
        },
        () => serviceProvidersDelete(targetId),
      );
      if (result === null) return;
      await loadProviders(true);
      if (currentProviderId === targetId) {
        setCurrentProviderId(null);
      }
      if (detailProvider?.id === targetId) {
        setDetailProvider(null);
        setViewMode('list');
      }
      emit('refresh-counts');
      setMessage({ type: 'success', text: t('deleteSuccess', 'Preset deleted successfully') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    }
  };

  const handleToggleFavorite = async (providerId: string, favorite: boolean) => {
    if (!isTauri) return;
    setFavoritePendingIds(prev => new Set(prev).add(providerId));
    try {
      await serviceProvidersSetFavorite(providerId, favorite);
      await loadProviders(true);
      if (activeTool === 'claude') {
        await loadClaudeProfiles();
      }
    } catch (e: any) {
      const text = String(e?.message || e);
      setMessage({ type: 'error', text });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      pushToast({ title: t('saveFailed', 'Save failed'), description: text, kind: 'error' });
    } finally {
      setFavoritePendingIds(prev => {
        const next = new Set(prev);
        next.delete(providerId);
        return next;
      });
    }
  };

  const handleActivateSyncedProvider = async (deviceId: string, provider: SyncedDeviceProvider) => {
    const apiKey = String(provider.api_key || '').trim();
    if (!apiKey) {
      setMessage({
        type: 'error',
        text: t(
          'syncedProviderMissingApiKey',
          'This Service Provider is missing a decryptable API Key and cannot be activated directly.'
        ),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      return;
    }

    const activation = buildSyncedProviderActivationPayload(deviceId, provider);
    if (!activation) {
      return;
    }
    const { targetId, targetTool, payload } = activation;

    const actionKey = `${deviceId}:${provider.tool}:${provider.id}`;
    try {
      setLoading(true);
      setActivatingSyncedKey(actionKey);
      const savedData = unwrapApiResp(
        await serviceProvidersUpsert(payload),
        t('saveFailed', 'Save failed'),
      );
      const savedProviderId = String(savedData?.id || targetId);
      await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'activate',
          action: 'activate-synced-provider',
          target: { tab: 'ai-environments', entity_id: savedProviderId },
          dedupeKey: `ai-environments:activate:${targetTool}:${savedProviderId}`,
          metadata: { tool: targetTool, provider_id: savedProviderId, device_id: deviceId },
          confirm: {
            message: t(
              'confirmActivateSyncedProvider',
              'Import this synced Service Provider and apply it on this device now?',
            ),
            title: t('confirmActivateProviderTitle', 'Activate Service Provider'),
            okLabel: t('activate', 'Activate'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
            summary: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
          },
          error: {
            title: t('activationFailed', 'Activation failed'),
          },
        },
        async () => {
          await serviceProvidersSetActive(targetTool, savedProviderId);
          await projectionApply(targetTool, savedProviderId);
          return true;
        },
      );
      await loadProviders(true);
      setActiveTool(targetTool);
      setCurrentProviderId(savedProviderId);
      emit('refresh-counts');
      setMessage({
        type: 'success',
        text: t('syncedProviderActivated', 'Imported and activated this synced Service Provider.'),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: errorToDisplayMessage(e) });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLoading(false);
      setActivatingSyncedKey(null);
    }
  };

  const openServiceProviderDetail = async (id: string) => {
    rollbackDraftBeforeRef.current = null;
    const requestId = ++openCodeDetailRequestIdRef.current;
    openCodeDetailTargetRef.current = null;
    if (activeTool === 'claude') {
      const storedProvider = state.providers.find((item) => item.id === id && item.tool === 'claude');
      if (storedProvider) {
        const adapted = buildClaudeProviderFromState(storedProvider);
        setCurrentProviderId(id);
        setDetailProvider(adapted);
        setRawJson(JSON.stringify(adapted, null, 2));
        setOriginalJson(JSON.stringify(adapted, null, 2));
        setJsonError(null);
        setIsRollbackMode(false);
        setViewMode('detail');
        return;
      }

      const profile = claudeProfiles.find((item) => item.id === id);
      if (!profile) return;
      const adapted = buildClaudeProviderFromProfile(profile);
      setCurrentProviderId(id);
      setDetailProvider(adapted);
      setRawJson(JSON.stringify(adapted, null, 2));
      setOriginalJson(JSON.stringify(adapted, null, 2));
      setJsonError(null);
      setIsRollbackMode(false);
      setViewMode('detail');
      return;
    }
    const provider = state.providers.find(p => p.id === id && p.tool === activeTool);
    if (!provider) return;
    const adaptedProvider = {
      ...provider,
      api_key: activeTool === 'opencode' ? '' : provider.api_key,
      remark: provider.tool_config?.remark || '',
    };
    let json = activeTool === 'opencode' ? getOpenCodeJson(adaptedProvider) : JSON.stringify(adaptedProvider, null, 2);
    if (activeTool === 'opencode') {
      const providerKey = provider.provider_key || '';
      const cachedConfig = JSON.parse(json) as JsonObject;
      const cachedOptions = cachedConfig.options && typeof cachedConfig.options === 'object' && !Array.isArray(cachedConfig.options)
        ? cachedConfig.options as JsonObject
        : {};
      cachedConfig.options = { ...cachedOptions, apiKey: '' };
      json = stringifyOpenCodeModelConfig(cachedConfig);
      openCodeDetailTargetRef.current = { id, providerKey };

      setCurrentProviderId(id);
      setDetailProvider(adaptedProvider);
      initializeOpenCodeEditor(json, true);
      setIsRollbackMode(false);
      setViewMode('detail');

      try {
        const response = await serviceProviderReadOpenCodeConfig(providerKey);
        const runtimeProvider = unwrapApiResp<OpenCodeProviderConfig>(
          response,
          'Failed to read OpenCode provider config',
        );
        const target = openCodeDetailTargetRef.current;
        if (
          openCodeDetailRequestIdRef.current !== requestId
          || target?.id !== id
          || target.providerKey !== providerKey
        ) return;
        const runtimeOptions = runtimeProvider.options
          && typeof runtimeProvider.options === 'object'
          && !Array.isArray(runtimeProvider.options)
          ? runtimeProvider.options as Record<string, unknown>
          : null;
        const runtimeApiKey = runtimeOptions?.apiKey;
        json = JSON.stringify(runtimeProvider, null, 2);
        initializeOpenCodeEditor(json, true);
        if (typeof runtimeApiKey !== 'string' || runtimeApiKey.length === 0) {
          setMessage({
            type: 'error',
            text: 'Failed to read OpenCode provider config: options.apiKey is missing',
          });
          return;
        }
        setDetailProvider((current: AiProvider | null) => {
          if (!current || current.id !== id || current.provider_key !== providerKey) return current;
          return { ...current, api_key: runtimeApiKey };
        });
        setMessage({ type: '', text: '' });
      } catch (error) {
        const target = openCodeDetailTargetRef.current;
        if (
          openCodeDetailRequestIdRef.current !== requestId
          || target?.id !== id
          || target.providerKey !== providerKey
        ) return;
        setMessage({
          type: 'error',
          text: `Failed to read OpenCode provider config: ${errorToDisplayMessage(error)}`,
        });
      }
      return;
    }
    setCurrentProviderId(id);
    setDetailProvider(adaptedProvider);
    if (activeTool === 'opencode') {
      initializeOpenCodeEditor(json, true);
    } else {
      setRawJson(json);
      setOriginalJson(json);
      setJsonError(null);
    }
    setIsRollbackMode(false);
    setViewMode('detail');
  };

  return {
    activateProvider,
    saveDetailProvider,
    returnToProviderList,
    handleSaveDetailAndReturnToList,
    handleRollback,
    handleAddCustom,
    handleDuplicateProvider,
    handleDelete,
    handleToggleFavorite,
    handleActivateSyncedProvider,
    openServiceProviderDetail,
  };
}
