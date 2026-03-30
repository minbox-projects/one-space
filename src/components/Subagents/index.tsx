import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { open } from '@tauri-apps/plugin-dialog';
import {
  Bot,
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
  X,
} from 'lucide-react';
import { subagentModelOptions, type SubagentModelId } from '../subagentsModelOptions';
import type {
  WorkspaceCapabilityContext,
  WorkspaceCapabilityEntry,
} from '../workspaceCapabilityContext';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '../ui/dialog';
import { useConfirmDialog } from '../ConfirmDialogProvider';

type ModelType = SubagentModelId;
type InstallScope = 'global' | 'project';

type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };
const WORKSPACE_SUBAGENTS_SYNC_EVENT = 'onespace:workspace-subagents-sync-request';

interface SubagentRecord {
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

interface CatalogSubagent {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: ModelType[];
  model?: string;
  tools?: string[];
  first_seen_at?: number;
}

interface StorageConfigLite {
  subagents_new_badge_hours?: number;
  subagents_sources?: Array<{ id?: string; name?: string }>;
}

interface SubagentDetail {
  subagent: SubagentRecord;
  markdown: string;
  local_path: string;
}

interface CatalogSubagentDetail {
  subagent: CatalogSubagent;
  markdown: string;
  source_path: string;
}

interface CatalogOpenFolderResult {
  repo_key: string;
  opened_path: string;
}

interface UpdateDiff {
  local_markdown: string;
  remote_markdown: string;
  local_changed_lines: number[];
  remote_changed_lines: number[];
  local_changed_blocks: { start_line: number; end_line: number; content: string }[];
  remote_changed_blocks: { start_line: number; end_line: number; content: string }[];
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

interface ReloadPreview {
  before_label: string;
  after_label: string;
  changed_files: ReloadChangedFile[];
  text_diffs: ReloadTextDiff[];
  installed_models: ModelType[];
  has_changes: boolean;
}

interface ReloadApplyResult {
  index_refreshed: boolean;
  synced_models: ModelType[];
  updated_files_count: number;
  applied_at: number;
}

interface SourceSyncState {
  source_id: string;
  last_synced_at?: number;
  last_status: string;
  last_error?: string;
}

interface SubagentsSyncState {
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

interface RepositorySubagentView {
  repo_key: string;
  subagent_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  source_path?: string;
  name: string;
  description: string;
  models: ModelType[];
  model?: string;
  tools?: string[];
  icon_seed: string;
  hash?: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: RepoModelInstallState;
}

interface RepoImportFolderResult {
  repo_key: string;
  subagent_id: string;
  source_id: string;
  source_rel_path: string;
}

interface InstallTargetSubagent extends CatalogSubagent {
  repo_key?: string;
  installed?: RepoModelInstallState;
}

const modelTabs: { id: ModelType; label: string }[] = [
  { id: 'claude', label: 'Claude' },
  { id: 'gemini', label: 'Gemini' },
  { id: 'codex', label: 'Codex' },
  { id: 'opencode', label: 'OpenCode' },
];

const modelIconMap: Record<ModelType, ComponentType<{ className?: string }>> = subagentModelOptions.reduce(
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

function errorContainsCode(error: unknown, code: string) {
  return String(error ?? '').includes(code);
}

function catalogSupportsModel(item: CatalogSubagent, model: ModelType) {
  if (Array.isArray(item.models) && item.models.length > 0) {
    return item.models.includes(model);
  }
  return String(item.model || '').trim().toLowerCase() === model;
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

export function Subagents({
  isVisible = true,
  lockedProjectRoot,
  workspaceInstalledOnly = false,
  workspaceContext,
  onConsumeWorkspaceContext,
  onDismissWorkspaceContext,
  onNavigateToGlobalPage,
}: {
  isVisible?: boolean;
  lockedProjectRoot?: string;
  workspaceInstalledOnly?: boolean;
  workspaceContext?: WorkspaceCapabilityContext;
  onConsumeWorkspaceContext?: () => void;
  onDismissWorkspaceContext?: () => void;
  onNavigateToGlobalPage?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const lockedProjectRootNormalized = lockedProjectRoot?.trim() || '';
  const isLockedProjectRoot = lockedProjectRootNormalized.length > 0;
  const isWorkspaceDetail = isLockedProjectRoot;
  const installedOnlyMode = isLockedProjectRoot && workspaceInstalledOnly && !workspaceContext;
  const effectiveWorkspaceRoot = lockedProjectRootNormalized;
  const forceProjectInstall = isLockedProjectRoot;
  const showWorkspaceContextBanner = false;

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
    installedOnlyMode ? 'installed' : 'recommended',
  );
  const installedSectionRef = useRef<HTMLDivElement | null>(null);
  const discoverySectionRef = useRef<HTMLDivElement | null>(null);
  const lastAutoRevealKeyRef = useRef('');
  const [recommendedSourceFilter, setRecommendedSourceFilter] = useState<'all' | string>('all');
  const [recommendedSearch, setRecommendedSearch] = useState('');
  const [repositorySourceFilter, setRepositorySourceFilter] = useState<'all' | 'local' | 'remote'>('all');
  const [repositorySearch, setRepositorySearch] = useState('');
  const [installedByModel, setInstalledByModel] = useState<Record<ModelType, SubagentRecord[]>>({
    claude: [],
    gemini: [],
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
  const [loading, setLoading] = useState(installedOnlyMode);
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
  const allModels: ModelType[] = ['claude', 'gemini', 'codex', 'opencode'];
  const [reinstallingKeys, setReinstallingKeys] = useState<Record<string, boolean>>({});
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installTarget, setInstallTarget] = useState<InstallTargetSubagent | null>(null);
  const [installMode, setInstallMode] = useState<'catalog' | 'repository'>('catalog');
  const [installModels, setInstallModels] = useState<ModelType[]>([]);
  const [installScope, setInstallScope] = useState<InstallScope>('global');
  const [installProjectRoot, setInstallProjectRoot] = useState('');
  const [installFormError, setInstallFormError] = useState('');
  const [activeProjectRoot, setActiveProjectRoot] = useState(() => {
    return lockedProjectRootNormalized;
  });
  const [installSubmitting, setInstallSubmitting] = useState(false);

  const groupInstalledSubagentsByModel = (subagents: SubagentRecord[]) => {
    const next: Record<ModelType, SubagentRecord[]> = {
      claude: [],
      gemini: [],
      codex: [],
      opencode: [],
    };
    subagents.forEach((subagent) => {
      const model = subagent.model as ModelType;
      if (model === 'claude' || model === 'gemini' || model === 'codex' || model === 'opencode') {
        next[model].push(subagent);
      }
    });
    return next;
  };

  const mergeInstalledSubagentsByModel = (
    prev: Record<ModelType, SubagentRecord[]>,
    subagents: SubagentRecord[],
  ) => {
    const next: Record<ModelType, SubagentRecord[]> = {
      claude: [...(prev.claude || [])],
      gemini: [...(prev.gemini || [])],
      codex: [...(prev.codex || [])],
      opencode: [...(prev.opencode || [])],
    };
    for (const subagent of subagents) {
      const model = subagent.model as ModelType;
      if (!(model in next)) continue;
      const exists = next[model].some(
        (item) =>
          item.id === subagent.id &&
          (item.scope || 'global') === (subagent.scope || 'global') &&
          (item.project_root || '') === (subagent.project_root || '')
      );
      if (!exists) {
        next[model].push(subagent);
      }
    }
    return next;
  };

  const loadInstalledAll = async () => {
    const requestSeq = installedLoadSeqRef.current + 1;
    installedLoadSeqRef.current = requestSeq;
    const projectRoot = isLockedProjectRoot ? lockedProjectRootNormalized : activeProjectRoot.trim();
    const requests: Promise<ApiResp<SubagentRecord[]>>[] = [];
    if (isLockedProjectRoot && projectRoot) {
      requests.push(
        invoke<ApiResp<SubagentRecord[]>>('subagents_list_installed', {
          model: null,
          scope: 'project',
          projectRoot: projectRoot,
        })
      );
    } else {
      requests.push(
        invoke<ApiResp<SubagentRecord[]>>('subagents_list_installed', {
          model: null,
          scope: 'global',
          projectRoot: null,
        })
      );
    }
    const responses = await Promise.all(requests);
    const merged = responses.flatMap((resp) => resp.data || []);
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
    const res = await invoke<ApiResp<CatalogSubagent[]>>('subagents_list_catalog', {
      model: null,
    });
    setCatalog(res.data || []);
  };

  const fetchRepositorySubagents = async (includeUpdate = false) => {
    const projectRoot = isLockedProjectRoot ? lockedProjectRootNormalized : activeProjectRoot.trim();
    const requests: Promise<ApiResp<RepositorySubagentView[]>>[] = [];
    if (isLockedProjectRoot && projectRoot) {
      requests.push(
        includeUpdate
          ? invoke<ApiResp<RepositorySubagentView[]>>('subagents_repo_list_with_update', {
              scope: 'project',
              projectRoot: projectRoot,
            })
          : invoke<ApiResp<RepositorySubagentView[]>>('subagents_repo_list', {
              includeUpdate: false,
              scope: 'project',
              projectRoot: projectRoot,
            })
      );
    } else {
      requests.push(
        includeUpdate
          ? invoke<ApiResp<RepositorySubagentView[]>>('subagents_repo_list_with_update', {
              scope: 'global',
            })
          : invoke<ApiResp<RepositorySubagentView[]>>('subagents_repo_list', {
              includeUpdate: false,
              scope: 'global',
            })
      );
    }
    const responses = await Promise.all(requests);
    const mergedRows = responses.flatMap((res) => res.data || []);
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
    const rows = await fetchRepositorySubagents(includeUpdate);
    setRepositorySubagents(rows);
    return rows;
  };

  const loadSyncState = async () => {
    const res = await invoke<ApiResp<SubagentsSyncState>>('subagents_sync_status_get');
    setSyncState(res.data);
    const syncAt = Number(res.data?.last_sync_at || 0);
    if (syncAt > 0) {
      lastSeenSyncAtRef.current = syncAt;
    }
  };

  const loadDisplayConfig = async () => {
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
    if (installedOnlyMode) {
      await loadInstalledAll();
      return;
    }
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
          if (!installedOnlyMode) {
            try {
              await invoke('subagents_rescan_mirror');
            } catch {
              // ignore best-effort rescan errors
            }
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
  }, [isVisible, activeMode, activeProjectRoot, installedOnlyMode]);

  useEffect(() => {
    if (!isVisible || installedOnlyMode) return;
    let pending = false;
    const pollSyncState = async () => {
      if (pending) return;
      pending = true;
      try {
        const res = await invoke<ApiResp<SubagentsSyncState>>("subagents_sync_status_get");
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
  }, [isVisible, activeMode, activeProjectRoot, installedOnlyMode]);

  useEffect(() => {
    if (!isVisible || installedOnlyMode || activeMode !== 'repository') return;
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
  }, [isVisible, activeMode, activeProjectRoot, installedOnlyMode]);

  useEffect(() => {
    if (!isVisible || installedOnlyMode || activeMode !== 'recommended' || !hasConfiguredSources) return;
    triggerSyncSources(false).catch(() => undefined);
  }, [isVisible, activeMode, hasConfiguredSources, installedOnlyMode]);

  useEffect(() => {
    if (!isLockedProjectRoot) return;
    setActiveProjectRoot(lockedProjectRootNormalized);
    setInstallProjectRoot(lockedProjectRootNormalized);
    setInstallScope('project');
  }, [isLockedProjectRoot, lockedProjectRootNormalized]);

  useEffect(() => {
    if (isWorkspaceDetail) return;
    if (!workspaceContext?.entry) return;
    setActiveMode(workspaceContext.entry);
  }, [isWorkspaceDetail, workspaceContext?.entry]);

  useEffect(() => {
    if (!isWorkspaceDetail || activeMode !== 'installed') return;
    setActiveMode('recommended');
  }, [activeMode, isWorkspaceDetail]);

  useEffect(() => {
    if (!installedOnlyMode || activeMode === 'installed') return;
    setActiveMode('installed');
  }, [activeMode, installedOnlyMode]);

  useEffect(() => {
    if (!installedOnlyMode) return;
    setCatalog([]);
    setRepositorySubagents([]);
    setSyncState(null);
  }, [installedOnlyMode]);

  useEffect(() => {
    if (!isVisible || !initialLoadDone) return;
    reloadAll(activeMode === 'repository').catch(console.error);
  }, [isVisible, initialLoadDone, activeProjectRoot, activeMode, installedOnlyMode]);

  useEffect(() => {
    if (isLockedProjectRoot || !activeProjectRoot) return;
    setActiveProjectRoot('');
  }, [activeProjectRoot, isLockedProjectRoot]);

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
  const workspaceVisibleRepository = useMemo(
    () =>
      isWorkspaceDetail
        ? visibleRepository.filter((repo) => repo.models.includes(activeModel))
        : visibleRepository,
    [activeModel, isWorkspaceDetail, visibleRepository],
  );

  useEffect(() => {
    if (!isWorkspaceDetail || !isVisible || !initialLoadDone) return;
    if (visibleInstalled.length > 0) return;
    const revealKey = `${activeModel}:${activeMode}`;
    if (lastAutoRevealKeyRef.current === revealKey) return;
    lastAutoRevealKeyRef.current = revealKey;
    window.setTimeout(() => {
      discoverySectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 60);
  }, [activeMode, activeModel, initialLoadDone, isVisible, isWorkspaceDetail, visibleInstalled.length]);

  const handleWorkspaceDiscoveryEntry = (entry: 'recommended' | 'repository') => {
    setActiveMode(entry);
    window.setTimeout(() => {
      discoverySectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 20);
  };

  const revealInstalledModels = (models: ModelType[]) => {
    if (models.length === 0) return;
    const nextModel = models.includes(activeModel) ? activeModel : models[0];
    if (nextModel !== activeModel) {
      setActiveModel(nextModel);
    }
    if (!isWorkspaceDetail) return;
    window.setTimeout(() => {
      installedSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 20);
  };

  const handleRefreshWorkspaceStation = async () => {
    const startedAt = Date.now();
    setLoading(true);
    try {
      await reloadAll(activeMode === 'repository');
    } finally {
      const elapsed = Date.now() - startedAt;
      if (elapsed < TAB_LOADING_MIN_MS) {
        await new Promise((resolve) => window.setTimeout(resolve, TAB_LOADING_MIN_MS - elapsed));
      }
      setLoading(false);
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
    if (installedOnlyMode) return;
    if (sourceSyncingRef.current) return;
    sourceSyncingRef.current = true;
    try {
      if (manual) {
        setLoading(true);
      }
      setRefreshingSources(true);
      await invoke('subagents_sync_now');
      await reloadAll();
      if (manual) {
        setMessage({ type: 'success', text: t('subagentsSourceSyncSuccess', 'Subagents sources synced successfully') });
      }
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

  const handleSyncSources = async () => {
    await triggerSyncSources(true);
  };

  const handleSyncSourcesRef = useRef(handleSyncSources);
  handleSyncSourcesRef.current = handleSyncSources;

  const consumeOneShotWorkspaceContext = () => {
    if (workspaceContext?.persistence === 'one_shot') {
      onConsumeWorkspaceContext?.();
    }
  };

  useEffect(() => {
    if (!isVisible || !isLockedProjectRoot || installedOnlyMode) return;
    let disposed = false;
    let unlisten: null | (() => void | Promise<void>) = null;

    const disposeListener = () => {
      if (!unlisten) return;
      const cleanup = unlisten;
      unlisten = null;
      void Promise.resolve(cleanup()).catch(() => {
        // Tauri may already have removed the listener during rapid remounts.
      });
    };

    const register = async () => {
      try {
        const cleanup = await listen(WORKSPACE_SUBAGENTS_SYNC_EVENT, () => {
          void handleSyncSourcesRef.current();
        });
        if (disposed) {
          void Promise.resolve(cleanup()).catch(() => {
            // Ignore already-disposed listeners during teardown races.
          });
          return;
        }
        unlisten = cleanup;
      } catch (error) {
        console.error('Failed to register workspace subagents sync listener', error);
      }
    };

    void register();

    return () => {
      disposed = true;
      disposeListener();
    };
  }, [installedOnlyMode, isLockedProjectRoot, isVisible]);

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

  const installSubagentToModels = async (
    item: CatalogSubagent,
    selectedModels: ModelType[],
    scope: InstallScope,
    projectRoot?: string
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
          invoke('subagents_install', {
            input: {
              source_id: item.source_id,
              subagent_ref: item.rel_path,
              model,
              scope,
              project_root: scope === 'project' ? projectRoot : undefined,
            },
          })
        )
      );
      await reloadAll();
      if (scope === 'project' && projectRoot?.trim()) {
        const nextProjectRoot = projectRoot.trim();
        setActiveProjectRoot(nextProjectRoot);
        const requestSeq = installedLoadSeqRef.current + 1;
        installedLoadSeqRef.current = requestSeq;
        const projectRes = await invoke<ApiResp<SubagentRecord[]>>('subagents_list_installed', {
          model: null,
          scope: 'project',
          projectRoot: nextProjectRoot,
        });
        if (requestSeq !== installedLoadSeqRef.current) {
          return;
        }
        if (isWorkspaceDetail) {
          setInstalledByModel(groupInstalledSubagentsByModel(projectRes.data || []));
        } else {
          setInstalledByModel((prev) => mergeInstalledSubagentsByModel(prev, projectRes.data || []));
        }
      }
      notifyCountsChanged();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      if (succeeded > 0) {
        revealInstalledModels(targetModels);
        consumeOneShotWorkspaceContext();
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
    scope: InstallScope,
    projectRoot?: string
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
          invoke('subagents_repo_set_model', {
            input: {
              repo_key: item.repo_key,
              model,
              enabled: true,
              scope,
              project_root: scope === 'project' ? projectRoot : undefined,
            },
          })
        )
      );
      await reloadAll();
      if (scope === 'project' && projectRoot?.trim()) {
        const nextProjectRoot = projectRoot.trim();
        setActiveProjectRoot(nextProjectRoot);
        const requestSeq = installedLoadSeqRef.current + 1;
        installedLoadSeqRef.current = requestSeq;
        const projectRes = await invoke<ApiResp<SubagentRecord[]>>('subagents_list_installed', {
          model: null,
          scope: 'project',
          projectRoot: nextProjectRoot,
        });
        if (requestSeq !== installedLoadSeqRef.current) {
          return;
        }
        if (isWorkspaceDetail) {
          setInstalledByModel(groupInstalledSubagentsByModel(projectRes.data || []));
        } else {
          setInstalledByModel((prev) => mergeInstalledSubagentsByModel(prev, projectRes.data || []));
        }
      }
      notifyCountsChanged();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      if (succeeded > 0) {
        revealInstalledModels(targetModels);
        consumeOneShotWorkspaceContext();
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
    setInstallScope(forceProjectInstall ? 'project' : 'global');
    setInstallProjectRoot(forceProjectInstall ? effectiveWorkspaceRoot : '');
    setInstallFormError('');
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

  const handleInstallRepository = (repo: RepositorySubagentView) => {
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
    setInstallFormError('');
    const effectiveInstallScope: InstallScope = forceProjectInstall ? 'project' : installScope;
    const projectRoot = (forceProjectInstall ? effectiveWorkspaceRoot : installProjectRoot).trim();
    if (effectiveInstallScope === 'project' && !projectRoot) {
      setInstallFormError(t('installProjectRootRequired', 'Please choose a project folder for project scope install.'));
      return;
    }
    if (installMode === 'repository') {
      await installRepositoryToModels(
        installTarget,
        installModels,
        effectiveInstallScope,
        effectiveInstallScope === 'project' ? projectRoot : undefined
      );
    } else {
      await installSubagentToModels(
        installTarget,
        installModels,
        effectiveInstallScope,
        effectiveInstallScope === 'project' ? projectRoot : undefined
      );
    }
    if (effectiveInstallScope === 'project' && projectRoot) {
      setActiveProjectRoot(projectRoot);
    }
    setInstallDialogOpen(false);
    setInstallTarget(null);
    setInstallMode('catalog');
    setInstallModels([]);
    setInstallScope(forceProjectInstall ? 'project' : 'global');
    setInstallProjectRoot(forceProjectInstall ? effectiveWorkspaceRoot : '');
    setInstallFormError('');
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
    const ok = await confirmDialog(t('confirmDelete', { name: skill.name }), {
      okLabel: t('ok', 'OK'),
      cancelLabel: t('cancel', 'Cancel'),
    });
    if (!ok) return;

    try {
      setLoading(true);
      await invoke('subagents_uninstall', {
        input: {
          model: skill.model,
          subagent_id: skill.id,
          scope: skill.scope || 'global',
          project_root: skill.project_root || undefined,
        },
      });
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

    const reinstallKey = `${skill.model}:${skill.id}:${skill.scope || 'global'}:${skill.project_root || ''}`;
    setReinstallingKeys((prev) => ({ ...prev, [reinstallKey]: true }));
    try {
      setLoading(true);
      await invoke('subagents_repo_set_model', {
        input: {
          repo_key: matchedRepo.repo_key,
          model: skill.model,
          enabled: true,
          scope: skill.scope || 'global',
          project_root: skill.project_root || undefined,
        },
      });
      await reloadAll();
      setMessage({
        type: 'success',
        text: t('subagentsReinstallSuccess', 'Subagent reinstalled successfully.'),
      });
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
      await invoke('subagents_repo_delete', {
        input: {
          repo_key: repo.repo_key,
        },
      });
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
      const res = await invoke<ApiResp<SubagentDetail>>('subagents_detail_get', {
        input: {
          model: skill.model,
          subagent_id: skill.id,
          scope: skill.scope || 'global',
          project_root: skill.project_root || undefined,
        },
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
      const res = await invoke<ApiResp<CatalogSubagentDetail>>('subagents_catalog_detail_get', {
        input: {
          source_id: item.source_id,
          subagent_ref: item.rel_path,
        },
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
      const res = await invoke<ApiResp<CatalogSubagentDetail>>('subagents_repo_detail_get', {
        input: {
          repo_key: repo.repo_key,
        },
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
      const res = await invoke<ApiResp<CatalogOpenFolderResult>>('subagents_catalog_open_folder', {
        input: {
          source_id: catalogDetailData.subagent.source_id,
          subagent_ref: catalogDetailData.subagent.rel_path,
        },
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
      const res = await invoke<ApiResp<UpdateDiff>>('subagents_update_diff_preview', {
        input: {
          model: skill.model,
          subagent_id: skill.id,
          scope: skill.scope || 'global',
          project_root: skill.project_root || undefined,
        },
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
      const res = await invoke<ApiResp<ReloadPreview>>('subagents_repo_reload_preview', {
        input: { repo_key: repoKey },
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
      const res = await invoke<ApiResp<ReloadApplyResult>>('subagents_repo_reload_apply', {
        input: {
          repo_key: reloadTargetRepoKey,
          sync_to_models: shouldSync,
        },
      });
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
      await invoke('subagents_update_apply', {
        input: {
          model: diffSubagent.model,
          subagent_id: diffSubagent.id,
          scope: diffSubagent.scope || 'global',
          project_root: diffSubagent.project_root || undefined,
        },
      });
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
      await invoke('subagents_open_folder', {
        input: {
          model: skill.model,
          subagent_id: skill.id,
          scope: skill.scope || 'global',
          project_root: skill.project_root || undefined,
        },
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
      await invoke<ApiResp<RepoImportFolderResult>>('subagents_repo_import_folder', {
        input: { folder_path: selected },
      });
      await Promise.all([loadRepository(true), loadSyncState(), loadDisplayConfig()]);
      setMessage({ type: 'success', text: t('subagentsLocalImportRepoSuccess', 'Subagent imported to repository.') });
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

  return (
    <div className="flex h-full flex-col gap-4">
      {showWorkspaceContextBanner && workspaceContext && (
        <div className="flex items-start justify-between gap-3 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm">
          <div className="min-w-0">
            <p className="font-medium text-foreground">
              {t('workspaceContextBannerTitle', 'Installing for workspace {{name}}', {
                name: workspaceContext.workspaceName,
              })}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t('workspaceContextBannerDesc', 'This install will target {{path}} once, then the workspace context will clear automatically.', {
                path: workspaceContext.rootPath,
              })}
            </p>
          </div>
          <button
            type="button"
            onClick={onDismissWorkspaceContext}
            className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-background/80 hover:text-foreground"
            title={t('clear', 'Clear')}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      )}

      {isWorkspaceDetail ? (
        <>
          <div className="rounded-xl border bg-card p-4">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
              <div className="min-w-0">
                <h2 className="text-lg font-semibold tracking-tight">{t('subagents', 'Subagents')}</h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  {t(
                    'workspaceSubagentsSectionDesc',
                    'Manage project subagents available to this workspace from recommended, repository, and installed views.',
                  )}
                </p>
                <div className="mt-3 inline-flex max-w-full items-center gap-2 rounded-md border bg-muted/30 px-3 py-1.5 text-xs text-muted-foreground">
                  <FolderOpen className="h-3.5 w-3.5 shrink-0" />
                  <span className="truncate">
                    {t('workspaceTargetDirectory', 'Target directory')}: {lockedProjectRootNormalized}
                  </span>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2 lg:justify-end">
                <button
                  type="button"
                  onClick={() => {
                    void handleRefreshWorkspaceStation();
                  }}
                  disabled={loading}
                  className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-60"
                >
                  <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
                  {t('refresh', 'Refresh')}
                </button>
                <button
                  type="button"
                  onClick={() => onNavigateToGlobalPage?.('recommended')}
                  className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
                >
                  <Sparkles className="h-4 w-4" />
                  {t('workspaceManageSources', 'Manage Sources')}
                </button>
                <button
                  type="button"
                  onClick={() => onNavigateToGlobalPage?.('repository')}
                  className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
                >
                  <BookOpen className="h-4 w-4" />
                  {t('workspaceOpenRepository', 'Open Repository')}
                </button>
              </div>
            </div>
          </div>

          {(message || loading) && (
            <div className="flex flex-wrap items-center justify-end gap-2">
              {loading && (
                <div className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t('loading', 'Loading...')}
                </div>
              )}
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
            </div>
          )}
        </>
      ) : (
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-xl font-bold tracking-tight">{t('subagents', 'Subagents')}</h2>
            <p className="text-sm text-muted-foreground">
              {t('subagentsDesc', 'Manage subagents by model')}
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
                {t('subagentsSyncSources', '同步源列表')}
              </button>
            )}
            {activeMode === 'repository' && !isLockedProjectRoot && (
              <button
                onClick={handleImportRepositoryFolder}
                disabled={loading}
                className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
              >
                <FolderPlus className="w-4 h-4" />
                {t('subagentsLocalImportButton', 'Import From Folder')}
              </button>
            )}
          </div>
        </div>
      )}

      {!installedOnlyMode && !isWorkspaceDetail && (
        <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
          <button
            onClick={handleSwitchToRecommended}
            className={`px-3 py-1.5 rounded-md text-sm ${
              activeMode === 'recommended'
                ? 'bg-black text-white'
                : 'bg-white text-black'
            }`}
          >
            {t('recommended', '推荐')}
          </button>
          <button
            onClick={handleSwitchToRepository}
            className={`px-3 py-1.5 rounded-md text-sm ${
              activeMode === 'repository'
                ? 'bg-black text-white'
                : 'bg-white text-black'
            }`}
          >
            {t('repository', '仓库')}
          </button>
          <button
            onClick={() => setActiveMode('installed')}
            className={`px-3 py-1.5 rounded-md text-sm ${
              activeMode === 'installed'
                ? 'bg-black text-white'
                : 'bg-white text-black'
            }`}
          >
            {t('installed', '已安装')}
          </button>
        </div>
      )}

      {(isWorkspaceDetail || activeMode !== 'repository') && (
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
                      {isWorkspaceDetail
                        ? t('subagentsInstalledCount', 'Installed {{count}} subagents', { count: installedCounts[m.id] ?? 0 })
                        : activeMode === 'recommended'
                        ? t('subagentsRecommendedCount', 'Recommended {{count}} subagents', { count: recommendedCounts[m.id] ?? 0 })
                        : t('subagentsInstalledCount', 'Installed {{count}} subagents', { count: installedCounts[m.id] ?? 0 })}
                    </span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {(isWorkspaceDetail || activeMode === 'installed') && (
        <>
          {isWorkspaceDetail && (
            <div ref={installedSectionRef} className="rounded-xl border bg-card p-4">
              <div className="flex flex-col gap-1">
                <h3 className="text-base font-semibold tracking-tight">
                  {t('workspaceInstalledSectionTitle', 'Installed in This Workspace')}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {t(
                    'workspaceInstalledSubagentsSectionDesc',
                    'Review, update, or remove subagents already installed in this workspace.',
                  )}
                </p>
              </div>
            </div>
          )}
          {!initialLoadDone ? (
            <div className="text-center py-12 text-muted-foreground">
              <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
              <p>{t('loading', 'Loading...')}</p>
            </div>
          ) : visibleInstalled.length === 0 ? (
            <div className="text-center py-12">
              <Bot className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noInstalledSubagentsForModel', '该模型下暂无已安装 Subagents')}</h3>
              <p className="text-muted-foreground">
                {installedOnlyMode
                  ? t(
                      'workspaceNoInstalledSubagentsForModelDesc',
                      'This workspace has no installed subagents for the selected model yet.',
                    )
                  : t('noInstalledSubagentsForModelDesc', '你可以先到“推荐”中安装 Subagents。')}
              </p>
              {isWorkspaceDetail ? (
                <div className="mt-4 flex flex-wrap justify-center gap-2">
                  <button
                    onClick={() => handleWorkspaceDiscoveryEntry('recommended')}
                    className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
                  >
                    {t('workspaceInstallFromRecommended', 'Install from Recommended')}
                  </button>
                  <button
                    onClick={() => handleWorkspaceDiscoveryEntry('repository')}
                    className="px-4 py-2 rounded-md border text-sm hover:bg-muted"
                  >
                    {t('workspaceInstallFromRepository', 'Install from Repository')}
                  </button>
                </div>
              ) : !installedOnlyMode ? (
                <button
                  onClick={handleSwitchToRecommended}
                  className="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
                >
                  {t('recommended', '推荐')}
                </button>
              ) : null}
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleInstalled.map((skill) => {
                const Icon = pickIcon(skill.icon_seed || skill.id);
                const reinstallKey = `${skill.model}:${skill.id}:${skill.scope || 'global'}:${skill.project_root || ''}`;
                const reinstalling = !!reinstallingKeys[reinstallKey];
                const matchedRepo = findLatestRepository({
                  source_id: skill.source_id,
                  source_rel_path: skill.source_rel_path,
                  id: skill.id,
                  dir_name: skill.dir_name,
                });
                const modelText = (matchedRepo?.model || String(skill.model || '')).trim();
                const toolsText = (matchedRepo?.tools || []).filter(Boolean).join(', ');
                return (
                  <div
                    key={`${skill.model}:${skill.id}:${skill.scope || 'global'}:${skill.project_root || ''}`}
                    className="border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer"
                    onClick={() => handleOpenDetail(skill)}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="p-2 rounded-md bg-primary/10 text-primary">
                        <Icon className="w-4 h-4" />
                      </div>
                      <div className="flex flex-col items-end gap-1">
                        <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                          {skill.dir_name || skill.source_rel_path.split('/').pop() || skill.id}
                        </span>
                        <span className="text-[10px] px-1.5 py-0.5 rounded border bg-muted/50 text-muted-foreground">
                          {skill.scope === 'project'
                            ? t('installScopeProjectShort', 'Project')
                            : t('installScopeGlobalShort', 'Global')}
                        </span>
                        {skill.has_update && (
                          <button
                            className="text-[10px] px-2 py-0.5 rounded-full bg-amber-100 text-amber-700 border border-amber-200"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleOpenDiff(skill);
                            }}
                          >
                            {t('hasUpdate', '有更新')}
                          </button>
                        )}
                      </div>
                    </div>

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{skill.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{skill.description}</p>
                    {(modelText || toolsText) && (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5">
                        {modelText && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-700 border-blue-500/30 inline-flex items-center gap-1">
                            <Cpu className="w-3 h-3" />
                            {modelText}
                          </span>
                        )}
                        {toolsText && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-amber-500/10 text-amber-700 border-amber-500/30 inline-flex items-center gap-1 max-w-full">
                            <Wrench className="w-3 h-3 shrink-0" />
                            <span className="truncate">{toolsText}</span>
                          </span>
                        )}
                      </div>
                    )}

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
                        {t('subagentsReinstall', '重新安装')}
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

      {isWorkspaceDetail && (
        <div ref={discoverySectionRef} className="rounded-xl border bg-card p-4">
          <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
            <div>
              <h3 className="text-base font-semibold tracking-tight">
                {t('workspaceDiscoverySectionTitle', 'Discover and Install')}
              </h3>
              <p className="text-sm text-muted-foreground">
                {t(
                  'workspaceSubagentsDiscoveryDesc',
                  'Find recommended or repository subagents and install them directly into this workspace.',
                )}
              </p>
            </div>
            <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
              <button
                onClick={() => handleWorkspaceDiscoveryEntry('recommended')}
                className={`px-3 py-1.5 rounded-md text-sm ${
                  activeMode === 'repository' ? 'bg-white text-black' : 'bg-black text-white'
                }`}
              >
                {t('recommended', '推荐')}
              </button>
              <button
                onClick={() => handleWorkspaceDiscoveryEntry('repository')}
                className={`px-3 py-1.5 rounded-md text-sm ${
                  activeMode === 'repository' ? 'bg-black text-white' : 'bg-white text-black'
                }`}
              >
                {t('repository', '仓库')}
              </button>
            </div>
          </div>
        </div>
      )}

      {activeMode === 'repository' && (
        <>
          <div className="mb-4 flex items-center gap-2">
            <div className="w-[170px] sm:w-[220px] lg:w-[260px] shrink-0">
              <input
                value={repositorySearch}
                onChange={(e) => setRepositorySearch(e.target.value)}
                placeholder={t('subagentsSearchPlaceholder', '搜索 Subagent 名称或描述')}
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
                    {t('subagentsSourceTypeLocalImport', '本地导入')}
                  </button>
                  <button
                    onClick={() => setRepositorySourceFilter('remote')}
                    className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                      repositorySourceFilter === 'remote'
                        ? 'bg-black text-white'
                        : 'bg-white text-black'
                    }`}
                  >
                    {t('subagentsSourceTypeRemote', '推荐源')}
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
          ) : workspaceVisibleRepository.length === 0 ? (
            <div className="text-center py-12">
              <Bot className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noResultsFound', 'No subagents found.')}</h3>
              <p className="text-muted-foreground mb-4">
                {t('subagentsRepoEmptyHint', '请从文件夹导入 Subagent，或先在推荐模式同步源列表。')}
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {workspaceVisibleRepository.map((repo) => {
                const Icon = pickIcon(repo.icon_seed || repo.subagent_id);
                const sourceMeta = getRepoSourceMeta(repo.source_type);
                const cardTitle = repo.name?.trim() || repo.dir_name || repo.source_rel_path.split('/').pop() || repo.subagent_id;
                const toolsText = (repo.tools || []).filter(Boolean).join(', ');
                const modelText = (repo.model || '').trim();
                const sourcePathText = repo.dir_name || repo.source_rel_path.split('/').pop() || repo.subagent_id;
                const installedCount = allModels.reduce(
                  (sum, model) => sum + (repo.installed[model] ? 1 : 0),
                  0,
                );
                const installableCount = allModels.filter(
                  (model) => repo.models.includes(model) && !repo.installed[model]
                ).length;
                const repoHasUpdate = !!repo.has_update;
                const isNewRepo = isRecentRepositorySubagent(repo);
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
                        <div className="flex items-center gap-1.5">
                          {isNewRepo && (
                            <span className="text-[10px] px-1.5 py-0.5 rounded border bg-emerald-500/10 text-emerald-700 border-emerald-500/30">
                              {t('new', 'New')}
                            </span>
                          )}
                          {repoHasUpdate && (
                            <span className="text-[10px] px-2 py-0.5 rounded-full bg-amber-100 text-amber-700 border border-amber-200">
                              {t('hasUpdate', '有更新')}
                            </span>
                          )}
                        </div>
                        <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                          {sourcePathText}
                        </span>
                      </div>
                    </div>

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{cardTitle}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{repo.description}</p>
                    {(modelText || toolsText) && (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5">
                        {modelText && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-700 border-blue-500/30 inline-flex items-center gap-1">
                            <Cpu className="w-3 h-3" />
                            {modelText}
                          </span>
                        )}
                        {toolsText && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-amber-500/10 text-amber-700 border-amber-500/30 inline-flex items-center gap-1 max-w-full">
                            <Wrench className="w-3 h-3 shrink-0" />
                            <span className="truncate">{toolsText}</span>
                          </span>
                        )}
                      </div>
                    )}
                    <div className="mt-3 text-[11px] text-muted-foreground flex items-center gap-4">
                      <span>
                        {t('subagentsRepositoryLastUpdated', '最后更新')}: {formatTs(repo.updated_at || repo.created_at)}
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
                        {installableCount > 0 && (
                          <button
                            className="text-xs px-2.5 py-1 rounded-md bg-primary text-primary-foreground inline-flex items-center gap-1"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleInstallRepository(repo);
                            }}
                          >
                            <Download className="w-3.5 h-3.5" />
                            {isWorkspaceDetail
                              ? t('workspaceInstallAction', 'Install to Workspace')
                              : t('install', 'Install')}
                          </button>
                        )}
                        {installedCount === 0 && (
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
                        )}
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
                    placeholder={t('subagentsSearchPlaceholder', '搜索 Subagent 名称或描述')}
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
              <Bot className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noRecommendedSubagents', '当前没有可推荐的 Subagents')}</h3>
              <p className="text-muted-foreground mb-4">{t('noRecommendedSubagentsDesc', '请检查 Subagents 源配置，或同步源列表后重试。')}</p>
            </div>
          ) : (
            <div className="relative z-0 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleCatalog.map((item) => {
                const installedSubagent = installedById.get(`${item.source_id}:${item.rel_path}`);
                const Icon = pickIcon(item.id);
                const srcStatus = sourceStatusMap.get(item.source_id);
                const isNewSubagent = isRecentCatalogSubagent(item);
                const cardTitle = item.name?.trim() || item.dir_name || item.rel_path.split('/').pop() || item.id;
                const toolsText = (item.tools || []).filter(Boolean).join(', ');
                const modelText = (item.model || '').trim();
                const sourcePathText = item.dir_name || item.rel_path.split('/').pop() || item.id;
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
                      <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                        {sourcePathText}
                      </span>
                    </div>
                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{cardTitle}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{item.description}</p>
                    {(modelText || toolsText) && (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5">
                        {modelText && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-700 border-blue-500/30 inline-flex items-center gap-1">
                            <Cpu className="w-3 h-3" />
                            {modelText}
                          </span>
                        )}
                        {toolsText && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-amber-500/10 text-amber-700 border-amber-500/30 inline-flex items-center gap-1 max-w-full">
                            <Wrench className="w-3 h-3 shrink-0" />
                            <span className="truncate">{toolsText}</span>
                          </span>
                        )}
                      </div>
                    )}
                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('lastSynced', 'Last synced')}: {formatTs(srcStatus?.last_synced_at || syncState?.last_sync_at)}
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-2">
                      <div className="flex items-center gap-2">
                        <span className="text-[10px] px-2 py-1 rounded border bg-muted/50 text-muted-foreground">
                          {item.source_id}
                        </span>
                        {isNewSubagent && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded border bg-emerald-500/10 text-emerald-700 border-emerald-500/30">
                            {t('new', 'New')}
                          </span>
                        )}
                      </div>
                      <div className="flex justify-end">
                        {installedSubagent ? (
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
                            {isWorkspaceDetail
                              ? t('workspaceInstallAction', 'Install to Workspace')
                              : t('install', 'Install')}
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
              <DialogTitle>{catalogDetailData?.subagent.name}</DialogTitle>
              <DialogDescription>{catalogDetailData?.subagent.description}</DialogDescription>
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
                  {t('subagentsReload', 'Compare & Apply')}
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
            setInstallScope(forceProjectInstall ? 'project' : 'global');
            setInstallProjectRoot(forceProjectInstall ? effectiveWorkspaceRoot : '');
            setInstallFormError('');
          }
        }}
      >
        {installDialogOpen && (
          <DialogContent className="max-w-xl">
            <DialogHeader>
              <DialogTitle>{t('subagentsInstallSelectModelsTitle', 'Select models to install')}</DialogTitle>
              <DialogDescription>
                {t('subagentsInstallSelectModelsDesc', 'Choose model targets for {{name}}', {
                  name: installTarget?.name || '',
                })}
              </DialogDescription>
            </DialogHeader>
            {forceProjectInstall && (
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('installProjectRoot', 'Project Folder')}
                </label>
                <div className="flex gap-2">
                  <input
                    value={effectiveWorkspaceRoot}
                    onChange={(e) => {
                      setInstallProjectRoot(e.target.value);
                      if (installFormError) setInstallFormError('');
                    }}
                    placeholder={t('installProjectRootPlaceholder', 'Choose a project folder')}
                    className="h-9 w-full rounded-lg border border-black/20 bg-white px-3 text-sm shadow-sm outline-none focus:border-black"
                    readOnly
                  />
                </div>
                {installFormError && <p className="text-xs text-destructive">{installFormError}</p>}
              </div>
            )}
            {(forceProjectInstall || installScope === 'project') && installModels.includes('codex') && (
              <p className="text-xs text-muted-foreground">
                {t(
                  'subagentsCodexProjectConfigHint',
                  'For Codex project installs, entries are written to .codex/config.toml (agents.*).'
                )}
              </p>
            )}
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t('sourceModels', 'Apply Models')}</label>
              <div className="grid grid-cols-2 gap-2">
                {installAllowedModels.map((model) => {
                  const option = subagentModelOptions.find((item) => item.id === model);
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
                  setInstallScope(forceProjectInstall ? 'project' : 'global');
                  setInstallProjectRoot(forceProjectInstall ? effectiveWorkspaceRoot : '');
                  setInstallFormError('');
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
              <DialogTitle>{detailData?.subagent.name}</DialogTitle>
              <DialogDescription>{detailData?.subagent.description}</DialogDescription>
            </DialogHeader>
            <div className="px-6 py-4 min-h-0 overflow-auto">
              <div className="border rounded-md p-4 prose prose-sm dark:prose-invert max-w-none">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{detailData?.markdown || ''}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="border-t px-6 py-4">
              <button
                className="px-4 py-2 border rounded-md text-sm hover:bg-muted inline-flex items-center gap-2 disabled:opacity-50"
                onClick={() => detailData && handleOpenFolder(detailData.subagent)}
                disabled={!detailData}
              >
                <FolderOpen className="w-4 h-4" />
                {t('openFolder', 'Open Folder')}
              </button>
              <button
                className="px-4 py-2 border rounded-md text-sm text-destructive hover:bg-destructive/10 inline-flex items-center gap-2 disabled:opacity-50"
                onClick={() => detailData && handleUninstall(detailData.subagent)}
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
              <DialogTitle>{t('subagentsReloadPreviewTitle', 'Compare & Apply Preview')}</DialogTitle>
              <DialogDescription>
                {t(
                  'subagentsReloadPreviewDesc',
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
                {(reloadPreview?.installed_models || []).length > 0 ? (
                  <div className="text-xs text-muted-foreground">
                    {t('subagentsReloadInstalledModels', 'Installed models')}: {(reloadPreview?.installed_models || []).join(', ')}
                  </div>
                ) : (
                  <div className="text-xs text-muted-foreground">
                    {t('subagentsReloadNoInstalledModels', 'This subagent is not installed to any model.')}
                  </div>
                )}

                {!reloadPreview?.has_changes ? (
                  <div className="rounded-md border border-dashed px-4 py-5 text-sm text-muted-foreground">
                    {t('subagentsReloadNoChanges', 'No differences found between baseline and current repository snapshot.')}
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
                                {t('subagentsReloadBinaryFile', 'Binary file')}
                              </div>
                            )}
                          </button>
                        );
                      })}
                    </div>

                    <div className="border rounded-md p-3">
                      {!reloadSelectedFile ? (
                        <div className="text-sm text-muted-foreground">
                          {t('subagentsReloadSelectFile', 'Select a changed file to inspect details.')}
                        </div>
                      ) : reloadSelectedFile.is_binary ? (
                        <div className="text-sm text-muted-foreground">
                          {t('subagentsReloadBinaryChanged', 'Binary file changed. Line-level diff is unavailable.')}
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
                {(reloadPreview?.installed_models || []).length > 0
                  ? t('subagentsReloadApplyAndSync', 'Sync to installed models')
                  : t('subagentsReloadApplyIndexOnly', 'Refresh index only')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={diffOpen} onOpenChange={setDiffOpen}>
        {diffOpen && (
          <DialogContent className="max-w-6xl">
            <DialogHeader>
              <DialogTitle>{t('updateDiff', 'Update Diff')}</DialogTitle>
              <DialogDescription>
                {t('subagentsUpdateDiffDesc', 'Compare local and remote subagent markdown before updating')}
              </DialogDescription>
            </DialogHeader>
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
              <div className="border rounded-md p-3 max-h-[58vh] overflow-auto">
                <div className="text-xs font-semibold mb-2">{t('localVersion', 'Local')}</div>
                <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                  {t('changedLines', 'Changed lines')}: {diffData?.local_changed_lines.join(', ') || '--'}
                </div>
                {renderDiffDocument(diffData?.local_markdown || '', diffData?.local_changed_lines || [])}
              </div>
              <div className="border rounded-md p-3 max-h-[58vh] overflow-auto">
                <div className="text-xs font-semibold mb-2">{t('remoteVersion', 'Remote')}</div>
                <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                  {t('changedLines', 'Changed lines')}: {diffData?.remote_changed_lines.join(', ') || '--'}
                </div>
                {renderDiffDocument(diffData?.remote_markdown || '', diffData?.remote_changed_lines || [])}
              </div>
            </div>
            <div className="flex justify-end">
              <button
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium"
                onClick={handleApplyUpdate}
              >
                {t('update', 'Update')}
              </button>
            </div>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
