import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import {
  AlertCircle,
  ArrowLeft,
  CheckCircle2,
  Code2,
  ExternalLink,
  FileJson,
  FileText,
  FolderOpen,
  GitBranch,
  Layers3,
  Loader2,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  ShieldCheck,
  Terminal,
  Waypoints,
  X,
  XCircle,
} from 'lucide-react';
import {
  aiFlowConfigGet,
  aiFlowConfigSave,
  aiFlowFormatError,
  aiFlowHealthCheck,
  aiFlowInstallLatest,
  aiFlowLaunchAction,
  aiFlowLaunchPreview,
  aiFlowOpenPath,
  aiFlowPlanContentGet,
  aiFlowProjectStatus,
  aiFlowProjectsList,
  aiFlowQueueCreate,
  type AiFlowConfigDocument,
  type AiFlowHealthCheck,
  type AiFlowInstallStatus,
  type AiFlowPlanContent,
  type AiFlowPlanGroupState,
  type AiFlowPlanState,
  type AiFlowProjectStatus,
  type AiFlowProjectSummary,
  type AiFlowQueueState,
} from '@/lib/aiFlow';
import { useToast } from '../ToastProvider';
import { TerminalPermissionConfirmDialog } from '../TerminalPermissionConfirmDialog';
import type { TerminalPermissionMode } from '@/lib/terminalPermissions';

type ToolId = 'claude' | 'codex';
type ConfigScope = 'global_setting' | 'project_setting' | 'project_rule';
type ProjectTab = 'plans' | 'queues' | 'groups' | 'config';
type LaunchAction =
  | 'plan-review'
  | 'coding'
  | 'review'
  | 'resume'
  | 'reopen-current'
  | 'group-review'
  | 'group-final-review';
type PlanLaunchAction = Extract<LaunchAction, 'plan-review' | 'coding' | 'review'>;
type TFunction = (key: string, fallback: string, options?: Record<string, unknown>) => string;

const STATUS_DONE = new Set(['DONE']);
const STATUS_FAILED_MATCHES = ['FAILED'];
const CONFIG_SCOPES: Array<{ id: ConfigScope; labelKey: string; fallback: string; format: 'json' | 'yaml' }> = [
  { id: 'global_setting', labelKey: 'aiFlowConfigGlobalSetting', fallback: 'Global setting.json', format: 'json' },
  { id: 'project_setting', labelKey: 'aiFlowConfigProjectSetting', fallback: 'Project setting.json', format: 'json' },
  { id: 'project_rule', labelKey: 'aiFlowConfigProjectRule', fallback: 'Project rule.yaml', format: 'yaml' },
];

function statusTone(status?: string | null) {
  const normalized = (status || '').toUpperCase();
  if (STATUS_DONE.has(normalized)) return 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900/60 dark:bg-emerald-950/30 dark:text-emerald-300';
  if (STATUS_FAILED_MATCHES.some((item) => normalized.includes(item))) return 'border-red-200 bg-red-50 text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300';
  if (normalized.includes('AWAITING') || normalized.includes('PENDING') || normalized.includes('PROGRESS')) return 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-300';
  return 'border-border bg-muted/40 text-muted-foreground';
}

function shortCommit(commit?: string | null) {
  return commit ? commit.slice(0, 10) : 'unknown';
}

function formatUpdated(value?: string | null) {
  if (!value) return 'No timestamp';
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(parsed);
}

function projectHealthLabel(project: AiFlowProjectSummary, t: TFunction) {
  if (!project.has_ai_flow) return t('aiFlowProjectNotInitialized', 'Not initialized');
  if (project.invalid_state_count > 0) return t('aiFlowProjectInvalidCount', '{{count}} invalid', { count: project.invalid_state_count });
  if (project.failed_count > 0) return t('aiFlowProjectFailedCount', '{{count}} failed', { count: project.failed_count });
  if (project.pending_count > 0) return t('aiFlowProjectPendingCount', '{{count}} pending', { count: project.pending_count });
  return t('aiFlowProjectHealthy', 'Healthy');
}

function planLaunchActionKeysForStatus(status?: string | null): PlanLaunchAction[] {
  const normalized = (status || '').trim().toUpperCase();
  if (!normalized) return ['plan-review'];
  if (normalized === 'DONE' || normalized.includes('CANCEL') || normalized.includes('ARCHIV')) return [];

  if (normalized.includes('PLAN_REVIEW')) return ['plan-review'];
  if (normalized === 'AWAITING_REVIEW' || normalized.includes('AWAITING_CODING_REVIEW')) return ['review'];
  if (
    normalized === 'PLANNED' ||
    normalized === 'IMPLEMENTING' ||
    normalized === 'REVIEW_FAILED' ||
    normalized === 'FIXING_REVIEW' ||
    normalized.includes('IMPLEMENT') ||
    normalized.includes('CODING') ||
    normalized.includes('FIXING')
  ) {
    return ['coding'];
  }
  if (normalized.includes('AWAITING') || normalized.includes('PENDING') || normalized.includes('READY')) {
    return ['plan-review'];
  }

  return [];
}

function planLaunchActionsForStatus(status: string | null | undefined, t: TFunction) {
  const labelByAction: Record<PlanLaunchAction, string> = {
    'plan-review': t('aiFlowPlanReview', 'Plan review'),
    coding: t('aiFlowCoding', 'Coding'),
    review: t('aiFlowReview', 'Review'),
  };
  return planLaunchActionKeysForStatus(status).map((action) => ({
    action,
    label: labelByAction[action],
  }));
}

function ConfigEditor({
  projectRoot,
  scope,
  onScopeChange,
}: {
  projectRoot?: string;
  scope: ConfigScope;
  onScopeChange: (scope: ConfigScope) => void;
}) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [document, setDocument] = useState<AiFlowConfigDocument | null>(null);
  const [draft, setDraft] = useState('');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedScope = CONFIG_SCOPES.find((item) => item.id === scope) || CONFIG_SCOPES[0];

  const load = useCallback(async () => {
    if (scope !== 'global_setting' && !projectRoot) return;
    setLoading(true);
    setError(null);
    try {
      const res = await aiFlowConfigGet(scope, projectRoot);
      setDocument(res.data);
      setDraft(res.data.content);
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setLoading(false);
    }
  }, [projectRoot, scope]);

  useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const res = await aiFlowConfigSave({
        scope,
        project_root: projectRoot || null,
        format: selectedScope.format,
        content: draft,
      });
      pushToast({
        title: t('aiFlowConfigSaved', 'AI Flow config saved'),
        description: res.data.backup_path
          ? t('aiFlowConfigBackupCreated', 'Backup created before writing.')
          : t('aiFlowConfigNewWritten', 'New config written.'),
        kind: 'success',
      });
      await load();
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="border rounded-lg bg-background min-h-[320px] flex flex-col">
      <div className="border-b px-4 py-3 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Settings2 className="w-4 h-4 text-muted-foreground" />
          <h3 className="text-sm font-semibold">{t('aiFlowConfiguration', 'Configuration')}</h3>
        </div>
        <div className="flex rounded-md border overflow-hidden">
          {CONFIG_SCOPES.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`px-3 py-1.5 text-xs ${scope === item.id ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`}
              onClick={() => onScopeChange(item.id)}
            >
              {t(item.labelKey, item.fallback)}
            </button>
          ))}
        </div>
      </div>
      <div className="p-4 space-y-3 flex-1 flex flex-col">
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span className="truncate">{document?.path || t(selectedScope.labelKey, selectedScope.fallback)}</span>
          <span>{document?.exists ? t('aiFlowExistingFile', 'Existing file') : t('aiFlowNewFile', 'New file')}</span>
        </div>
        {error && (
          <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
            <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
            <span>{error}</span>
          </div>
        )}
        <textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          spellCheck={false}
          className="min-h-[260px] flex-1 resize-none rounded-md border bg-muted/20 p-3 font-mono text-xs leading-5 outline-none focus:ring-2 focus:ring-ring"
          disabled={loading}
        />
        <div className="flex justify-end gap-2">
          <button
            type="button"
            className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
            onClick={load}
            disabled={loading || saving}
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            {t('reload', 'Reload')}
          </button>
          <button
            type="button"
            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            onClick={save}
            disabled={loading || saving}
          >
            {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            {t('save', 'Save')}
          </button>
        </div>
      </div>
    </section>
  );
}

function HealthDialog({
  open,
  installStatus,
  health,
  loading,
  installing,
  error,
  onClose,
  onInstall,
  onRefresh,
}: {
  open: boolean;
  installStatus: AiFlowInstallStatus | null;
  health: AiFlowHealthCheck | null;
  loading: boolean;
  installing: boolean;
  error: string | null;
  onClose: () => void;
  onInstall: () => void;
  onRefresh: () => void;
}) {
  const { t } = useTranslation();
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4" role="presentation">
      <section className="w-full max-w-3xl max-h-[86vh] overflow-hidden rounded-lg border bg-background shadow-xl" role="dialog" aria-modal="true" aria-labelledby="ai-flow-health-title">
        <div className="border-b px-5 py-4 flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h2 id="ai-flow-health-title" className="text-base font-semibold">{t('aiFlowInstallHealthTitle', 'Install and Health Check')}</h2>
            <p className="mt-1 text-sm text-muted-foreground">{t('aiFlowInstallHealthDesc', 'Install the AI Flow runtime and verify local dependencies.')}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="inline-flex h-8 w-8 items-center justify-center rounded-md border hover:bg-muted"
            aria-label={t('close', 'Close')}
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="overflow-y-auto p-5 space-y-4 max-h-[calc(86vh-132px)]">
          {error && (
            <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
              <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}
          <div className="grid grid-cols-2 gap-3">
            <div className="rounded-md border p-3">
              <p className="text-xs text-muted-foreground">{t('aiFlowRepository', 'Repository')}</p>
              <p className="text-sm font-medium mt-1">{shortCommit(installStatus?.commit || health?.repo_commit)}</p>
            </div>
            <div className="rounded-md border p-3">
              <p className="text-xs text-muted-foreground">{t('aiFlowBranch', 'Branch')}</p>
              <p className="text-sm font-medium mt-1">{installStatus?.branch || health?.repo_branch || 'unknown'}</p>
            </div>
          </div>
          <div className="space-y-2">
            {(health?.items || []).map((item) => (
              <div key={item.id} className="flex items-center justify-between gap-3 rounded-md border px-3 py-2">
                <div className="min-w-0">
                  <p className="text-sm truncate">{item.label}</p>
                  <p className="text-xs text-muted-foreground truncate">{item.path || item.detail || item.status}</p>
                </div>
                {item.ok ? <CheckCircle2 className="w-4 h-4 text-emerald-600 shrink-0" /> : <XCircle className="w-4 h-4 text-red-600 shrink-0" />}
              </div>
            ))}
            {!health && <p className="text-sm text-muted-foreground">{t('aiFlowHealthNotRun', 'Health check has not run yet.')}</p>}
          </div>
          {installStatus?.log ? (
            <pre className="max-h-48 overflow-auto rounded-md border bg-muted/30 p-3 text-xs leading-5">{installStatus.log}</pre>
          ) : null}
        </div>
        <div className="border-t px-5 py-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onRefresh}
            disabled={loading || installing}
            className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            {t('aiFlowRunHealthCheck', 'Run health check')}
          </button>
          <button
            type="button"
            onClick={onInstall}
            disabled={installing}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {installing ? <Loader2 className="w-4 h-4 animate-spin" /> : <GitBranch className="w-4 h-4" />}
            {t('aiFlowInstallLatest', 'Install latest')}
          </button>
        </div>
      </section>
    </div>
  );
}

function ProjectCard({
  project,
  onOpen,
}: {
  project: AiFlowProjectSummary;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  const healthLabel = projectHealthLabel(project, t as TFunction);

  return (
    <button
      type="button"
      onClick={onOpen}
      className="group min-w-0 rounded-lg border bg-card p-4 text-left shadow-sm transition hover:border-primary/40 hover:bg-muted/30"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="font-semibold truncate">{project.name}</h2>
          <p className="mt-1 text-xs text-muted-foreground truncate">{project.root_path}</p>
        </div>
        <span className={`shrink-0 rounded-full border px-2 py-0.5 text-[11px] ${statusTone(healthLabel)}`}>
          {healthLabel}
        </span>
      </div>
      <div className="mt-4 grid grid-cols-4 gap-2 text-center">
        <div className="rounded-md border bg-background/60 p-2">
          <p className="text-base font-semibold">{project.plan_count}</p>
          <p className="text-[11px] text-muted-foreground">{t('aiFlowPlans', 'Plans')}</p>
        </div>
        <div className="rounded-md border bg-background/60 p-2">
          <p className="text-base font-semibold">{project.queue_count}</p>
          <p className="text-[11px] text-muted-foreground">{t('aiFlowQueues', 'Queues')}</p>
        </div>
        <div className="rounded-md border bg-background/60 p-2">
          <p className="text-base font-semibold">{project.group_count}</p>
          <p className="text-[11px] text-muted-foreground">{t('aiFlowGroups', 'Groups')}</p>
        </div>
        <div className="rounded-md border bg-background/60 p-2">
          <p className="text-base font-semibold">{project.failed_count}</p>
          <p className="text-[11px] text-muted-foreground">{t('aiFlowFailed', 'Failed')}</p>
        </div>
      </div>
      <div className="mt-3 flex items-center justify-between gap-3 text-xs text-muted-foreground">
        <span>{project.from_workspace ? t('aiFlowWorkspaceSource', 'Workspace') : t('aiFlowManualSource', 'Manual')}</span>
        <span>{formatUpdated(project.updated_at)}</span>
      </div>
    </button>
  );
}

function ProjectListView({
  projects,
  onOpenProject,
}: {
  projects: AiFlowProjectSummary[];
  onOpenProject: (project: AiFlowProjectSummary) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="min-h-0 flex-1 overflow-y-auto pr-1 space-y-4">
      {projects.length === 0 ? (
        <section className="min-h-[320px] rounded-lg border bg-background flex items-center justify-center">
          <div className="text-center">
            <Code2 className="w-8 h-8 mx-auto text-muted-foreground" />
            <p className="mt-3 text-sm text-muted-foreground">{t('aiFlowNoProjects', 'No AI Flow projects found.')}</p>
          </div>
        </section>
      ) : (
        <section className="grid gap-4 md:grid-cols-2 2xl:grid-cols-3">
          {projects.map((project) => (
            <ProjectCard key={project.root_path} project={project} onOpen={() => onOpenProject(project)} />
          ))}
        </section>
      )}
    </div>
  );
}

function AddWorkingDirectoryDialog({
  open: isOpen,
  directory,
  submitting,
  error,
  onClose,
  onBrowse,
  onSubmit,
}: {
  open: boolean;
  directory: string;
  submitting: boolean;
  error: string | null;
  onClose: () => void;
  onBrowse: () => void;
  onSubmit: () => void;
}) {
  const { t } = useTranslation();
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4" role="presentation">
      <section className="w-full max-w-xl overflow-hidden rounded-lg border bg-background shadow-xl" role="dialog" aria-modal="true" aria-labelledby="ai-flow-add-dir-title">
        <div className="border-b px-5 py-4 flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h2 id="ai-flow-add-dir-title" className="text-base font-semibold">{t('aiFlowAddWorkingDirectory', 'Add Working Directory')}</h2>
            <p className="mt-1 text-sm text-muted-foreground">{t('aiFlowAddWorkingDirectoryDesc', 'Select a folder that contains an AI Flow .ai-flow directory.')}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="inline-flex h-8 w-8 items-center justify-center rounded-md border hover:bg-muted"
            aria-label={t('close', 'Close')}
            disabled={submitting}
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="p-5 space-y-4">
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">{t('workingDirectory', 'Directory')}</label>
            <div className="flex gap-2">
              <div
                className={`flex h-10 min-w-0 flex-1 items-center rounded-md border px-3 text-sm ${
                  directory ? 'bg-background' : 'bg-muted/60 text-muted-foreground'
                }`}
              >
                <span className="truncate">{directory || t('aiFlowNoDirectorySelected', 'No directory selected')}</span>
              </div>
              <button
                type="button"
                onClick={onBrowse}
                className="inline-flex items-center justify-center rounded-md border px-3 hover:bg-muted"
                title={t('browse', 'Browse')}
                aria-label={t('browse', 'Browse')}
                disabled={submitting}
              >
                <FolderOpen className="h-4 w-4" />
              </button>
            </div>
          </div>
          {error && (
            <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
              <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}
        </div>
        <div className="border-t px-5 py-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
            disabled={submitting}
          >
            {t('cancel', 'Cancel')}
          </button>
          <button
            type="button"
            onClick={onSubmit}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            disabled={submitting}
          >
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : <FolderOpen className="h-4 w-4" />}
            {t('add', 'Add')}
          </button>
        </div>
      </section>
    </div>
  );
}

function ProjectActions({
  selectedTool,
  selectedSessionId,
  htmlStatusPath,
  onToolChange,
  onSessionIdChange,
}: {
  selectedTool: ToolId;
  selectedSessionId: string;
  htmlStatusPath?: string | null;
  onToolChange: (tool: ToolId) => void;
  onSessionIdChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <section className="rounded-lg border bg-background p-4">
      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_220px_auto] lg:items-end">
        <label className="min-w-0">
          <span className="text-xs font-medium text-muted-foreground">{t('aiFlowExistingSessionId', 'Existing OneSpace session id')}</span>
          <input
            value={selectedSessionId}
            onChange={(event) => onSessionIdChange(event.target.value)}
            placeholder={t('aiFlowSessionPlaceholder', 'Leave blank to create a new AI session')}
            className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
          />
        </label>
        <div>
          <span className="text-xs font-medium text-muted-foreground">{t('aiFlowLaunchTool', 'Launch tool')}</span>
          <div className="mt-1 flex rounded-md border overflow-hidden">
            {(['claude', 'codex'] as ToolId[]).map((tool) => (
              <button
                key={tool}
                type="button"
                className={`flex-1 px-3 py-2 text-sm ${selectedTool === tool ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`}
                onClick={() => onToolChange(tool)}
              >
                {tool === 'claude' ? 'Claude' : 'Codex'}
              </button>
            ))}
          </div>
        </div>
        {htmlStatusPath ? (
          <button
            type="button"
            onClick={() => void aiFlowOpenPath(htmlStatusPath)}
            className="inline-flex items-center justify-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
          >
            <ExternalLink className="w-4 h-4" />
            {t('aiFlowOpenHtmlStatus', 'Open HTML status')}
          </button>
        ) : null}
      </div>
    </section>
  );
}

function PlanList({
  status,
  selectedTool,
  launchingAction,
  listRef,
  onOpenPlan,
  onLaunch,
}: {
  status: AiFlowProjectStatus;
  selectedTool: ToolId;
  launchingAction: string | null;
  listRef: React.RefObject<HTMLDivElement | null>;
  onOpenPlan: (slug: string) => void;
  onLaunch: (action: PlanLaunchAction, slug: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div ref={listRef} data-testid="ai-flow-plan-list" className="space-y-3 max-h-[calc(100vh-360px)] overflow-y-auto pr-1">
      {status.plans.length === 0 ? (
        <section className="rounded-lg border bg-background p-6 text-sm text-muted-foreground">
          {t('aiFlowNoPlanStateFiles', 'No plan state files found.')}
        </section>
      ) : (
        status.plans.map((plan) => {
          const actions = planLaunchActionsForStatus(plan.current_status, t as TFunction);
          return (
            <div key={plan.slug} className="relative rounded-lg border bg-card p-4 shadow-sm transition-all hover:border-primary/30">
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
                <button
                  type="button"
                  onClick={() => onOpenPlan(plan.slug)}
                  className="flex flex-1 items-start gap-3 text-left min-w-0"
                >
                  <div className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-md border bg-background">
                    <FileText className="h-5 w-5 text-muted-foreground" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-base font-semibold text-foreground truncate">{plan.title}</span>
                      <span className={`rounded-full border px-2 py-0.5 text-[11px] ${statusTone(plan.current_status)}`}>
                        {plan.current_status || 'UNKNOWN'}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground font-mono truncate">{plan.slug}</p>
                    <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                      <span>{t('aiFlowUpdated', 'Updated')}: {formatUpdated(plan.updated_at)}</span>
                      {plan.plan_file ? <span className="font-mono truncate">{plan.plan_file}</span> : null}
                    </div>
                  </div>
                </button>
                {actions.length > 0 ? (
                  <div className="flex shrink-0 flex-wrap items-center gap-2 lg:justify-end">
                    {actions.map(({ action, label }) => (
                      <button
                        key={action}
                        type="button"
                        disabled={!plan.slug || launchingAction === `${action}:${plan.slug}`}
                        onClick={() => onLaunch(action, plan.slug)}
                        className="inline-flex h-8 items-center justify-center gap-1.5 rounded-md border bg-background px-2.5 text-sm hover:bg-muted disabled:opacity-50"
                        title={`${selectedTool} ${label}`}
                      >
                        {launchingAction === `${action}:${plan.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
                        {label}
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

function PlanDetails({
  status,
  selectedPlan,
  selectedTool,
  content,
  loadingContent,
  contentError,
  launchingAction,
  onBack,
  onLaunch,
}: {
  status: AiFlowProjectStatus;
  selectedPlan: AiFlowPlanState | null;
  selectedTool: ToolId;
  content: AiFlowPlanContent | null;
  loadingContent: boolean;
  contentError: string | null;
  launchingAction: string | null;
  onBack: () => void;
  onLaunch: (action: PlanLaunchAction, slug: string) => void;
}) {
  const { t } = useTranslation();
  if (!selectedPlan) {
    return (
      <section className="border rounded-lg bg-background p-6 text-sm text-muted-foreground">
        {t('aiFlowPlanNotFound', 'Plan not found.')}
      </section>
    );
  }

  const disabled = !selectedPlan.slug;
  const transitionTail = selectedPlan.transitions.slice(-8).reverse();
  const planActions = planLaunchActionsForStatus(selectedPlan.current_status, t as TFunction);

  return (
    <section className="border rounded-lg bg-background">
      <div className="border-b px-4 py-3 flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
        <div className="min-w-0">
          <button
            type="button"
            onClick={onBack}
            className="mb-3 inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
            {t('aiFlowBackToPlanList', 'Back to Plan list')}
          </button>
          <div className="flex items-center gap-2 flex-wrap">
            <h3 className="font-semibold truncate">{selectedPlan.title}</h3>
            <span className={`rounded-full border px-2 py-0.5 text-xs ${statusTone(selectedPlan.current_status)}`}>
              {selectedPlan.current_status || 'UNKNOWN'}
            </span>
          </div>
          <p className="text-xs text-muted-foreground mt-1 font-mono">{selectedPlan.slug}</p>
        </div>
        {planActions.length > 0 ? (
          <div className="flex flex-wrap items-center gap-2">
            {planActions.map(({ action, label }) => (
              <button
                key={action}
                type="button"
                disabled={disabled || launchingAction === `${action}:${selectedPlan.slug}`}
                onClick={() => onLaunch(action, selectedPlan.slug)}
                className="inline-flex items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                title={`${selectedTool} ${label}`}
              >
                {launchingAction === `${action}:${selectedPlan.slug}` ? <Loader2 className="w-4 h-4 animate-spin" /> : <Terminal className="w-4 h-4" />}
                {label}
              </button>
            ))}
          </div>
        ) : null}
      </div>
      <div className="p-4 grid gap-4 xl:grid-cols-[minmax(0,1fr)_320px]">
        <div className="space-y-4 min-w-0">
          <div className="grid gap-2 sm:grid-cols-2">
            {selectedPlan.plan_path && (
              <button
                type="button"
                onClick={() => void aiFlowOpenPath(selectedPlan.plan_path!)}
                className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted min-w-0"
              >
                <FileJson className="w-4 h-4 shrink-0" />
                <span className="truncate">{t('aiFlowOpenPlanMarkdown', 'Open plan Markdown')}</span>
              </button>
            )}
            {status.html_status_path && (
              <button
                type="button"
                onClick={() => void aiFlowOpenPath(status.html_status_path!)}
                className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted min-w-0"
              >
                <ExternalLink className="w-4 h-4 shrink-0" />
                <span className="truncate">{t('aiFlowOpenHtmlStatus', 'Open HTML status')}</span>
              </button>
            )}
          </div>

          <section className="rounded-lg border bg-muted/10">
            <div className="border-b px-3 py-2 flex items-center justify-between gap-3">
              <h4 className="text-xs font-semibold uppercase text-muted-foreground">{t('aiFlowPlanContent', 'Plan content')}</h4>
              <span className="text-xs text-muted-foreground truncate">{content?.plan_path || selectedPlan.plan_path || selectedPlan.plan_file}</span>
            </div>
            {loadingContent ? (
              <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
                <Loader2 className="w-4 h-4 animate-spin" />
                {t('loading', 'Loading...')}
              </div>
            ) : contentError ? (
              <div className="flex items-start gap-2 p-4 text-sm text-red-700 dark:text-red-300">
                <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
                <span>{contentError}</span>
              </div>
            ) : content?.exists ? (
              <pre className="max-h-[58vh] overflow-auto whitespace-pre-wrap break-words p-4 font-mono text-xs leading-5">{content.content}</pre>
            ) : (
              <div className="p-4 text-sm text-muted-foreground">
                {content?.error || t('aiFlowPlanContentMissing', 'Plan content file was not found.')}
              </div>
            )}
          </section>
        </div>
        <aside className="space-y-4 min-w-0">
          <section className="space-y-2">
            <h4 className="text-xs font-semibold uppercase text-muted-foreground">{t('aiFlowMetadata', 'Metadata')}</h4>
            <div className="rounded-md border divide-y text-sm">
              <div className="flex justify-between gap-3 px-3 py-2">
                <span className="text-muted-foreground">{t('aiFlowCreated', 'Created')}</span>
                <span className="truncate">{formatUpdated(selectedPlan.created_at)}</span>
              </div>
              <div className="flex justify-between gap-3 px-3 py-2">
                <span className="text-muted-foreground">{t('aiFlowUpdated', 'Updated')}</span>
                <span className="truncate">{formatUpdated(selectedPlan.updated_at)}</span>
              </div>
              <div className="px-3 py-2">
                <p className="text-muted-foreground">{t('aiFlowStateFile', 'State file')}</p>
                <p className="mt-1 truncate font-mono text-xs">{selectedPlan.raw_state_path}</p>
              </div>
            </div>
          </section>

          <section className="space-y-2">
            <h4 className="text-xs font-semibold uppercase text-muted-foreground">{t('aiFlowReviewFiles', 'Review files')}</h4>
            {selectedPlan.review_files.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t('aiFlowNoReviewFiles', 'No review files found.')}</p>
            ) : (
              <div className="space-y-2">
                {selectedPlan.review_files.map((file) => (
                  <button
                    key={file}
                    type="button"
                    onClick={() => void aiFlowOpenPath(file)}
                    className="w-full min-w-0 rounded-md border px-3 py-2 text-left text-xs font-mono hover:bg-muted"
                  >
                    <span className="block truncate">{file}</span>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="space-y-2">
            <h4 className="text-xs font-semibold uppercase text-muted-foreground">{t('aiFlowRecentTransitions', 'Recent transitions')}</h4>
            <div className="space-y-2">
              {transitionTail.length === 0 ? (
                <p className="text-sm text-muted-foreground">{t('aiFlowNoTransitions', 'No transitions found.')}</p>
              ) : (
                transitionTail.map((transition, index) => (
                  <div key={`${transition.seq || index}-${transition.at || index}`} className="rounded-md border px-3 py-2">
                    <div className="flex items-center justify-between gap-3 text-xs">
                      <span className="font-medium">{transition.event || 'transition'}</span>
                      <span className="text-muted-foreground">{formatUpdated(transition.at)}</span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {(transition.from || 'none')} to {(transition.to || 'none')}
                    </p>
                    {transition.note && <p className="mt-1 text-xs leading-5">{transition.note}</p>}
                  </div>
                ))
              )}
            </div>
          </section>
        </aside>
      </div>
    </section>
  );
}

function OrchestrationList({
  title,
  items,
  type,
  onLaunch,
  launchingAction,
}: {
  title: string;
  items: Array<AiFlowQueueState | AiFlowPlanGroupState>;
  type: 'queue' | 'group';
  onLaunch: (action: 'resume' | 'reopen-current' | 'group-review' | 'group-final-review', slug: string) => void;
  launchingAction: string | null;
}) {
  const { t } = useTranslation();
  return (
    <section className="border rounded-lg bg-background">
      <div className="border-b px-4 py-3 flex items-center gap-2">
        <Waypoints className="w-4 h-4 text-muted-foreground" />
        <h3 className="text-sm font-semibold">{title}</h3>
      </div>
      <div className="p-4 space-y-3">
        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {type === 'queue'
              ? t('aiFlowNoQueueStateFiles', 'No queue state files found.')
              : t('aiFlowNoGroupStateFiles', 'No group state files found.')}
          </p>
        ) : (
          items.map((item) => (
            <div key={item.raw_state_path} className="rounded-md border p-3 space-y-2">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-medium truncate">{item.title || item.slug}</p>
                  <p className="text-xs text-muted-foreground font-mono truncate">{item.slug}</p>
                </div>
                <span className={`rounded-full border px-2 py-0.5 text-xs ${statusTone(item.current_status)}`}>
                  {item.current_status || 'UNKNOWN'}
                </span>
              </div>
              <div className="flex flex-wrap gap-2">
                {type === 'queue' ? (
                  <>
                    <button
                      type="button"
                      onClick={() => onLaunch('resume', item.slug)}
                      className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                    >
                      {launchingAction === `resume:${item.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
                      {t('aiFlowResume', 'Resume')}
                    </button>
                    <button
                      type="button"
                      onClick={() => onLaunch('reopen-current', item.slug)}
                      className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                    >
                      {launchingAction === `reopen-current:${item.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RotateCcw className="w-3.5 h-3.5" />}
                      {t('aiFlowReopenCurrent', 'Reopen current')}
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      type="button"
                      onClick={() => onLaunch('group-review', item.slug)}
                      className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                    >
                      {launchingAction === `group-review:${item.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
                      {t('aiFlowGroupReview', 'Group review')}
                    </button>
                    <button
                      type="button"
                      onClick={() => onLaunch('group-final-review', item.slug)}
                      className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                    >
                      {launchingAction === `group-final-review:${item.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
                      {t('aiFlowFinalReview', 'Final review')}
                    </button>
                  </>
                )}
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function InvalidStates({ status }: { status: AiFlowProjectStatus }) {
  const { t } = useTranslation();
  if (status.invalid_states.length === 0) return null;
  return (
    <section className="border border-red-200 rounded-lg bg-red-50 p-4 text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
      <div className="flex items-center gap-2 font-medium">
        <AlertCircle className="w-4 h-4" />
        {t('aiFlowInvalidStateFiles', 'Invalid state files')}
      </div>
      <div className="mt-3 space-y-2">
        {status.invalid_states.map((item) => (
          <div key={item.path} className="text-xs font-mono">
            {item.path}: {item.error}
          </div>
        ))}
      </div>
    </section>
  );
}

function ProjectDetailView({
  status,
  activeTab,
  selectedTool,
  selectedSessionId,
  selectedPlan,
  selectedPlanSlug,
  planContent,
  loadingPlanContent,
  planContentError,
  queueSlugDraft,
  queuePlanSlugsDraft,
  configScope,
  creatingQueue,
  launchingAction,
  planListRef,
  onBackProjects,
  onRefreshProject,
  onTabChange,
  onToolChange,
  onSessionIdChange,
  onOpenPlan,
  onBackPlanList,
  onLaunch,
  onQueueSlugChange,
  onQueuePlanSlugsChange,
  onCreateQueue,
  onConfigScopeChange,
}: {
  status: AiFlowProjectStatus;
  activeTab: ProjectTab;
  selectedTool: ToolId;
  selectedSessionId: string;
  selectedPlan: AiFlowPlanState | null;
  selectedPlanSlug: string;
  planContent: AiFlowPlanContent | null;
  loadingPlanContent: boolean;
  planContentError: string | null;
  queueSlugDraft: string;
  queuePlanSlugsDraft: string;
  configScope: ConfigScope;
  creatingQueue: boolean;
  launchingAction: string | null;
  planListRef: React.RefObject<HTMLDivElement | null>;
  onBackProjects: () => void;
  onRefreshProject: () => void;
  onTabChange: (tab: ProjectTab) => void;
  onToolChange: (tool: ToolId) => void;
  onSessionIdChange: (value: string) => void;
  onOpenPlan: (slug: string) => void;
  onBackPlanList: () => void;
  onLaunch: (action: LaunchAction, slug: string) => void;
  onQueueSlugChange: (value: string) => void;
  onQueuePlanSlugsChange: (value: string) => void;
  onCreateQueue: () => void;
  onConfigScopeChange: (scope: ConfigScope) => void;
}) {
  const { t } = useTranslation();
  const tabs: Array<{ id: ProjectTab; label: string; icon: React.ElementType }> = [
    { id: 'plans', label: t('aiFlowPlans', 'Plans'), icon: FileText },
    { id: 'queues', label: t('aiFlowQueues', 'Queues'), icon: Waypoints },
    { id: 'groups', label: t('aiFlowPlanGroups', 'Plan Groups'), icon: Layers3 },
    { id: 'config', label: t('aiFlowConfig', 'Config'), icon: Settings2 },
  ];

  return (
    <div className="min-h-0 flex-1 overflow-y-auto pr-1 space-y-4">
      <section className="border rounded-lg bg-background">
        <div className="border-b px-4 py-3 flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <div className="min-w-0">
            <button
              type="button"
              onClick={onBackProjects}
              className="mb-2 inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
            >
              <ArrowLeft className="w-3.5 h-3.5" />
              {t('aiFlowBackToProjects', 'Back to projects')}
            </button>
            <h2 className="font-semibold truncate">{status.project.name}</h2>
            <p className="text-xs text-muted-foreground truncate">{status.project.root_path}</p>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span>{formatUpdated(status.project.updated_at)}</span>
            <button
              type="button"
              onClick={onRefreshProject}
              className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 hover:bg-muted"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              {t('refresh', 'Refresh')}
            </button>
          </div>
        </div>
        <div className="px-4 pt-3">
          <div className="flex flex-wrap gap-2 border-b">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => onTabChange(tab.id)}
                  className={`inline-flex items-center gap-2 border-b-2 px-3 py-2 text-sm ${
                    activeTab === tab.id
                      ? 'border-primary text-foreground'
                      : 'border-transparent text-muted-foreground hover:text-foreground'
                  }`}
                >
                  <Icon className="w-4 h-4" />
                  {tab.label}
                </button>
              );
            })}
          </div>
        </div>
      </section>

      <InvalidStates status={status} />

      {activeTab === 'plans' ? (
        <>
          <ProjectActions
            selectedTool={selectedTool}
            selectedSessionId={selectedSessionId}
            htmlStatusPath={status.html_status_path}
            onToolChange={onToolChange}
            onSessionIdChange={onSessionIdChange}
          />
          {selectedPlanSlug ? (
            <PlanDetails
              status={status}
              selectedPlan={selectedPlan}
              selectedTool={selectedTool}
              content={planContent}
              loadingContent={loadingPlanContent}
              contentError={planContentError}
              launchingAction={launchingAction}
              onBack={onBackPlanList}
              onLaunch={(action, slug) => onLaunch(action, slug)}
            />
          ) : (
            <PlanList
              status={status}
              selectedTool={selectedTool}
              launchingAction={launchingAction}
              listRef={planListRef}
              onOpenPlan={onOpenPlan}
              onLaunch={(action, slug) => onLaunch(action, slug)}
            />
          )}
        </>
      ) : null}

      {activeTab === 'queues' ? (
        <div className="space-y-4">
          <section className="border rounded-lg bg-background">
            <div className="border-b px-4 py-3 flex items-center gap-2">
              <Waypoints className="w-4 h-4 text-muted-foreground" />
              <h3 className="text-sm font-semibold">{t('aiFlowCreateQueue', 'Create Queue')}</h3>
            </div>
            <div className="p-4 grid gap-3 lg:grid-cols-[220px_minmax(0,1fr)_auto]">
              <input
                value={queueSlugDraft}
                onChange={(event) => onQueueSlugChange(event.target.value)}
                placeholder={t('aiFlowQueueSlugPlaceholder', 'queue-slug')}
                className="rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
              />
              <input
                value={queuePlanSlugsDraft}
                onChange={(event) => onQueuePlanSlugsChange(event.target.value)}
                placeholder={t('aiFlowQueuePlansPlaceholder', 'plan slug list, separated by space or comma')}
                className="rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
              />
              <button
                type="button"
                disabled={creatingQueue}
                onClick={onCreateQueue}
                className="inline-flex items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              >
                {creatingQueue ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4" />}
                {t('create', 'Create')}
              </button>
            </div>
          </section>
          <OrchestrationList
            title={t('aiFlowQueues', 'Queues')}
            items={status.queues}
            type="queue"
            onLaunch={(action, slug) => onLaunch(action, slug)}
            launchingAction={launchingAction}
          />
        </div>
      ) : null}

      {activeTab === 'groups' ? (
        <OrchestrationList
          title={t('aiFlowPlanGroups', 'Plan Groups')}
          items={status.groups}
          type="group"
          onLaunch={(action, slug) => onLaunch(action, slug)}
          launchingAction={launchingAction}
        />
      ) : null}

      {activeTab === 'config' ? (
        <ConfigEditor
          projectRoot={status.project.root_path}
          scope={configScope}
          onScopeChange={onConfigScopeChange}
        />
      ) : null}
    </div>
  );
}

export function AiFlow({ isVisible = false }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [installStatus, setInstallStatus] = useState<AiFlowInstallStatus | null>(null);
  const [health, setHealth] = useState<AiFlowHealthCheck | null>(null);
  const [healthDialogOpen, setHealthDialogOpen] = useState(false);
  const [projects, setProjects] = useState<AiFlowProjectSummary[]>([]);
  const [projectStatus, setProjectStatus] = useState<AiFlowProjectStatus | null>(null);
  const [selectedRoot, setSelectedRoot] = useState<string>('');
  const [selectedPlanSlug, setSelectedPlanSlug] = useState<string>('');
  const [activeTab, setActiveTab] = useState<ProjectTab>('plans');
  const [manualProjectPath, setManualProjectPath] = useState('');
  const [addDirectoryOpen, setAddDirectoryOpen] = useState(false);
  const [addDirectoryPath, setAddDirectoryPath] = useState('');
  const [addDirectoryError, setAddDirectoryError] = useState<string | null>(null);
  const [addingDirectory, setAddingDirectory] = useState(false);
  const [selectedTool, setSelectedTool] = useState<ToolId>('claude');
  const [selectedSessionId, setSelectedSessionId] = useState('');
  const [queueSlugDraft, setQueueSlugDraft] = useState('');
  const [queuePlanSlugsDraft, setQueuePlanSlugsDraft] = useState('');
  const [configScope, setConfigScope] = useState<ConfigScope>('project_rule');
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [creatingQueue, setCreatingQueue] = useState(false);
  const [loadingPlanContent, setLoadingPlanContent] = useState(false);
  const [launchingAction, setLaunchingAction] = useState<string | null>(null);
  const [pendingLaunch, setPendingLaunch] = useState<{
    action: LaunchAction;
    slug: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [planContent, setPlanContent] = useState<AiFlowPlanContent | null>(null);
  const [planContentError, setPlanContentError] = useState<string | null>(null);
  const planListRef = useRef<HTMLDivElement>(null);
  const planListScrollTopRef = useRef(0);

  const loadHealth = useCallback(async () => {
    const res = await aiFlowHealthCheck();
    setHealth(res.data);
  }, []);

  const loadProjects = useCallback(async (manualPath?: string) => {
    const res = await aiFlowProjectsList(manualPath);
    setProjects(res.data);
  }, []);

  const loadProjectStatus = useCallback(async (root: string) => {
    if (!root) {
      setProjectStatus(null);
      return;
    }
    const res = await aiFlowProjectStatus(root);
    setProjectStatus(res.data);
  }, []);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await Promise.all([loadHealth(), loadProjects(manualProjectPath || undefined)]);
      if (selectedRoot) await loadProjectStatus(selectedRoot);
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setLoading(false);
    }
  }, [loadHealth, loadProjectStatus, loadProjects, manualProjectPath, selectedRoot]);

  const runHealthCheck = useCallback(async () => {
    setLoading(true);
    setHealthError(null);
    try {
      await loadHealth();
    } catch (err) {
      setHealthError(aiFlowFormatError(err));
    } finally {
      setLoading(false);
    }
  }, [loadHealth]);

  useEffect(() => {
    if (!isVisible) return;
    void refreshAll();
  }, [isVisible, refreshAll]);

  useEffect(() => {
    if (!selectedRoot || !isVisible) return;
    void loadProjectStatus(selectedRoot).catch((err) => setError(aiFlowFormatError(err)));
  }, [isVisible, loadProjectStatus, selectedRoot]);

  const selectedPlan = useMemo(() => {
    if (!projectStatus || !selectedPlanSlug) return null;
    return projectStatus.plans.find((plan) => plan.slug === selectedPlanSlug) || null;
  }, [projectStatus, selectedPlanSlug]);

  useEffect(() => {
    if (!projectStatus || !selectedPlanSlug) {
      setPlanContent(null);
      setPlanContentError(null);
      setLoadingPlanContent(false);
      return;
    }
    let cancelled = false;
    setLoadingPlanContent(true);
    setPlanContent(null);
    setPlanContentError(null);
    aiFlowPlanContentGet(projectStatus.project.root_path, selectedPlanSlug)
      .then((res) => {
        if (!cancelled) setPlanContent(res.data);
      })
      .catch((err) => {
        if (!cancelled) setPlanContentError(aiFlowFormatError(err));
      })
      .finally(() => {
        if (!cancelled) setLoadingPlanContent(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectStatus, selectedPlanSlug]);

  const installLatest = async () => {
    setInstalling(true);
    setHealthError(null);
    try {
      const res = await aiFlowInstallLatest();
      setInstallStatus(res.data);
      await loadHealth();
      pushToast({
        title: t('aiFlowInstalled', 'AI Flow installed'),
        description: `Commit ${shortCommit(res.data.commit)}`,
        kind: 'success',
      });
    } catch (err) {
      setHealthError(aiFlowFormatError(err));
    } finally {
      setInstalling(false);
    }
  };

  const browseWorkingDirectory = async () => {
    setAddDirectoryError(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setAddDirectoryPath(selected);
      }
    } catch (err) {
      setAddDirectoryError(t('aiFlowSelectDirectoryFailed', 'Failed to select directory: {{error}}', { error: String(err) }));
    }
  };

  const addWorkingDirectory = async () => {
    const path = addDirectoryPath.trim();
    if (!path) {
      setAddDirectoryError(t('aiFlowWorkingDirectoryRequired', 'Please select a working directory.'));
      return;
    }
    setAddingDirectory(true);
    setAddDirectoryError(null);
    try {
      const res = await aiFlowProjectsList(path);
      setProjects(res.data);
      setManualProjectPath(path);
      setSelectedRoot(path);
      setSelectedPlanSlug('');
      setActiveTab('plans');
      setAddDirectoryOpen(false);
      setAddDirectoryPath('');
    } catch (err) {
      setAddDirectoryError(aiFlowFormatError(err));
    } finally {
      setAddingDirectory(false);
    }
  };

  const openProject = (project: AiFlowProjectSummary) => {
    setSelectedRoot(project.root_path);
    setSelectedPlanSlug('');
    setActiveTab('plans');
  };

  const backToProjects = () => {
    setSelectedRoot('');
    setProjectStatus(null);
    setSelectedPlanSlug('');
    setActiveTab('plans');
  };

  const openPlan = (slug: string) => {
    planListScrollTopRef.current = planListRef.current?.scrollTop || 0;
    setSelectedPlanSlug(slug);
  };

  const backToPlanList = () => {
    setSelectedPlanSlug('');
    window.requestAnimationFrame(() => {
      if (planListRef.current) {
        planListRef.current.scrollTop = planListScrollTopRef.current;
      }
    });
  };

  const executeLaunch = async (
    action: LaunchAction,
    slug: string,
    permissionMode?: TerminalPermissionMode,
  ) => {
    if (!projectStatus || !slug.trim()) return;
    const launchKey = `${action}:${slug}`;
    setLaunchingAction(launchKey);
    setError(null);
    try {
      await aiFlowLaunchAction({
        tool: selectedTool,
        action,
        slug,
        project_root: projectStatus.project.root_path,
        session_id: selectedSessionId || null,
        permission_mode: permissionMode || null,
      });
      pushToast({
        title: t('aiFlowSessionLaunched', 'AI Flow session launched'),
        description: `${selectedTool} ${action} ${slug}`,
        kind: 'success',
      });
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setLaunchingAction(null);
    }
  };

  const launch = async (action: LaunchAction, slug: string) => {
    if (!projectStatus || !slug.trim()) return;
    setError(null);
    try {
      const preview = await aiFlowLaunchPreview({
        tool: selectedTool,
        action,
        slug,
        project_root: projectStatus.project.root_path,
        session_id: selectedSessionId || null,
      });
      if (preview.data.permission_confirmation_required) {
        setPendingLaunch({ action, slug });
        return;
      }
      await executeLaunch(action, slug);
    } catch (err) {
      setError(aiFlowFormatError(err));
    }
  };

  const confirmLaunchPermission = async (mode: TerminalPermissionMode) => {
    const target = pendingLaunch;
    setPendingLaunch(null);
    if (!target) return;
    await executeLaunch(target.action, target.slug, mode);
  };

  const createQueue = async () => {
    if (!projectStatus) return;
    const queueSlug = queueSlugDraft.trim();
    const planSlugs = queuePlanSlugsDraft
      .split(/[\s,]+/)
      .map((item) => item.trim())
      .filter(Boolean);
    if (!queueSlug || planSlugs.length === 0) {
      setError(t('aiFlowQueueValidation', 'Queue slug and at least one plan slug are required.'));
      return;
    }
    setCreatingQueue(true);
    setError(null);
    try {
      await aiFlowQueueCreate({
        project_root: projectStatus.project.root_path,
        queue_slug: queueSlug,
        plan_slugs: planSlugs,
      });
      pushToast({
        title: t('aiFlowQueueCreated', 'AI Flow queue created'),
        description: queueSlug,
        kind: 'success',
      });
      setQueueSlugDraft('');
      setQueuePlanSlugsDraft('');
      await loadProjectStatus(projectStatus.project.root_path);
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setCreatingQueue(false);
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col gap-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-normal">AI Flow</h1>
          <p className="text-sm text-muted-foreground mt-1">
            {t('aiFlowDescription', 'Operational control for Claude Code and Codex plan workflows.')}
          </p>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <button
            type="button"
            onClick={() => {
              setAddDirectoryPath(manualProjectPath);
              setAddDirectoryError(null);
              setAddDirectoryOpen(true);
            }}
            className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
          >
            <FolderOpen className="w-4 h-4" />
            {t('aiFlowAddWorkingDirectory', 'Add Working Directory')}
          </button>
          <button
            type="button"
            onClick={refreshAll}
            disabled={loading || installing}
            className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            {t('refresh', 'Refresh')}
          </button>
          <button
            type="button"
            onClick={() => setHealthDialogOpen(true)}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90"
          >
            <ShieldCheck className="w-4 h-4" />
            {t('aiFlowInstallHealthButton', 'Install and Health')}
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
          <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {selectedRoot && projectStatus ? (
        <ProjectDetailView
          status={projectStatus}
          activeTab={activeTab}
          selectedTool={selectedTool}
          selectedSessionId={selectedSessionId}
          selectedPlan={selectedPlan}
          selectedPlanSlug={selectedPlanSlug}
          planContent={planContent}
          loadingPlanContent={loadingPlanContent}
          planContentError={planContentError}
          queueSlugDraft={queueSlugDraft}
          queuePlanSlugsDraft={queuePlanSlugsDraft}
          configScope={configScope}
          creatingQueue={creatingQueue}
          launchingAction={launchingAction}
          planListRef={planListRef}
          onBackProjects={backToProjects}
          onRefreshProject={() => void loadProjectStatus(projectStatus.project.root_path)}
          onTabChange={(tab) => {
            setActiveTab(tab);
            setSelectedPlanSlug('');
          }}
          onToolChange={setSelectedTool}
          onSessionIdChange={setSelectedSessionId}
          onOpenPlan={openPlan}
          onBackPlanList={backToPlanList}
          onLaunch={(action, slug) => void launch(action, slug)}
          onQueueSlugChange={setQueueSlugDraft}
          onQueuePlanSlugsChange={setQueuePlanSlugsDraft}
          onCreateQueue={() => void createQueue()}
          onConfigScopeChange={setConfigScope}
        />
      ) : (
        <ProjectListView
          projects={projects}
          onOpenProject={openProject}
        />
      )}

      <HealthDialog
        open={healthDialogOpen}
        installStatus={installStatus}
        health={health}
        loading={loading}
        installing={installing}
        error={healthError}
        onClose={() => setHealthDialogOpen(false)}
        onInstall={() => void installLatest()}
        onRefresh={() => void runHealthCheck()}
      />
      <AddWorkingDirectoryDialog
        open={addDirectoryOpen}
        directory={addDirectoryPath}
        submitting={addingDirectory}
        error={addDirectoryError}
        onClose={() => {
          if (addingDirectory) return;
          setAddDirectoryOpen(false);
        }}
        onBrowse={() => void browseWorkingDirectory()}
        onSubmit={() => void addWorkingDirectory()}
      />
      <TerminalPermissionConfirmDialog
        open={!!pendingLaunch}
        toolId={selectedTool}
        toolLabel={selectedTool === 'claude' ? 'Claude Code' : 'Codex'}
        onConfirm={(mode) => void confirmLaunchPermission(mode)}
        onCancel={() => setPendingLaunch(null)}
      />
    </div>
  );
}
