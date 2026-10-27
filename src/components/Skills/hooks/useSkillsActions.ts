import { useMemo } from 'react';
import { emit } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { runUserAction } from '@/lib/userActions';
import { buildUninstallSkillActionDescriptor } from '@/lib/actionDescriptors/skills';
import {
  skillsCatalogDetailGet,
  skillsCatalogOpenFolder,
  skillsDetailGet,
  skillsInstall,
  skillsOpenFolder,
  skillsRepoDelete,
  skillsRepoDetailGet,
  skillsRepoImportFolder,
  skillsRepoReloadApply,
  skillsRepoReloadPreview,
  skillsRepoSetModel,
  skillsUninstall,
} from '@/lib/skills';
import { allModels, errorContainsCode } from '../helpers';
import type {
  ApiResp,
  CatalogOpenFolderResult,
  CatalogSkill,
  CatalogSkillDetail,
  InstallTargetSkill,
  ModelType,
  ReloadApplyResult,
  ReloadPreview,
  RepoImportFolderResult,
  RepositorySkillView,
  SkillDetail,
  SkillRecord,
} from '../types';
import type { SkillsData } from './useSkillsData';

export function useSkillsActions(data: SkillsData) {
  const {
    t,
    actionContext,
    activeModel,
    setActiveModel,
    setActiveMode,
    loading,
    setLoading,
    setMessage,
    repositorySkills,
    catalogDetailData,
    catalogDetailInstallTarget,
    setCatalogDetailOpen,
    setCatalogDetailData,
    setCatalogDetailInstallTarget,
    setDetailData,
    setDetailOpen,
    reloadTargetRepoKey,
    reloadPreview,
    setReloadOpen,
    setReloadPreview,
    setReloadSelectedPath,
    setReloadTargetRepoKey,
    setReloadSubmitting,
    setReinstallingKeys,
    installTarget,
    setInstallTarget,
    installMode,
    setInstallMode,
    installModels,
    installSubmitting,
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
    toInstallTargetFromRepo,
    buildInstallStateForCatalog,
    findLatestRepository,
    resolveRepositorySkill,
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

  const installSkillToModels = async (
    item: CatalogSkill,
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
          skillsInstall({
            source_id: item.source_id,
            skill_ref: item.rel_path,
            model,
            scope: 'global',
          })
        )
      );
      await reloadAll();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      if (succeeded > 0) {
        emit('refresh-counts').catch(() => {});
        revealInstalledModels(targetModels);
      }
      if (failed.length === 0) {
        setMessage({
          type: 'success',
          text:
            succeeded === 1
              ? t('installed', 'Installed')
              : t('skillsInstallSuccessMulti', 'Installed for {{count}} models', { count: succeeded }),
        });
      } else {
        setMessage({
          type: 'error',
          text: t('skillsInstallPartialFailed', 'Installed {{success}}, failed {{failed}} ({{models}})', {
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
    item: InstallTargetSkill,
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
          skillsRepoSetModel({
            repo_key: item.repo_key!,
            model,
            enabled: true,
            scope: 'global',
          })
        )
      );
      await reloadAll();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      if (succeeded > 0) {
        emit('refresh-counts').catch(() => {});
        revealInstalledModels(targetModels);
      }
      if (failed.length === 0) {
        setMessage({
          type: 'success',
          text:
            succeeded === 1
              ? t('installed', 'Installed')
              : t('skillsInstallSuccessMulti', 'Installed for {{count}} models', { count: succeeded }),
        });
      } else {
        setMessage({
          type: 'error',
          text: t('skillsInstallPartialFailed', 'Installed {{success}}, failed {{failed}} ({{models}})', {
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
    target: InstallTargetSkill,
    mode: 'catalog' | 'repository',
    preferredModel?: ModelType
  ) => {
    const allowed = allModels.filter((model) => {
      if (!target.models.includes(model)) return false;
      return true;
    });
    if (allowed.length === 0) {
      setMessage({
        type: 'success',
        text: t('installed', 'Installed'),
      });
      return;
    }
    setInstallMode(mode);
    setInstallTarget(target);
    const preferredDefault = preferredModel || activeModel;
    const installedDefaults =
      mode === 'repository' && target.installed
        ? allowed.filter((model) => target.installed?.[model])
        : [];
    setInstallModels(
      installedDefaults.length > 0
        ? installedDefaults
        : [allowed.includes(preferredDefault) ? preferredDefault : allowed[0]]
    );
    setInstallDialogOpen(true);
  };

  const handleInstall = async (item: CatalogSkill) => {
    const allowed = allModels.filter((model) => item.models.includes(model));
    if (allowed.length === 0) {
      setMessage({
        type: 'error',
        text: t('skillsInstallUnavailableForModel', 'This skill is not available for the selected model.'),
      });
      return;
    }
    openInstallDialog(item, 'catalog');
  };

  const handleInstallRepository = (repo: RepositorySkillView) => {
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
      await installSkillToModels(installTarget, installModels);
    }
    setInstallDialogOpen(false);
    setInstallTarget(null);
    setInstallMode('catalog');
    setInstallModels([]);
  };
  const handleInstallFromCatalogDetail = async () => {
    if (catalogDetailInstallTarget) {
      setCatalogDetailOpen(false);
      const latestRepo = await resolveRepositorySkill(catalogDetailInstallTarget);
      openInstallDialog(
        latestRepo ? toInstallTargetFromRepo(latestRepo) : catalogDetailInstallTarget,
        'repository'
      );
      return;
    }
    if (!catalogDetailData) return;
    setCatalogDetailOpen(false);
    await handleInstall(catalogDetailData.skill);
  };
  const handleSwitchToRecommended = () => {
    setActiveMode('recommended');
    setActiveModel('claude');
  };
  const handleSwitchToRepository = () => {
    setActiveMode('repository');
    setActiveModel('claude');
  };

  const handleUninstall = async (skill: SkillRecord) => {
    try {
      setLoading(true);
      await runUserAction(
        actionContext,
        buildUninstallSkillActionDescriptor(t, {
          model: skill.model,
          id: skill.id,
          name: skill.name,
        }),
        () =>
          skillsUninstall({
            model: skill.model,
            skill_id: skill.id,
            scope: 'global',
          }),
      );
      setDetailOpen(false);
      await reloadAll();
      emit('refresh-counts').catch(() => {});
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleReinstall = async (skill: SkillRecord) => {
    const ok = await confirmDialog(t('skillsReinstallConfirm', '是否使用仓库中最新内容重新安装并覆盖？'), {
      okLabel: t('ok', 'OK'),
      cancelLabel: t('cancel', 'Cancel'),
    });
    if (!ok) return;

    const matchedRepo = await resolveRepositorySkill({
      source_id: skill.source_id,
      source_rel_path: skill.source_rel_path,
      id: skill.id,
      dir_name: skill.dir_name,
    });
    if (!matchedRepo) {
      setMessage({
        type: 'error',
        text: t('skillsReinstallRepoNotFound', 'Repository snapshot not found for this skill.'),
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
          source: 'skills',
          category: 'apply',
          action: 'reinstall-skill',
          target: { tab: 'skills', entity_id: skill.id },
          dedupeKey: `skills:reinstall:${skill.model}:${skill.id}`,
          success: {
            title: t('skillsReinstallSuccess', 'Skill reinstalled successfully.'),
            summary: t('skillsReinstallSuccess', 'Skill reinstalled successfully.'),
          },
          error: {
            title: t('skillsReinstallFailed', 'Reinstall failed: {{message}}'),
          },
        },
        () =>
          skillsRepoSetModel({
            repo_key: matchedRepo.repo_key,
            model: skill.model,
            enabled: true,
            scope: 'global',
          }),
      );
      await reloadAll();
      emit('refresh-counts').catch(() => {});
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('skillsReinstallFailed', 'Reinstall failed: {{message}}', { message: String(e) }),
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

  const handleDeleteRepository = async (repo: RepositorySkillView) => {
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
          source: 'skills',
          category: 'delete',
          action: 'delete-repository-skill',
          target: { tab: 'skills', entity_id: repo.repo_key },
          dedupeKey: `skills:repo-delete:${repo.repo_key}`,
          success: {
            title: t('skillsRepositoryDeleteSuccess', 'Repository skill deleted'),
            summary: t('skillsRepositoryDeleteSuccess', 'Repository skill deleted'),
          },
          error: {
            title: t('skillsRepositoryDeleteFailed', 'Failed to delete repository skill'),
          },
        },
        () => skillsRepoDelete({ repo_key: repo.repo_key }),
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

  const handleOpenDetail = async (skill: SkillRecord) => {
    try {
      const res = await skillsDetailGet<ApiResp<SkillDetail>>({
        model: skill.model,
        skill_id: skill.id,
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

  const handleOpenCatalogDetail = async (item: CatalogSkill) => {
    try {
      const res = await skillsCatalogDetailGet<ApiResp<CatalogSkillDetail>>({
        source_id: item.source_id,
        skill_ref: item.rel_path,
      });
      const matchedRepo = repositorySkills.find(
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

  const handleOpenRepositoryDetail = async (repo: RepositorySkillView) => {
    try {
      const res = await skillsRepoDetailGet<ApiResp<CatalogSkillDetail>>({
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
      const res = await skillsCatalogOpenFolder<ApiResp<CatalogOpenFolderResult>>({
        source_id: catalogDetailData.skill.source_id,
        skill_ref: catalogDetailData.skill.rel_path,
      });
      setCatalogDetailInstallTarget((prev) => ({
        source_id: catalogDetailData.skill.source_id,
        id: catalogDetailData.skill.id,
        rel_path: catalogDetailData.skill.rel_path,
        name: catalogDetailData.skill.name,
        description: catalogDetailData.skill.description,
        models: catalogDetailData.skill.models,
        repo_key: res.data.repo_key,
        installed: prev?.installed || buildInstallStateForCatalog(catalogDetailData.skill),
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

  const openReloadPreviewByRepoKey = async (repoKey: string) => {
    if (!repoKey) return;
    try {
      setLoading(true);
      const res = await skillsRepoReloadPreview<ApiResp<ReloadPreview>>({
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
        text: t('skillsReloadPreviewFailed', 'Reload preview failed: {{message}}', { message: String(e) }),
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
    const shouldSync = (reloadPreview.installed_targets || []).length > 0;
    try {
      setLoading(true);
      setReloadSubmitting(true);
      const res = await runUserAction(
        actionContext,
        {
          source: 'skills',
          category: 'apply',
          action: 'reload-repository-skill',
          target: { tab: 'skills', entity_id: reloadTargetRepoKey },
          dedupeKey: `skills:reload:${reloadTargetRepoKey}`,
          success: false,
          error: {
            title: t('skillsReloadApplyFailed', 'Reload apply failed: {{message}}'),
          },
        },
        () =>
          skillsRepoReloadApply<ApiResp<ReloadApplyResult>>({
            repo_key: reloadTargetRepoKey,
            sync_to_models: shouldSync,
          }),
      );
      if (!res) return;
      const result = res.data;
      await reloadAll();
      if ((result.synced_targets || []).length > 0) {
        setMessage({
          type: 'success',
          text: t(
            'skillsReloadAppliedSynced',
            'Repository skill updated and synced to {{count}} installed targets.',
            {
              count: result.synced_targets.length,
            }
          ),
        });
      } else if (result.updated_files_count > 0) {
        setMessage({
          type: 'success',
          text: t(
            'skillsReloadAppliedIndexOnly',
            'Repository skill updated successfully.',
            {
              count: result.updated_files_count,
            }
          ),
        });
      } else {
        setMessage({
          type: 'success',
          text: t('skillsReloadAppliedNoOp', 'Repository skill is already up to date.'),
        });
      }
      setReloadOpen(false);
      setReloadPreview(null);
      setReloadSelectedPath('');
      setReloadTargetRepoKey(null);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('skillsReloadApplyFailed', 'Reload apply failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setReloadSubmitting(false);
      setLoading(false);
    }
  };

  const handleOpenFolder = async (skill: SkillRecord) => {
    try {
      await skillsOpenFolder({
        model: skill.model,
        skill_id: skill.id,
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
          source: 'skills',
          category: 'import',
          action: 'import-repository-folder',
          target: { tab: 'skills' },
          dedupeKey: `skills:import:${selected}`,
          success: {
            title: t('skillsLocalImportRepoSuccess', 'Skill imported to repository.'),
            summary: t('skillsLocalImportRepoSuccess', 'Skill imported to repository.'),
          },
          error: {
            title: t('skillsLocalImportFailed', 'Import failed: {{message}}'),
          },
        },
        () => skillsRepoImportFolder<ApiResp<RepoImportFolderResult>>({ folder_path: selected }),
      );
      await Promise.all([loadRepository(true), loadSyncState(), loadDisplayConfig()]);
    } catch (e: any) {
      if (errorContainsCode(e, 'skills/import_busy')) {
        setMessage({
          type: 'error',
          text: t('skillsLocalImportBusy', 'Import task is running. Please try again later.'),
        });
        return;
      }
      setMessage({
        type: 'error',
        text: t('skillsLocalImportFailed', 'Import failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  return {
    installAllowedModels,
    canSubmitInstall,
    handleSyncSources,
    installSkillToModels,
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
    openReloadPreviewByRepoKey,
    handleOpenReloadPreview,
    handleApplyReload,
    handleOpenFolder,
    handleImportRepositoryFolder,
  };
}

export type SkillsActions = ReturnType<typeof useSkillsActions>;
