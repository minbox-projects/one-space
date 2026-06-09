import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  CheckCircle2,
  Code2,
  ExternalLink,
  FileJson,
  FolderOpen,
  GitBranch,
  Loader2,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  Terminal,
  Waypoints,
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
  aiFlowProjectStatus,
  aiFlowProjectsList,
  aiFlowQueueCreate,
  type AiFlowConfigDocument,
  type AiFlowHealthCheck,
  type AiFlowInstallStatus,
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

const STATUS_DONE = new Set(['DONE']);
const STATUS_FAILED_MATCHES = ['FAILED'];
const CONFIG_SCOPES: Array<{ id: ConfigScope; label: string; format: 'json' | 'yaml' }> = [
  { id: 'global_setting', label: 'Global setting.json', format: 'json' },
  { id: 'project_setting', label: 'Project setting.json', format: 'json' },
  { id: 'project_rule', label: 'Project rule.yaml', format: 'yaml' },
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

function projectHealthLabel(project: AiFlowProjectSummary) {
  if (!project.has_ai_flow) return 'Not initialized';
  if (project.invalid_state_count > 0) return `${project.invalid_state_count} invalid`;
  if (project.failed_count > 0) return `${project.failed_count} failed`;
  if (project.pending_count > 0) return `${project.pending_count} pending`;
  return 'Healthy';
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
        title: 'AI Flow config saved',
        description: res.data.backup_path ? 'Backup created before writing.' : 'New config written.',
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
          <h3 className="text-sm font-semibold">Configuration</h3>
        </div>
        <div className="flex rounded-md border overflow-hidden">
          {CONFIG_SCOPES.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`px-3 py-1.5 text-xs ${scope === item.id ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`}
              onClick={() => onScopeChange(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
      </div>
      <div className="p-4 space-y-3 flex-1 flex flex-col">
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span className="truncate">{document?.path || selectedScope.label}</span>
          <span>{document?.exists ? 'Existing file' : 'New file'}</span>
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
          className="min-h-[220px] flex-1 resize-none rounded-md border bg-muted/20 p-3 font-mono text-xs leading-5 outline-none focus:ring-2 focus:ring-ring"
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
            Reload
          </button>
          <button
            type="button"
            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            onClick={save}
            disabled={loading || saving}
          >
            {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            Save
          </button>
        </div>
      </div>
    </section>
  );
}

function PlanDetails({
  status,
  selectedPlan,
  selectedTool,
  onToolChange,
  onLaunch,
  launchingAction,
}: {
  status: AiFlowProjectStatus;
  selectedPlan: AiFlowPlanState | null;
  selectedTool: ToolId;
  onToolChange: (tool: ToolId) => void;
  onLaunch: (action: 'plan-review' | 'coding' | 'review', slug: string) => void;
  launchingAction: string | null;
}) {
  if (!selectedPlan) {
    return (
      <section className="border rounded-lg bg-background p-6 text-sm text-muted-foreground">
        Select a plan to inspect files, transitions, and launch actions.
      </section>
    );
  }

  const disabled = !selectedPlan.slug;
  const transitionTail = selectedPlan.transitions.slice(-8).reverse();

  return (
    <section className="border rounded-lg bg-background">
      <div className="border-b px-4 py-3 flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <h3 className="font-semibold truncate">{selectedPlan.title}</h3>
            <span className={`rounded-full border px-2 py-0.5 text-xs ${statusTone(selectedPlan.current_status)}`}>
              {selectedPlan.current_status}
            </span>
          </div>
          <p className="text-xs text-muted-foreground mt-1 font-mono">{selectedPlan.slug}</p>
        </div>
        <div className="flex items-center rounded-md border overflow-hidden shrink-0">
          {(['claude', 'codex'] as ToolId[]).map((tool) => (
            <button
              key={tool}
              type="button"
              className={`px-3 py-1.5 text-xs ${selectedTool === tool ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`}
              onClick={() => onToolChange(tool)}
            >
              {tool === 'claude' ? 'Claude' : 'Codex'}
            </button>
          ))}
        </div>
      </div>
      <div className="p-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_280px]">
        <div className="space-y-4 min-w-0">
          <div className="grid gap-2 sm:grid-cols-2">
            {selectedPlan.plan_path && (
              <button
                type="button"
                onClick={() => void aiFlowOpenPath(selectedPlan.plan_path!)}
                className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted min-w-0"
              >
                <FileJson className="w-4 h-4 shrink-0" />
                <span className="truncate">Open plan Markdown</span>
              </button>
            )}
            {status.html_status_path && (
              <button
                type="button"
                onClick={() => void aiFlowOpenPath(status.html_status_path!)}
                className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted min-w-0"
              >
                <ExternalLink className="w-4 h-4 shrink-0" />
                <span className="truncate">Open HTML status</span>
              </button>
            )}
          </div>
          <div className="space-y-2">
            <h4 className="text-xs font-semibold uppercase text-muted-foreground">Review files</h4>
            {selectedPlan.review_files.length === 0 ? (
              <p className="text-sm text-muted-foreground">No review files found.</p>
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
          </div>
          <div className="space-y-2">
            <h4 className="text-xs font-semibold uppercase text-muted-foreground">Recent transitions</h4>
            <div className="space-y-2">
              {transitionTail.map((transition, index) => (
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
              ))}
            </div>
          </div>
        </div>
        <div className="space-y-2">
          {[
            ['plan-review', 'Plan review'],
            ['coding', 'Coding'],
            ['review', 'Review'],
          ].map(([action, label]) => (
            <button
              key={action}
              type="button"
              disabled={disabled || launchingAction === `${action}:${selectedPlan.slug}`}
              onClick={() => onLaunch(action as 'plan-review' | 'coding' | 'review', selectedPlan.slug)}
              className="w-full inline-flex items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {launchingAction === `${action}:${selectedPlan.slug}` ? <Loader2 className="w-4 h-4 animate-spin" /> : <Terminal className="w-4 h-4" />}
              {label}
            </button>
          ))}
          <p className="text-xs text-muted-foreground leading-5">
            Launch actions start a {selectedTool === 'claude' ? 'Claude Code' : 'Codex'} terminal and inject the explicit plan slug.
          </p>
        </div>
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
  return (
    <section className="border rounded-lg bg-background">
      <div className="border-b px-4 py-3 flex items-center gap-2">
        <Waypoints className="w-4 h-4 text-muted-foreground" />
        <h3 className="text-sm font-semibold">{title}</h3>
      </div>
      <div className="p-4 space-y-3">
        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">No {type} state files found.</p>
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
                      Resume
                    </button>
                    <button
                      type="button"
                      onClick={() => onLaunch('reopen-current', item.slug)}
                      className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                    >
                      {launchingAction === `reopen-current:${item.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RotateCcw className="w-3.5 h-3.5" />}
                      Reopen current
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
                      Group review
                    </button>
                    <button
                      type="button"
                      onClick={() => onLaunch('group-final-review', item.slug)}
                      className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                    >
                      {launchingAction === `group-final-review:${item.slug}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
                      Final review
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

export function AiFlow({ isVisible = false }: { isVisible?: boolean }) {
  const { pushToast } = useToast();
  const [installStatus, setInstallStatus] = useState<AiFlowInstallStatus | null>(null);
  const [health, setHealth] = useState<AiFlowHealthCheck | null>(null);
  const [projects, setProjects] = useState<AiFlowProjectSummary[]>([]);
  const [projectStatus, setProjectStatus] = useState<AiFlowProjectStatus | null>(null);
  const [selectedRoot, setSelectedRoot] = useState<string>('');
  const [selectedPlanSlug, setSelectedPlanSlug] = useState<string>('');
  const [extraPath, setExtraPath] = useState('');
  const [selectedTool, setSelectedTool] = useState<ToolId>('claude');
  const [selectedSessionId, setSelectedSessionId] = useState('');
  const [queueSlugDraft, setQueueSlugDraft] = useState('');
  const [queuePlanSlugsDraft, setQueuePlanSlugsDraft] = useState('');
  const [configScope, setConfigScope] = useState<ConfigScope>('project_rule');
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [creatingQueue, setCreatingQueue] = useState(false);
  const [launchingAction, setLaunchingAction] = useState<string | null>(null);
  const [pendingLaunch, setPendingLaunch] = useState<{
    action: 'plan-review' | 'coding' | 'review' | 'resume' | 'reopen-current' | 'group-review' | 'group-final-review';
    slug: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadHealth = useCallback(async () => {
    const res = await aiFlowHealthCheck();
    setHealth(res.data);
  }, []);

  const loadProjects = useCallback(async (manualPath?: string) => {
    const res = await aiFlowProjectsList(manualPath);
    setProjects(res.data);
    if (!selectedRoot && res.data.length > 0) {
      setSelectedRoot(res.data[0].root_path);
    }
  }, [selectedRoot]);

  const loadProjectStatus = useCallback(async (root: string) => {
    if (!root) {
      setProjectStatus(null);
      return;
    }
    const res = await aiFlowProjectStatus(root);
    setProjectStatus(res.data);
    if (res.data.plans.length > 0) {
      setSelectedPlanSlug((current) => current || res.data.plans[0].slug);
    }
  }, []);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await Promise.all([loadHealth(), loadProjects(extraPath || undefined)]);
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setLoading(false);
    }
  }, [extraPath, loadHealth, loadProjects]);

  useEffect(() => {
    if (!isVisible) return;
    void refreshAll();
  }, [isVisible, refreshAll]);

  useEffect(() => {
    if (!selectedRoot || !isVisible) return;
    void loadProjectStatus(selectedRoot).catch((err) => setError(aiFlowFormatError(err)));
  }, [isVisible, loadProjectStatus, selectedRoot]);

  const selectedPlan = useMemo(() => {
    if (!projectStatus) return null;
    return (
      projectStatus.plans.find((plan) => plan.slug === selectedPlanSlug) ||
      projectStatus.plans[0] ||
      null
    );
  }, [projectStatus, selectedPlanSlug]);

  const totals = useMemo(() => {
    return projects.reduce(
      (acc, project) => {
        acc.plans += project.plan_count;
        acc.pending += project.pending_count;
        acc.failed += project.failed_count;
        acc.done += project.done_count;
        return acc;
      },
      { plans: 0, pending: 0, failed: 0, done: 0 },
    );
  }, [projects]);

  const installLatest = async () => {
    setInstalling(true);
    setError(null);
    try {
      const res = await aiFlowInstallLatest();
      setInstallStatus(res.data);
      await loadHealth();
      pushToast({
        title: 'AI Flow installed',
        description: `Commit ${shortCommit(res.data.commit)}`,
        kind: 'success',
      });
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setInstalling(false);
    }
  };

  const importPath = async () => {
    await loadProjects(extraPath || undefined);
    if (extraPath) setSelectedRoot(extraPath);
  };

  const executeLaunch = async (
    action: 'plan-review' | 'coding' | 'review' | 'resume' | 'reopen-current' | 'group-review' | 'group-final-review',
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
        title: 'AI Flow session launched',
        description: `${selectedTool} ${action} ${slug}`,
        kind: 'success',
      });
    } catch (err) {
      setError(aiFlowFormatError(err));
    } finally {
      setLaunchingAction(null);
    }
  };

  const launch = async (
    action: 'plan-review' | 'coding' | 'review' | 'resume' | 'reopen-current' | 'group-review' | 'group-final-review',
    slug: string,
  ) => {
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
      setError('Queue slug and at least one plan slug are required.');
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
        title: 'AI Flow queue created',
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
            Operational control for Claude Code and Codex plan workflows.
          </p>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={refreshAll}
            disabled={loading || installing}
            className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
          <button
            type="button"
            onClick={installLatest}
            disabled={installing}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {installing ? <Loader2 className="w-4 h-4 animate-spin" /> : <GitBranch className="w-4 h-4" />}
            Install latest
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
          <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)] min-h-0 flex-1">
        <aside className="space-y-4 min-h-0 overflow-y-auto pr-1">
          <section className="border rounded-lg bg-background">
            <div className="border-b px-4 py-3 flex items-center gap-2">
              <Terminal className="w-4 h-4 text-muted-foreground" />
              <h2 className="text-sm font-semibold">Install and Health</h2>
            </div>
            <div className="p-4 space-y-3">
              <div className="grid grid-cols-2 gap-2">
                <div className="rounded-md border p-3">
                  <p className="text-xs text-muted-foreground">Repository</p>
                  <p className="text-sm font-medium mt-1">{shortCommit(installStatus?.commit || health?.repo_commit)}</p>
                </div>
                <div className="rounded-md border p-3">
                  <p className="text-xs text-muted-foreground">Branch</p>
                  <p className="text-sm font-medium mt-1">{installStatus?.branch || health?.repo_branch || 'unknown'}</p>
                </div>
              </div>
              <div className="space-y-2">
                {(health?.items || []).map((item) => (
                  <div key={item.id} className="flex items-center justify-between gap-3 rounded-md border px-3 py-2">
                    <div className="min-w-0">
                      <p className="text-sm truncate">{item.label}</p>
                      <p className="text-xs text-muted-foreground truncate">{item.path || item.status}</p>
                    </div>
                    {item.ok ? <CheckCircle2 className="w-4 h-4 text-emerald-600 shrink-0" /> : <XCircle className="w-4 h-4 text-red-600 shrink-0" />}
                  </div>
                ))}
                {!health && <p className="text-sm text-muted-foreground">Health check has not run yet.</p>}
              </div>
            </div>
          </section>

          <section className="border rounded-lg bg-background">
            <div className="border-b px-4 py-3 flex items-center gap-2">
              <FolderOpen className="w-4 h-4 text-muted-foreground" />
              <h2 className="text-sm font-semibold">Projects</h2>
            </div>
            <div className="p-4 space-y-3">
              <div className="grid grid-cols-4 gap-2">
                <div className="rounded-md border p-2 text-center">
                  <p className="text-base font-semibold">{totals.plans}</p>
                  <p className="text-[11px] text-muted-foreground">Plans</p>
                </div>
                <div className="rounded-md border p-2 text-center">
                  <p className="text-base font-semibold">{totals.pending}</p>
                  <p className="text-[11px] text-muted-foreground">Pending</p>
                </div>
                <div className="rounded-md border p-2 text-center">
                  <p className="text-base font-semibold">{totals.failed}</p>
                  <p className="text-[11px] text-muted-foreground">Failed</p>
                </div>
                <div className="rounded-md border p-2 text-center">
                  <p className="text-base font-semibold">{totals.done}</p>
                  <p className="text-[11px] text-muted-foreground">Done</p>
                </div>
              </div>
              <div className="flex gap-2">
                <input
                  value={extraPath}
                  onChange={(event) => setExtraPath(event.target.value)}
                  placeholder="Import project path"
                  className="min-w-0 flex-1 rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
                />
                <button
                  type="button"
                  onClick={importPath}
                  className="rounded-md border px-3 py-2 text-sm hover:bg-muted"
                >
                  Import
                </button>
              </div>
              <div className="space-y-2">
                {projects.map((project) => (
                  <button
                    key={project.root_path}
                    type="button"
                    onClick={() => {
                      setSelectedRoot(project.root_path);
                      setSelectedPlanSlug('');
                    }}
                    className={`w-full rounded-md border p-3 text-left hover:bg-muted ${selectedRoot === project.root_path ? 'border-primary bg-primary/5' : ''}`}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <p className="font-medium truncate">{project.name}</p>
                      <span className={`rounded-full border px-2 py-0.5 text-[11px] ${statusTone(projectHealthLabel(project))}`}>
                        {projectHealthLabel(project)}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground truncate">{project.root_path}</p>
                    <p className="mt-2 text-xs text-muted-foreground">
                      {project.plan_count} plans · {project.queue_count} queues · {project.group_count} groups
                    </p>
                  </button>
                ))}
                {projects.length === 0 && <p className="text-sm text-muted-foreground">No workspaces found.</p>}
              </div>
            </div>
          </section>
        </aside>

        <main className="min-h-0 overflow-y-auto space-y-4 pr-1">
          {projectStatus ? (
            <>
              <section className="border rounded-lg bg-background">
                <div className="border-b px-4 py-3 flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="font-semibold truncate">{projectStatus.project.name}</h2>
                    <p className="text-xs text-muted-foreground truncate">{projectStatus.project.root_path}</p>
                  </div>
                  <span className="text-xs text-muted-foreground">{formatUpdated(projectStatus.project.updated_at)}</span>
                </div>
                <div className="p-4 space-y-4">
                  <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_220px]">
                    <div>
                      <label className="text-xs font-medium text-muted-foreground">Existing OneSpace session id</label>
                      <input
                        value={selectedSessionId}
                        onChange={(event) => setSelectedSessionId(event.target.value)}
                        placeholder="Leave blank to create a new AI session"
                        className="mt-1 w-full rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
                      />
                    </div>
                    <div>
                      <label className="text-xs font-medium text-muted-foreground">Launch tool</label>
                      <div className="mt-1 flex rounded-md border overflow-hidden">
                        {(['claude', 'codex'] as ToolId[]).map((tool) => (
                          <button
                            key={tool}
                            type="button"
                            className={`flex-1 px-3 py-2 text-sm ${selectedTool === tool ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`}
                            onClick={() => setSelectedTool(tool)}
                          >
                            {tool === 'claude' ? 'Claude' : 'Codex'}
                          </button>
                        ))}
                      </div>
                    </div>
                  </div>
                  <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                    {projectStatus.plans.map((plan) => (
                      <button
                        key={plan.slug}
                        type="button"
                        onClick={() => setSelectedPlanSlug(plan.slug)}
                        className={`rounded-md border p-3 text-left hover:bg-muted min-w-0 ${selectedPlan?.slug === plan.slug ? 'border-primary bg-primary/5' : ''}`}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <p className="font-medium truncate">{plan.title}</p>
                          <span className={`rounded-full border px-2 py-0.5 text-[11px] ${statusTone(plan.current_status)}`}>
                            {plan.current_status}
                          </span>
                        </div>
                        <p className="mt-1 text-xs text-muted-foreground font-mono truncate">{plan.slug}</p>
                        <p className="mt-2 text-xs text-muted-foreground">{formatUpdated(plan.updated_at)}</p>
                      </button>
                    ))}
                  </div>
                </div>
              </section>

              {projectStatus.invalid_states.length > 0 && (
                <section className="border border-red-200 rounded-lg bg-red-50 p-4 text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
                  <div className="flex items-center gap-2 font-medium">
                    <AlertCircle className="w-4 h-4" />
                    Invalid state files
                  </div>
                  <div className="mt-3 space-y-2">
                    {projectStatus.invalid_states.map((item) => (
                      <div key={item.path} className="text-xs font-mono">
                        {item.path}: {item.error}
                      </div>
                    ))}
                  </div>
                </section>
              )}

              <PlanDetails
                status={projectStatus}
                selectedPlan={selectedPlan}
                selectedTool={selectedTool}
                onToolChange={setSelectedTool}
                onLaunch={(action, slug) => void launch(action, slug)}
                launchingAction={launchingAction}
              />

              <div className="grid gap-4 xl:grid-cols-2">
                <OrchestrationList
                  title="Queues"
                  items={projectStatus.queues}
                  type="queue"
                  onLaunch={(action, slug) => void launch(action, slug)}
                  launchingAction={launchingAction}
                />
                <OrchestrationList
                  title="Plan Groups"
                  items={projectStatus.groups}
                  type="group"
                  onLaunch={(action, slug) => void launch(action, slug)}
                  launchingAction={launchingAction}
                />
              </div>

              <section className="border rounded-lg bg-background">
                <div className="border-b px-4 py-3 flex items-center gap-2">
                  <Waypoints className="w-4 h-4 text-muted-foreground" />
                  <h3 className="text-sm font-semibold">Create Queue</h3>
                </div>
                <div className="p-4 grid gap-3 lg:grid-cols-[220px_minmax(0,1fr)_auto]">
                  <input
                    value={queueSlugDraft}
                    onChange={(event) => setQueueSlugDraft(event.target.value)}
                    placeholder="queue-slug"
                    className="rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
                  />
                  <input
                    value={queuePlanSlugsDraft}
                    onChange={(event) => setQueuePlanSlugsDraft(event.target.value)}
                    placeholder="plan slug list, separated by space or comma"
                    className="rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
                  />
                  <button
                    type="button"
                    disabled={creatingQueue}
                    onClick={createQueue}
                    className="inline-flex items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                  >
                    {creatingQueue ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4" />}
                    Create
                  </button>
                </div>
              </section>

              <ConfigEditor
                projectRoot={projectStatus.project.root_path}
                scope={configScope}
                onScopeChange={setConfigScope}
              />
            </>
          ) : (
            <section className="h-full min-h-[420px] rounded-lg border bg-background flex items-center justify-center">
              <div className="text-center">
                <Code2 className="w-8 h-8 mx-auto text-muted-foreground" />
                <p className="mt-3 text-sm text-muted-foreground">Select an AI Flow project.</p>
              </div>
            </section>
          )}
        </main>
      </div>
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
