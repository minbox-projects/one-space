import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { confirm as tauriConfirm } from '@tauri-apps/plugin-dialog';
import {
  Sparkles,
  Wrench,
  Shield,
  Cpu,
  BookOpen,
  Trash2,
  Settings,
  FolderOpen,
  RefreshCw,
  Download,
} from 'lucide-react';
import { ClaudeIcon, GeminiIcon, OpenAIIcon, OpenCodeIcon } from '../AiEnvironments/icons';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '../ui/dialog';

type ModelType = 'claude' | 'gemini' | 'codex' | 'opencode';

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

const modelTabs: { id: ModelType; label: string }[] = [
  { id: 'claude', label: 'Claude' },
  { id: 'gemini', label: 'Gemini' },
  { id: 'codex', label: 'Codex' },
  { id: 'opencode', label: 'OpenCode' },
];

const modelIconMap: Record<ModelType, ComponentType<{ className?: string }>> = {
  claude: ClaudeIcon,
  gemini: GeminiIcon,
  codex: OpenAIIcon,
  opencode: OpenCodeIcon,
};

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
  const [activeModel, setActiveModel] = useState<ModelType>('claude');
  const [activeMode, setActiveMode] = useState<'recommended' | 'installed'>('recommended');
  const [installedByModel, setInstalledByModel] = useState<Record<ModelType, SkillRecord[]>>({
    claude: [],
    gemini: [],
    codex: [],
    opencode: [],
  });
  const [catalog, setCatalog] = useState<CatalogSkill[]>([]);
  const [syncState, setSyncState] = useState<SkillsSyncState | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [diffViewMode, setDiffViewMode] = useState<'blocks' | 'full'>('blocks');
  const didAutoSyncRef = useRef(false);

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailData, setDetailData] = useState<SkillDetail | null>(null);

  const [diffOpen, setDiffOpen] = useState(false);
  const [diffData, setDiffData] = useState<UpdateDiff | null>(null);
  const [diffSkill, setDiffSkill] = useState<SkillRecord | null>(null);

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

  const loadCatalog = async (model = activeModel) => {
    const res = await invoke<ApiResp<CatalogSkill[]>>('skills_list_catalog', {
      model,
    });
    setCatalog(res.data || []);
  };

  const loadSyncState = async () => {
    const res = await invoke<ApiResp<SkillsSyncState>>('skills_sync_status_get');
    setSyncState(res.data);
  };

  const doRescan = async () => {
    await invoke('skills_rescan_local');
    await loadInstalledAll();
  };

  const reloadAll = async () => {
    await Promise.all([loadInstalledAll(), loadCatalog(activeModel), loadSyncState()]);
  };

  useEffect(() => {
    const init = async () => {
      if (!didAutoSyncRef.current) {
        didAutoSyncRef.current = true;
        try {
          setLoading(true);
          await invoke('skills_sync_now');
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
    Promise.all([loadCatalog(activeModel), loadSyncState()]).catch(console.error);
  }, [activeModel]);

  useEffect(() => {
    const timer = setInterval(() => {
      doRescan().catch(() => undefined);
    }, 15000);
    return () => clearInterval(timer);
  }, [activeModel]);

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

  const modelCounts = useMemo(
    () => ({
      claude: installedByModel.claude.length,
      gemini: installedByModel.gemini.length,
      codex: installedByModel.codex.length,
      opencode: installedByModel.opencode.length,
    }),
    [installedByModel]
  );

  const sourceStatuses = useMemo(() => syncState?.sources || [], [syncState]);
  const sourceStatusMap = useMemo(() => {
    const m = new Map<string, SourceSyncState>();
    sourceStatuses.forEach((s) => m.set(s.source_id, s));
    return m;
  }, [sourceStatuses]);

  const filteredInstalled = useMemo(() => activeInstalled, [activeInstalled]);

  const filteredCatalog = useMemo(() => catalog, [catalog]);

  const visibleInstalled = filteredInstalled;
  const visibleCatalog = filteredCatalog;
  const hideHeaderSyncButton = activeMode === 'recommended' && visibleCatalog.length === 0;

  const renderStatusBadge = (status: string) => {
    if (status.includes('error')) return 'bg-destructive/10 text-destructive border-destructive/20';
    if (status.includes('skip')) return 'bg-muted text-muted-foreground border-border';
    if (status.includes('no_change')) return 'bg-blue-100 text-blue-700 border-blue-200';
    return 'bg-green-100 text-green-700 border-green-200';
  };

  const handleSyncNow = async () => {
    try {
      setLoading(true);
      await invoke('skills_sync_now');
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

  const handleInstall = async (item: CatalogSkill) => {
    try {
      setLoading(true);
      await invoke('skills_install', {
        input: {
          source_id: item.source_id,
          skill_ref: item.rel_path,
          model: activeModel,
        },
      });
      await reloadAll();
      setMessage({ type: 'success', text: t('installed', 'Installed') });
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('error', 'Error: {{message}}', { message: String(e) }),
      });
    } finally {
      setLoading(false);
    }
  };

  const handleUninstall = async (skill: SkillRecord) => {
    const ok = await tauriConfirm(t('confirmDelete', { name: skill.name }), {
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
          onClick={() => setActiveMode('recommended')}
          className={`px-3 py-1.5 rounded-md text-sm ${
            activeMode === 'recommended'
              ? 'bg-background shadow-sm text-foreground'
              : 'text-muted-foreground hover:text-foreground'
          }`}
        >
          {t('recommended', '推荐')}
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

      <div className="border rounded-xl bg-card p-3">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          {modelTabs.map((m) => {
            const ModelIcon = modelIconMap[m.id];
            return (
              <button
                key={m.id}
                type="button"
                onClick={() => setActiveModel(m.id)}
                className={`rounded-lg border px-4 py-3 text-left transition-colors ${
                  activeModel === m.id ? 'border-primary bg-primary/5' : 'hover:bg-muted/40'
                }`}
              >
                <div className="flex items-center gap-2">
                  <ModelIcon className="w-5 h-5" />
                  <span className="text-sm font-semibold">{m.label}</span>
                </div>
                <div className="mt-2.5">
                  <span className="text-sm leading-none text-muted-foreground">
                    {t('skillsCount', '{{count}} skills', { count: modelCounts[m.id] ?? 0 })}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="text-xs text-muted-foreground">
        {t('lastSynced', 'Last synced')}: {formatTs(syncState?.last_sync_at)}
        {syncState?.last_error ? ` · ${syncState.last_error}` : ''}
      </div>

      {sourceStatuses.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
          {sourceStatuses.map((s) => (
            <div key={s.source_id} className="border rounded-md p-2 text-xs">
              <div className="font-medium flex items-center justify-between gap-2">
                <span className="truncate">{s.source_id}</span>
                <span className={`px-1.5 py-0.5 rounded border text-[10px] ${renderStatusBadge(s.last_status)}`}>
                  {s.last_status}
                </span>
              </div>
              <div className="text-muted-foreground mt-1">
                {formatTs(s.last_synced_at)}
              </div>
              {s.last_error && <div className="text-destructive mt-1 break-words">{s.last_error}</div>}
            </div>
          ))}
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
                onClick={() => setActiveMode('recommended')}
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
              >
                {t('recommended', '推荐')}
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
              {visibleInstalled.map((skill) => {
                const Icon = pickIcon(skill.icon_seed || skill.id);
                const srcStatus = sourceStatusMap.get(skill.source_id);
                return (
                  <div
                    key={`${skill.model}:${skill.id}`}
                    className="group border rounded-xl p-4 bg-card hover:shadow-sm transition cursor-pointer relative"
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

                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{skill.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{skill.description}</p>

                    <div className="mt-3 text-[11px] text-muted-foreground">
                      {t('lastUpdated', 'Last updated')}: {formatTs(skill.updated_at || skill.installed_at)}
                    </div>
                    <div className="mt-1 text-[11px] text-muted-foreground">
                      {skill.source_id}
                      {srcStatus ? ` · ${srcStatus.last_status}` : ''}
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
                return (
                  <div key={`${item.source_id}:${item.id}`} className="border rounded-xl p-4 bg-card">
                    <div className="flex items-start justify-between">
                      <div className="p-2 rounded-md bg-muted text-foreground">
                        <Icon className="w-4 h-4" />
                      </div>
                      <span className="text-[10px] text-muted-foreground">{item.source_id}</span>
                    </div>
                    <h4 className="mt-3 font-semibold text-sm line-clamp-1">{item.name}</h4>
                    <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{item.description}</p>

                    <div className="mt-3 flex justify-end">
                      {installedSkill ? (
                        <span className="text-xs px-2.5 py-1 rounded-md border text-muted-foreground inline-flex items-center gap-1">
                          <Download className="w-3.5 h-3.5" />
                          {t('installed', 'Installed')}
                        </span>
                      ) : (
                        <button
                          className="text-xs px-2.5 py-1 rounded-md bg-primary text-primary-foreground inline-flex items-center gap-1"
                          onClick={() => handleInstall(item)}
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

      <Dialog open={detailOpen} onOpenChange={setDetailOpen}>
        <DialogContent className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>{detailData?.skill.name}</DialogTitle>
            <DialogDescription>{detailData?.skill.description}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <button
                className="text-sm text-primary underline inline-flex items-center gap-1"
                onClick={() => detailData && handleOpenFolder(detailData.skill)}
              >
                <FolderOpen className="w-4 h-4" />
                {t('openFolder', 'Open Folder')}
              </button>
              {detailData && (
                <button
                  className="text-sm px-3 py-1.5 rounded-md border text-destructive hover:bg-destructive/10 inline-flex items-center gap-1"
                  onClick={() => handleUninstall(detailData.skill)}
                >
                  <Trash2 className="w-4 h-4" />
                  {t('uninstall', 'Uninstall')}
                </button>
              )}
            </div>
            <div className="max-h-[60vh] overflow-auto border rounded-md p-4 prose prose-sm dark:prose-invert max-w-none">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{detailData?.markdown || ''}</ReactMarkdown>
            </div>
          </div>
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
