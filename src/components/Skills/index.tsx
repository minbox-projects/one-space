import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { emit } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { open } from '@tauri-apps/plugin-dialog';
import {
  Sparkles,
  Wrench,
  Shield,
  Cpu,
  BookOpen,
  Trash2,
  FolderOpen,
  FolderPlus,
  RefreshCw,
  Download,
  Loader2,
} from 'lucide-react';
import { skillModelOptions, type SkillModelId } from '../skillsModelOptions';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '../ui/dialog';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { safeRecordMessage } from '@/lib/messages';
import { runUserAction } from '@/lib/userActions';
import { buildUninstallSkillActionDescriptor } from '@/lib/actionDescriptors/skills';
import {
  skillsCatalogDetailGet,
  skillsCatalogOpenFolder,
  skillsDetailGet,
  skillsInstall,
  skillsListCatalog,
  skillsListInstalled,
  skillsOpenFolder,
  skillsRepoDelete,
  skillsRepoDetailGet,
  skillsRepoImportFolder,
  skillsRepoList,
  skillsRepoReloadApply,
  skillsRepoReloadPreview,
  skillsRepoSetModel,
  skillsRescanMirror,
  skillsSyncNow,
  skillsSyncStatusGet,
  skillsUninstall,
} from '@/lib/skills';

type ModelType = SkillModelId;
type InstallScope = 'global' | 'project';

type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };
const SKILLS_AUTO_UPDATED_EVENT = 'onespace:skills-auto-updated';

interface SkillRecord {
  id: string;
  dir_name?: string;
  model: ModelType;
  models: ModelType[];
  name: string;
  description: string;
  source_id: string;
  source_rel_path: string;
  installed_at: number;
  updated_at?: number;
  has_update: boolean;
  icon_seed: string;
  scope?: InstallScope;
  project_root?: string | null;
  target_path?: string | null;
}

interface CatalogSkill {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: ModelType[];
  first_seen_at?: number;
}

interface StorageConfigLite {
  skills_new_badge_hours?: number;
  skills_sources?: Array<{ id?: string; name?: string }>;
}

interface SkillDetail {
  skill: SkillRecord;
  markdown: string;
  local_path: string;
}

interface CatalogSkillDetail {
  skill: CatalogSkill;
  markdown: string;
  source_path: string;
}

interface CatalogOpenFolderResult {
  repo_key: string;
  opened_path: string;
}

interface ReloadChangedFile {
  path: string;
  status: 'added' | 'modified' | 'deleted' | string;
  is_binary: boolean;
}

interface ReloadTextDiff {
  path: string;
  before_content: string;
  after_content: string;
  before_changed_lines: number[];
  after_changed_lines: number[];
}

interface InstalledSkillTarget {
  model: ModelType;
  scope: InstallScope;
  project_root?: string | null;
  dir_name: string;
}

interface ReloadPreview {
  before_label: string;
  after_label: string;
  changed_files: ReloadChangedFile[];
  text_diffs: ReloadTextDiff[];
  installed_models: ModelType[];
  installed_targets: InstalledSkillTarget[];
  has_changes: boolean;
}

interface ReloadApplyResult {
  index_refreshed: boolean;
  synced_models: ModelType[];
  synced_targets: InstalledSkillTarget[];
  updated_files_count: number;
  applied_at: number;
}

interface SourceSyncState {
  source_id: string;
  last_synced_at?: number;
  last_status: string;
  last_error?: string;
}

interface SkillsSyncState {
  status: string;
  last_error?: string;
  last_sync_at?: number;
  sources: SourceSyncState[];
}

interface RepoModelInstallState {
  claude: boolean;
  gemini: boolean;
  codex: boolean;
  opencode: boolean;
}

interface RepositorySkillView {
  repo_key: string;
  skill_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  source_path?: string;
  name: string;
  description: string;
  models: ModelType[];
  icon_seed: string;
  hash?: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: RepoModelInstallState;
}

interface RepoImportFolderResult {
  repo_key: string;
  skill_id: string;
  source_id: string;
  source_rel_path: string;
}

interface InstallTargetSkill extends CatalogSkill {
  repo_key?: string;
  installed?: RepoModelInstallState;
}

const modelTabs: { id: ModelType; label: string }[] = [
  { id: 'claude', label: 'Claude' },
  { id: 'gemini', label: 'Gemini' },
  { id: 'codex', label: 'Codex' },
  { id: 'opencode', label: 'OpenCode' },
];

const modelIconMap: Record<ModelType, ComponentType<{ className?: string }>> = skillModelOptions.reduce(
  (acc, item) => {
    acc[item.id] = item.Icon;
    return acc;
  },
  {} as Record<ModelType, ComponentType<{ className?: string }>>
);

const iconPool = [Sparkles, Wrench, Shield, Cpu, BookOpen];
const TAB_LOADING_MIN_MS = 200;

function formatTs(ts?: number) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString();
}

function formatInstalledTarget(target: InstalledSkillTarget) {
  const scopeLabel = target.scope === 'project' ? 'Project' : 'Global';
  if (target.project_root) {
    return `${target.model} · ${scopeLabel} · ${target.project_root}`;
  }
  return `${target.model} · ${scopeLabel}`;
}

function errorContainsCode(error: unknown, code: string) {
  return String(error ?? '').includes(code);
}

function renderDiffDocument(markdown: string, changedLines: number[]) {
  const normalized = markdown.replace(/\r\n/g, '\n');
  const lines = normalized.length > 0 ? normalized.split('\n') : [''];
  const changedSet = new Set((changedLines || []).map((n) => Number(n)));

  return (
    <div className="rounded-md border overflow-hidden">
      <div className="max-h-[46vh] overflow-auto font-mono text-[11px] leading-5">
        {lines.map((line, idx) => {
          const lineNumber = idx + 1;
          const changed = changedSet.has(lineNumber);
          return (
            <div
              key={`diff-line-${lineNumber}`}
              className={`grid grid-cols-[56px,1fr] ${changed ? 'bg-amber-50/80' : 'bg-background'}`}
            >
              <div
                className={`border-r px-2 py-1 text-right select-none ${
                  changed
                    ? 'text-amber-700 bg-amber-100/80 border-amber-200'
                    : 'text-muted-foreground bg-muted/30 border-border'
                }`}
              >
                {lineNumber}
              </div>
              <pre
                className={`px-3 py-1 whitespace-pre-wrap break-words ${
                  changed ? 'text-amber-900' : 'text-foreground'
                }`}
              >
                {line || ' '}
              </pre>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function Skills({
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
        return 'skills-inline-toast';
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
  const [installedByModel, setInstalledByModel] = useState<Record<ModelType, SkillRecord[]>>({
    claude: [],
    gemini: [],
    codex: [],
    opencode: [],
  });
  const [catalog, setCatalog] = useState<CatalogSkill[]>([]);
  const [repositorySkills, setRepositorySkills] = useState<RepositorySkillView[]>([]);
  const [syncState, setSyncState] = useState<SkillsSyncState | null>(null);
  const [sourceNamesById, setSourceNamesById] = useState<Record<string, string>>({});
  const [newSkillBadgeHours, setNewSkillBadgeHours] = useState(72);
  const [hasConfiguredSources, setHasConfiguredSources] = useState(false);
  const [refreshingSources, setRefreshingSources] = useState(false);
  const [loading, setLoading] = useState(false);
  const [initialLoadDone, setInitialLoadDone] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const didInitialLoadRef = useRef(false);
  const installedLoadSeqRef = useRef(0);
  const lastSeenSyncAtRef = useRef<number>(0);
  const sourceSyncingRef = useRef(false);

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<SkillDetail | null>(null);
  const [catalogDetailOpen, setCatalogDetailOpen] = useState(false);
  const [catalogDetailData, setCatalogDetailData] = useState<CatalogSkillDetail | null>(null);
  const [catalogDetailInstallTarget, setCatalogDetailInstallTarget] = useState<InstallTargetSkill | null>(null);

  const [reloadOpen, setReloadOpen] = useState(false);
  const [reloadPreview, setReloadPreview] = useState<ReloadPreview | null>(null);
  const [reloadTargetRepoKey, setReloadTargetRepoKey] = useState<string | null>(null);
  const [reloadSelectedPath, setReloadSelectedPath] = useState<string>('');
  const [reloadSubmitting, setReloadSubmitting] = useState(false);
  const allModels: ModelType[] = ['claude', 'gemini', 'codex', 'opencode'];
  const [reinstallingKeys, setReinstallingKeys] = useState<Record<string, boolean>>({});
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installTarget, setInstallTarget] = useState<InstallTargetSkill | null>(null);
  const [installMode, setInstallMode] = useState<'catalog' | 'repository'>('catalog');
  const [installModels, setInstallModels] = useState<ModelType[]>([]);
  const [installSubmitting, setInstallSubmitting] = useState(false);

  const groupInstalledSkillsByModel = (skills: SkillRecord[]) => {
    const next: Record<ModelType, SkillRecord[]> = {
      claude: [],
      gemini: [],
      codex: [],
      opencode: [],
    };
    skills.forEach((skill) => {
      const model = skill.model as ModelType;
      if (model === 'claude' || model === 'gemini' || model === 'codex' || model === 'opencode') {
        next[model].push(skill);
      }
    });
    return next;
  };

  const loadInstalledAll = async () => {
    const requestSeq = installedLoadSeqRef.current + 1;
    installedLoadSeqRef.current = requestSeq;
    const res = await skillsListInstalled<ApiResp<SkillRecord[]>>({
      model: null,
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
    setInstalledByModel(groupInstalledSkillsByModel(all));
  };

  const loadCatalog = async () => {
    const res = await skillsListCatalog<ApiResp<CatalogSkill[]>>();
    setCatalog(res.data || []);
  };

  const fetchRepositorySkills = async (includeUpdate = false) => {
    const res = await skillsRepoList<ApiResp<RepositorySkillView[]>>(includeUpdate);
    const mergedRows = res.data || [];
    const mergedMap = new Map<string, RepositorySkillView>();
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
        gemini: existing.installed.gemini || row.installed.gemini,
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
    const rows = await fetchRepositorySkills(includeUpdate);
    setRepositorySkills(rows);
    return rows;
  };

  const loadSyncState = async () => {
    const res = await skillsSyncStatusGet<ApiResp<SkillsSyncState>>();
    setSyncState(res.data);
    const syncAt = Number(res.data?.last_sync_at || 0);
    if (syncAt > 0) {
      lastSeenSyncAtRef.current = syncAt;
    }
  };

  const loadDisplayConfig = async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const cfg = await invoke<StorageConfigLite>('get_storage_config');
    const hours = Number(cfg?.skills_new_badge_hours ?? 72);
    const safe = Number.isFinite(hours) ? Math.max(1, Math.min(720, Math.floor(hours))) : 72;
    setNewSkillBadgeHours(safe);
    const sourceNameMap: Record<string, string> = {};
    (cfg?.skills_sources || []).forEach((item) => {
      const sourceId = String(item?.id || '').trim();
      const sourceName = String(item?.name || '').trim();
      if (sourceId) {
        sourceNameMap[sourceId] = sourceName || sourceId;
      }
    });
    setSourceNamesById(sourceNameMap);
    const configuredSources = Array.isArray(cfg?.skills_sources)
      ? cfg.skills_sources.filter((item) => !!item?.id).length
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
            await skillsRescanMirror();
          } catch {
            // ignore best-effort rescan errors
          }
          await reloadAll(activeMode === 'repository');
          emit('refresh-counts').catch(() => {});
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
        const res = await skillsSyncStatusGet<ApiResp<SkillsSyncState>>();
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
    const onAutoUpdated = () => {
      Promise.all([
        loadCatalog(),
        loadRepository(activeMode === 'repository'),
        loadInstalledAll(),
        loadSyncState(),
      ]).catch(() => undefined);
    };
    window.addEventListener(SKILLS_AUTO_UPDATED_EVENT, onAutoUpdated);
    return () => {
      clearInterval(timer);
      window.removeEventListener(SKILLS_AUTO_UPDATED_EVENT, onAutoUpdated);
    };
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
    const m = new Map<string, SkillRecord>();
    activeInstalled.forEach((s) => m.set(`${s.source_id}:${s.source_rel_path}`, s));
    return m;
  }, [activeInstalled]);

  const installedCounts = useMemo(
    () => ({
      claude: installedByModel.claude.length,
      gemini: installedByModel.gemini.length,
      codex: installedByModel.codex.length,
      opencode: installedByModel.opencode.length,
    }),
    [installedByModel]
  );
  const recommendedCounts = useMemo(() => {
    const counts: Record<ModelType, number> = {
      claude: 0,
      gemini: 0,
      codex: 0,
      opencode: 0,
    };
    catalog.forEach((skill) => {
      allModels.forEach((model) => {
        if (skill.models.includes(model)) {
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
  const catalogSources = useMemo(() => {
    const seen = new Set<string>();
    const list: Array<{ id: string; label: string }> = [];
    catalog
      .filter((item) => item.models.includes(activeModel))
      .forEach((item) => {
      const sourceId = String(item.source_id || '').trim();
      if (!sourceId || seen.has(sourceId)) return;
      seen.add(sourceId);
      list.push({ id: sourceId, label: sourceNamesById[sourceId] || sourceId });
      });
    return list;
  }, [catalog, sourceNamesById, activeModel]);
  useEffect(() => {
    if (recommendedSourceFilter === 'all') return;
    const stillExists = catalogSources.some((source) => source.id === recommendedSourceFilter);
    if (!stillExists) {
      setRecommendedSourceFilter('all');
    }
  }, [catalogSources, recommendedSourceFilter]);
  const filteredCatalog = useMemo(() => {
    const byModel = catalog.filter((item) => item.models.includes(activeModel));
    const bySource =
      recommendedSourceFilter === 'all'
        ? byModel
        : byModel.filter((item) => item.source_id === recommendedSourceFilter);
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
  }, [catalog, recommendedSourceFilter, recommendedSearch, activeModel]);
  const visibleInstalled = filteredInstalled;
  const visibleCatalog = filteredCatalog;
  const visibleRepository = useMemo(() => {
    const bySource = (() => {
      if (repositorySourceFilter === 'all') {
        return repositorySkills;
      }
      if (repositorySourceFilter === 'remote') {
        return repositorySkills.filter((repo) => repo.source_type === 'remote');
      }
      return repositorySkills.filter((repo) =>
        repo.source_type === 'local_import' || repo.source_type === 'mirror'
      );
    })();
    const keyword = repositorySearch.trim().toLowerCase();
    if (!keyword) return bySource;
    return bySource.filter((repo) =>
      [
        repo.name,
        repo.description,
        repo.skill_id,
        repo.source_rel_path,
        repo.dir_name,
        repo.source_type,
      ].some((field) => String(field || '').toLowerCase().includes(keyword))
    );
  }, [repositorySkills, repositorySourceFilter, repositorySearch]);
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
          label: t('skillsSourceTypeRemote', 'Recommended Source'),
          className: 'bg-blue-500/10 text-blue-700 border-blue-500/30',
        };
      case 'local_import':
        return {
          label: t('skillsSourceTypeLocalImport', 'Local Import'),
          className: 'bg-emerald-500/10 text-emerald-700 border-emerald-500/30',
        };
      case 'mirror':
        return {
          label: t('skillsSourceTypeMirror', 'Mirror'),
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
          label: t('skillsReloadStatusAdded', 'Added'),
          className: 'bg-emerald-500/10 text-emerald-700 border-emerald-500/30',
        };
      case 'deleted':
        return {
          label: t('skillsReloadStatusDeleted', 'Deleted'),
          className: 'bg-red-500/10 text-red-700 border-red-500/30',
        };
      default:
        return {
          label: t('skillsReloadStatusModified', 'Modified'),
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
          source: 'skills',
          category: 'sync',
          action: 'sync-sources',
          target: { tab: 'skills' },
          dedupeKey: 'skills:manual-sync',
          success: {
            title: t('skillsSourceSyncSuccess', 'Skills sources synced successfully'),
            summary: t('skillsSourceSyncSuccess', 'Skills sources synced successfully'),
          },
          error: {
            title: t('skillsSyncFailedMessageTitle', 'Skills source sync failed'),
          },
        },
        () => skillsSyncNow(),
      );
      await reloadAll();
    } catch (e: any) {
      if (manual) {
        setMessage({
          type: 'error',
          text: t('skillsSourceSyncFailed', 'Skills source sync failed: {{message}}', { message: String(e) }),
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

  const handleSyncSources = async () => {
    await triggerSyncSources(true);
  };

  const toInstallTargetFromRepo = (repo: RepositorySkillView): InstallTargetSkill => ({
    source_id: repo.source_id,
    id: repo.skill_id,
    rel_path: repo.source_rel_path,
    name: repo.name,
    description: repo.description,
    models: repo.models,
    repo_key: repo.repo_key,
    installed: repo.installed,
  });

  const buildInstallStateForCatalog = (item: CatalogSkill): RepoModelInstallState => ({
    claude: (installedByModel.claude || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) ||
        skill.id === item.id
    ),
    gemini: (installedByModel.gemini || []).some(
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

  const hasInstallableRepoModels = (target: InstallTargetSkill | null) => {
    if (!target?.installed) return true;
    return allModels.some((model) => target.models.includes(model) && !target.installed?.[model]);
  };

  const isRecentCatalogSkill = (item: CatalogSkill) => {
    if (!item.first_seen_at) return false;
    const ttlSeconds = newSkillBadgeHours * 60 * 60;
    const age = Math.floor(Date.now() / 1000) - item.first_seen_at;
    return age >= 0 && age <= ttlSeconds;
  };
  const isRecentRepositorySkill = (item: RepositorySkillView) => {
    if (!item.created_at) return false;
    const ttlSeconds = newSkillBadgeHours * 60 * 60;
    const age = Math.floor(Date.now() / 1000) - item.created_at;
    return age >= 0 && age <= ttlSeconds;
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

  const matchesRepositorySkill = (
    repo: RepositorySkillView,
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
    if (candidate.id && repo.skill_id === candidate.id) {
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
      const byKey = repositorySkills.find((repo) => repo.repo_key === candidate.repo_key);
      if (byKey) return byKey;
    }
    return repositorySkills.find((repo) => matchesRepositorySkill(repo, candidate));
  };

  const resolveRepositorySkill = async (candidate: {
    repo_key?: string;
    source_id: string;
    source_rel_path?: string;
    rel_path?: string;
    id?: string;
    dir_name?: string;
  }) => {
    const existing = findLatestRepository(candidate);
    if (existing) return existing;
    const fetched = await fetchRepositorySkills(false);
    return fetched.find((repo) => {
      if (candidate.repo_key && repo.repo_key === candidate.repo_key) {
        return true;
      }
      return matchesRepositorySkill(repo, candidate);
    });
  };

  const handleInstallRepository = (repo: RepositorySkillView) => {
    const latest = findLatestRepository(repo) || repo;
    openInstallDialog(toInstallTargetFromRepo(latest), 'repository');
  };

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

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('skillsPageTitle', 'Global Skills')}</h2>
          <p className="text-sm text-muted-foreground">
            {t(
              'skillsPageDesc',
              'Install and manage skills at global scope by model. These installs are not tied to a single workspace. Use workspace pages for project-specific installs.'
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {message && (
            <div
              className={`text-xs rounded-md border px-2.5 py-1.5 ${
                message.type === 'error'
                  ? 'bg-destructive/10 text-destructive border-destructive/20'
                  : 'bg-green-500/10 text-green-700 border-green-500/20'
              }`}
            >
              {message.text}
            </div>
          )}
          {activeMode === 'recommended' && (
            <button
              onClick={handleSyncSources}
              disabled={loading || refreshingSources}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm inline-flex items-center gap-2 disabled:opacity-50"
            >
              <RefreshCw className={`w-4 h-4 ${loading || refreshingSources ? 'animate-spin' : ''}`} />
              {t('skillsSyncSources', '同步源列表')}
            </button>
          )}
          {activeMode === 'repository' && (
            <button
              onClick={handleImportRepositoryFolder}
              disabled={loading}
              className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
            >
              <FolderPlus className="w-4 h-4" />
              {t('skillsLocalImportButton', 'Import From Folder')}
            </button>
          )}
        </div>
      </div>

      <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
        <button
          onClick={handleSwitchToRecommended}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'recommended' ? 'bg-black text-white' : 'bg-white text-black'
          }`}
        >
          {t('recommended', '推荐')}
        </button>
        <button
          onClick={handleSwitchToRepository}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'repository' ? 'bg-black text-white' : 'bg-white text-black'
          }`}
        >
          {t('repository', '仓库')}
        </button>
        <button
          onClick={() => setActiveMode('installed')}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'installed' ? 'bg-black text-white' : 'bg-white text-black'
          }`}
        >
          {t('installed', '已安装')}
        </button>
      </div>

      {activeMode !== 'repository' && (
        <div className="border rounded-xl bg-card p-3">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
            {modelTabs.map((m) => {
              const ModelIcon = modelIconMap[m.id];
              return (
                <button
                  key={m.id}
                  type="button"
                  onClick={() => setActiveModel(m.id)}
                  className={`rounded-lg border px-4 py-3 text-left transition-all ${
                    activeModel === m.id ? 'border-primary bg-primary/5' : 'hover:bg-muted/40 hover:-translate-y-0.5'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <ModelIcon className="w-5 h-5" />
                    <span className="text-sm font-semibold">{m.label}</span>
                  </div>
                  <div className="mt-2.5">
                    <span className="text-sm leading-none text-muted-foreground">
                      {activeMode === 'recommended'
                        ? t('skillsRecommendedCount', 'Recommended {{count}} skills', { count: recommendedCounts[m.id] ?? 0 })
                        : t('skillsInstalledCount', 'Installed {{count}} skills', { count: installedCounts[m.id] ?? 0 })}
                    </span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {activeMode === 'installed' && (
        <>
          {!initialLoadDone ? (
            <div className="text-center py-12 text-muted-foreground">
              <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
              <p>{t('loading', 'Loading...')}</p>
            </div>
          ) : visibleInstalled.length === 0 ? (
            <div className="text-center py-12">
              <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noInstalledSkillsForModel', '该模型下暂无已安装 Skills')}</h3>
              <p className="text-muted-foreground">{t('noInstalledSkillsForModelDesc', '你可以先到“推荐”中安装 Skills。')}</p>
              <button
                onClick={handleSwitchToRecommended}
                className="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
              >
                {t('recommended', '推荐')}
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleInstalled.map((skill) => {
                const Icon = pickIcon(skill.icon_seed || skill.id);
                const reinstallKey = `${skill.model}:${skill.id}`;
                const reinstalling = !!reinstallingKeys[reinstallKey];
                return (
                  <div
                    key={`${skill.model}:${skill.id}`}
                    className="border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer"
                    onClick={() => handleOpenDetail(skill)}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="p-2 rounded-md bg-primary/10 text-primary">
                        <Icon className="w-4 h-4" />
                      </div>
                      <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                        {skill.dir_name || skill.source_rel_path.split('/').pop() || skill.id}
                      </span>
                    </div>

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{skill.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{skill.description}</p>

                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('lastUpdated', 'Last updated')}: {formatTs(skill.updated_at || skill.installed_at)}
                    </div>

                    <div className="mt-3 flex items-center justify-end gap-2">
                      <button
                        disabled={reinstalling}
                        className="text-xs px-2.5 py-1 rounded-md border hover:bg-muted inline-flex items-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleReinstall(skill);
                        }}
                      >
                        <RefreshCw className={`w-3.5 h-3.5 ${reinstalling ? 'animate-spin' : ''}`} />
                        {t('skillsReinstall', '重新安装')}
                      </button>
                      <button
                        className="text-xs px-2.5 py-1 rounded-md border hover:bg-destructive/10 text-destructive inline-flex items-center gap-1"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleUninstall(skill);
                        }}
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                        {t('uninstall', 'Uninstall')}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}

      {activeMode === 'repository' && (
        <>
          <div className="mb-4 flex items-center gap-2">
            <div className="w-[170px] sm:w-[220px] lg:w-[260px] shrink-0">
              <input
                value={repositorySearch}
                onChange={(e) => setRepositorySearch(e.target.value)}
                placeholder={t('skillsSearchPlaceholder', '搜索 Skill 名称或描述')}
                className="h-9 w-full rounded-lg border border-black/20 bg-white px-3 text-sm shadow-sm outline-none focus:border-black"
              />
            </div>
            <div className="min-w-0 flex-1 overflow-x-auto">
              <div className="flex w-max min-w-full justify-end">
                <div className="inline-flex w-max rounded-lg border border-black bg-white p-1 whitespace-nowrap shadow-sm">
                  <button
                    onClick={() => setRepositorySourceFilter('all')}
                    className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                      repositorySourceFilter === 'all'
                        ? 'bg-black text-white'
                        : 'bg-white text-black'
                    }`}
                  >
                    {t('all', '全部')}
                  </button>
                  <button
                    onClick={() => setRepositorySourceFilter('local')}
                    className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                      repositorySourceFilter === 'local'
                        ? 'bg-black text-white'
                        : 'bg-white text-black'
                    }`}
                  >
                    {t('skillsSourceTypeLocalImport', '本地导入')}
                  </button>
                  <button
                    onClick={() => setRepositorySourceFilter('remote')}
                    className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                      repositorySourceFilter === 'remote'
                        ? 'bg-black text-white'
                        : 'bg-white text-black'
                    }`}
                  >
                    {t('skillsSourceTypeRemote', '推荐源')}
                  </button>
                </div>
              </div>
            </div>
          </div>

          {!initialLoadDone ? (
            <div className="text-center py-12 text-muted-foreground">
              <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
              <p>{t('loading', 'Loading...')}</p>
            </div>
          ) : visibleRepository.length === 0 ? (
            <div className="text-center py-12">
              <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noResultsFound', 'No skills found.')}</h3>
              <p className="text-muted-foreground mb-4">
                {t('skillsRepoEmptyHint', '请从文件夹导入 Skill，或先在推荐模式同步源列表。')}
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleRepository.map((repo) => {
                const Icon = pickIcon(repo.icon_seed || repo.skill_id);
                const sourceMeta = getRepoSourceMeta(repo.source_type);
                const installedCount = allModels.reduce(
                  (sum, model) => sum + (repo.installed[model] ? 1 : 0),
                  0,
                );
                const repoHasUpdate = !!repo.has_update;
                const isNewRepo = isRecentRepositorySkill(repo);
                return (
                  <div
                    key={repo.repo_key}
                    className="border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer"
                    onClick={() => handleOpenRepositoryDetail(repo)}
                  >
                    <div className="flex items-start justify-between">
                      <div className="p-2 rounded-md bg-muted text-foreground">
                        <Icon className="w-4 h-4" />
                      </div>
                      <div className="flex flex-col items-end gap-1.5">
                        <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                          {repo.dir_name || repo.source_rel_path.split('/').pop() || repo.skill_id}
                        </span>
                        <div className="flex items-center gap-1.5">
                          {isNewRepo && (
                            <span className="text-[10px] px-1.5 py-0.5 rounded border bg-emerald-500/10 text-emerald-700 border-emerald-500/30">
                              {t('new', 'New')}
                            </span>
                          )}
                          {repoHasUpdate && (
                            <button
                              type="button"
                              className="text-[10px] px-2 py-0.5 rounded-full bg-amber-100 text-amber-700 border border-amber-200"
                              onClick={(e) => {
                                e.stopPropagation();
                                void openReloadPreviewByRepoKey(repo.repo_key);
                              }}
                            >
                              {t('hasUpdate', '有更新')}
                            </button>
                          )}
                        </div>
                      </div>
                    </div>

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{repo.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{repo.description}</p>
                    <div className="mt-3 text-[11px] text-muted-foreground flex items-center gap-4">
                      <span>
                        {t('skillsRepositoryLastUpdated', '最后更新')}: {formatTs(repo.updated_at || repo.created_at)}
                      </span>
                      <span>
                        {t('installed', 'Installed')} {installedCount}/4
                      </span>
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-2">
                      <span className={`text-[10px] px-2 py-1 rounded border ${sourceMeta.className}`}>
                        {sourceMeta.label}
                      </span>
                      <div className="flex justify-end gap-2">
                        {repo.models.length > 0 && (
                          <button
                            className="text-xs px-2.5 py-1 rounded-md bg-primary text-primary-foreground inline-flex items-center gap-1"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleInstallRepository(repo);
                            }}
                          >
                            <Download className="w-3.5 h-3.5" />
                            {t('install', 'Install')}
                          </button>
                        )}
                        <button
                          className="text-xs px-2.5 py-1 rounded-md border hover:bg-destructive/10 text-destructive inline-flex items-center gap-1"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleDeleteRepository(repo);
                          }}
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                          {t('delete', 'Delete')}
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}

      {activeMode === 'recommended' && (
        <div className="relative isolate">
          {catalogSources.length > 0 && (
            <div className="sticky top-0 z-[90] mb-4 pointer-events-none">
              <div className="pointer-events-auto relative z-[100] flex items-center gap-2">
                <div className="w-[170px] sm:w-[220px] lg:w-[260px] shrink-0">
                  <input
                    value={recommendedSearch}
                    onChange={(e) => setRecommendedSearch(e.target.value)}
                    placeholder={t('skillsSearchPlaceholder', '搜索 Skill 名称或描述')}
                    className="h-9 w-full rounded-lg border border-black/20 bg-white px-3 text-sm shadow-sm outline-none focus:border-black"
                  />
                </div>
                <div className="min-w-0 flex-1 overflow-x-auto">
                  <div className="flex w-max min-w-full justify-end">
                    <div className="inline-flex w-max rounded-lg border border-black bg-white p-1 whitespace-nowrap shadow-sm">
                      <button
                        onClick={() => setRecommendedSourceFilter('all')}
                        className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                          recommendedSourceFilter === 'all' ? 'bg-black text-white' : 'bg-white text-black'
                        }`}
                      >
                        {t('all', '全部')}
                      </button>
                      {catalogSources.map((source) => (
                        <button
                          key={source.id}
                          title={source.id}
                          onClick={() => setRecommendedSourceFilter(source.id)}
                          className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                            recommendedSourceFilter === source.id ? 'bg-black text-white' : 'bg-white text-black'
                          }`}
                        >
                          {source.label}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}
          {!initialLoadDone ? (
            <div className="text-center py-12 text-muted-foreground">
              <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
              <p>{t('loading', 'Loading...')}</p>
            </div>
          ) : visibleCatalog.length === 0 ? (
            <div className="text-center py-12">
              <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noRecommendedSkills', '当前没有可推荐的 Skills')}</h3>
              <p className="text-muted-foreground mb-4">{t('noRecommendedSkillsDesc', '请检查 Skills 源配置，或同步源列表后重试。')}</p>
            </div>
          ) : (
            <div className="relative z-0 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleCatalog.map((item) => {
                const installedSkill = installedById.get(`${item.source_id}:${item.rel_path}`);
                const Icon = pickIcon(item.id);
                const srcStatus = sourceStatusMap.get(item.source_id);
                const isNewSkill = isRecentCatalogSkill(item);
                return (
                  <div
                    key={`${item.source_id}:${item.id}`}
                    className="relative z-0 border rounded-xl p-4 bg-card transition-all duration-200 hover:shadow-md hover:border-primary/30 cursor-pointer"
                    onClick={() => handleOpenCatalogDetail(item)}
                  >
                    <div className="flex items-start justify-between">
                      <div className="p-2 rounded-md bg-muted text-foreground">
                        <Icon className="w-4 h-4" />
                      </div>
                      <div className="flex flex-col items-end gap-1">
                        <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem]">
                          {item.dir_name || item.rel_path.split('/').pop() || item.id}
                        </span>
                      </div>
                    </div>
                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{item.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{item.description}</p>
                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('lastSynced', 'Last synced')}: {formatTs(srcStatus?.last_synced_at || syncState?.last_sync_at)}
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-2">
                      <div className="flex items-center gap-2">
                        <span className="text-[10px] px-2 py-1 rounded border bg-muted/50 text-muted-foreground">
                          {item.source_id}
                        </span>
                        {isNewSkill && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-emerald-500/10 text-emerald-700 border-emerald-500/30">
                            {t('new', 'New')}
                          </span>
                        )}
                      </div>
                      <div className="flex justify-end">
                        {installedSkill ? (
                          <span className="text-xs px-2.5 py-1 rounded-md border text-muted-foreground inline-flex items-center gap-1">
                            <Download className="w-3.5 h-3.5" />
                            {t('installed', 'Installed')}
                          </span>
                        ) : (
                          <button
                            className="text-xs px-2.5 py-1 rounded-md bg-primary text-primary-foreground inline-flex items-center gap-1"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleInstall(item);
                            }}
                          >
                            <Download className="w-3.5 h-3.5" />
                            {t('install', 'Install')}
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      <Dialog
        open={catalogDetailOpen}
        onOpenChange={(open) => {
          setCatalogDetailOpen(open);
          if (!open) {
            setCatalogDetailData(null);
            setCatalogDetailInstallTarget(null);
          }
        }}
      >
        {catalogDetailOpen && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="px-6 pt-6 pb-4 border-b">
              <DialogTitle>{catalogDetailData?.skill.name}</DialogTitle>
              <DialogDescription>{catalogDetailData?.skill.description}</DialogDescription>
            </DialogHeader>
            <div className="px-6 py-4 min-h-0 overflow-auto">
              <div className="border rounded-md p-4 prose prose-sm dark:prose-invert max-w-none">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{catalogDetailData?.markdown || ''}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="border-t px-6 py-4 flex items-center gap-2">
              <button
                className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
                onClick={handleOpenCatalogFolder}
                disabled={loading || !catalogDetailData}
              >
                <FolderOpen className="w-4 h-4" />
                {t('openFolder', 'Open Folder')}
              </button>
              {catalogDetailInstallTarget?.repo_key && (
                <button
                  className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
                  onClick={handleOpenReloadPreview}
                  disabled={loading}
                >
                  {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
                  {t('skillsReload', 'Compare & Apply')}
                </button>
              )}
              {hasInstallableRepoModels(catalogDetailInstallTarget) && (
                <button
                  className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium inline-flex items-center gap-2 disabled:opacity-50"
                  onClick={handleInstallFromCatalogDetail}
                  disabled={loading}
                >
                  {loading && <RefreshCw className="w-4 h-4 animate-spin" />}
                  <Download className="w-4 h-4" />
                  {t('install', 'Install')}
                </button>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog
        open={installDialogOpen}
        onOpenChange={(open) => {
          if (installSubmitting && !open) return;
          setInstallDialogOpen(open);
          if (!open) {
            setInstallTarget(null);
            setInstallMode('catalog');
            setInstallModels([]);
          }
        }}
      >
        {installDialogOpen && (
          <DialogContent className="max-w-xl">
            <DialogHeader>
              <DialogTitle>{t('skillsInstallSelectModelsTitle', 'Select models to install')}</DialogTitle>
              <DialogDescription>
                {t('skillsInstallSelectModelsDesc', 'Choose model targets for {{name}}', {
                  name: installTarget?.name || '',
                })}
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t('sourceModels', 'Apply Models')}</label>
              <div className="grid grid-cols-2 gap-2">
                {installAllowedModels.map((model) => {
                  const option = skillModelOptions.find((item) => item.id === model);
                  if (!option) return null;
                  const active = installModels.includes(model);
                  return (
                    <button
                      key={`install-model-${model}`}
                      type="button"
                      onClick={() => toggleInstallModel(model)}
                      className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                        active
                          ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                          : 'bg-background hover:bg-muted/50 text-foreground border-border'
                      }`}
                    >
                      <option.Icon className="w-4 h-4 shrink-0" />
                      <span className="truncate">{option.label}</span>
                    </button>
                  );
                })}
              </div>
              {installModels.length === 0 && (
                <p className="text-xs text-destructive">{t('sourceModelsRequired', 'Select at least one model.')}</p>
              )}
            </div>
            <DialogFooter>
              <button
                type="button"
                onClick={() => {
                  setInstallDialogOpen(false);
                  setInstallTarget(null);
                  setInstallMode('catalog');
                  setInstallModels([]);
                }}
                className="px-4 py-2 border rounded-md text-sm hover:bg-muted"
                disabled={installSubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                disabled={!canSubmitInstall}
                onClick={handleInstallConfirm}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium disabled:opacity-50 inline-flex items-center gap-2"
              >
                {installSubmitting && <RefreshCw className="w-4 h-4 animate-spin" />}
                {t('install', 'Install')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={detailOpen} onOpenChange={setDetailOpen}>
        {detailOpen && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="px-6 pt-6 pb-4 border-b">
              <DialogTitle>{detailData?.skill.name}</DialogTitle>
              <DialogDescription>{detailData?.skill.description}</DialogDescription>
            </DialogHeader>
            <div className="px-6 py-4 min-h-0 overflow-auto">
              <div className="border rounded-md p-4 prose prose-sm dark:prose-invert max-w-none">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{detailData?.markdown || ''}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="border-t px-6 py-4">
              <button
                className="px-4 py-2 border rounded-md text-sm hover:bg-muted inline-flex items-center gap-2 disabled:opacity-50"
                onClick={() => detailData && handleOpenFolder(detailData.skill)}
                disabled={!detailData}
              >
                <FolderOpen className="w-4 h-4" />
                {t('openFolder', 'Open Folder')}
              </button>
              <button
                className="px-4 py-2 border rounded-md text-sm text-destructive hover:bg-destructive/10 inline-flex items-center gap-2 disabled:opacity-50"
                onClick={() => detailData && handleUninstall(detailData.skill)}
                disabled={!detailData}
              >
                <Trash2 className="w-4 h-4" />
                {t('uninstall', 'Uninstall')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog
        open={reloadOpen}
        onOpenChange={(open) => {
          setReloadOpen(open);
          if (!open) {
            setReloadPreview(null);
            setReloadSelectedPath('');
            setReloadTargetRepoKey(null);
          }
        }}
      >
        {reloadOpen && (
          <DialogContent className="w-[calc(100vw-2rem)] max-w-7xl h-[90vh] max-h-[90vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="px-6 pt-6 pb-4 border-b">
              <DialogTitle>{t('skillsReloadPreviewTitle', 'Compare & Apply Preview')}</DialogTitle>
              <DialogDescription>
                {t(
                  'skillsReloadPreviewDesc',
                  'Compare indexed baseline and current repository snapshot before applying changes.'
                )}
              </DialogDescription>
            </DialogHeader>

            <div className="px-6 py-4 min-h-0 overflow-auto">
              <div className="space-y-3">
                <div className="text-xs text-muted-foreground">
                  {reloadPreview?.before_label || t('localVersion', 'Local')}
                  {' -> '}
                  {reloadPreview?.after_label || t('remoteVersion', 'Remote')}
                </div>
                {(reloadPreview?.installed_targets || []).length > 0 ? (
                  <div className="space-y-2">
                    <div className="text-xs text-muted-foreground">
                      {t('skillsReloadInstalledTargets', 'Installed targets')}
                    </div>
                    <div className="rounded-md border px-3 py-2">
                      <div className="flex flex-wrap gap-1.5">
                        {(reloadPreview?.installed_targets || []).map((target, index) => (
                          <span
                            key={`reload-target-${target.model}-${target.scope}-${target.project_root || 'global'}-${index}`}
                            className="text-[11px] px-2 py-1 rounded border bg-muted/40 text-muted-foreground"
                          >
                            {formatInstalledTarget(target)}
                          </span>
                        ))}
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="text-xs text-muted-foreground">
                    {t('skillsReloadNoInstalledTargets', 'This skill is not installed to any target.')}
                  </div>
                )}

                {!reloadPreview?.has_changes ? (
                  <div className="rounded-md border border-dashed px-4 py-5 text-sm text-muted-foreground">
                    {t('skillsReloadNoChanges', 'No differences found between repository snapshot and latest source.')}
                  </div>
                ) : (
                  <div className="grid grid-cols-1 lg:grid-cols-[280px,1fr] gap-3">
                    <div className="border rounded-md max-h-[58vh] overflow-auto divide-y">
                      {(reloadPreview?.changed_files || []).map((file) => {
                        const active = reloadSelectedPath === file.path;
                        const statusMeta = getReloadStatusMeta(file.status);
                        return (
                          <button
                            key={`reload-file-${file.path}`}
                            type="button"
                            onClick={() => setReloadSelectedPath(file.path)}
                            className={`w-full text-left px-3 py-2 transition-colors ${
                              active ? 'bg-muted' : 'hover:bg-muted/40'
                            }`}
                          >
                            <div className="flex items-center justify-between gap-2">
                              <span className="text-xs font-mono break-all">{file.path}</span>
                              <span className={`text-[10px] px-1.5 py-0.5 rounded border shrink-0 ${statusMeta.className}`}>
                                {statusMeta.label}
                              </span>
                            </div>
                            {file.is_binary && (
                              <div className="mt-1 text-[10px] text-muted-foreground">
                                {t('skillsReloadBinaryFile', 'Binary file')}
                              </div>
                            )}
                          </button>
                        );
                      })}
                    </div>

                    <div className="border rounded-md p-3">
                      {!reloadSelectedFile ? (
                        <div className="text-sm text-muted-foreground">
                          {t('skillsReloadSelectFile', 'Select a changed file to inspect details.')}
                        </div>
                      ) : reloadSelectedFile.is_binary ? (
                        <div className="text-sm text-muted-foreground">
                          {t('skillsReloadBinaryChanged', 'Binary file changed. Line-level diff is unavailable.')}
                        </div>
                      ) : (
                        <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
                          <div className="border rounded-md p-3 max-h-[52vh] overflow-auto">
                            <div className="text-xs font-semibold mb-2">
                              {reloadPreview?.before_label || t('localVersion', 'Local')}
                            </div>
                            <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                              {t('changedLines', 'Changed lines')}: {reloadSelectedDiff?.before_changed_lines.join(', ') || '--'}
                            </div>
                            {renderDiffDocument(
                              reloadSelectedDiff?.before_content || '',
                              reloadSelectedDiff?.before_changed_lines || []
                            )}
                          </div>
                          <div className="border rounded-md p-3 max-h-[52vh] overflow-auto">
                            <div className="text-xs font-semibold mb-2">
                              {reloadPreview?.after_label || t('remoteVersion', 'Remote')}
                            </div>
                            <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                              {t('changedLines', 'Changed lines')}: {reloadSelectedDiff?.after_changed_lines.join(', ') || '--'}
                            </div>
                            {renderDiffDocument(
                              reloadSelectedDiff?.after_content || '',
                              reloadSelectedDiff?.after_changed_lines || []
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </div>

            <DialogFooter className="border-t px-6 py-4">
              <button
                type="button"
                className="px-4 py-2 border rounded-md text-sm hover:bg-muted"
                onClick={() => setReloadOpen(false)}
                disabled={reloadSubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium inline-flex items-center gap-2 disabled:opacity-50"
                onClick={handleApplyReload}
                disabled={!reloadPreview || reloadSubmitting}
              >
                {reloadSubmitting && <RefreshCw className="w-4 h-4 animate-spin" />}
                {(reloadPreview?.installed_targets || []).length > 0
                  ? t('skillsReloadApplyAndSync', 'Sync to installed models')
                  : t('skillsReloadApplyIndexOnly', 'Update repository only')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
