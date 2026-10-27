import { useLayoutEffect, useMemo } from 'react';
import type { MutableRefObject } from 'react';
import type { TFunction } from 'i18next';
import type {
  AiProvidersState,
  ClaudeProfileSummary,
  CliTool,
  SyncedDeviceProvidersView,
} from '@/lib/serviceProviders';
import type { ServiceProviderListItem } from '../ServiceProviderList';
import { getClaudeApiFormat, getClaudeMappingTags } from '../helpers/claudeProvider';
import { getIsGlobalForTool, sortServiceProviderListItems } from '../helpers/providerUtils';

interface UseProviderListItemsParams {
  t: TFunction;
  activeTool: string;
  state: AiProvidersState;
  claudeProfiles: ClaudeProfileSummary[];
  unsavedNewProviderIds: Set<string>;
  favoritePendingIds: Set<string>;
  launchingClaudeProfileId: string | null;
  applyingClaudeProfileId: string | null;
  copiedClaudeProfileId: string | null;
  loading: boolean;
  applyingGlobal: boolean;
  viewMode: 'list' | 'detail';
  syncedOtherDeviceProviders: SyncedDeviceProvidersView[];
  listScrollContainerRef: MutableRefObject<HTMLDivElement | null>;
  pendingRestoreListScrollTopRef: MutableRefObject<number | null>;
}

export function useProviderListItems({
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
}: UseProviderListItemsParams) {
  // The manual dependency list is carried over unchanged from the original component.
  // eslint-disable-next-line react-hooks/preserve-manual-memoization
  const currentToolListItems = useMemo<ServiceProviderListItem[]>(() => {
    if (activeTool === 'claude') {
      const items = claudeProfiles.map((profile) => {
        const upstreamTags = getClaudeMappingTags(profile);

        const modelTags = [
          (profile.tool_config?.claude_default_model || profile.model || '').trim(),
        ].filter((value): value is string => Boolean(value));

        const description =
          profile.tilde_config_dir || profile.config_dir || profile.code || '';
        const apiFormatKey = getClaudeApiFormat(profile);
        const apiFormatTag =
          apiFormatKey === 'open_ai_chat'
            ? t('openAiChatFormat', 'OpenAI Chat')
            : apiFormatKey === 'open_ai_responses'
              ? t('openAiResponsesFormat', 'OpenAI Responses')
              : t('anthropicMessagesFormat', 'Anthropic Messages');
        const isActiveForSort = !!profile.is_default;

        return {
          id: profile.id,
          name: profile.name,
          tool: 'claude',
          icon: profile.icon || undefined,
          description,
          remark: profile.tool_config?.remark || '',
          modelTags,
          claudeUpstreamModelTags: upstreamTags,
          apiFormatTag,
          isGlobal: getIsGlobalForTool(state, 'claude', profile.id),
          isFavorite: !!profile.favorite_at,
          favoriteAt: profile.favorite_at ?? null,
          canFavorite: !unsavedNewProviderIds.has(profile.id),
          favoritePending: favoritePendingIds.has(profile.id),
          isActiveForSort,
          canLaunch: true,
          canDelete: true,
          launchBusy: launchingClaudeProfileId === profile.id,
          applyBusy: applyingClaudeProfileId === profile.id,
          deleteBusy: false,
          copiedCommand: copiedClaudeProfileId === profile.id,
          canDuplicate: state.providers.some((provider) => provider.id === profile.id && provider.tool === 'claude')
            && !unsavedNewProviderIds.has(profile.id),
        } satisfies ServiceProviderListItem;
      });

      return sortServiceProviderListItems(items);
    }

    const items = state.providers
      .filter((provider) => provider.tool === activeTool)
      .map((provider) => {
        const description = [
          provider.base_url?.trim(),
          activeTool === 'opencode' ? provider.provider_key?.trim() : '',
        ].find((value) => Boolean(value)) || '';

        const footerTags = [
          activeTool === 'opencode' ? '' : provider.model?.trim(),
          activeTool === 'codex' && provider.personality ? `personality: ${provider.personality}` : '',
          activeTool === 'codex' && provider.wire_api ? `wire: ${provider.wire_api}` : '',
          activeTool === 'codex' && provider.approval_policy ? `approval: ${provider.approval_policy}` : '',
          activeTool === 'codex' && provider.sandbox_mode ? `sandbox: ${provider.sandbox_mode}` : '',
          activeTool === 'antigravity' && provider.theme ? `theme: ${provider.theme}` : '',
          activeTool === 'antigravity' && provider.default_approval_mode
            ? `approval: ${provider.default_approval_mode}`
            : '',
        ].filter((value): value is string => Boolean(value));

        const authLabel =
          activeTool === 'antigravity' && provider.antigravity_auth_type
            ? provider.antigravity_auth_type
            : provider.api_key
              ? t('apiKey', 'API Key')
              : undefined;

        return {
          id: provider.id,
          name: provider.name,
          tool: provider.tool,
          icon: provider.icon,
          description,
          remark: provider.tool_config?.remark || provider.remark || '',
          authLabel,
          modelTags: footerTags,
          claudeUpstreamModelTags: [],
          apiFormatTag: null,
          isGlobal: getIsGlobalForTool(state, activeTool, provider.id),
          isFavorite: !!provider.favorite_at,
          favoriteAt: provider.favorite_at ?? null,
          canFavorite: !unsavedNewProviderIds.has(provider.id),
          favoritePending: favoritePendingIds.has(provider.id),
          isActiveForSort: getIsGlobalForTool(state, activeTool, provider.id),
          canLaunch: false,
          canDelete: true,
          launchBusy: false,
          applyBusy: loading,
          deleteBusy: false,
          copiedCommand: false,
          canDuplicate: !unsavedNewProviderIds.has(provider.id),
        } satisfies ServiceProviderListItem;
      });

    return sortServiceProviderListItems(items);
  }, [
    activeTool,
    applyingGlobal,
    claudeProfiles,
    copiedClaudeProfileId,
    favoritePendingIds,
    launchingClaudeProfileId,
    applyingClaudeProfileId,
    loading,
    getClaudeMappingTags,
    state,
    unsavedNewProviderIds,
  ]);

  const providerCountsByTool = useMemo<Record<CliTool, number>>(
    () => ({
      claude: claudeProfiles.length,
      codex: state.providers.filter((provider) => provider.tool === 'codex').length,
      antigravity: state.providers.filter((provider) => provider.tool === 'antigravity').length,
      opencode: state.providers.filter((provider) => provider.tool === 'opencode').length,
    }),
    [claudeProfiles.length, state.providers],
  );

  useLayoutEffect(() => {
    if (viewMode !== 'list') return;
    const pendingScrollTop = pendingRestoreListScrollTopRef.current;
    if (pendingScrollTop === null) return;
    const container = listScrollContainerRef.current;
    if (!container) return;
    container.scrollTop = pendingScrollTop;
    pendingRestoreListScrollTopRef.current = null;
  }, [viewMode, currentToolListItems, syncedOtherDeviceProviders]);

  return { currentToolListItems, providerCountsByTool };
}
