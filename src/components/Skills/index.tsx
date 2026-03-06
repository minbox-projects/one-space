import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { invoke } from '@tauri-apps/api/core';
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
  Settings,
  FolderOpen,
  FolderPlus,
  RefreshCw,
  Download,
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

type ModelType = SkillModelId;

type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };

interface SkillRecord {
  id: string;
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
}

interface CatalogSkill {
  source_id: string;
  id: string;
  rel_path: string;
  name: string;
  description: string;
  models: ModelType[];
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

interface UpdateDiff {
  local_markdown: string;
  remote_markdown: string;
  local_changed_lines: number[];
  remote_changed_lines: number[];
  local_changed_blocks: { start_line: number; end_line: number; content: string }[];
  remote_changed_blocks: { start_line: number; end_line: number; content: string }[];
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
  source_id: string;
  source_rel_path: string;
  source_type: string;
  source_path?: string;
  name: string;
  description: string;
  models: ModelType[];
  icon_seed: string;
  hash?: string;
  updated_at?: number;
  installed: RepoModelInstallState;
}

interface LocalSkillCandidate {
  rel_path: string;
  skill_id: string;
  source_id: string;
  name: string;
  description: string;
  declared_models: ModelType[];
}

type ConflictStrategy = 'overwrite' | 'skip';

interface LocalImportSelection {
  rel_path: string;
  conflict_strategy: ConflictStrategy;
}

interface LocalImportSkipped {
  rel_path: string;
  skill_id: string;
  model: ModelType;
  reason: string;
}

interface LocalImportFailed {
  rel_path: string;
  skill_id?: string;
  model: ModelType;
  reason: string;
}

interface LocalImportResult {
  repo_added: {
    repo_key: string;
    skill_id: string;
    source_id: string;
    source_rel_path: string;
  }[];
  installed: SkillRecord[];
  skipped: LocalImportSkipped[];
  failed: LocalImportFailed[];
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

function pickIcon(seed: string) {
  const sum = seed.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
  return iconPool[sum % iconPool.length];
}

function formatTs(ts?: number) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString();
}

export function Skills() {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [activeModel, setActiveModel] = useState<ModelType>('claude');
  const [activeMode, setActiveMode] = useState<'recommended' | 'repository' | 'installed'>('recommended');
  const [repositorySourceFilter, setRepositorySourceFilter] = useState<'all' | 'local' | 'remote'>('all');
  const [installedByModel, setInstalledByModel] = useState<Record<ModelType, SkillRecord[]>>({
    claude: [],
    gemini: [],
    codex: [],
    opencode: [],
  });
  const [catalog, setCatalog] = useState<CatalogSkill[]>([]);
  const [repositorySkills, setRepositorySkills] = useState<RepositorySkillView[]>([]);
  const [syncState, setSyncState] = useState<SkillsSyncState | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [diffViewMode, setDiffViewMode] = useState<'blocks' | 'full'>('blocks');
  const didAutoSyncRef = useRef(false);

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<SkillDetail | null>(null);
  const [catalogDetailOpen, setCatalogDetailOpen] = useState(false);
  const [catalogDetailData, setCatalogDetailData] = useState<CatalogSkillDetail | null>(null);
  const [catalogDetailInstallTarget, setCatalogDetailInstallTarget] = useState<InstallTargetSkill | null>(null);

  const [diffOpen, setDiffOpen] = useState(false);
  const [diffData, setDiffData] = useState<UpdateDiff | null>(null);
  const [diffSkill, setDiffSkill] = useState<SkillRecord | null>(null);
  const allModels: ModelType[] = ['claude', 'gemini', 'codex', 'opencode'];

  const [localImportOpen, setLocalImportOpen] = useState(false);
  const [localImportScanning, setLocalImportScanning] = useState(false);
  const [localImportSubmitting, setLocalImportSubmitting] = useState(false);
  const [localImportRootPath, setLocalImportRootPath] = useState('');
  const [localCandidates, setLocalCandidates] = useState<LocalSkillCandidate[]>([]);
  const [localCandidateChecked, setLocalCandidateChecked] = useState<Record<string, boolean>>({});
  const [localImportModels, setLocalImportModels] = useState<ModelType[]>([...allModels]);
  const [localConflictDecisions, setLocalConflictDecisions] = useState<Record<string, ConflictStrategy | undefined>>({});
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [installTarget, setInstallTarget] = useState<InstallTargetSkill | null>(null);
  const [installMode, setInstallMode] = useState<'catalog' | 'repository'>('catalog');
  const [installModels, setInstallModels] = useState<ModelType[]>([]);
  const [installSubmitting, setInstallSubmitting] = useState(false);

  const loadInstalledAll = async () => {
    const models: ModelType[] = ['claude', 'gemini', 'codex', 'opencode'];
    const results = await Promise.all(
      models.map((model) =>
        invoke<ApiResp<SkillRecord[]>>('skills_list_installed', { model }).then((res) => ({
          model,
          list: res.data || [],
        }))
      )
    );
    const next: Record<ModelType, SkillRecord[]> = {
      claude: [],
      gemini: [],
      codex: [],
      opencode: [],
    };
    results.forEach(({ model, list }) => {
      next[model] = list;
    });
    setInstalledByModel(next);
  };

  const loadCatalog = async () => {
    const res = await invoke<ApiResp<CatalogSkill[]>>('skills_list_catalog', {
      model: null,
    });
    setCatalog(res.data || []);
  };

  const loadRepository = async () => {
    const res = await invoke<ApiResp<RepositorySkillView[]>>('skills_repo_list');
    setRepositorySkills(res.data || []);
  };

  const loadSyncState = async () => {
    const res = await invoke<ApiResp<SkillsSyncState>>('skills_sync_status_get');
    setSyncState(res.data);
  };

  const doRescan = async () => {
    await invoke('skills_rescan_mirror');
    await Promise.all([loadInstalledAll(), loadRepository()]);
  };

  const reloadAll = async () => {
    await Promise.all([loadInstalledAll(), loadCatalog(), loadRepository(), loadSyncState()]);
  };

  useEffect(() => {
    const init = async () => {
      if (!didAutoSyncRef.current) {
        didAutoSyncRef.current = true;
        try {
          setLoading(true);
          await invoke('skills_repo_refresh');
          setMessage({ type: 'success', text: t('skillsSyncSuccess', 'Skills synced successfully') });
        } catch (e: any) {
          setMessage({
            type: 'error',
            text: t('skillsSyncFailed', 'Skills sync failed: {{message}}', { message: String(e) }),
          });
        } finally {
          setLoading(false);
        }
      }
      await reloadAll();
      await doRescan();
    };
    init().catch(console.error);
  }, []);

  useEffect(() => {
    const timer = setInterval(() => {
      doRescan().catch(() => undefined);
    }, 15000);
    return () => clearInterval(timer);
  }, []);

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

  const filteredInstalled = useMemo(() => activeInstalled, [activeInstalled]);

  const filteredCatalog = useMemo(
    () => catalog.filter((skill) => skill.models.includes(activeModel)),
    [catalog, activeModel]
  );
  const visibleInstalled = filteredInstalled;
  const visibleCatalog = filteredCatalog;
  const visibleRepository = useMemo(() => {
    if (repositorySourceFilter === 'all') {
      return repositorySkills;
    }
    if (repositorySourceFilter === 'remote') {
      return repositorySkills.filter((repo) => repo.source_type === 'remote');
    }
    return repositorySkills.filter((repo) =>
      repo.source_type === 'local_import' || repo.source_type === 'mirror'
    );
  }, [repositorySkills, repositorySourceFilter]);
  const hideHeaderSyncButton = activeMode === 'recommended' && visibleCatalog.length === 0;
  const localSelectedCandidates = useMemo(
    () => localCandidates.filter((item) => !!localCandidateChecked[item.rel_path]),
    [localCandidates, localCandidateChecked]
  );
  const localConflictEntries = useMemo(() => {
    return localSelectedCandidates
      .map((candidate) => {
        const conflictModels = localImportModels.filter((model) =>
          (installedByModel[model] || []).some((skill) => skill.id === candidate.skill_id)
        );
        return { candidate, conflictModels };
      })
      .filter((item) => item.conflictModels.length > 0);
  }, [localSelectedCandidates, localImportModels, installedByModel]);
  const localUnresolvedConflicts = useMemo(
    () => localConflictEntries.filter((entry) => !localConflictDecisions[entry.candidate.rel_path]),
    [localConflictEntries, localConflictDecisions]
  );
  const canSubmitLocalImport =
    localSelectedCandidates.length > 0 &&
    localImportModels.length > 0 &&
    localUnresolvedConflicts.length === 0 &&
    !localImportSubmitting;

  const getRepoSourceMeta = (sourceType: string) => {
    switch (sourceType) {
      case 'remote':
        return {
          label: t('skillsSourceTypeRemote', 'Remote'),
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

  const resetLocalImportState = () => {
    setLocalImportOpen(false);
    setLocalImportScanning(false);
    setLocalImportSubmitting(false);
    setLocalImportRootPath('');
    setLocalCandidates([]);
    setLocalCandidateChecked({});
    setLocalImportModels([...allModels]);
    setLocalConflictDecisions({});
  };

  const handleSyncNow = async () => {
    try {
      setLoading(true);
      if (activeMode === 'recommended') {
        await invoke('skills_sync_now');
      } else if (activeMode === 'repository') {
        await invoke('skills_repo_refresh');
      } else {
        await invoke('skills_rescan_mirror');
      }
      await reloadAll();
      setMessage({ type: 'success', text: t('skillsSyncSuccess', 'Skills synced successfully') });
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('skillsSyncFailed', 'Skills sync failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
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

  const installSkillToModels = async (item: CatalogSkill, selectedModels: ModelType[]) => {
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
          invoke('skills_install', {
            input: {
              source_id: item.source_id,
              skill_ref: item.rel_path,
              model,
            },
          })
        )
      );
      await reloadAll();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
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

  const installRepositoryToModels = async (item: InstallTargetSkill, selectedModels: ModelType[]) => {
    if (!item.repo_key) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: 'Missing repository key' }),
      });
      return;
    }

    const targetModels = allModels.filter(
      (model) => item.models.includes(model) && selectedModels.includes(model) && !item.installed?.[model]
    );
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
          invoke('skills_repo_set_model', {
            input: {
              repo_key: item.repo_key,
              model,
              enabled: true,
            },
          })
        )
      );
      await reloadAll();
      const succeeded = results.filter((r) => r.status === 'fulfilled').length;
      const failed = targetModels.filter((_, idx) => results[idx].status === 'rejected');
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
      if (mode === 'repository') {
        return !target.installed?.[model];
      }
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
    setInstallModels([allowed.includes(preferredModel || activeModel) ? (preferredModel || activeModel) : allowed[0]]);
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
    if (allowed.length === 1) {
      await installSkillToModels(item, allowed);
      return;
    }
    openInstallDialog(item, 'catalog');
  };

  const handleInstallRepository = (repo: RepositorySkillView) => {
    openInstallDialog(toInstallTargetFromRepo(repo), 'repository');
  };

  const installAllowedModels = useMemo(
    () =>
      installTarget
        ? allModels.filter((model) => {
            if (!installTarget.models.includes(model)) return false;
            if (installMode === 'repository') {
              return !installTarget.installed?.[model];
            }
            return true;
          })
        : [],
    [installTarget, installMode]
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
      openInstallDialog(catalogDetailInstallTarget, 'repository');
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
        },
      });
      setDetailOpen(false);
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

  const handleOpenDetail = async (skill: SkillRecord) => {
    try {
      const res = await invoke<ApiResp<SkillDetail>>('skills_detail_get', {
        input: {
          model: skill.model,
          skill_id: skill.id,
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

  const handleOpenCatalogDetail = async (item: CatalogSkill) => {
    try {
      const res = await invoke<ApiResp<CatalogSkillDetail>>('skills_catalog_detail_get', {
        input: {
          source_id: item.source_id,
          skill_ref: item.rel_path,
        },
      });
      setCatalogDetailInstallTarget(null);
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
      const res = await invoke<ApiResp<CatalogSkillDetail>>('skills_repo_detail_get', {
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

  const handleOpenDiff = async (skill: SkillRecord) => {
    try {
      const res = await invoke<ApiResp<UpdateDiff>>('skills_update_diff_preview', {
        input: {
          model: skill.model,
          skill_id: skill.id,
        },
      });
      setDiffData(res.data);
      setDiffSkill(skill);
      setDiffOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const handleApplyUpdate = async () => {
    if (!diffSkill) return;
    try {
      setLoading(true);
      await invoke('skills_update_apply', {
        input: {
          model: diffSkill.model,
          skill_id: diffSkill.id,
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

  const handleOpenFolder = async (skill: SkillRecord) => {
    try {
      await invoke('skills_open_folder', {
        input: { model: skill.model, skill_id: skill.id },
      });
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    }
  };

  const handleOpenLocalImport = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (!selected || typeof selected !== 'string') {
        return;
      }

      setLocalImportScanning(true);
      const res = await invoke<ApiResp<LocalSkillCandidate[]>>('skills_local_scan', {
        input: { root_path: selected },
      });
      const list = res.data || [];
      if (list.length === 0) {
        setMessage({ type: 'error', text: t('skillsLocalNoSkillsFound', 'No skills found in selected folder.') });
        return;
      }

      const checked: Record<string, boolean> = {};
      list.forEach((item) => {
        checked[item.rel_path] = true;
      });
      setLocalImportRootPath(selected);
      setLocalCandidates(list);
      setLocalCandidateChecked(checked);
      setLocalImportModels([...allModels]);
      setLocalConflictDecisions({});
      setLocalImportOpen(true);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('skillsLocalScanFailed', 'Folder scan failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLocalImportScanning(false);
    }
  };

  const toggleLocalImportModel = (model: ModelType) => {
    setLocalImportModels((prev) => {
      if (prev.includes(model)) {
        return prev.filter((m) => m !== model);
      }
      return [...prev, model];
    });
  };

  const handleLocalImportSubmit = async () => {
    if (!canSubmitLocalImport) {
      return;
    }
    try {
      setLoading(true);
      setLocalImportSubmitting(true);
      const conflictMap = new Map<string, boolean>(
        localConflictEntries.map((item) => [item.candidate.rel_path, true])
      );
      const selections: LocalImportSelection[] = localSelectedCandidates.map((item) => ({
        rel_path: item.rel_path,
        conflict_strategy: conflictMap.get(item.rel_path)
          ? (localConflictDecisions[item.rel_path] || 'skip')
          : 'overwrite',
      }));
      const res = await invoke<ApiResp<LocalImportResult>>('skills_local_import', {
        input: {
          root_path: localImportRootPath,
          models: localImportModels,
          selections,
        },
      });
      const result = res.data || { repo_added: [], installed: [], skipped: [], failed: [] };
      const text = t('skillsLocalImportSummary', 'Imported {{installed}}, skipped {{skipped}}, failed {{failed}}', {
        installed: result.installed.length,
        skipped: result.skipped.length,
        failed: result.failed.length,
      });
      setMessage({ type: result.failed.length > 0 ? 'error' : 'success', text });
      resetLocalImportState();
      await reloadAll();
      await doRescan();
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('skillsLocalImportFailed', 'Import failed: {{message}}', { message: String(e) }),
      });
    } finally {
      setLocalImportSubmitting(false);
      setLoading(false);
    }
  };

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('skills', 'Skills')}</h2>
          <p className="text-sm text-muted-foreground">
            {t('skillsDesc', 'Manage skills by model')}
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
          <button
            onClick={handleOpenLocalImport}
            disabled={loading || localImportScanning || localImportSubmitting}
            className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
          >
            {localImportScanning ? <RefreshCw className="w-4 h-4 animate-spin" /> : <FolderPlus className="w-4 h-4" />}
            {t('skillsLocalImportButton', 'Import From Folder')}
          </button>
          {!hideHeaderSyncButton && (
            <button
              onClick={handleSyncNow}
              disabled={loading}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium inline-flex items-center gap-2 disabled:opacity-50"
            >
              {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
              {t('syncNow', 'Sync Now')}
            </button>
          )}
        </div>
      </div>

      <div className="inline-flex w-fit rounded-lg border bg-muted/30 p-1">
        <button
          onClick={handleSwitchToRecommended}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'recommended'
              ? 'bg-background shadow-sm text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          }`}
        >
          {t('recommended', '推荐')}
        </button>
        <button
          onClick={handleSwitchToRepository}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'repository'
              ? 'bg-background shadow-sm text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          }`}
        >
          {t('repository', '仓库')}
        </button>
        <button
          onClick={() => setActiveMode('installed')}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'installed'
              ? 'bg-background shadow-sm text-foreground'
              : 'text-muted-foreground hover:text-foreground'
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
          {visibleInstalled.length === 0 ? (
            <div className="text-center py-12">
              <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noInstalledSkillsForModel', '该模型下暂无已安装 Skills')}</h3>
              <p className="text-muted-foreground mb-4">{t('noInstalledSkillsForModelDesc', '你可以先到“推荐”中安装 Skills。')}</p>
              <button
                onClick={handleSwitchToRecommended}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
              >
                {t('recommended', '推荐')}
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleInstalled.map((skill) => {
                const Icon = pickIcon(skill.icon_seed || skill.id);
                return (
                  <div
                    key={`${skill.model}:${skill.id}`}
                    className="group border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer relative"
                    onClick={() => handleOpenDetail(skill)}
                  >
                    <button
                      className="absolute right-2 top-2 p-1 rounded-md opacity-0 group-hover:opacity-100 hover:bg-muted"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleOpenFolder(skill);
                      }}
                      title={t('settings', 'Settings')}
                    >
                      <Settings className="w-4 h-4" />
                    </button>

                    <div className="flex items-start justify-between gap-3">
                      <div className="p-2 rounded-md bg-primary/10 text-primary">
                        <Icon className="w-4 h-4" />
                      </div>
                      <div className="flex flex-col items-end gap-1 pr-7">
                        <span className="text-[10px] text-muted-foreground">{skill.source_id}</span>
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

                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('lastUpdated', 'Last updated')}: {formatTs(skill.updated_at || skill.installed_at)}
                    </div>

                    <div className="mt-3 flex items-center justify-end">
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
          <div className="flex justify-end">
            <div className="inline-flex w-fit rounded-lg border bg-muted/30 p-1">
              <button
                onClick={() => setRepositorySourceFilter('all')}
                className={`px-3 py-1.5 rounded-md text-sm ${
                  repositorySourceFilter === 'all'
                    ? 'bg-background shadow-sm text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t('all', '全部')}
              </button>
              <button
                onClick={() => setRepositorySourceFilter('local')}
                className={`px-3 py-1.5 rounded-md text-sm ${
                  repositorySourceFilter === 'local'
                    ? 'bg-background shadow-sm text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t('local', '本地')}
              </button>
              <button
                onClick={() => setRepositorySourceFilter('remote')}
                className={`px-3 py-1.5 rounded-md text-sm ${
                  repositorySourceFilter === 'remote'
                    ? 'bg-background shadow-sm text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t('skillsSourceTypeRemote', '远端')}
              </button>
            </div>
          </div>

          {visibleRepository.length === 0 ? (
            <div className="text-center py-12">
              <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noResultsFound', 'No skills found.')}</h3>
              <p className="text-muted-foreground mb-4">
                {t('noRecommendedSkillsDesc', '请检查 Skills 源配置，或立即同步后重试。')}
              </p>
              <button
                onClick={handleSyncNow}
                disabled={loading}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm inline-flex items-center gap-2 disabled:opacity-50"
              >
                <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
                {t('syncNow', 'Sync Now')}
              </button>
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
                const installableCount = allModels.filter(
                  (model) => repo.models.includes(model) && !repo.installed[model]
                ).length;
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
                      <div className="flex items-center gap-1.5">
                        <span className="text-[10px] text-muted-foreground">{repo.source_id}</span>
                        <span className={`text-[10px] px-2 py-1 rounded border ${sourceMeta.className}`}>
                          {sourceMeta.label}
                        </span>
                      </div>
                    </div>

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{repo.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{repo.description}</p>
                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('installed', 'Installed')} {installedCount}/4
                    </div>

                    <div className="mt-3 flex justify-end">
                      {installableCount === 0 ? (
                        <span className="text-xs px-2.5 py-1 rounded-md border text-muted-foreground inline-flex items-center gap-1">
                          <Download className="w-3.5 h-3.5" />
                          {t('installed', 'Installed')}
                        </span>
                      ) : (
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
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}

      {activeMode === 'recommended' && (
        <>
          {visibleCatalog.length === 0 ? (
            <div className="text-center py-12">
              <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t('noRecommendedSkills', '当前没有可推荐的 Skills')}</h3>
              <p className="text-muted-foreground mb-4">{t('noRecommendedSkillsDesc', '请检查 Skills 源配置，或立即同步后重试。')}</p>
              <button
                onClick={handleSyncNow}
                disabled={loading}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm inline-flex items-center gap-2 disabled:opacity-50"
              >
                <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
                {t('syncNow', 'Sync Now')}
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleCatalog.map((item) => {
                const installedSkill = installedById.get(`${item.source_id}:${item.rel_path}`);
                const Icon = pickIcon(item.id);
                const srcStatus = sourceStatusMap.get(item.source_id);
                return (
                  <div
                    key={`${item.source_id}:${item.id}`}
                    className="border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer"
                    onClick={() => handleOpenCatalogDetail(item)}
                  >
                    <div className="flex items-start justify-between">
                      <div className="p-2 rounded-md bg-muted text-foreground">
                        <Icon className="w-4 h-4" />
                      </div>
                      <span className="text-[10px] text-muted-foreground">{item.source_id}</span>
                    </div>
                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{item.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{item.description}</p>
                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('lastSynced', 'Last synced')}: {formatTs(srcStatus?.last_synced_at || syncState?.last_sync_at)}
                    </div>

                    <div className="mt-3 flex justify-end">
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
          <DialogFooter className="border-t px-6 py-4">
            <button
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium inline-flex items-center gap-2 disabled:opacity-50"
              onClick={handleInstallFromCatalogDetail}
              disabled={loading}
            >
              {loading && <RefreshCw className="w-4 h-4 animate-spin" />}
              <Download className="w-4 h-4" />
              {t('install', 'Install')}
            </button>
          </DialogFooter>
        </DialogContent>
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
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t('skillsInstallSelectModelsTitle', 'Select models to install')}</DialogTitle>
            <DialogDescription>
              {t('skillsInstallSelectModelsDesc', 'Choose model targets for {{name}}', { name: installTarget?.name || '' })}
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
      </Dialog>

      <Dialog
        open={localImportOpen}
        onOpenChange={(open) => {
          if (!open) {
            resetLocalImportState();
            return;
          }
          setLocalImportOpen(open);
        }}
      >
        <DialogContent className="max-w-5xl">
          <DialogHeader>
            <DialogTitle>{t('skillsLocalImportTitle', 'Import Local Skills')}</DialogTitle>
            <DialogDescription>{t('skillsLocalImportDesc', 'Select skills and models to import from local folder')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="text-xs text-muted-foreground break-all">
              {t('skillsLocalImportPath', 'Folder')}: {localImportRootPath}
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t('sourceModels', 'Apply Models')}</label>
              <div className="grid grid-cols-2 gap-2">
                {skillModelOptions.map(({ id, label, Icon }) => {
                  const active = localImportModels.includes(id);
                  return (
                    <button
                      key={`local-import-model-${id}`}
                      type="button"
                      onClick={() => toggleLocalImportModel(id)}
                      className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                        active
                          ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                          : 'bg-background hover:bg-muted/50 text-foreground border-border'
                      }`}
                    >
                      <Icon className="w-4 h-4 shrink-0" />
                      <span className="truncate">{label}</span>
                    </button>
                  );
                })}
              </div>
              {localImportModels.length === 0 && (
                <p className="text-xs text-destructive">{t('sourceModelsRequired', 'Select at least one model.')}</p>
              )}
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <label className="text-sm font-medium text-muted-foreground">{t('skillsLocalCandidates', 'Detected Skills')}</label>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    className="text-xs px-2 py-1 rounded border hover:bg-muted"
                    onClick={() => {
                      const next: Record<string, boolean> = {};
                      localCandidates.forEach((item) => {
                        next[item.rel_path] = true;
                      });
                      setLocalCandidateChecked(next);
                    }}
                  >
                    {t('selectAll', 'Select All')}
                  </button>
                  <button
                    type="button"
                    className="text-xs px-2 py-1 rounded border hover:bg-muted"
                    onClick={() => {
                      setLocalCandidateChecked({});
                    }}
                  >
                    {t('clear', 'Clear')}
                  </button>
                </div>
              </div>
              <div className="max-h-[32vh] overflow-auto rounded-md border divide-y">
                {localCandidates.map((item) => {
                  const checked = !!localCandidateChecked[item.rel_path];
                  return (
                    <label key={item.rel_path} className="flex items-start gap-3 p-3 cursor-pointer hover:bg-muted/30">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(e) =>
                          setLocalCandidateChecked((prev) => ({
                            ...prev,
                            [item.rel_path]: e.target.checked,
                          }))
                        }
                        className="mt-0.5"
                      />
                      <div className="min-w-0">
                        <div className="text-sm font-medium">{item.name}</div>
                        <div className="text-xs text-muted-foreground mt-1">{item.description}</div>
                        <div className="text-[11px] text-muted-foreground mt-1 font-mono break-all">
                          {item.rel_path === '.' ? '/' : item.rel_path}
                        </div>
                      </div>
                    </label>
                  );
                })}
              </div>
            </div>

            {localConflictEntries.length > 0 && (
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t('skillsLocalConflictTitle', 'Conflict Handling')}
                </label>
                <div className="space-y-2 max-h-[24vh] overflow-auto rounded-md border p-2">
                  {localConflictEntries.map(({ candidate, conflictModels }) => {
                    const decision = localConflictDecisions[candidate.rel_path];
                    return (
                      <div key={`conflict-${candidate.rel_path}`} className="rounded-md border p-2">
                        <div className="text-xs font-medium">{candidate.name}</div>
                        <div className="mt-1 text-[11px] text-muted-foreground">
                          {t('skillsLocalConflictModels', 'Conflicts on models')}: {conflictModels.join(', ')}
                        </div>
                        <div className="mt-2 flex items-center gap-2">
                          <button
                            type="button"
                            className={`text-xs px-2.5 py-1 rounded border ${
                              decision === 'overwrite'
                                ? 'bg-primary text-primary-foreground border-primary'
                                : 'hover:bg-muted'
                            }`}
                            onClick={() =>
                              setLocalConflictDecisions((prev) => ({
                                ...prev,
                                [candidate.rel_path]: 'overwrite',
                              }))
                            }
                          >
                            {t('skillsLocalConflictOverwrite', 'Overwrite')}
                          </button>
                          <button
                            type="button"
                            className={`text-xs px-2.5 py-1 rounded border ${
                              decision === 'skip'
                                ? 'bg-primary text-primary-foreground border-primary'
                                : 'hover:bg-muted'
                            }`}
                            onClick={() =>
                              setLocalConflictDecisions((prev) => ({
                                ...prev,
                                [candidate.rel_path]: 'skip',
                              }))
                            }
                          >
                            {t('skillsLocalConflictSkip', 'Skip')}
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
                {localUnresolvedConflicts.length > 0 && (
                  <div className="text-xs text-destructive">
                    {t('skillsLocalConflictRequired', 'Please resolve all conflicts before importing.')}
                  </div>
                )}
              </div>
            )}
          </div>
          <DialogFooter>
            <button
              type="button"
              onClick={resetLocalImportState}
              className="px-4 py-2 border rounded-md text-sm hover:bg-muted"
            >
              {t('cancel', 'Cancel')}
            </button>
            <button
              type="button"
              disabled={!canSubmitLocalImport}
              onClick={handleLocalImportSubmit}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium disabled:opacity-50 inline-flex items-center gap-2"
            >
              {localImportSubmitting && <RefreshCw className="w-4 h-4 animate-spin" />}
              {t('skillsLocalImportConfirm', 'Import Selected Skills')}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={detailOpen} onOpenChange={setDetailOpen}>
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
      </Dialog>

      <Dialog open={diffOpen} onOpenChange={setDiffOpen}>
        <DialogContent className="max-w-6xl">
          <DialogHeader>
            <DialogTitle>{t('updateDiff', 'Update Diff')}</DialogTitle>
            <DialogDescription>
              {t('updateDiffDesc', 'Compare local and remote skill markdown before updating')}
            </DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
            <div className="border rounded-md p-3 max-h-[58vh] overflow-auto">
              <div className="text-xs font-semibold mb-2">{t('localVersion', 'Local')}</div>
              <div className="mb-2 flex items-center gap-2">
                <button
                  className={`text-[11px] px-2 py-1 rounded border ${diffViewMode === 'blocks' ? 'bg-primary text-primary-foreground border-primary' : 'bg-background'}`}
                  onClick={() => setDiffViewMode('blocks')}
                >
                  {t('diffBlocks', 'Diff Blocks')}
                </button>
                <button
                  className={`text-[11px] px-2 py-1 rounded border ${diffViewMode === 'full' ? 'bg-primary text-primary-foreground border-primary' : 'bg-background'}`}
                  onClick={() => setDiffViewMode('full')}
                >
                  {t('fullDoc', 'Full Document')}
                </button>
              </div>
              <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                {t('changedLines', 'Changed lines')}: {diffData?.local_changed_lines.join(', ') || '--'}
              </div>
              {diffViewMode === 'blocks' && (diffData?.local_changed_blocks?.length || 0) > 0 && (
                <div className="mb-3 space-y-2">
                  {diffData!.local_changed_blocks.map((b, idx) => (
                    <div key={`l-${idx}`} className="rounded-md border border-amber-200 bg-amber-50/70 p-2">
                      <div className="text-[10px] text-amber-700 mb-1">
                        L{b.start_line}{b.end_line > b.start_line ? `-L${b.end_line}` : ''}
                      </div>
                      <pre className="text-[11px] whitespace-pre-wrap break-words text-amber-900">{b.content}</pre>
                    </div>
                  ))}
                </div>
              )}
              {diffViewMode === 'full' && (
                <div className="prose prose-sm dark:prose-invert max-w-none">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>{diffData?.local_markdown || ''}</ReactMarkdown>
                </div>
              )}
            </div>
            <div className="border rounded-md p-3 max-h-[58vh] overflow-auto">
              <div className="text-xs font-semibold mb-2">{t('remoteVersion', 'Remote')}</div>
              <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                {t('changedLines', 'Changed lines')}: {diffData?.remote_changed_lines.join(', ') || '--'}
              </div>
              {diffViewMode === 'blocks' && (diffData?.remote_changed_blocks?.length || 0) > 0 && (
                <div className="mb-3 space-y-2">
                  {diffData!.remote_changed_blocks.map((b, idx) => (
                    <div key={`r-${idx}`} className="rounded-md border border-amber-200 bg-amber-50/70 p-2">
                      <div className="text-[10px] text-amber-700 mb-1">
                        L{b.start_line}{b.end_line > b.start_line ? `-L${b.end_line}` : ''}
                      </div>
                      <pre className="text-[11px] whitespace-pre-wrap break-words text-amber-900">{b.content}</pre>
                    </div>
                  ))}
                </div>
              )}
              {diffViewMode === 'full' && (
                <div className="prose prose-sm dark:prose-invert max-w-none">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>{diffData?.remote_markdown || ''}</ReactMarkdown>
                </div>
              )}
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
      </Dialog>
    </div>
  );
}
