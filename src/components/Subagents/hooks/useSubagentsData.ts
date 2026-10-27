import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { emit } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { useConfirmDialog } from '../../ConfirmDialogProvider';
import { safeRecordMessage } from '@/lib/messages';
import { runUserAction } from '@/lib/userActions';
import {
  subagentsListCatalog,
  subagentsListInstalled,
  subagentsRepoList,
  subagentsRescanMirror,
  subagentsSyncNow,
  subagentsSyncStatusGet,
} from '@/lib/subagents';
import { allModels, catalogSupportsModel, iconPool, TAB_LOADING_MIN_MS } from '../helpers';
import type {
  ApiResp,
  CatalogSubagent,
  CatalogSubagentDetail,
  InstallTargetSubagent,
  ModelType,
  ReloadPreview,
  ReloadTextDiff,
  RepoModelInstallState,
  RepositorySubagentView,
  SubagentDetail,
  SubagentRecord,
  SubagentsSyncState,
  SourceSyncState,
  StorageConfigLite,
  UpdateDiff,
} from '../types';

export function useSubagentsData({
  isVisible = true,
  initialEntry,
  onConsumeInitialEntry,
}: {
  isVisible?: boolean;
  initialEntry?: 'installed' | 'recommended' | 'repository';
  onConsumeInitialEntry?: () => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast: (_toast: {
        title: string;
        description?: string;
        kind?: 'info' | 'success' | 'warning' | 'error' | 'loading';
        durationMs?: number;
      }) => {
        setMessage({
          type: _toast.kind === 'error' ? 'error' : 'success',
          text: _toast.description || _toast.title,
        });
        return 'subagents-inline-toast';
      },
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, t],
  );

  const iconCache = useRef<Record<string, ComponentType<{ className?: string }>>>({});
  const pickIcon = (seed: string) => {
    if (iconCache.current[seed]) return iconCache.current[seed];
    const sum = seed.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
    const Icon = iconPool[sum % iconPool.length];
    iconCache.current[seed] = Icon;
    return Icon;
  };

  const [activeModel, setActiveModel] = useState<ModelType>('claude');
  const [activeMode, setActiveMode] = useState<'recommended' | 'repository' | 'installed'>(
    initialEntry || 'recommended',
  );
  const [recommendedSourceFilter, setRecommendedSourceFilter] = useState<'all' | string>('all');
  const [recommendedSearch, setRecommendedSearch] = useState('');
  const [repositorySourceFilter, setRepositorySourceFilter] = useState<'all' | 'local' | 'remote'>('all');
  const [repositorySearch, setRepositorySearch] = useState('');
  const [installedByModel, setInstalledByModel] = useState<Record<ModelType, SubagentRecord[]>>({
    claude: [],
    antigravity: [],
    codex: [],
    opencode: [],
  });
  const [catalog, setCatalog] = useState<CatalogSubagent[]>([]);
  const [repositorySubagents, setRepositorySubagents] = useState<RepositorySubagentView[]>([]);
  const [syncState, setSyncState] = useState<SubagentsSyncState | null>(null);
  const [sourceNamesById, setSourceNamesById] = useState<Record<string, string>>({});
  const [newSubagentBadgeHours, setNewSubagentBadgeHours] = useState(72);
  const [hasConfiguredSources, setHasConfiguredSources] = useState(false);
  const [refreshingSources, setRefreshingSources] = useState(false);

  const notifyCountsChanged = () => {
    emit('refresh-counts').catch(() => {});
  };
  const [loading, setLoading] = useState(false);
  const [initialLoadDone, setInitialLoadDone] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const didInitialLoadRef = useRef(false);
  const installedLoadSeqRef = useRef(0);
  const lastSeenSyncAtRef = useRef<number>(0);
  const sourceSyncingRef = useRef(false);

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<SubagentDetail | null>(null);
  const [catalogDetailOpen, setCatalogDetailOpen] = useState(false);
  const [catalogDetailData, setCatalogDetailData] = useState<CatalogSubagentDetail | null>(null);
  const [catalogDetailInstallTarget, setCatalogDetailInstallTarget] = useState<InstallTargetSubagent | null>(null);

  const [diffOpen, setDiffOpen] = useState(false);
  const [diffData, setDiffData] = useState<UpdateDiff | null>(null);
  const [diffSubagent, setDiffSubagent] = useState<SubagentRecord | null>(null);
  const [reloadOpen, setReloadOpen] = useState(false);
  const [reloadPreview, setReloadPreview] = useState<ReloadPreview | null>(null);
  const [reloadTargetRepoKey, setReloadTargetRepoKey] = useState<string | null>(null);
  const [reloadSelectedPath, setReloadSelectedPath] = useState<string>('');
  const [reloadSubmitting, setReloadSubmitting] = useState(false);
  const [reinstallingKeys, setReinstallingKeys] = useState<Record<string, boolean>>({});
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installTarget, setInstallTarget] = useState<InstallTargetSubagent | null>(null);
  const [installMode, setInstallMode] = useState<'catalog' | 'repository'>('catalog');
  const [installModels, setInstallModels] = useState<ModelType[]>([]);
  const [installSubmitting, setInstallSubmitting] = useState(false);

  const groupInstalledSubagentsByModel = (subagents: SubagentRecord[]) => {
    const next: Record<ModelType, SubagentRecord[]> = {
      claude: [],
      antigravity: [],
      codex: [],
      opencode: [],
    };
    subagents.forEach((subagent) => {
      const model = subagent.model as ModelType;
      if (model === 'claude' || model === 'antigravity' || model === 'codex' || model === 'opencode') {
        next[model].push(subagent);
      }
    });
    return next;
  };

  const loadInstalledAll = async () => {
    const requestSeq = installedLoadSeqRef.current + 1;
    installedLoadSeqRef.current = requestSeq;
    const res = await subagentsListInstalled<ApiResp<SubagentRecord[]>>({
      model: 'claude',
      scope: 'global',
      project_root: null,
    });
    const merged = res.data || [];
    const seen = new Set<string>();
    const all = merged.filter((item) => {
      const key = [
        item.model,
        item.id,
        item.scope || 'global',
        item.project_root || '',
      ].join('::');
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
    if (requestSeq !== installedLoadSeqRef.current) {
      return;
    }
    setInstalledByModel(groupInstalledSubagentsByModel(all));
  };

  const loadCatalog = async () => {
    const res = await subagentsListCatalog<ApiResp<CatalogSubagent[]>>();
    setCatalog(res.data || []);
  };

  const fetchRepositorySubagents = async (includeUpdate = false) => {
    const res = await subagentsRepoList<ApiResp<RepositorySubagentView[]>>(includeUpdate);
    const mergedRows = res.data || [];
    const mergedMap = new Map<string, RepositorySubagentView>();
    for (const row of mergedRows) {
      const existing = mergedMap.get(row.repo_key);
      if (!existing) {
        mergedMap.set(row.repo_key, {
          ...row,
          installed: { ...row.installed },
        });
        continue;
      }
      existing.installed = {
        claude: existing.installed.claude || row.installed.claude,
        antigravity: existing.installed.antigravity || row.installed.antigravity,
        codex: existing.installed.codex || row.installed.codex,
        opencode: existing.installed.opencode || row.installed.opencode,
      };
      existing.has_update = existing.has_update || row.has_update;
      const existingUpdated = existing.updated_at || 0;
      const rowUpdated = row.updated_at || 0;
      existing.updated_at = existingUpdated >= rowUpdated ? existing.updated_at : row.updated_at;
    }
    return Array.from(mergedMap.values());
  };

  const loadRepository = async (includeUpdate = false) => {
    const rows = await fetchRepositorySubagents(includeUpdate);
    setRepositorySubagents(rows);
    return rows;
  };

  const loadSyncState = async () => {
    const res = await subagentsSyncStatusGet<ApiResp<SubagentsSyncState>>();
    setSyncState(res.data);
    const syncAt = Number(res.data?.last_sync_at || 0);
    if (syncAt > 0) {
      lastSeenSyncAtRef.current = syncAt;
    }
  };

  const loadDisplayConfig = async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const cfg = await invoke<StorageConfigLite>('get_storage_config');
    const hours = Number(cfg?.subagents_new_badge_hours ?? 72);
    const safe = Number.isFinite(hours) ? Math.max(1, Math.min(720, Math.floor(hours))) : 72;
    setNewSubagentBadgeHours(safe);
    const sourceNameMap: Record<string, string> = {};
    (cfg?.subagents_sources || []).forEach((item) => {
      const sourceId = String(item?.id || '').trim();
      const sourceName = String(item?.name || '').trim();
      if (sourceId) {
        sourceNameMap[sourceId] = sourceName || sourceId;
      }
    });
    setSourceNamesById(sourceNameMap);
    const configuredSources = Array.isArray(cfg?.subagents_sources)
      ? cfg.subagents_sources.filter((item) => !!item?.id).length
      : 0;
    setHasConfiguredSources(configuredSources > 0);
  };

  const reloadAll = async (includeRepoUpdate = activeMode === 'repository') => {
    await Promise.all([
      loadInstalledAll(),
      loadCatalog(),
      loadRepository(includeRepoUpdate),
      loadSyncState(),
      loadDisplayConfig(),
    ]);
  };

  useEffect(() => {
    if (!isVisible) return;
    if (!didInitialLoadRef.current) {
      didInitialLoadRef.current = true;
      (async () => {
        const startedAt = Date.now();
        setLoading(true);
        try {
          try {
            await subagentsRescanMirror();
          } catch {
            // ignore best-effort rescan errors
          }
          await reloadAll(activeMode === 'repository');
        } finally {
          const elapsed = Date.now() - startedAt;
          if (elapsed < TAB_LOADING_MIN_MS) {
            await new Promise((resolve) => window.setTimeout(resolve, TAB_LOADING_MIN_MS - elapsed));
          }
          setInitialLoadDone(true);
          setLoading(false);
        }
      })().catch(console.error);
    }
  }, [isVisible, activeMode]);

  useEffect(() => {
    if (!isVisible) return;
    let pending = false;
    const pollSyncState = async () => {
      if (pending) return;
      pending = true;
      try {
        const res = await subagentsSyncStatusGet<ApiResp<SubagentsSyncState>>();
        setSyncState(res.data);
        const nextSyncAt = Number(res.data?.last_sync_at || 0);
        if (nextSyncAt > 0 && nextSyncAt !== lastSeenSyncAtRef.current) {
          lastSeenSyncAtRef.current = nextSyncAt;
          await Promise.all([loadCatalog(), loadRepository(activeMode === 'repository'), loadInstalledAll()]);
        }
      } catch {
        // keep silent for background polling
      } finally {
        pending = false;
      }
    };
    const timer = setInterval(() => {
      pollSyncState().catch(() => undefined);
    }, 10000);
    return () => clearInterval(timer);
  }, [isVisible, activeMode]);

  useEffect(() => {
    if (!isVisible || activeMode !== 'repository') return;
    let pending = false;
    const refreshRepository = async () => {
      if (pending) return;
      pending = true;
      try {
        await loadRepository(true);
      } finally {
        pending = false;
      }
    };
    refreshRepository().catch(() => undefined);
    const timer = setInterval(() => {
      refreshRepository().catch(() => undefined);
    }, 8000);
    const onFocus = () => {
      refreshRepository().catch(() => undefined);
    };
    window.addEventListener('focus', onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener('focus', onFocus);
    };
  }, [isVisible, activeMode]);

  useEffect(() => {
    if (!isVisible || activeMode !== 'recommended' || !hasConfiguredSources) return;
    triggerSyncSources(false).catch(() => undefined);
  }, [isVisible, activeMode, hasConfiguredSources]);

  useEffect(() => {
    if (!initialEntry) return;
    setActiveMode(initialEntry);
    onConsumeInitialEntry?.();
  }, [initialEntry, onConsumeInitialEntry]);

  useEffect(() => {
    if (!isVisible || !initialLoadDone) return;
    reloadAll(activeMode === 'repository').catch(console.error);
  }, [isVisible, initialLoadDone, activeMode]);

  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(() => setMessage(null), 3000);
    return () => clearTimeout(timer);
  }, [message]);

  const activeInstalled = useMemo(() => installedByModel[activeModel] || [], [installedByModel, activeModel]);

  const installedById = useMemo(() => {
    const m = new Map<string, SubagentRecord>();
    activeInstalled.forEach((s) => m.set(`${s.source_id}:${s.source_rel_path}`, s));
    return m;
  }, [activeInstalled]);

  const installedCounts = useMemo(
    () => ({
      claude: installedByModel.claude.length,
      antigravity: installedByModel.antigravity.length,
      codex: installedByModel.codex.length,
      opencode: installedByModel.opencode.length,
    }),
    [installedByModel]
  );
  const recommendedCounts = useMemo(() => {
    const counts: Record<ModelType, number> = {
      claude: 0,
      antigravity: 0,
      codex: 0,
      opencode: 0,
    };
    catalog.forEach((skill) => {
      allModels.forEach((model) => {
        if (catalogSupportsModel(skill, model)) {
          counts[model] += 1;
        }
      });
    });
    return counts;
  }, [catalog]);
  const sourceStatuses = useMemo(() => syncState?.sources || [], [syncState]);
  const sourceStatusMap = useMemo(() => {
    const m = new Map<string, SourceSyncState>();
    sourceStatuses.forEach((s) => m.set(s.source_id, s));
    return m;
  }, [sourceStatuses]);
  const reloadDiffMap = useMemo(() => {
    const m = new Map<string, ReloadTextDiff>();
    (reloadPreview?.text_diffs || []).forEach((item) => m.set(item.path, item));
    return m;
  }, [reloadPreview]);
  const reloadSelectedFile = useMemo(
    () => (reloadPreview?.changed_files || []).find((file) => file.path === reloadSelectedPath) || null,
    [reloadPreview, reloadSelectedPath]
  );
  const reloadSelectedDiff = useMemo(
    () => (reloadSelectedPath ? reloadDiffMap.get(reloadSelectedPath) || null : null),
    [reloadDiffMap, reloadSelectedPath]
  );

  const filteredInstalled = useMemo(() => {
    return [...activeInstalled].sort((a, b) => {
      const bUpdated = b.updated_at || b.installed_at || 0;
      const aUpdated = a.updated_at || a.installed_at || 0;
      return bUpdated - aUpdated;
    });
  }, [activeInstalled]);
  const catalogForActiveModel = useMemo(
    () => catalog.filter((item) => catalogSupportsModel(item, activeModel)),
    [catalog, activeModel]
  );
  const catalogSources = useMemo(() => {
    const seen = new Set<string>();
    const list: Array<{ id: string; label: string }> = [];
    catalogForActiveModel.forEach((item) => {
      const sourceId = String(item.source_id || '').trim();
      if (!sourceId || seen.has(sourceId)) return;
      seen.add(sourceId);
      list.push({ id: sourceId, label: sourceNamesById[sourceId] || sourceId });
    });
    return list;
  }, [catalogForActiveModel, sourceNamesById]);
  useEffect(() => {
    if (recommendedSourceFilter === 'all') return;
    const stillExists = catalogSources.some((source) => source.id === recommendedSourceFilter);
    if (!stillExists) {
      setRecommendedSourceFilter('all');
    }
  }, [catalogSources, recommendedSourceFilter]);
  const filteredCatalog = useMemo(() => {
    const bySource =
      recommendedSourceFilter === 'all'
        ? catalogForActiveModel
        : catalogForActiveModel.filter((item) => item.source_id === recommendedSourceFilter);
    const keyword = recommendedSearch.trim().toLowerCase();
    if (!keyword) return bySource;
    return bySource.filter((item) =>
      [
        item.name,
        item.description,
        item.id,
        item.rel_path,
        item.dir_name,
        item.source_id,
      ].some((field) => String(field || '').toLowerCase().includes(keyword))
    );
  }, [catalogForActiveModel, recommendedSourceFilter, recommendedSearch]);
  const visibleInstalled = filteredInstalled;
  const visibleCatalog = filteredCatalog;
  const visibleRepository = useMemo(() => {
    const bySource = (() => {
      if (repositorySourceFilter === 'all') {
        return repositorySubagents;
      }
      if (repositorySourceFilter === 'remote') {
        return repositorySubagents.filter((repo) => repo.source_type === 'remote');
      }
      return repositorySubagents.filter((repo) =>
        repo.source_type === 'local_import' || repo.source_type === 'mirror'
      );
    })();
    const keyword = repositorySearch.trim().toLowerCase();
    if (!keyword) return bySource;
    return bySource.filter((repo) =>
      [
        repo.name,
        repo.description,
        repo.subagent_id,
        repo.source_rel_path,
        repo.dir_name,
        repo.source_type,
      ].some((field) => String(field || '').toLowerCase().includes(keyword))
    );
  }, [repositorySubagents, repositorySourceFilter, repositorySearch]);
  const revealInstalledModels = (models: ModelType[]) => {
    if (models.length === 0) return;
    const nextModel = models.includes(activeModel) ? activeModel : models[0];
    if (nextModel !== activeModel) {
      setActiveModel(nextModel);
    }
  };

  const getRepoSourceMeta = (sourceType: string) => {
    switch (sourceType) {
      case 'remote':
        return {
          label: t('subagentsSourceTypeRemote', 'Recommended Source'),
          className: 'bg-blue-500/10 text-blue-700 border-blue-500/30',
        };
      case 'local_import':
        return {
          label: t('subagentsSourceTypeLocalImport', 'Local Import'),
          className: 'bg-emerald-500/10 text-emerald-700 border-emerald-500/30',
        };
      case 'mirror':
        return {
          label: t('subagentsSourceTypeMirror', 'Mirror'),
          className: 'bg-amber-500/10 text-amber-700 border-amber-500/30',
        };
      default:
        return {
          label: sourceType,
          className: 'bg-muted/50 text-muted-foreground border-border',
        };
    }
  };

  const getReloadStatusMeta = (status: string) => {
    switch (status) {
      case 'added':
        return {
          label: t('subagentsReloadStatusAdded', 'Added'),
          className: 'bg-emerald-500/10 text-emerald-700 border-emerald-500/30',
        };
      case 'deleted':
        return {
          label: t('subagentsReloadStatusDeleted', 'Deleted'),
          className: 'bg-red-500/10 text-red-700 border-red-500/30',
        };
      default:
        return {
          label: t('subagentsReloadStatusModified', 'Modified'),
          className: 'bg-amber-500/10 text-amber-700 border-amber-500/30',
        };
    }
  };

  const triggerSyncSources = async (manual: boolean) => {
    if (sourceSyncingRef.current) return;
    sourceSyncingRef.current = true;
    try {
      if (manual) {
        setLoading(true);
      }
      setRefreshingSources(true);
      await runUserAction(
        actionContext,
        {
          source: 'subagents',
          category: 'sync',
          action: 'sync-sources',
          target: { tab: 'subagents' },
          dedupeKey: 'subagents:manual-sync',
          success: {
            title: t('subagentsSourceSyncSuccess', 'Subagents sources synced successfully'),
            summary: t('subagentsSourceSyncSuccess', 'Subagents sources synced successfully'),
          },
          error: {
            title: t('subagentsSyncFailedMessageTitle', 'Subagents source sync failed'),
          },
        },
        () => subagentsSyncNow(),
      );
      await reloadAll();
    } catch (e: any) {
      if (manual) {
        setMessage({
          type: 'error',
          text: t('subagentsSourceSyncFailed', 'Subagents source sync failed: {{message}}', { message: String(e) }),
        });
      }
    } finally {
      if (manual) {
        setLoading(false);
      }
      setRefreshingSources(false);
      sourceSyncingRef.current = false;
    }
  };

  const toInstallTargetFromRepo = (repo: RepositorySubagentView): InstallTargetSubagent => ({
    source_id: repo.source_id,
    id: repo.subagent_id,
    rel_path: repo.source_rel_path,
    name: repo.name,
    description: repo.description,
    models: repo.models,
    repo_key: repo.repo_key,
    installed: repo.installed,
  });

  const buildInstallStateForCatalog = (item: CatalogSubagent): RepoModelInstallState => ({
    claude: (installedByModel.claude || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) ||
        skill.id === item.id
    ),
    antigravity: (installedByModel.antigravity || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) ||
        skill.id === item.id
    ),
    codex: (installedByModel.codex || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) ||
        skill.id === item.id
    ),
    opencode: (installedByModel.opencode || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) ||
        skill.id === item.id
    ),
  });

  const hasInstallableRepoModels = (target: InstallTargetSubagent | null) => {
    if (!target?.installed) return true;
    return allModels.some((model) => target.models.includes(model) && !target.installed?.[model]);
  };

  const isRecentCatalogSubagent = (item: CatalogSubagent) => {
    if (!item.first_seen_at) return false;
    const ttlSeconds = newSubagentBadgeHours * 60 * 60;
    const age = Math.floor(Date.now() / 1000) - item.first_seen_at;
    return age >= 0 && age <= ttlSeconds;
  };
  const isRecentRepositorySubagent = (item: RepositorySubagentView) => {
    if (!item.created_at) return false;
    const ttlSeconds = newSubagentBadgeHours * 60 * 60;
    const age = Math.floor(Date.now() / 1000) - item.created_at;
    return age >= 0 && age <= ttlSeconds;
  };

  const matchesRepositorySubagent = (
    repo: RepositorySubagentView,
    candidate: {
      repo_key?: string;
      source_id: string;
      source_rel_path?: string;
      rel_path?: string;
      id?: string;
      dir_name?: string;
    }
  ) => {
    const relPath = candidate.source_rel_path || candidate.rel_path;
    if (relPath && repo.source_id === candidate.source_id && repo.source_rel_path === relPath) {
      return true;
    }
    if (candidate.id && repo.subagent_id === candidate.id) {
      return true;
    }
    if (candidate.dir_name && repo.dir_name && repo.dir_name === candidate.dir_name) {
      return true;
    }
    return false;
  };

  const findLatestRepository = (candidate: {
    repo_key?: string;
    source_id: string;
    source_rel_path?: string;
    rel_path?: string;
    id?: string;
    dir_name?: string;
  }) => {
    if (candidate.repo_key) {
      const byKey = repositorySubagents.find((repo) => repo.repo_key === candidate.repo_key);
      if (byKey) return byKey;
    }
    return repositorySubagents.find((repo) => matchesRepositorySubagent(repo, candidate));
  };

  const resolveRepositorySubagent = async (candidate: {
    repo_key?: string;
    source_id: string;
    source_rel_path?: string;
    rel_path?: string;
    id?: string;
    dir_name?: string;
  }) => {
    const existing = findLatestRepository(candidate);
    if (existing) return existing;
    const fetched = await fetchRepositorySubagents(false);
    return fetched.find((repo) => {
      if (candidate.repo_key && repo.repo_key === candidate.repo_key) {
        return true;
      }
      return matchesRepositorySubagent(repo, candidate);
    });
  };

  return {
    t,
    confirmDialog,
    actionContext,
    pickIcon,
    activeModel,
    setActiveModel,
    activeMode,
    setActiveMode,
    recommendedSourceFilter,
    setRecommendedSourceFilter,
    recommendedSearch,
    setRecommendedSearch,
    repositorySourceFilter,
    setRepositorySourceFilter,
    repositorySearch,
    setRepositorySearch,
    installedByModel,
    catalog,
    repositorySubagents,
    syncState,
    newSubagentBadgeHours,
    hasConfiguredSources,
    refreshingSources,
    notifyCountsChanged,
    loading,
    setLoading,
    initialLoadDone,
    message,
    setMessage,
    detailOpen,
    setDetailOpen,
    detailData,
    setDetailData,
    catalogDetailOpen,
    setCatalogDetailOpen,
    catalogDetailData,
    setCatalogDetailData,
    catalogDetailInstallTarget,
    setCatalogDetailInstallTarget,
    diffOpen,
    setDiffOpen,
    diffData,
    setDiffData,
    diffSubagent,
    setDiffSubagent,
    reloadOpen,
    setReloadOpen,
    reloadPreview,
    setReloadPreview,
    reloadTargetRepoKey,
    setReloadTargetRepoKey,
    reloadSelectedPath,
    setReloadSelectedPath,
    reloadSubmitting,
    setReloadSubmitting,
    reinstallingKeys,
    setReinstallingKeys,
    installDialogOpen,
    setInstallDialogOpen,
    installTarget,
    setInstallTarget,
    installMode,
    setInstallMode,
    installModels,
    setInstallModels,
    installSubmitting,
    setInstallSubmitting,
    reloadAll,
    loadRepository,
    loadSyncState,
    loadDisplayConfig,
    triggerSyncSources,
    revealInstalledModels,
    getRepoSourceMeta,
    getReloadStatusMeta,
    toInstallTargetFromRepo,
    buildInstallStateForCatalog,
    hasInstallableRepoModels,
    isRecentCatalogSubagent,
    isRecentRepositorySubagent,
    findLatestRepository,
    resolveRepositorySubagent,
    installedById,
    installedCounts,
    recommendedCounts,
    sourceStatusMap,
    reloadSelectedFile,
    reloadSelectedDiff,
    visibleInstalled,
    visibleCatalog,
    visibleRepository,
    catalogSources,
  };
}

export type SubagentsData = ReturnType<typeof useSubagentsData>;
