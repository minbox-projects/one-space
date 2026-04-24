import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  BookOpen,
  Cpu,
  Download,
  FolderOpen,
  Info,
  Loader2,
  RefreshCw,
  Shield,
  Sparkles,
  Trash2,
  Wrench,
} from 'lucide-react';
import { useConfirmDialog } from '../ConfirmDialogProvider';
import { skillModelOptions, type SkillModelId } from '../skillsModelOptions';
import type { WorkspaceCapabilityEntry } from '../workspaceCapabilityContext';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';

type ModelType = SkillModelId;

type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };

type SkillRecord = {
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
  scope?: 'global' | 'project';
  project_root?: string | null;
};

type CatalogSkill = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: ModelType[];
  first_seen_at?: number;
};

type SkillDetail = {
  skill: SkillRecord;
  markdown: string;
  local_path: string;
};

type CatalogSkillDetail = {
  skill: CatalogSkill;
  markdown: string;
  source_path: string;
};

type CatalogOpenFolderResult = {
  repo_key: string;
  opened_path: string;
};

type RepoModelInstallState = {
  claude: boolean;
  gemini: boolean;
  codex: boolean;
  opencode: boolean;
};

type RepositorySkillView = {
  repo_key: string;
  skill_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  name: string;
  description: string;
  models: ModelType[];
  icon_seed: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: RepoModelInstallState;
};

type InstallTargetSkill = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: ModelType[];
  repo_key?: string;
  installed?: RepoModelInstallState;
};

type StorageConfigLite = {
  skills_sources?: Array<{ id?: string; name?: string }>;
};

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
  {} as Record<ModelType, ComponentType<{ className?: string }>>,
);

const iconPool = [Sparkles, Wrench, Shield, Cpu, BookOpen];

function createEmptyInstalledByModel(): Record<ModelType, SkillRecord[]> {
  return {
    claude: [],
    gemini: [],
    codex: [],
    opencode: [],
  };
}

function formatTs(ts?: number) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString();
}

function normalizeSourceNameMap(config: StorageConfigLite | null | undefined) {
  const next: Record<string, string> = {};
  (config?.skills_sources || []).forEach((item) => {
    const sourceId = String(item?.id || '').trim();
    const sourceName = String(item?.name || '').trim();
    if (sourceId) {
      next[sourceId] = sourceName || sourceId;
    }
  });
  return next;
}

function matchesRepositorySkill(
  repo: RepositorySkillView,
  candidate: { source_id: string; source_rel_path: string; id: string; dir_name?: string },
) {
  if (repo.source_id === candidate.source_id && repo.source_rel_path === candidate.source_rel_path) {
    return true;
  }
  if (repo.skill_id === candidate.id) {
    return true;
  }
  return !!candidate.dir_name && !!repo.dir_name && repo.dir_name === candidate.dir_name;
}

function getSourceTypeLabel(sourceType: string, t: (...args: any[]) => unknown) {
  switch (sourceType) {
    case 'remote':
      return String(t('skillsSourceTypeRemote', 'Recommended Source'));
    case 'local_import':
      return String(t('skillsSourceTypeLocalImport', 'Local Import'));
    case 'mirror':
      return String(t('skillsSourceTypeMirror', 'Mirror'));
    default:
      return sourceType || '-';
  }
}

function getScopeBadgeClassName(scope?: string) {
  return scope === 'global'
    ? 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300'
    : 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300';
}

function getCapabilityMergeKey(item: Pick<SkillRecord, 'id' | 'dir_name' | 'name' | 'source_rel_path'>) {
  return String(item.dir_name || item.id || item.source_rel_path.split('/').pop() || item.name || '')
    .trim()
    .toLowerCase();
}

function getSkillScopePriority(model: ModelType, skill: SkillRecord) {
  const isProject = skill.scope === 'project';
  switch (model) {
    case 'codex':
    case 'claude':
    case 'gemini':
    case 'opencode':
    default:
      return isProject ? 2 : 1;
  }
}

function mergeInstalledSkillsForModel(model: ModelType, skills: SkillRecord[]) {
  const exactSeen = new Set<string>();
  const exact = skills.filter((item) => {
    const key = [
      item.model,
      item.id,
      item.scope || 'global',
      item.project_root || '',
    ].join('::');
    if (exactSeen.has(key)) return false;
    exactSeen.add(key);
    return true;
  });

  const byName = new Map<string, SkillRecord>();
  exact.forEach((skill) => {
    const key = getCapabilityMergeKey(skill);
    if (!key) return;
    const previous = byName.get(key);
    if (!previous || getSkillScopePriority(model, skill) > getSkillScopePriority(model, previous)) {
      byName.set(key, skill);
    }
  });
  return Array.from(byName.values());
}

export function WorkspaceSkillsPanel({
  rootPath,
  isVisible = true,
  onNavigateToGlobalPage,
}: {
  rootPath: string;
  isVisible?: boolean;
  onNavigateToGlobalPage?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const normalizedRootPath = rootPath.trim();
  const discoverySectionRef = useRef<HTMLDivElement | null>(null);
  const loadSeqRef = useRef(0);

  const [activeModel, setActiveModel] = useState<ModelType>('claude');
  const [discoveryMode, setDiscoveryMode] = useState<'recommended' | 'repository'>('recommended');
  const [installedByModel, setInstalledByModel] = useState<Record<ModelType, SkillRecord[]>>(
    createEmptyInstalledByModel,
  );
  const [catalog, setCatalog] = useState<CatalogSkill[]>([]);
  const [repositorySkills, setRepositorySkills] = useState<RepositorySkillView[]>([]);
  const [sourceNamesById, setSourceNamesById] = useState<Record<string, string>>({});
  const [recommendedSourceFilter, setRecommendedSourceFilter] = useState<'all' | string>('all');
  const [repositorySourceFilter, setRepositorySourceFilter] = useState<'all' | 'local' | 'remote'>('all');
  const [recommendedSearch, setRecommendedSearch] = useState('');
  const [repositorySearch, setRepositorySearch] = useState('');
  const [loading, setLoading] = useState(false);
  const [initialLoadDone, setInitialLoadDone] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<SkillDetail | null>(null);
  const [catalogDetailOpen, setCatalogDetailOpen] = useState(false);
  const [catalogDetailData, setCatalogDetailData] = useState<CatalogSkillDetail | null>(null);
  const [catalogDetailInstallTarget, setCatalogDetailInstallTarget] = useState<InstallTargetSkill | null>(null);
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installMode, setInstallMode] = useState<'catalog' | 'repository'>('catalog');
  const [installTarget, setInstallTarget] = useState<InstallTargetSkill | null>(null);
  const [installModels, setInstallModels] = useState<ModelType[]>([]);
  const [installSubmitting, setInstallSubmitting] = useState(false);
  const [installError, setInstallError] = useState('');
  const [reinstallingKeys, setReinstallingKeys] = useState<Record<string, boolean>>({});

  const pickIcon = (seed: string) => {
    const sum = seed.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
    return iconPool[sum % iconPool.length];
  };

  const groupInstalledByModel = (skills: SkillRecord[]) => {
    const next = createEmptyInstalledByModel();
    skills.forEach((skill) => {
      const model = skill.model as ModelType;
      if (model in next) {
        next[model].push(skill);
      }
    });
    modelTabs.forEach((tab) => {
      next[tab.id] = mergeInstalledSkillsForModel(tab.id, next[tab.id]);
    });
    return next;
  };

  const dedupeInstalledSkills = (skills: SkillRecord[]) => {
    const seen = new Set<string>();
    return skills.filter((item) => {
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
  };

  const fetchProjectInstalledSkills = async () => {
    return invoke<ApiResp<SkillRecord[]>>('skills_list_installed', {
      model: null,
      scope: 'project',
      projectRoot: normalizedRootPath,
    });
  };

  const fetchInstalledSkills = async () => {
    const [globalRes, projectRes] = await Promise.allSettled([
      invoke<ApiResp<SkillRecord[]>>('skills_rescan_mirror'),
      fetchProjectInstalledSkills(),
    ]);
    const fallbackGlobalRes = async () =>
      invoke<ApiResp<SkillRecord[]>>('skills_list_installed', {
        model: null,
        scope: 'global',
        projectRoot: null,
      }).catch(() => null);
    const globalData =
      globalRes.status === 'fulfilled'
        ? globalRes.value.data || []
        : (await fallbackGlobalRes())?.data || [];
    if (projectRes.status !== 'fulfilled') {
      throw projectRes.reason;
    }
    return {
      ...projectRes.value,
      data: dedupeInstalledSkills([...globalData, ...(projectRes.value.data || [])]),
    };
  };

  const reloadAll = async () => {
    const seq = loadSeqRef.current + 1;
    loadSeqRef.current = seq;
    const [installedRes, catalogRes, repositoryRes, configRes] = await Promise.allSettled([
      fetchInstalledSkills(),
      invoke<ApiResp<CatalogSkill[]>>('skills_list_catalog', { model: null }),
      invoke<ApiResp<RepositorySkillView[]>>('skills_repo_list', {
        includeUpdate: false,
        scope: 'project',
        projectRoot: normalizedRootPath,
      }),
      invoke<StorageConfigLite>('get_storage_config'),
    ]);
    if (seq !== loadSeqRef.current) {
      return;
    }
    if (installedRes.status !== 'fulfilled') {
      throw installedRes.reason;
    }

    setInstalledByModel(groupInstalledByModel(installedRes.value.data || []));
    setCatalog(catalogRes.status === 'fulfilled' ? catalogRes.value.data || [] : []);
    setRepositorySkills(repositoryRes.status === 'fulfilled' ? repositoryRes.value.data || [] : []);
    setSourceNamesById(
      configRes.status === 'fulfilled' ? normalizeSourceNameMap(configRes.value) : {},
    );
  };

  useEffect(() => {
    if (!isVisible) return;
    let disposed = false;
    const run = async () => {
      setLoading(true);
      setInitialLoadDone(false);
      setInstalledByModel(createEmptyInstalledByModel());
      try {
        await reloadAll();
      } finally {
        if (!disposed) {
          setInitialLoadDone(true);
          setLoading(false);
        }
      }
    };
    run().catch((error) => {
      if (!disposed) {
        setMessage({
          type: 'error',
          text: t('error', 'Error: {{message}}', { message: String(error) }),
        });
        setInitialLoadDone(true);
        setLoading(false);
      }
    });
    return () => {
      disposed = true;
    };
  }, [isVisible, normalizedRootPath, t]);

  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => setMessage(null), 3000);
    return () => {
      window.clearTimeout(timer);
    };
  }, [message]);

  const installedCounts = useMemo(
    () => ({
      claude: installedByModel.claude.length,
      gemini: installedByModel.gemini.length,
      codex: installedByModel.codex.length,
      opencode: installedByModel.opencode.length,
    }),
    [installedByModel],
  );

  const activeInstalled = useMemo(
    () =>
      [...(installedByModel[activeModel] || [])].sort((a, b) => {
        const aTs = a.updated_at || a.installed_at || 0;
        const bTs = b.updated_at || b.installed_at || 0;
        return bTs - aTs;
      }),
    [activeModel, installedByModel],
  );

  const activeSkillLoadRule = useMemo(() => {
    switch (activeModel) {
      case 'claude':
        return t(
          'workspaceSkillsLoadRuleClaude',
          'OneSpace workspace view merges user-level and directory-level skills. Same-name directory-level skills take precedence; non-conflicting user-level skills remain.',
        );
      case 'gemini':
        return t(
          'workspaceSkillsLoadRuleGemini',
          'OneSpace workspace view merges user-level and directory-level skills. Same-name directory-level skills take precedence; non-conflicting user-level skills remain.',
        );
      case 'codex':
        return t(
          'workspaceSkillsLoadRuleCodex',
          'OneSpace workspace view merges user-level and directory-level skills. Same-name directory-level skills take precedence; non-conflicting user-level skills remain.',
        );
      case 'opencode':
      default:
        return t(
          'workspaceSkillsLoadRuleOpenCode',
          'OneSpace workspace view merges user-level and directory-level skills. Same-name directory-level skills take precedence; non-conflicting user-level skills remain.',
        );
    }
  }, [activeModel, t]);

  const installedBySourcePath = useMemo(() => {
    const next = new Map<string, SkillRecord>();
    activeInstalled.forEach((skill) => {
      next.set(`${skill.source_id}:${skill.source_rel_path}`, skill);
    });
    return next;
  }, [activeInstalled]);

  const catalogSources = useMemo(() => {
    const seen = new Set<string>();
    return catalog
      .filter((item) => item.models.includes(activeModel))
      .flatMap((item) => {
        const sourceId = String(item.source_id || '').trim();
        if (!sourceId || seen.has(sourceId)) {
          return [];
        }
        seen.add(sourceId);
        return [{ id: sourceId, label: sourceNamesById[sourceId] || sourceId }];
      });
  }, [activeModel, catalog, sourceNamesById]);

  useEffect(() => {
    if (recommendedSourceFilter === 'all') return;
    if (catalogSources.some((source) => source.id === recommendedSourceFilter)) return;
    setRecommendedSourceFilter('all');
  }, [catalogSources, recommendedSourceFilter]);

  const visibleCatalog = useMemo(() => {
    const keyword = recommendedSearch.trim().toLowerCase();
    return catalog.filter((item) => {
      if (!item.models.includes(activeModel)) return false;
      if (recommendedSourceFilter !== 'all' && item.source_id !== recommendedSourceFilter) return false;
      if (!keyword) return true;
      return [item.name, item.description, item.id, item.rel_path, item.dir_name, item.source_id].some((field) =>
        String(field || '').toLowerCase().includes(keyword),
      );
    });
  }, [activeModel, catalog, recommendedSearch, recommendedSourceFilter]);

  const visibleRepository = useMemo(() => {
    const keyword = repositorySearch.trim().toLowerCase();
    return repositorySkills.filter((repo) => {
      if (!repo.models.includes(activeModel)) return false;
      if (repositorySourceFilter === 'remote' && repo.source_type !== 'remote') return false;
      if (
        repositorySourceFilter === 'local' &&
        repo.source_type !== 'local_import' &&
        repo.source_type !== 'mirror'
      ) {
        return false;
      }
      if (!keyword) return true;
      return [
        repo.name,
        repo.description,
        repo.skill_id,
        repo.source_rel_path,
        repo.dir_name,
        repo.source_type,
      ].some((field) => String(field || '').toLowerCase().includes(keyword));
    });
  }, [activeModel, repositorySearch, repositorySkills, repositorySourceFilter]);

  const toInstallTargetFromRepo = (repo: RepositorySkillView): InstallTargetSkill => ({
    source_id: repo.source_id,
    id: repo.skill_id,
    rel_path: repo.source_rel_path,
    dir_name: repo.dir_name,
    name: repo.name,
    description: repo.description,
    models: repo.models,
    repo_key: repo.repo_key,
    installed: repo.installed,
  });

  const buildInstallStateForCatalog = (item: CatalogSkill): RepoModelInstallState => ({
    claude: (installedByModel.claude || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) || skill.id === item.id,
    ),
    gemini: (installedByModel.gemini || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) || skill.id === item.id,
    ),
    codex: (installedByModel.codex || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) || skill.id === item.id,
    ),
    opencode: (installedByModel.opencode || []).some(
      (skill) =>
        (skill.source_id === item.source_id && skill.source_rel_path === item.rel_path) || skill.id === item.id,
    ),
  });

  const hasInstallableRepoModels = (target: InstallTargetSkill | null) => {
    if (!target?.installed) return true;
    return target.models.some((model) => !target.installed?.[model]);
  };

  const openInstallDialog = (target: InstallTargetSkill, mode: 'catalog' | 'repository') => {
    const allowed = target.models.filter((model) => modelTabs.some((tab) => tab.id === model));
    if (allowed.length === 0) {
      return;
    }
    setInstallMode(mode);
    setInstallTarget(target);
    setInstallModels([allowed.includes(activeModel) ? activeModel : allowed[0]]);
    setInstallError('');
    setInstallDialogOpen(true);
  };

  const handleRefresh = async () => {
    setLoading(true);
    try {
      await reloadAll();
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleWorkspaceDiscoveryEntry = (entry: 'recommended' | 'repository') => {
    setDiscoveryMode(entry);
    window.setTimeout(() => {
      discoverySectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 20);
  };

  const handleOpenDetail = async (skill: SkillRecord) => {
    try {
      const res = await invoke<ApiResp<SkillDetail>>('skills_detail_get', {
        input: {
          model: skill.model,
          skill_id: skill.id,
          scope: skill.scope || 'project',
          project_root: skill.project_root || normalizedRootPath,
        },
      });
      setDetailData(res.data);
      setDetailOpen(true);
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
      });
    }
  };

  const handleOpenCatalogDetail = async (item: CatalogSkill) => {
    try {
      const res = await invoke<ApiResp<CatalogSkillDetail>>('skills_catalog_detail_get', {
        input: {
          source_id: item.source_id,
          skill_ref: item.rel_path,
        },
      });
      const matchedRepo = repositorySkills.find(
        (repo) => repo.source_id === item.source_id && repo.source_rel_path === item.rel_path,
      );
      setCatalogDetailInstallTarget(
        matchedRepo
          ? toInstallTargetFromRepo(matchedRepo)
          : {
              source_id: item.source_id,
              id: item.id,
              rel_path: item.rel_path,
              dir_name: item.dir_name,
              name: item.name,
              description: item.description,
              models: item.models,
              installed: buildInstallStateForCatalog(item),
            },
      );
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
      });
    }
  };

  const handleOpenRepositoryDetail = async (repo: RepositorySkillView) => {
    try {
      const res = await invoke<ApiResp<CatalogSkillDetail>>('skills_repo_detail_get', {
        input: {
          repo_key: repo.repo_key,
        },
      });
      setCatalogDetailInstallTarget(toInstallTargetFromRepo(repo));
      setCatalogDetailData(res.data);
      setCatalogDetailOpen(true);
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
      });
    }
  };

  const handleOpenFolder = async (skill: SkillRecord) => {
    try {
      await invoke('skills_open_folder', {
        input: {
          model: skill.model,
          skill_id: skill.id,
          scope: skill.scope || 'project',
          project_root: skill.project_root || normalizedRootPath,
        },
      });
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
      });
    }
  };

  const handleOpenCatalogFolder = async () => {
    if (!catalogDetailData) return;
    try {
      setLoading(true);
      const res = await invoke<ApiResp<CatalogOpenFolderResult>>('skills_catalog_open_folder', {
        input: {
          source_id: catalogDetailData.skill.source_id,
          skill_ref: catalogDetailData.skill.rel_path,
        },
      });
      setCatalogDetailInstallTarget((prev) => ({
        source_id: catalogDetailData.skill.source_id,
        id: catalogDetailData.skill.id,
        rel_path: catalogDetailData.skill.rel_path,
        dir_name: catalogDetailData.skill.dir_name,
        name: catalogDetailData.skill.name,
        description: catalogDetailData.skill.description,
        models: catalogDetailData.skill.models,
        repo_key: res.data.repo_key,
        installed: prev?.installed || buildInstallStateForCatalog(catalogDetailData.skill),
      }));
      await reloadAll();
      setMessage({ type: 'success', text: t('openFolder', 'Open Folder') });
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleInstallFromCatalogDetail = () => {
    if (catalogDetailInstallTarget) {
      setCatalogDetailOpen(false);
      openInstallDialog(
        catalogDetailInstallTarget,
        catalogDetailInstallTarget.repo_key ? 'repository' : 'catalog',
      );
      return;
    }
    if (!catalogDetailData) return;
    setCatalogDetailOpen(false);
    openInstallDialog(
      {
        source_id: catalogDetailData.skill.source_id,
        id: catalogDetailData.skill.id,
        rel_path: catalogDetailData.skill.rel_path,
        dir_name: catalogDetailData.skill.dir_name,
        name: catalogDetailData.skill.name,
        description: catalogDetailData.skill.description,
        models: catalogDetailData.skill.models,
        installed: buildInstallStateForCatalog(catalogDetailData.skill),
      },
      'catalog',
    );
  };

  const handleInstallConfirm = async () => {
    if (!installTarget || installModels.length === 0) {
      setInstallError(t('sourceModelsRequired', 'Select at least one model.'));
      return;
    }
    const targetModels = installTarget.models.filter((model) => installModels.includes(model));
    if (targetModels.length === 0) {
      setInstallError(t('sourceModelsRequired', 'Select at least one model.'));
      return;
    }

    try {
      setInstallSubmitting(true);
      setInstallError('');
      const results = await Promise.allSettled(
        targetModels.map((model) =>
          installMode === 'repository'
            ? invoke('skills_repo_set_model', {
                input: {
                  repo_key: installTarget.repo_key,
                  model,
                  enabled: true,
                  scope: 'project',
                  project_root: normalizedRootPath,
                },
              })
            : invoke('skills_install', {
                input: {
                  source_id: installTarget.source_id,
                  skill_ref: installTarget.rel_path,
                  model,
                  scope: 'project',
                  project_root: normalizedRootPath,
                },
              }),
        ),
      );

      const succeeded = targetModels.filter((_, idx) => results[idx].status === 'fulfilled');
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
      await reloadAll();
      emit('refresh-counts').catch(() => {});
      if (succeeded.length > 0) {
        setActiveModel(succeeded[0]);
      }
      if (failed.length === 0) {
        setMessage({
          type: 'success',
          text:
            succeeded.length === 1
              ? t('installed', 'Installed')
              : t('skillsInstallSuccessMulti', 'Installed for {{count}} models', { count: succeeded.length }),
        });
        setInstallDialogOpen(false);
        setInstallTarget(null);
        setInstallModels([]);
        return;
      }
      setMessage({
        type: 'error',
        text: t('skillsInstallPartialFailed', 'Installed {{success}}, failed {{failed}} ({{models}})', {
          success: succeeded.length,
          failed: failed.length,
          models: failed.join(', '),
        }),
      });
    } catch (error) {
      setInstallError(t('error', 'Error: {{message}}', { message: String(error) }));
    } finally {
      setInstallSubmitting(false);
    }
  };

  const handleUninstall = async (skill: SkillRecord) => {
    const skillScope = skill.scope || 'project';
    const ok = await confirmDialog(t('confirmDelete', { name: skill.name }), {
      okLabel: t('ok', 'OK'),
      cancelLabel: t('cancel', 'Cancel'),
    });
    if (!ok) return;

    try {
      setLoading(true);
      await invoke('skills_uninstall', {
        input: {
          model: skill.model,
          skill_id: skill.id,
          scope: skillScope,
          project_root: skillScope === 'project' ? skill.project_root || normalizedRootPath : null,
        },
      });
      await reloadAll();
      emit('refresh-counts').catch(() => {});
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(error) }),
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

    const repo = repositorySkills.find((item) =>
      matchesRepositorySkill(item, {
        source_id: skill.source_id,
        source_rel_path: skill.source_rel_path,
        id: skill.id,
        dir_name: skill.dir_name,
      }),
    );
    if (!repo) {
      setMessage({
        type: 'error',
        text: t('skillsReinstallRepoNotFound', 'Repository snapshot not found for this skill.'),
      });
      return;
    }

    const reinstallKey = `${skill.model}:${skill.id}`;
    setReinstallingKeys((prev) => ({ ...prev, [reinstallKey]: true }));
    try {
      await invoke('skills_repo_set_model', {
        input: {
          repo_key: repo.repo_key,
          model: skill.model,
          enabled: true,
          scope: skill.scope || 'project',
          project_root: (skill.scope || 'project') === 'project' ? skill.project_root || normalizedRootPath : null,
        },
      });
      await reloadAll();
      setMessage({
        type: 'success',
        text: t('skillsReinstallSuccess', 'Skill reinstalled successfully.'),
      });
    } catch (error) {
      setMessage({
        type: 'error',
        text: t('skillsReinstallFailed', 'Reinstall failed: {{message}}', { message: String(error) }),
      });
    } finally {
      setReinstallingKeys((prev) => {
        const next = { ...prev };
        delete next[reinstallKey];
        return next;
      });
    }
  };

  const toggleInstallModel = (model: ModelType) => {
    if (!installTarget?.models.includes(model)) return;
    setInstallModels((prev) => {
      if (prev.includes(model)) {
        return prev.filter((item) => item !== model);
      }
      return [...prev, model];
    });
  };

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="rounded-xl border bg-card p-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0">
            <h2 className="text-lg font-semibold tracking-tight">{t('skills', 'Skills')}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t(
                'workspaceSkillsSectionDesc',
                'Manage effective user-level and directory-level skills available to this workspace.',
              )}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2 lg:justify-end">
            <button
              type="button"
              onClick={() => {
                void handleRefresh();
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
              className={`rounded-md border px-2.5 py-1.5 text-xs ${
                message.type === 'error'
                  ? 'border-destructive/20 bg-destructive/10 text-destructive'
                  : 'border-green-500/20 bg-green-500/10 text-green-700'
              }`}
            >
              {message.text}
            </div>
          )}
        </div>
      )}

      <div className="rounded-xl border bg-card p-3">
        <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
          {modelTabs.map((tab) => {
            const ModelIcon = modelIconMap[tab.id];
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveModel(tab.id)}
                className={`rounded-lg border px-4 py-3 text-left transition-all ${
                  activeModel === tab.id ? 'border-primary bg-primary/5' : 'hover:bg-muted/40 hover:-translate-y-0.5'
                }`}
              >
                <div className="flex items-center gap-2">
                  <ModelIcon className="h-5 w-5" />
                  <span className="text-sm font-semibold">{tab.label}</span>
                </div>
                <div className="mt-2.5 text-sm leading-none text-muted-foreground">
                  {t('skillsInstalledCount', 'Installed {{count}} skills', {
                    count: installedCounts[tab.id] ?? 0,
                  })}
                </div>
              </button>
            );
          })}
        </div>
      </div>
      <div className="flex items-start gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
        <Info className="mt-0.5 h-4 w-4 shrink-0" />
        <p>
          <span className="font-medium text-foreground">
            {t('workspaceEffectiveLoadRule', 'Effective load rule')}:
          </span>{' '}
          {activeSkillLoadRule}
        </p>
      </div>

      {!initialLoadDone ? (
        <div className="py-12 text-center text-muted-foreground">
          <Loader2 className="mx-auto mb-3 h-8 w-8 animate-spin" />
          <p>{t('loading', 'Loading...')}</p>
        </div>
      ) : activeInstalled.length === 0 ? (
        <div className="py-12 text-center">
          <Sparkles className="mx-auto mb-4 h-16 w-16 text-muted-foreground" />
          <h3 className="mb-2 text-lg font-semibold">{t('noInstalledSkillsForModel', '该模型下暂无已安装 Skills')}</h3>
          <p className="text-muted-foreground">
            {t(
              'workspaceNoInstalledSkillsForModelDesc',
              'This workspace has no installed skills for the selected model yet.',
            )}
          </p>
          <div className="mt-4 flex flex-wrap justify-center gap-2">
            <button
              type="button"
              onClick={() => handleWorkspaceDiscoveryEntry('recommended')}
              className="rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground"
            >
              {t('workspaceInstallFromRecommended', 'Install from Recommended')}
            </button>
            <button
              type="button"
              onClick={() => handleWorkspaceDiscoveryEntry('repository')}
              className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
            >
              {t('workspaceInstallFromRepository', 'Install from Repository')}
            </button>
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {activeInstalled.map((skill) => {
            const Icon = pickIcon(skill.icon_seed || skill.id);
            const reinstallKey = `${skill.model}:${skill.id}`;
            const reinstalling = !!reinstallingKeys[reinstallKey];
            const isUserLevel = skill.scope === 'global';
            return (
              <div
                key={`${skill.model}:${skill.id}`}
                className="cursor-pointer rounded-xl border bg-card p-4 transition-all duration-200 hover:border-primary/30 hover:shadow-md"
                onClick={() => {
                  void handleOpenDetail(skill);
                }}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="rounded-md bg-primary/10 p-2 text-primary">
                    <Icon className="h-4 w-4" />
                  </div>
                  <div className="flex max-w-[60%] flex-col items-end gap-1">
                    <span
                      className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${getScopeBadgeClassName(skill.scope)}`}
                    >
                      {skill.scope === 'global'
                        ? t('workspaceScopeUser', 'User-level')
                        : t('workspaceScopeDirectory', 'Directory-level')}
                    </span>
                    <span className="truncate text-[10px] text-muted-foreground">
                      {skill.dir_name || skill.source_rel_path.split('/').pop() || skill.id}
                    </span>
                  </div>
                </div>
                <h4 className="mt-3 text-sm font-semibold">{skill.name}</h4>
                <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{skill.description}</p>
                <div className="mt-3 text-[11px] text-muted-foreground">
                  {t('lastUpdated', 'Last updated')}: {formatTs(skill.updated_at || skill.installed_at)}
                </div>
                <div className="mt-3 flex items-center justify-end gap-2">
                  {isUserLevel ? (
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onNavigateToGlobalPage?.('installed');
                      }}
                      className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs hover:bg-muted"
                    >
                      <BookOpen className="h-3.5 w-3.5" />
                      {t('workspaceManageUserLevel', 'Manage User-level')}
                    </button>
                  ) : (
                    <>
                      <button
                        type="button"
                        disabled={reinstalling}
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleReinstall(skill);
                        }}
                        className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs hover:bg-muted disabled:opacity-50"
                      >
                        <RefreshCw className={`h-3.5 w-3.5 ${reinstalling ? 'animate-spin' : ''}`} />
                        {t('skillsReinstall', '重新安装')}
                      </button>
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleUninstall(skill);
                        }}
                        className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-destructive hover:bg-destructive/10"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        {t('uninstall', 'Uninstall')}
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div ref={discoverySectionRef} className="rounded-xl border bg-card p-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <h3 className="text-base font-semibold tracking-tight">
              {t('workspaceDiscoverySectionTitle', 'Discover and Install')}
            </h3>
            <p className="text-sm text-muted-foreground">
              {t(
                'workspaceSkillsDiscoveryDesc',
                'Find recommended or repository skills and install them directly into this workspace.',
              )}
            </p>
          </div>
          <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
            <button
              type="button"
              onClick={() => handleWorkspaceDiscoveryEntry('recommended')}
              className={`rounded-md px-3 py-1.5 text-sm ${
                discoveryMode === 'recommended' ? 'bg-black text-white' : 'bg-white text-black'
              }`}
            >
              {t('recommended', '推荐')}
            </button>
            <button
              type="button"
              onClick={() => handleWorkspaceDiscoveryEntry('repository')}
              className={`rounded-md px-3 py-1.5 text-sm ${
                discoveryMode === 'repository' ? 'bg-black text-white' : 'bg-white text-black'
              }`}
            >
              {t('repository', '仓库')}
            </button>
          </div>
        </div>
      </div>

      {discoveryMode === 'recommended' ? (
        <>
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <input
              value={recommendedSearch}
              onChange={(event) => setRecommendedSearch(event.target.value)}
              placeholder={t('skillsSearchPlaceholder', '搜索 Skill 名称或描述')}
              className="h-10 w-full rounded-lg border px-3 text-sm lg:max-w-sm"
            />
            <div className="overflow-x-auto">
              <div className="inline-flex w-max rounded-lg border border-black bg-white p-1 whitespace-nowrap">
                <button
                  type="button"
                  onClick={() => setRecommendedSourceFilter('all')}
                  className={`rounded-md px-3 py-1.5 text-sm ${
                    recommendedSourceFilter === 'all' ? 'bg-black text-white' : 'bg-white text-black'
                  }`}
                >
                  {t('all', '全部')}
                </button>
                {catalogSources.map((source) => (
                  <button
                    key={source.id}
                    type="button"
                    onClick={() => setRecommendedSourceFilter(source.id)}
                    className={`rounded-md px-3 py-1.5 text-sm ${
                      recommendedSourceFilter === source.id ? 'bg-black text-white' : 'bg-white text-black'
                    }`}
                  >
                    {source.label}
                  </button>
                ))}
              </div>
            </div>
          </div>

          {visibleCatalog.length === 0 ? (
            <div className="py-12 text-center">
              <Sparkles className="mx-auto mb-4 h-16 w-16 text-muted-foreground" />
              <h3 className="mb-2 text-lg font-semibold">{t('noRecommendedSkills', '当前没有可推荐的 Skills')}</h3>
              <p className="text-muted-foreground">{t('noRecommendedSkillsDesc', '请检查 Skills 源配置，或同步源列表后重试。')}</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {visibleCatalog.map((item) => {
                const installed = installedBySourcePath.get(`${item.source_id}:${item.rel_path}`);
                const Icon = pickIcon(item.id);
                return (
                  <div
                    key={`${item.source_id}:${item.id}`}
                    className="cursor-pointer rounded-xl border bg-card p-4 transition-all duration-200 hover:border-primary/30 hover:shadow-md"
                    onClick={() => {
                      void handleOpenCatalogDetail(item);
                    }}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="rounded-md bg-muted p-2 text-foreground">
                        <Icon className="h-4 w-4" />
                      </div>
                      <span className="text-[10px] text-muted-foreground">
                        {item.dir_name || item.rel_path.split('/').pop() || item.id}
                      </span>
                    </div>
                    <h4 className="mt-3 text-sm font-semibold">{item.name}</h4>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.description}</p>
                    <div className="mt-3 flex items-center justify-between gap-2">
                      <span className="rounded border bg-muted/50 px-2 py-1 text-[10px] text-muted-foreground">
                        {sourceNamesById[item.source_id] || item.source_id}
                      </span>
                      {installed ? (
                        <span className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-muted-foreground">
                          <Download className="h-3.5 w-3.5" />
                          {t('installed', 'Installed')}
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            openInstallDialog(
                              {
                                source_id: item.source_id,
                                id: item.id,
                                rel_path: item.rel_path,
                                dir_name: item.dir_name,
                                name: item.name,
                                description: item.description,
                                models: item.models,
                              },
                              'catalog',
                            );
                          }}
                          className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs text-primary-foreground"
                        >
                          <Download className="h-3.5 w-3.5" />
                          {t('workspaceInstallAction', 'Install to Workspace')}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      ) : (
        <>
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <input
              value={repositorySearch}
              onChange={(event) => setRepositorySearch(event.target.value)}
              placeholder={t('skillsSearchPlaceholder', '搜索 Skill 名称或描述')}
              className="h-10 w-full rounded-lg border px-3 text-sm lg:max-w-sm"
            />
            <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
              <button
                type="button"
                onClick={() => setRepositorySourceFilter('all')}
                className={`rounded-md px-3 py-1.5 text-sm ${
                  repositorySourceFilter === 'all' ? 'bg-black text-white' : 'bg-white text-black'
                }`}
              >
                {t('all', '全部')}
              </button>
              <button
                type="button"
                onClick={() => setRepositorySourceFilter('remote')}
                className={`rounded-md px-3 py-1.5 text-sm ${
                  repositorySourceFilter === 'remote' ? 'bg-black text-white' : 'bg-white text-black'
                }`}
              >
                {t('skillsSourceTypeRemote', '推荐源')}
              </button>
              <button
                type="button"
                onClick={() => setRepositorySourceFilter('local')}
                className={`rounded-md px-3 py-1.5 text-sm ${
                  repositorySourceFilter === 'local' ? 'bg-black text-white' : 'bg-white text-black'
                }`}
              >
                {t('skillsSourceTypeLocalImport', '本地导入')}
              </button>
            </div>
          </div>

          {visibleRepository.length === 0 ? (
            <div className="py-12 text-center">
              <BookOpen className="mx-auto mb-4 h-16 w-16 text-muted-foreground" />
              <h3 className="mb-2 text-lg font-semibold">{t('skillsRepositoryEmpty', '仓库中暂无 Skills')}</h3>
              <p className="text-muted-foreground">{t('skillsRepositoryEmptyDesc', '请先在左侧 Skills 页面同步来源或导入仓库。')}</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {visibleRepository.map((repo) => {
                const Icon = pickIcon(repo.icon_seed || repo.skill_id);
                const installed = repo.installed[activeModel];
                return (
                  <div
                    key={repo.repo_key}
                    className="cursor-pointer rounded-xl border bg-card p-4 transition-all duration-200 hover:border-primary/30 hover:shadow-md"
                    onClick={() => {
                      void handleOpenRepositoryDetail(repo);
                    }}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="rounded-md bg-muted p-2 text-foreground">
                        <Icon className="h-4 w-4" />
                      </div>
                      <span className="rounded border bg-muted/50 px-2 py-1 text-[10px] text-muted-foreground">
                        {getSourceTypeLabel(repo.source_type, t)}
                      </span>
                    </div>
                    <h4 className="mt-3 text-sm font-semibold">{repo.name}</h4>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{repo.description}</p>
                    <div className="mt-3 flex items-center justify-between gap-2">
                      <span className="text-[10px] text-muted-foreground">
                        {repo.dir_name || repo.source_rel_path.split('/').pop() || repo.skill_id}
                      </span>
                      {installed ? (
                        <span className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-muted-foreground">
                          <Download className="h-3.5 w-3.5" />
                          {t('installed', 'Installed')}
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            openInstallDialog(
                              {
                                source_id: repo.source_id,
                                id: repo.skill_id,
                                rel_path: repo.source_rel_path,
                                dir_name: repo.dir_name,
                                name: repo.name,
                                description: repo.description,
                                models: repo.models,
                                repo_key: repo.repo_key,
                              },
                              'repository',
                            );
                          }}
                          className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs text-primary-foreground"
                        >
                          <Download className="h-3.5 w-3.5" />
                          {t('workspaceInstallAction', 'Install to Workspace')}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
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
        {catalogDetailOpen && catalogDetailData && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="border-b px-6 pt-6 pb-4">
              <DialogTitle>{catalogDetailData.skill.name}</DialogTitle>
              <DialogDescription>{catalogDetailData.skill.description}</DialogDescription>
            </DialogHeader>
            <div className="min-h-0 overflow-auto px-6 py-4">
              <div className="prose prose-sm max-w-none rounded-md border p-4 dark:prose-invert">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{catalogDetailData.markdown || ''}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="flex items-center gap-2 border-t px-6 py-4">
              <button
                type="button"
                onClick={() => {
                  void handleOpenCatalogFolder();
                }}
                className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted disabled:opacity-50"
                disabled={loading}
              >
                <FolderOpen className="h-4 w-4" />
                {t('openFolder', 'Open Folder')}
              </button>
              {hasInstallableRepoModels(catalogDetailInstallTarget) && (
                <button
                  type="button"
                  onClick={handleInstallFromCatalogDetail}
                  className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                  disabled={loading}
                >
                  <Download className="h-4 w-4" />
                  {t('workspaceInstallAction', 'Install to Workspace')}
                </button>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={detailOpen} onOpenChange={setDetailOpen}>
        {detailOpen && detailData && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="border-b px-6 pt-6 pb-4">
              <DialogTitle>{detailData.skill.name}</DialogTitle>
              <DialogDescription>{detailData.skill.description}</DialogDescription>
            </DialogHeader>
            <div className="min-h-0 overflow-auto px-6 py-4">
              <div className="prose prose-sm max-w-none rounded-md border p-4 dark:prose-invert">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{detailData.markdown || ''}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="border-t px-6 py-4">
              <button
                type="button"
                onClick={() => {
                  void handleOpenFolder(detailData.skill);
                }}
                className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted"
              >
                <FolderOpen className="h-4 w-4" />
                {t('openFolder', 'Open Folder')}
              </button>
              {detailData.skill.scope === 'global' ? (
                <button
                  type="button"
                  onClick={() => {
                    setDetailOpen(false);
                    onNavigateToGlobalPage?.('installed');
                  }}
                  className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted"
                >
                  <BookOpen className="h-4 w-4" />
                  {t('workspaceManageUserLevel', 'Manage User-level')}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    void handleUninstall(detailData.skill);
                  }}
                  className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm text-destructive hover:bg-destructive/10"
                >
                  <Trash2 className="h-4 w-4" />
                  {t('uninstall', 'Uninstall')}
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
            setInstallModels([]);
            setInstallError('');
            setInstallMode('catalog');
          }
        }}
      >
        {installDialogOpen && installTarget && (
          <DialogContent className="max-w-lg">
            <DialogHeader>
              <DialogTitle>{t('workspaceInstallAction', 'Install to Workspace')}</DialogTitle>
              <DialogDescription>
                {t('workspaceTargetDirectory', 'Target directory')}: {normalizedRootPath}
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-2">
                <div className="text-sm font-medium">{installTarget.name}</div>
                <div className="text-xs text-muted-foreground">{installTarget.description}</div>
              </div>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                {modelTabs.map((tab) => {
                  const allowed = installTarget.models.includes(tab.id);
                  const ModelIcon = modelIconMap[tab.id];
                  const selected = installModels.includes(tab.id);
                  return (
                    <button
                      key={`workspace-skills-install-${tab.id}`}
                      type="button"
                      disabled={!allowed}
                      onClick={() => toggleInstallModel(tab.id)}
                      className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
                        selected ? 'border-primary bg-primary/10 text-primary' : 'hover:bg-muted'
                      } ${!allowed ? 'cursor-not-allowed opacity-40' : ''}`}
                    >
                      <ModelIcon className="h-5 w-5" />
                      <div className="min-w-0 flex-1">
                        <div className="font-medium">{tab.label}</div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          {!allowed
                            ? t('skillsInstallUnavailableForModel', 'This skill is not available for the selected model.')
                            : selected
                            ? t('selected', 'Selected')
                            : t('clickToSelect', 'Click to select')}
                        </div>
                      </div>
                    </button>
                  );
                })}
              </div>
              {installError && <p className="text-sm text-destructive">{installError}</p>}
            </div>
            <DialogFooter>
              <button
                type="button"
                onClick={() => setInstallDialogOpen(false)}
                className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
                disabled={installSubmitting}
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleInstallConfirm();
                }}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                disabled={installSubmitting}
              >
                {installSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('workspaceInstallAction', 'Install to Workspace')}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
