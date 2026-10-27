import { useMemo } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { runUserAction } from '@/lib/userActions';
import { buildUninstallSubagentActionDescriptor } from '@/lib/actionDescriptors/subagents';
import {
  subagentsCatalogDetailGet,
  subagentsCatalogOpenFolder,
  subagentsDetailGet,
  subagentsInstall,
  subagentsOpenFolder,
  subagentsRepoDelete,
  subagentsRepoDetailGet,
  subagentsRepoImportFolder,
  subagentsRepoReloadApply,
  subagentsRepoReloadPreview,
  subagentsRepoSetModel,
  subagentsUninstall,
  subagentsUpdateApply,
  subagentsUpdateDiffPreview,
} from '@/lib/subagents';
import { allModels, errorContainsCode } from '../helpers';
import type {
  ApiResp,
  CatalogOpenFolderResult,
  CatalogSubagent,
  CatalogSubagentDetail,
  InstallTargetSubagent,
  ModelType,
  ReloadApplyResult,
  ReloadPreview,
  RepoImportFolderResult,
  RepositorySubagentView,
  SubagentDetail,
  SubagentRecord,
  UpdateDiff,
} from '../types';
import type { SubagentsData } from './useSubagentsData';

export function useSubagentsActions(data: SubagentsData) {
  const {
    t,
    actionContext,
    activeModel,
    setActiveModel,
    setActiveMode,
    loading,
    setLoading,
    setMessage,
    repositorySubagents,
    catalogDetailData,
    catalogDetailInstallTarget,
    setCatalogDetailOpen,
    setCatalogDetailData,
    setCatalogDetailInstallTarget,
    setDetailData,
    setDetailOpen,
    diffSubagent,
    setDiffData,
    setDiffSubagent,
    setDiffOpen,
    reloadTargetRepoKey,
    reloadPreview,
    setReloadOpen,
    setReloadPreview,
    setReloadSelectedPath,
    setReloadTargetRepoKey,
    setReloadSubmitting,
    setReinstallingKeys,
    installTarget,
    installSubmitting,
    setInstallTarget,
    installMode,
    setInstallMode,
    installModels,
    setInstallModels,
    setInstallDialogOpen,
    setInstallSubmitting,
    confirmDialog,
    reloadAll,
    loadRepository,
    loadSyncState,
    loadDisplayConfig,
    triggerSyncSources,
    revealInstalledModels,
    notifyCountsChanged,
    toInstallTargetFromRepo,
    buildInstallStateForCatalog,
    findLatestRepository,
    resolveRepositorySubagent,
  } = data;

  const installAllowedModels = useMemo(
    () =>
      installTarget
        ? allModels.filter((model) => {
            if (!installTarget.models.includes(model)) return false;
            return true;
          })
        : [],
    [installTarget]
  );

  const handleSyncSources = async () => {
    await triggerSyncSources(true);
  };

  const installSubagentToModels = async (
    item: CatalogSubagent,
    selectedModels: ModelType[],
  ) => {
    const targetModels = allModels.filter((model) => item.models.includes(model) && selectedModels.includes(model));
    if (targetModels.length === 0) {
      setMessage({
        type: 'error',
        text: t('sourceModelsRequired', 'Select at least one model.'),
      });
      return;
    }
    try {
      setLoading(true);
      setInstallSubmitting(true);
      const results = await Promise.allSettled(
        targetModels.map((model) =>
          subagentsInstall({
            source_id: item.source_id,
            subagent_ref: item.rel_path,
            model,
            scope: 'global',
          })
        )
      );
      await reloadAll();
      notifyCountsChanged();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      if (succeeded > 0) {
        revealInstalledModels(targetModels);
      }
      if (failed.length === 0) {
        setMessage({
          type: 'success',
          text:
            succeeded === 1
              ? t('installed', 'Installed')
              : t('subagentsInstallSuccessMulti', 'Installed for {{count}} models', { count: succeeded }),
        });
      } else {
        setMessage({
          type: 'error',
          text: t('subagentsInstallPartialFailed', 'Installed {{success}}, failed {{failed}} ({{models}})', {
            success: succeeded,
            failed: failed.length,
            models: failed.join(', '),
          }),
        });
      }
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setInstallSubmitting(false);
      setLoading(false);
    }
  };

  const installRepositoryToModels = async (
    item: InstallTargetSubagent,
    selectedModels: ModelType[],
  ) => {
    if (!item.repo_key) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: 'Missing repository key' }),
      });
      return;
    }

    const targetModels = allModels.filter((model) => item.models.includes(model) && selectedModels.includes(model));
    if (targetModels.length === 0) {
      setMessage({
        type: 'error',
        text: t('sourceModelsRequired', 'Select at least one model.'),
      });
      return;
    }

    try {
      setLoading(true);
      setInstallSubmitting(true);
      const results = await Promise.allSettled(
        targetModels.map((model) =>
          subagentsRepoSetModel({
            repo_key: item.repo_key!,
            model,
            enabled: true,
            scope: 'global',
          })
        )
      );
      await reloadAll();
      notifyCountsChanged();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      if (succeeded > 0) {
        revealInstalledModels(targetModels);
      }
      if (failed.length === 0) {
        setMessage({
          type: 'success',
          text:
            succeeded === 1
              ? t('installed', 'Installed')
              : t('subagentsInstallSuccessMulti', 'Installed for {{count}} models', { count: succeeded }),
        });
      } else {
        setMessage({
          type: 'error',
          text: t('subagentsInstallPartialFailed', 'Installed {{success}}, failed {{failed}} ({{models}})', {
            success: succeeded,
            failed: failed.length,
            models: failed.join(', '),
          }),
        });
      }
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setInstallSubmitting(false);
      setLoading(false);
    }
  };

  const openInstallDialog = (
    target: InstallTargetSubagent,
    mode: 'catalog' | 'repository',
    preferredModel?: ModelType
  ) => {
    const allowed = allModels.filter((model) => target.models.includes(model));
    if (allowed.length === 0) {
      setMessage({
        type: 'success',
        text: t('installed', 'Installed'),
      });
      return;
    }
    setInstallMode(mode);
    setInstallTarget(target);
    setInstallModels([allowed.includes(preferredModel || activeModel) ? (preferredModel || activeModel) : allowed[0]]);
    setInstallDialogOpen(true);
  };

  const handleInstall = async (item: CatalogSubagent) => {
    const allowed = allModels.filter((model) => item.models.includes(model));
    if (allowed.length === 0) {
      setMessage({
        type: 'error',
        text: t('subagentsInstallUnavailableForModel', 'This subagent is not available for the selected model.'),
      });
      return;
    }
    openInstallDialog(item, 'catalog');
  };

  const handleInstallRepository = (repo: RepositorySubagentView) => {
    const latest = findLatestRepository(repo) || repo;
    openInstallDialog(toInstallTargetFromRepo(latest), 'repository');
  };

  const canSubmitInstall = installAllowedModels.length > 0 && installModels.length > 0 && !installSubmitting && !loading;
  const toggleInstallModel = (model: ModelType) => {
    if (!installAllowedModels.includes(model)) return;
    setInstallModels((prev) => {
      if (prev.includes(model)) {
        return prev.filter((m) => m !== model);
      }
      return [...prev, model];
    });
  };
  const handleInstallConfirm = async () => {
    if (!installTarget || installModels.length === 0) return;
    if (installMode === 'repository') {
      await installRepositoryToModels(installTarget, installModels);
    } else {
      await installSubagentToModels(installTarget, installModels);
    }
    setInstallDialogOpen(false);
    setInstallTarget(null);
    setInstallMode('catalog');
    setInstallModels([]);
  };
  const handleInstallFromCatalogDetail = async () => {
    if (catalogDetailInstallTarget) {
      setCatalogDetailOpen(false);
      const latestRepo = await resolveRepositorySubagent(catalogDetailInstallTarget);
      openInstallDialog(
        latestRepo ? toInstallTargetFromRepo(latestRepo) : catalogDetailInstallTarget,
        'repository'
      );
      return;
    }
    if (!catalogDetailData) return;
    setCatalogDetailOpen(false);
    await handleInstall(catalogDetailData.subagent);
  };
  const handleSwitchToRecommended = () => {
    setActiveMode('recommended');
    setActiveModel('claude');
  };
  const handleSwitchToRepository = () => {
    setActiveMode('repository');
    setActiveModel('claude');
  };

  const handleUninstall = async (skill: SubagentRecord) => {
    try {
      setLoading(true);
      await runUserAction(
        actionContext,
        buildUninstallSubagentActionDescriptor(t, {
          model: skill.model,
          id: skill.id,
          name: skill.name,
        }),
        () =>
          subagentsUninstall({
            model: skill.model,
            subagent_id: skill.id,
            scope: 'global',
          }),
      );
      setDetailOpen(false);
      setDiffOpen(false);
      await reloadAll();
      notifyCountsChanged();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleReinstall = async (skill: SubagentRecord) => {
    const ok = await confirmDialog(t('subagentsReinstallConfirm', '是否使用仓库中最新内容重新安装并覆盖？'), {
      okLabel: t('ok', 'OK'),
      cancelLabel: t('cancel', 'Cancel'),
    });
    if (!ok) return;

    const matchedRepo = await resolveRepositorySubagent({
      source_id: skill.source_id,
      source_rel_path: skill.source_rel_path,
      id: skill.id,
      dir_name: skill.dir_name,
    });
    if (!matchedRepo) {
      setMessage({
        type: 'error',
        text: t('subagentsReinstallRepoNotFound', 'Repository snapshot not found for this subagent.'),
      });
      return;
    }

    const reinstallKey = `${skill.model}:${skill.id}`;
    setReinstallingKeys((prev) => ({ ...prev, [reinstallKey]: true }));
    try {
      setLoading(true);
      await runUserAction(
        actionContext,
        {
          source: 'subagents',
          category: 'apply',
          action: 'reinstall-subagent',
          target: { tab: 'subagents', entity_id: skill.id },
          dedupeKey: `subagents:reinstall:${skill.model}:${skill.id}`,
          success: {
            title: t('subagentsReinstallSuccess', 'Subagent reinstalled successfully.'),
            summary: t('subagentsReinstallSuccess', 'Subagent reinstalled successfully.'),
          },
          error: {
            title: t('subagentsReinstallFailed', 'Reinstall failed: {{message}}'),
          },
        },
        () =>
          subagentsRepoSetModel({
            repo_key: matchedRepo.repo_key,
            model: skill.model,
            enabled: true,
            scope: 'global',
          }),
      );
      await reloadAll();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('subagentsReinstallFailed', 'Reinstall failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
      setReinstallingKeys((prev) => {
        const next = { ...prev };
        delete next[reinstallKey];
        return next;
      });
    }
  };

  const handleDeleteRepository = async (repo: RepositorySubagentView) => {
    const ok = await confirmDialog(t('confirmDelete', { name: repo.name }), {
      okLabel: t('delete', 'Delete'),
      cancelLabel: t('cancel', 'Cancel'),
    });
    if (!ok) return;

    try {
      setLoading(true);
      await runUserAction(
        actionContext,
        {
          source: 'subagents',
          category: 'delete',
          action: 'delete-repository-subagent',
          target: { tab: 'subagents', entity_id: repo.repo_key },
          dedupeKey: `subagents:repo-delete:${repo.repo_key}`,
          success: {
            title: t('subagentsRepositoryDeleteSuccess', 'Repository subagent deleted'),
            summary: t('subagentsRepositoryDeleteSuccess', 'Repository subagent deleted'),
          },
          error: {
            title: t('subagentsRepositoryDeleteFailed', 'Failed to delete repository subagent'),
          },
        },
        () => subagentsRepoDelete({ repo_key: repo.repo_key }),
      );
      await reloadAll();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenDetail = async (skill: SubagentRecord) => {
    try {
      const res = await subagentsDetailGet<ApiResp<SubagentDetail>>({
        model: skill.model,
        subagent_id: skill.id,
        scope: 'global',
      });
      setDetailData(res.data);
      setDetailOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const handleOpenCatalogDetail = async (item: CatalogSubagent) => {
    try {
      const res = await subagentsCatalogDetailGet<ApiResp<CatalogSubagentDetail>>({
        source_id: item.source_id,
        subagent_ref: item.rel_path,
      });
      const matchedRepo = repositorySubagents.find(
        (repo) => repo.source_id === item.source_id && repo.source_rel_path === item.rel_path
      );
      setCatalogDetailInstallTarget(matchedRepo ? toInstallTargetFromRepo(matchedRepo) : null);
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const handleOpenRepositoryDetail = async (repo: RepositorySubagentView) => {
    try {
      const res = await subagentsRepoDetailGet<ApiResp<CatalogSubagentDetail>>({
        repo_key: repo.repo_key,
      });
      setCatalogDetailInstallTarget(toInstallTargetFromRepo(repo));
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const handleOpenCatalogFolder = async () => {
    if (!catalogDetailData) return;
    try {
      setLoading(true);
      const res = await subagentsCatalogOpenFolder<ApiResp<CatalogOpenFolderResult>>({
        source_id: catalogDetailData.subagent.source_id,
        subagent_ref: catalogDetailData.subagent.rel_path,
      });
      setCatalogDetailInstallTarget((prev) => ({
        source_id: catalogDetailData.subagent.source_id,
        id: catalogDetailData.subagent.id,
        rel_path: catalogDetailData.subagent.rel_path,
        name: catalogDetailData.subagent.name,
        description: catalogDetailData.subagent.description,
        models: catalogDetailData.subagent.models,
        repo_key: res.data.repo_key,
        installed: prev?.installed || buildInstallStateForCatalog(catalogDetailData.subagent),
      }));
      await reloadAll();
      setMessage({ type: 'success', text: t('openFolder', 'Open Folder') });
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenDiff = async (skill: SubagentRecord) => {
    try {
      const res = await subagentsUpdateDiffPreview<ApiResp<UpdateDiff>>({
        model: skill.model,
        subagent_id: skill.id,
        scope: 'global',
      });
      setDiffData(res.data);
      setDiffSubagent(skill);
      setDiffOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const openReloadPreviewByRepoKey = async (repoKey: string) => {
    if (!repoKey) return;
    try {
      setLoading(true);
      const res = await subagentsRepoReloadPreview<ApiResp<ReloadPreview>>({
        repo_key: repoKey,
      });
      const preview = res.data;
      setReloadPreview(preview);
      setReloadTargetRepoKey(repoKey);
      const preferredPath =
        preview?.text_diffs?.[0]?.path ||
        preview?.changed_files?.find((item) => !item.is_binary)?.path ||
        preview?.changed_files?.[0]?.path ||
        '';
      setReloadSelectedPath(preferredPath);
      setReloadOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('subagentsReloadPreviewFailed', 'Reload preview failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenReloadPreview = async () => {
    const repoKey = catalogDetailInstallTarget?.repo_key;
    if (!repoKey) return;
    await openReloadPreviewByRepoKey(repoKey);
  };

  const handleApplyReload = async () => {
    if (!reloadTargetRepoKey || !reloadPreview) return;
    const shouldSync = (reloadPreview.installed_models || []).length > 0;
    try {
      setLoading(true);
      setReloadSubmitting(true);
      const res = await runUserAction(
        actionContext,
        {
          source: 'subagents',
          category: 'apply',
          action: 'reload-repository-subagent',
          target: { tab: 'subagents', entity_id: reloadTargetRepoKey },
          dedupeKey: `subagents:reload:${reloadTargetRepoKey}`,
          success: false,
          error: {
            title: t('subagentsReloadApplyFailed', 'Reload apply failed: {{message}}'),
          },
        },
        () =>
          subagentsRepoReloadApply<ApiResp<ReloadApplyResult>>({
            repo_key: reloadTargetRepoKey,
            sync_to_models: shouldSync,
          }),
      );
      if (!res) return;
      const result = res.data;
      await reloadAll();
      if (result.synced_models.length > 0) {
        setMessage({
          type: 'success',
          text: t('subagentsReloadAppliedSynced', 'Index refreshed and synced to {{models}}', {
            models: result.synced_models.join(', '),
          }),
        });
      } else {
        setMessage({
          type: 'success',
          text: t('subagentsReloadAppliedIndexOnly', 'Index refreshed successfully.'),
        });
      }
      setReloadOpen(false);
      setReloadPreview(null);
      setReloadSelectedPath('');
      setReloadTargetRepoKey(null);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('subagentsReloadApplyFailed', 'Reload apply failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setReloadSubmitting(false);
      setLoading(false);
    }
  };

  const handleApplyUpdate = async () => {
    if (!diffSubagent) return;
    try {
      setLoading(true);
      await runUserAction(
        actionContext,
        {
          source: 'subagents',
          category: 'apply',
          action: 'apply-subagent-update',
          target: { tab: 'subagents', entity_id: diffSubagent.id },
          dedupeKey: `subagents:update-apply:${diffSubagent.model}:${diffSubagent.id}`,
          success: {
            title: t('subagentsUpdateApplySuccess', 'Subagent updated successfully.'),
            summary: t('subagentsUpdateApplySuccess', 'Subagent updated successfully.'),
          },
          error: {
            title: t('subagentsUpdateApplyFailed', 'Failed to apply subagent update'),
          },
        },
        () =>
          subagentsUpdateApply({
            model: diffSubagent.model,
            subagent_id: diffSubagent.id,
            scope: 'global',
          }),
      );
      setDiffOpen(false);
      await reloadAll();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenFolder = async (skill: SubagentRecord) => {
    try {
      await subagentsOpenFolder({
        model: skill.model,
        subagent_id: skill.id,
        scope: 'global',
      });
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const handleImportRepositoryFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (!selected || typeof selected !== 'string') {
        return;
      }

      setLoading(true);
      await runUserAction(
        actionContext,
        {
          source: 'subagents',
          category: 'import',
          action: 'import-repository-folder',
          target: { tab: 'subagents' },
          dedupeKey: `subagents:import:${selected}`,
          success: {
            title: t('subagentsLocalImportRepoSuccess', 'Subagent imported to repository.'),
            summary: t('subagentsLocalImportRepoSuccess', 'Subagent imported to repository.'),
          },
          error: {
            title: t('subagentsLocalImportFailed', 'Import failed: {{message}}'),
          },
        },
        () =>
          subagentsRepoImportFolder<ApiResp<RepoImportFolderResult>>({
            folder_path: selected,
          }),
      );
      await Promise.all([loadRepository(true), loadSyncState(), loadDisplayConfig()]);
    } catch (e: any) {
      if (errorContainsCode(e, 'subagents/import_busy')) {
        setMessage({
          type: 'error',
          text: t('subagentsLocalImportBusy', 'Import task is running. Please try again later.'),
        });
        return;
      }
      setMessage({
        type: 'error',
        text: t('subagentsLocalImportFailed', 'Import failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  return {
    installAllowedModels,
    canSubmitInstall,
    handleSyncSources,
    installSubagentToModels,
    installRepositoryToModels,
    openInstallDialog,
    handleInstall,
    handleInstallRepository,
    toggleInstallModel,
    handleInstallConfirm,
    handleInstallFromCatalogDetail,
    handleSwitchToRecommended,
    handleSwitchToRepository,
    handleUninstall,
    handleReinstall,
    handleDeleteRepository,
    handleOpenDetail,
    handleOpenCatalogDetail,
    handleOpenRepositoryDetail,
    handleOpenCatalogFolder,
    handleOpenDiff,
    openReloadPreviewByRepoKey,
    handleOpenReloadPreview,
    handleApplyReload,
    handleApplyUpdate,
    handleOpenFolder,
    handleImportRepositoryFolder,
  };
}

export type SubagentsActions = ReturnType<typeof useSubagentsActions>;
