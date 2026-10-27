import type { Dispatch, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { emit } from '@tauri-apps/api/event';
import {
  getClaudeConfigDir,
  projectionApply,
  serviceProvidersSetActive,
} from '@/lib/serviceProviders';
import { sessionsCreate } from '@/lib/aiSessions';
import { openLocalPath } from '@/lib/externalActions';
import { runUserAction, type ActionContext } from '@/lib/userActions';
import type { TerminalPermissionMode } from '@/lib/terminalPermissions';
import { errorToDisplayMessage, getInvokeErrorCode } from '../helpers/providerUtils';

interface UseClaudeProfileActionsParams {
  t: TFunction;
  actionContext: ActionContext;
  isTauri: boolean;
  claudeLaunchCommand: string;
  permissionDialogClaudeProfileId: string | null;
  setCopiedClaudeProfileId: Dispatch<SetStateAction<string | null>>;
  setMessage: Dispatch<SetStateAction<{ type: string; text: string }>>;
  setApplyingGlobal: Dispatch<SetStateAction<boolean>>;
  setApplyingClaudeProfileId: Dispatch<SetStateAction<string | null>>;
  setLaunchingClaudeProfileId: Dispatch<SetStateAction<string | null>>;
  setPermissionDialogClaudeProfileId: Dispatch<SetStateAction<string | null>>;
  setPermissionDialogOpen: Dispatch<SetStateAction<boolean>>;
  setCurrentProviderId: Dispatch<SetStateAction<string | null>>;
  loadProviders: (silent?: boolean) => Promise<void>;
}

export function useClaudeProfileActions({
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
  loadProviders,
}: UseClaudeProfileActionsParams) {
  const handleClaudeCopyCommand = async (profileId: string, configDir: string) => {
    const cmd = `CLAUDE_CONFIG_DIR='${configDir}' ${claudeLaunchCommand.replace('{session_id}', 'new')}`;
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(cmd);
      } else {
        const input = document.createElement('textarea');
        input.value = cmd;
        input.setAttribute('readonly', 'true');
        input.style.position = 'fixed';
        input.style.left = '-9999px';
        document.body.appendChild(input);
        input.select();
        const copied = document.execCommand('copy');
        document.body.removeChild(input);
        if (!copied) throw new Error('copy_failed');
      }
      setCopiedClaudeProfileId(profileId);
      window.setTimeout(() => setCopiedClaudeProfileId(null), 2000);
      setMessage({ type: 'success', text: t('claudeProfileCopySuccess', 'Command copied to clipboard') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: t('claudeProfileCopyFailed', 'Failed to copy command') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    }
  };

  const handleClaudeOpenDir = async (profileId: string) => {
    if (!isTauri) return;
    try {
      const configDir = await getClaudeConfigDir(profileId);
      if (!configDir) throw new Error(t('configDirNotFound', 'Config directory not found'));
      await openLocalPath(configDir);
    } catch (e: any) {
      console.error('handleClaudeOpenDir error:', e);
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    }
  };

  const handleClaudeApplyGlobal = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setApplyingGlobal(true);
      setApplyingClaudeProfileId(profileId);
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'activate',
          action: 'apply-claude-profile',
          target: { tab: 'ai-environments', entity_id: profileId },
          dedupeKey: `ai-environments:activate:claude:${profileId}`,
          metadata: { tool: 'claude', provider_id: profileId },
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
            summary: t('appliedSuccess', 'Environment activated successfully!'),
          },
          error: {
            title: t('activationFailed', 'Activation failed'),
          },
        },
        async () => {
          await serviceProvidersSetActive('claude', profileId);
          await projectionApply('claude', profileId);
        },
      );
      if (result === null) return;
      await loadProviders(true);
      emit('refresh-counts');
      setMessage({ type: 'success', text: t('appliedSuccess', 'Environment activated successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setApplyingGlobal(false);
      setApplyingClaudeProfileId(null);
    }
  };

  const createClaudeProviderSession = async (
    profileId: string,
    permissionMode?: TerminalPermissionMode,
  ) => {
    const response = await sessionsCreate({
      name: '',
      working_dir: '',
      tool: 'claude',
      provider_id: profileId,
      status: 'active',
      ...(permissionMode ? { permission_mode: permissionMode } : {}),
    });

    if (response.ok) {
      setCurrentProviderId(response.data.provider_id || profileId);
      await emit('session-created');
      await emit('refresh-counts');
    }
  };

  const handleClaudeLaunch = async (profileId: string) => {
    if (!isTauri) return;
    try {
      setLaunchingClaudeProfileId(profileId);
      await createClaudeProviderSession(profileId);
      setMessage({ type: 'success', text: t('sessionCreated', 'Session created') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: unknown) {
      const code = getInvokeErrorCode(e);
      if (code === 'PERMISSION_CONFIRMATION_REQUIRED') {
        setPermissionDialogClaudeProfileId(profileId);
        setPermissionDialogOpen(true);
        return;
      }
      console.error('handleClaudeLaunch error:', e);
      setMessage({ type: 'error', text: errorToDisplayMessage(e) });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLaunchingClaudeProfileId(null);
    }
  };

  const handleClaudePermissionConfirm = async (mode: TerminalPermissionMode) => {
    if (!permissionDialogClaudeProfileId) return;
    const profileId = permissionDialogClaudeProfileId;
    setPermissionDialogOpen(false);
    setPermissionDialogClaudeProfileId(null);
    try {
      setLaunchingClaudeProfileId(profileId);
      await createClaudeProviderSession(profileId, mode);
      setMessage({ type: 'success', text: t('sessionCreated', 'Session created') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: unknown) {
      console.error('handleClaudePermissionConfirm error:', e);
      setMessage({ type: 'error', text: errorToDisplayMessage(e) });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setLaunchingClaudeProfileId(null);
    }
  };

  const handleClaudePermissionCancel = () => {
    setPermissionDialogOpen(false);
    setPermissionDialogClaudeProfileId(null);
  };

  return {
    handleClaudeCopyCommand,
    handleClaudeOpenDir,
    handleClaudeApplyGlobal,
    handleClaudeLaunch,
    handleClaudePermissionConfirm,
    handleClaudePermissionCancel,
  };
}
