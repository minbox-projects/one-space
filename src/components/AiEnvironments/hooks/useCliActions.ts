import type { Dispatch, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { invoke } from '@tauri-apps/api/core';
import type { CliTool } from '@/lib/serviceProviders';
import type { ActionContext } from '@/lib/userActions';
import { runUserAction } from '@/lib/userActions';
import { notifyMessage } from '@/lib/messages';
import type { CliUpdateApplyResult, CliUpdateInfo } from '../helpers/providerUtils';

interface UseCliActionsParams {
  t: TFunction;
  actionContext: ActionContext;
  cliUpdates: Partial<Record<CliTool, CliUpdateInfo>>;
  setUpdatingTool: Dispatch<SetStateAction<Partial<Record<CliTool, boolean>>>>;
  setMessage: Dispatch<SetStateAction<{ type: string; text: string }>>;
  pushToast: ActionContext['pushToast'];
}

export function useCliActions({
  t,
  actionContext,
  cliUpdates,
  setUpdatingTool,
  setMessage,
  pushToast,
}: UseCliActionsParams) {
  const handleApplyCliUpdate = async (tool: CliTool) => {
    const updateInfo = cliUpdates[tool];
    if (!updateInfo) return;

    const toolLabel = tool.charAt(0).toUpperCase() + tool.slice(1);
    const confirmMsg = t('confirmCliUpdateMessage', {
      tool: toolLabel,
      command: updateInfo.update_command,
    });

    setUpdatingTool(prev => ({ ...prev, [tool]: true }));
    try {
      const result = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'cli_update',
          action: 'apply-cli-update',
          target: { tab: 'ai-environments', entity_id: tool },
          dedupeKey: `ai-environments:cli-update:${tool}`,
          metadata: { tool, command: updateInfo.update_command },
          confirm: {
            message: confirmMsg,
            title: t('confirmCliUpdateTitle') + `: ${toolLabel}`,
            okLabel: t('cliUpdate'),
            cancelLabel: t('cancel'),
            kind: 'warning',
          },
          success: {
            title: t('cliUpdateTerminalLaunched', 'Update command opened in terminal'),
            summary: t('cliUpdateTerminalLaunched', 'Update command opened in terminal'),
          },
          error: {
            title: t('cliUpdateFailedTitle', 'CLI update failed'),
          },
        },
        () => invoke<CliUpdateApplyResult>('apply_cli_update', { tool }),
      );
      if (result === null) return;
      if (result.success && result.terminal_launched) {
        setMessage({ type: 'success', text: t('cliUpdateTerminalLaunched') });
        setTimeout(() => setMessage({ type: '', text: '' }), 5000);
      } else {
        await notifyMessage.error({
          source: 'ai_environments',
          category: 'cli_update',
          title: t('cliUpdateFailedTitle', 'CLI update failed'),
          summary: t('cliUpdateFailed', { error: result.error || 'Unknown error' }),
          error: result.error || 'Unknown error',
          dedupeKey: `ai-environments:cli-update:${tool}`,
          target: { tab: 'ai-environments', entity_id: tool },
          metadata: result,
        });
        pushToast({
          title: t('cliUpdateFailedTitle', 'CLI update failed'),
          description: result.error || 'Unknown error',
          kind: 'error',
        });
        setMessage({ type: 'error', text: t('cliUpdateFailed', { error: result.error || 'Unknown error' }) });
        setTimeout(() => setMessage({ type: '', text: '' }), 5000);
      }
    } catch (e: any) {
      setMessage({ type: 'error', text: t('cliUpdateFailed', { error: e.toString() }) });
      setTimeout(() => setMessage({ type: '', text: '' }), 5000);
    } finally {
      setUpdatingTool(prev => ({ ...prev, [tool]: false }));
    }
  };

  return { handleApplyCliUpdate };
}
