import type { Dispatch, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { emit } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  serviceProvidersExport,
  serviceProvidersImportApply,
  serviceProvidersImportPreview,
  type ProviderImportDecision,
  type ProvidersImportPreview,
} from '@/lib/serviceProviders';
import { notifyMessage } from '@/lib/messages';
import { runUserAction, type ActionContext } from '@/lib/userActions';

interface UseProviderImportExportActionsParams {
  t: TFunction;
  actionContext: ActionContext;
  isTauri: boolean;
  pushToast: ActionContext['pushToast'];
  setMessage: Dispatch<SetStateAction<{ type: string; text: string }>>;
  setExportingProviders: Dispatch<SetStateAction<boolean>>;
  setPreviewingImport: Dispatch<SetStateAction<boolean>>;
  setApplyingImport: Dispatch<SetStateAction<boolean>>;
  importPreview: ProvidersImportPreview | null;
  setImportPreview: Dispatch<SetStateAction<ProvidersImportPreview | null>>;
  importPath: string | null;
  setImportPath: Dispatch<SetStateAction<string | null>>;
  importDecisions: Record<string, 'overwrite' | 'new'>;
  setImportDecisions: Dispatch<SetStateAction<Record<string, 'overwrite' | 'new'>>>;
  loadProviders: (silent?: boolean) => Promise<void>;
}

export function useProviderImportExportActions({
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
  loadProviders,
}: UseProviderImportExportActionsParams) {
  const closeImportModal = () => {
    setImportPreview(null);
    setImportPath(null);
    setImportDecisions({});
  };

  const handleExportProviders = async () => {
    if (!isTauri) return;
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const outputPath = await save({
        defaultPath: `onespace-service-providers-${stamp}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;

      setExportingProviders(true);
      const res = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'export',
          action: 'export-providers',
          target: { tab: 'ai-environments' },
          dedupeKey: 'ai-environments:export',
          metadata: { output_path: outputPath },
          confirm: {
            message: t('providersExportConfirm', 'Export all Service Providers to the selected JSON file?'),
            title: t('providersExportConfirmTitle', 'Export Service Providers'),
            okLabel: t('export', 'Export'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('providersExportSuccessTitle', 'Service Providers exported'),
          },
          error: {
            title: t('providersExportFailedTitle', 'Failed to export Service Providers'),
          },
        },
        () => serviceProvidersExport(outputPath),
      );
      if (res === null) return;
      const successText = t('providersExportSuccess', {
        count: res.data?.count ?? 0,
        path: res.data?.path || outputPath,
        defaultValue: 'Exported {{count}} Service Provider(s) to {{path}}',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      const errorText = t('providersExportFailed', {
        error: String(e),
        defaultValue: 'Failed to export Service Providers: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setExportingProviders(false);
    }
  };

  const handleImportProviders = async () => {
    if (!isTauri) return;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!selected || Array.isArray(selected)) return;

      setPreviewingImport(true);
      const selectedPath = selected as string;
      const res = await serviceProvidersImportPreview(selectedPath);
      if (!res.data?.items?.length) {
        const emptyText = t('providersImportEmpty', 'No Service Providers found in the selected file.');
        await notifyMessage.warn({
          source: 'ai_environments',
          category: 'import',
          title: t('providersImportEmptyTitle', 'Import file is empty'),
          summary: emptyText,
          dedupeKey: 'ai-environments:import-preview-empty',
          target: { tab: 'ai-environments' },
          metadata: { import_path: selectedPath },
        });
        pushToast({
          title: t('providersImportEmptyTitle', 'Import file is empty'),
          description: emptyText,
          kind: 'warning',
        });
        setMessage({ type: 'error', text: emptyText });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
        return;
      }

      const defaults = (res.data.items || []).reduce((acc, item) => {
        if (item.conflict) {
          acc[item.import_key] = 'overwrite';
        }
        return acc;
      }, {} as Record<string, 'overwrite' | 'new'>);
      setImportDecisions(defaults);
      setImportPath(selectedPath);
      setImportPreview(res.data);
    } catch (e: any) {
      const errorText = t('providersImportPreviewFailed', {
        error: String(e),
        defaultValue: 'Failed to read import file: {{error}}',
      });
      await notifyMessage.error({
        source: 'ai_environments',
        category: 'import',
        title: t('providersImportPreviewFailedTitle', 'Failed to preview import'),
        summary: errorText,
        error: e,
        dedupeKey: 'ai-environments:import-preview-failed',
        target: { tab: 'ai-environments' },
      });
      pushToast({
        title: t('providersImportPreviewFailedTitle', 'Failed to preview import'),
        description: String(e),
        kind: 'error',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setPreviewingImport(false);
    }
  };

  const handleSetAllImportConflictActions = (action: 'overwrite' | 'new') => {
    if (!importPreview) return;
    const next = { ...importDecisions };
    for (const item of importPreview.items) {
      if (item.conflict) {
        next[item.import_key] = action;
      }
    }
    setImportDecisions(next);
  };

  const handleApplyImport = async () => {
    if (!importPreview || !importPath) return;
    try {
      setApplyingImport(true);
      const decisions: ProviderImportDecision[] = importPreview.items
        .filter(item => item.conflict)
        .map(item => ({
          import_key: item.import_key,
          action: importDecisions[item.import_key] || 'overwrite',
        }));

      const res = await runUserAction(
        actionContext,
        {
          source: 'ai_environments',
          category: 'import',
          action: 'apply-provider-import',
          target: { tab: 'ai-environments' },
          dedupeKey: 'ai-environments:import-apply',
          metadata: { import_path: importPath, decisions },
          confirm: {
            message: t(
              'providersImportConfirm',
              'Apply the selected import actions and update existing Service Providers where required?',
            ),
            title: t('providersImportConfirmTitle', 'Import Service Providers'),
            okLabel: t('import', 'Import'),
            cancelLabel: t('cancel', 'Cancel'),
            kind: 'warning',
          },
          success: {
            title: t('providersImportAppliedTitle', 'Service Providers imported'),
          },
          error: {
            title: t('providersImportApplyFailedTitle', 'Failed to import Service Providers'),
          },
        },
        () => serviceProvidersImportApply(importPath, decisions),
      );
      if (res === null) return;

      await loadProviders(true);
      emit('refresh-counts');
      closeImportModal();

      const successText = t('providersImportApplied', {
        imported: res.data?.imported ?? 0,
        overwritten: res.data?.overwritten ?? 0,
        created: res.data?.created ?? 0,
        activeRestored: res.data?.active_restored ?? 0,
        defaultValue:
          'Imported {{imported}} Service Provider(s): {{overwritten}} overwritten, {{created}} created, {{activeRestored}} active binding(s) restored.',
      });
      setMessage({ type: 'success', text: successText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      const errorText = t('providersImportApplyFailed', {
        error: String(e),
        defaultValue: 'Failed to import Service Providers: {{error}}',
      });
      setMessage({ type: 'error', text: errorText });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } finally {
      setApplyingImport(false);
    }
  };

  const importConflictItems = importPreview?.items.filter(item => item.conflict) || [];
  const importNewItems = importPreview?.items.filter(item => !item.conflict) || [];

  return {
    closeImportModal,
    handleExportProviders,
    handleImportProviders,
    handleSetAllImportConflictActions,
    handleApplyImport,
    importConflictItems,
    importNewItems,
  };
}
