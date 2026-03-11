import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { AlertCircle, CheckCircle2, Copy, Loader2, Play, RotateCcw, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  workflowsDeleteRun,
  workflowsListRuns,
  workflowsReplayRun,
  workflowsUpdateRun,
  type WorkflowPreset,
  type WorkflowRun,
} from '@/lib/workflows';

function statusClass(status: WorkflowRun['status']) {
  if (status === 'success') return 'text-green-600 bg-green-500/10 border-green-500/20';
  if (status === 'failed') return 'text-destructive bg-destructive/10 border-destructive/20';
  if (status === 'interrupted') return 'text-amber-600 bg-amber-500/10 border-amber-500/20';
  return 'text-primary bg-primary/10 border-primary/20';
}

function statusLabel(status: WorkflowRun['status'], t: (key: string, fallback?: string) => string) {
  if (status === 'success') return t('workflowStatusSuccess', 'success');
  if (status === 'failed') return t('workflowStatusFailed', 'failed');
  if (status === 'interrupted') return t('workflowStatusInterrupted', 'interrupted');
  return t('workflowStatusRunning', 'running');
}

function dependencyModeLabel(
  mode: WorkflowRun['dependency_apply_mode'],
  t: (key: string, fallback?: string) => string,
) {
  if (mode === 'global-compat') return t('workflowDependencyModeGlobalCompat', 'global-compat');
  if (mode === 'strict-local') return t('workflowDependencyModeStrictLocal', 'strict-local');
  return t('workflowDependencyModeSharedGlobal', 'shared-global');
}

function promptApplyStatusLabel(
  status: WorkflowRun['prompt_apply_status'],
  t: (key: string, fallback?: string) => string,
) {
  if (status === 'applied') return t('workflowPromptApplyApplied', 'applied');
  if (status === 'manual') return t('workflowPromptApplyManual', 'manual');
  return t('workflowPromptApplyUnsupported', 'unsupported');
}

function formatTime(ts?: number | null) {
  if (!ts) return '-';
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function RecentWorkflowRuns({
  presets,
  onRunsChanged,
}: {
  presets: WorkflowPreset[];
  onRunsChanged?: (runs: WorkflowRun[]) => void;
}) {
  const { t } = useTranslation();
  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>('all');
  const [loading, setLoading] = useState(false);
  const [busyRunId, setBusyRunId] = useState<string | null>(null);
  const [copiedPromptRunId, setCopiedPromptRunId] = useState<string | null>(null);
  const [runToDelete, setRunToDelete] = useState<WorkflowRun | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadRuns = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await workflowsListRuns({
        preset_id: selectedPresetId === 'all' ? undefined : selectedPresetId,
        limit: 100,
      });
      setRuns(resp.data || []);
      onRunsChanged?.(resp.data || []);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadRuns();
  }, [selectedPresetId]);

  const stats = useMemo(() => {
    const total = runs.length;
    const success = runs.filter((run) => run.status === 'success').length;
    const failed = runs.filter((run) => run.status === 'failed').length;
    const interrupted = runs.filter((run) => run.status === 'interrupted').length;
    const running = runs.filter((run) => run.status === 'running').length;
    const finished = success + failed + interrupted;
    const successRate = finished > 0 ? Math.round((success / finished) * 100) : 0;
    const lastFailed = runs.find((run) => run.status === 'failed' && run.error_message);
    return { total, success, failed, interrupted, running, successRate, lastFailed };
  }, [runs]);

  const handleReplay = async (run: WorkflowRun) => {
    setBusyRunId(run.id);
    setError(null);
    try {
      await workflowsReplayRun({ run_id: run.id });
      emit('refresh-counts').catch(() => {});
      await loadRuns();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusyRunId(null);
    }
  };

  const handleRecover = async (run: WorkflowRun) => {
    if (!run.session_id) return;
    setBusyRunId(run.id);
    setError(null);
    try {
      await invoke('sessions_launch', { sessionId: run.session_id });
      emit('refresh-counts').catch(() => {});
      await loadRuns();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusyRunId(null);
    }
  };

  const handleMarkStatus = async (run: WorkflowRun, status: 'success' | 'failed') => {
    setBusyRunId(run.id);
    setError(null);
    try {
      await workflowsUpdateRun({
        run_id: run.id,
        status,
        error_message:
          status === 'failed'
            ? run.error_message || t('workflowRecentRunsMarkedFailedDefault', 'Marked as failed manually')
            : '',
      });
      await loadRuns();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusyRunId(null);
    }
  };

  const handleDeleteRun = async () => {
    if (!runToDelete) return;
    setBusyRunId(runToDelete.id);
    setError(null);
    try {
      await workflowsDeleteRun({ run_id: runToDelete.id });
      await loadRuns();
      setRunToDelete(null);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusyRunId(null);
    }
  };

  const handleCopyPrompt = async (run: WorkflowRun) => {
    try {
      await navigator.clipboard.writeText(run.launch_prompt || '');
      setCopiedPromptRunId(run.id);
      window.setTimeout(() => {
        setCopiedPromptRunId((prev) => (prev === run.id ? null : prev));
      }, 1600);
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  return (
    <div className="bg-card border rounded-xl p-4 shadow-sm space-y-4">
      <div className="flex flex-col lg:flex-row lg:items-center lg:justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold">{t('workflowRecentRuns', 'Recent Workflow')}</h3>
          <p className="text-xs text-muted-foreground">
            {t('workflowRecentRunsStats', {
              defaultValue: 'Success rate {{successRate}}% · Running {{running}} · Failed {{failed}}',
              successRate: stats.successRate,
              running: stats.running,
              failed: stats.failed,
            })}
          </p>
          {stats.lastFailed?.error_message && (
            <p className="text-xs text-destructive mt-1">
              {t('workflowRecentRunsLastFailure', {
                defaultValue: 'Last failure: {{message}}',
                message: stats.lastFailed.error_message,
              })}
            </p>
          )}
        </div>
        <div className="flex items-center gap-2">
          <select
            value={selectedPresetId}
            onChange={(e) => setSelectedPresetId(e.target.value)}
            className="h-9 rounded-md border bg-background px-2.5 text-sm"
          >
            <option value="all">{t('workflowRecentRunsAllPresets', 'All Presets')}</option>
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
          <button
            onClick={() => void loadRuns()}
            className="h-9 px-3 rounded-md border text-sm hover:bg-muted transition-colors"
          >
            {t('workflowRecentRunsRefresh', 'Refresh')}
          </button>
        </div>
      </div>

      {error && (
        <div className="text-sm text-destructive bg-destructive/10 border border-destructive/20 rounded-md px-3 py-2">
          {error}
        </div>
      )}

      <div className="rounded-lg border overflow-hidden">
        {loading ? (
          <div className="px-3 py-3 text-sm text-muted-foreground flex items-center gap-2">
            <Loader2 className="w-4 h-4 animate-spin" />
            {t('workflowRecentRunsLoading', 'Loading runs...')}
          </div>
        ) : runs.length === 0 ? (
          <div className="px-3 py-3 text-sm text-muted-foreground">
            {t('workflowRecentRunsEmpty', 'No workflow runs yet.')}
          </div>
        ) : (
          <div className="divide-y">
            {runs.map((run) => {
              const busy = busyRunId === run.id;
              return (
                <div key={run.id} className="px-3 py-3 flex flex-col gap-2">
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-sm font-medium truncate">{run.preset_name}</div>
                      <div className="text-xs text-muted-foreground truncate">
                        {run.tool}/{run.launch_scope} · {run.working_dir}
                      </div>
                    </div>
                    <span
                      className={`text-xs px-2 py-1 rounded-md border capitalize ${statusClass(
                        run.status,
                      )}`}
                    >
                      {statusLabel(run.status, (key, fallback) => t(key, fallback || key))}
                    </span>
                  </div>
                  <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                    <span>
                      {t('workflowRecentRunsStarted', 'Started')}: {formatTime(run.started_at)}
                    </span>
                    <span>
                      {t('workflowRecentRunsEnded', 'Ended')}: {formatTime(run.ended_at)}
                    </span>
                    <span>
                      {t('workflowRecentRunsDepsLabel', 'Deps')}:{' '}
                      {dependencyModeLabel(run.dependency_apply_mode, (key, fallback) => t(key, fallback || key))}
                    </span>
                    <span>
                      {t('workflowRecentRunsPromptLabel', 'Prompt')}:{' '}
                      {promptApplyStatusLabel(run.prompt_apply_status, (key, fallback) => t(key, fallback || key))}
                    </span>
                    {run.runtime_profile_id && (
                      <span className="truncate">
                        {t('workflowRecentRunsProfileLabel', 'Profile')}: {run.runtime_profile_id}
                      </span>
                    )}
                    {run.error_message && (
                      <span className="text-destructive flex items-center gap-1">
                        <AlertCircle className="w-3.5 h-3.5" />
                        {run.error_message}
                      </span>
                    )}
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {run.status === 'running' && run.session_id && (
                      <button
                        onClick={() => void handleRecover(run)}
                        disabled={busy}
                        className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                      >
                        {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
                        {t('workflowRecentRunsRecover', 'Recover')}
                      </button>
                    )}
                    <button
                      onClick={() => void handleReplay(run)}
                      disabled={busy}
                      className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                    >
                      {busy ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <RotateCcw className="w-3.5 h-3.5" />
                      )}
                      {t('workflowRecentRunsReplay', 'Replay')}
                    </button>
                    <button
                      onClick={() => setRunToDelete(run)}
                      disabled={busy}
                      className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                      {t('workflowRecentRunsDelete', 'Delete')}
                    </button>
                    {run.prompt_apply_status === 'manual' && run.launch_prompt && (
                      <button
                        onClick={() => void handleCopyPrompt(run)}
                        disabled={busy}
                        className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                      >
                        {copiedPromptRunId === run.id ? (
                          <CheckCircle2 className="w-3.5 h-3.5 text-green-600" />
                        ) : (
                          <Copy className="w-3.5 h-3.5" />
                        )}
                        {t('workflowRunCopyPrompt', 'Copy Prompt')}
                      </button>
                    )}
                    {run.status === 'running' && (
                      <>
                        <button
                          onClick={() => void handleMarkStatus(run, 'success')}
                          disabled={busy}
                          className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                        >
                          <CheckCircle2 className="w-3.5 h-3.5" />
                          {t('workflowRecentRunsMarkSuccess', 'Mark Success')}
                        </button>
                        <button
                          onClick={() => void handleMarkStatus(run, 'failed')}
                          disabled={busy}
                          className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50"
                        >
                          {t('workflowRecentRunsMarkFailed', 'Mark Failed')}
                        </button>
                      </>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {runToDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            <div className="p-5">
              <div className="flex items-center gap-3 text-destructive mb-3">
                <div className="bg-destructive/10 p-2 rounded-full">
                  <AlertCircle className="w-5 h-5" />
                </div>
                <h3 className="font-semibold">
                  {t('workflowRecentRunsDeleteTitle', 'Delete Workflow')}
                </h3>
              </div>
              <p className="text-sm text-muted-foreground">
                {t('workflowRecentRunsDeleteConfirm', {
                  defaultValue: 'Delete this workflow run record? {{name}}',
                  name: runToDelete.preset_name || '-',
                })}
              </p>
            </div>
            <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
              <button
                onClick={() => setRunToDelete(null)}
                disabled={busyRunId === runToDelete.id}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors disabled:opacity-50"
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                onClick={() => void handleDeleteRun()}
                disabled={busyRunId === runToDelete.id}
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors disabled:opacity-50"
              >
                {busyRunId === runToDelete.id && <Loader2 className="w-4 h-4 animate-spin" />}
                {t('delete', 'Delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
