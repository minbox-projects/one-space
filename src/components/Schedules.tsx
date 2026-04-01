import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Clock3, PauseCircle, Play, Plus, Save, Search, Trash2 } from 'lucide-react';
import { useConfirmDialog } from './ConfirmDialogProvider';
import {
  assistantScheduleDelete,
  assistantScheduleRunNow,
  assistantSchedulesList,
  assistantScheduleToggle,
  assistantScheduleUpsert,
  assistantAgentsList,
  type AgentDefinition,
  type ScheduleJob,
  type ScheduleJobView,
} from '@/lib/aiAssistant';

function createSchedule(): ScheduleJob {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: '',
    name: 'New Schedule',
    agent_id: '',
    prompt: '',
    model_profile_id: null,
    web_search_enabled: false,
    trigger: {
      kind: 'daily',
      time_of_day: '09:00',
      interval_minutes: null,
      weekdays: [1, 2, 3, 4, 5],
    },
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    output_target: 'assistant_conversation',
    conversation_id: null,
    enabled: true,
    next_run_at: null,
    last_run_at: null,
    last_status: null,
    last_error: null,
    created_at: now,
    updated_at: now,
  };
}

function formatTimestamp(ts?: number | null) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function Schedules({ isVisible = false }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [schedules, setSchedules] = useState<ScheduleJobView[]>([]);
  const [selectedScheduleId, setSelectedScheduleId] = useState<string | null>(null);
  const [draft, setDraft] = useState<ScheduleJob | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const loadData = async () => {
    setLoading(true);
    try {
      const [loadedAgents, loadedSchedules] = await Promise.all([
        assistantAgentsList(),
        assistantSchedulesList(),
      ]);
      setAgents(loadedAgents);
      setSchedules(loadedSchedules);
      setSelectedScheduleId((current) => current || loadedSchedules[0]?.id || null);
    } catch (error) {
      console.error('Failed to load schedules', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isVisible) return;
    void loadData();
  }, [isVisible]);

  useEffect(() => {
    const selected = schedules.find((schedule) => schedule.id === selectedScheduleId) || null;
    setDraft(selected ? { ...selected, trigger: { ...selected.trigger, weekdays: [...selected.trigger.weekdays] } } : null);
  }, [schedules, selectedScheduleId]);

  const filteredSchedules = useMemo(() => {
    const normalized = searchQuery.trim().toLowerCase();
    if (!normalized) return schedules;
    return schedules.filter((schedule) => {
      const haystack = `${schedule.name} ${schedule.prompt} ${schedule.last_status || ''}`.toLowerCase();
      return haystack.includes(normalized);
    });
  }, [schedules, searchQuery]);

  const handleSave = async () => {
    if (!draft) return;
    const saved = await assistantScheduleUpsert(draft);
    await loadData();
    setSelectedScheduleId(saved.id);
    setMessage(t('presetSaved', 'Saved'));
    window.setTimeout(() => setMessage(null), 2000);
  };

  const handleDelete = async () => {
    if (!draft?.id) return;
    const confirmed = await confirmDialog(
      t('assistantDeleteScheduleConfirm', 'Delete this schedule and all of its recent run history?'),
      {
        title: t('assistantDeleteSchedule', 'Delete Schedule'),
        okLabel: t('delete', 'Delete'),
      },
    );
    if (!confirmed) return;
    await assistantScheduleDelete(draft.id);
    await loadData();
    setMessage(t('deleted', 'Deleted'));
    window.setTimeout(() => setMessage(null), 2000);
  };

  const handleToggle = async (enabled: boolean) => {
    if (!draft?.id) return;
    await assistantScheduleToggle({ schedule_id: draft.id, enabled });
    await loadData();
  };

  const handleRunNow = async () => {
    if (!draft?.id) return;
    await assistantScheduleRunNow({ schedule_id: draft.id });
    await loadData();
    setMessage(t('assistantScheduleTriggered', 'Run queued'));
    window.setTimeout(() => setMessage(null), 2000);
  };

  return (
    <div className="h-full">
      <div className="grid h-full gap-6 xl:grid-cols-[320px,minmax(0,1fr)]">
        <div className="flex min-h-0 flex-col rounded-2xl border bg-card">
          <div className="border-b px-4 py-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-base font-semibold">{t('schedules', '定时任务')}</div>
                <div className="text-xs text-muted-foreground">
                  {t('assistantSchedulesDesc', 'Run agents in the background while OneSpace stays alive in tray mode.')}
                </div>
              </div>
              <button
                type="button"
                onClick={() => {
                  const created = createSchedule();
                  setSchedules((current) => [{ ...created, recent_runs: [] }, ...current]);
                  setSelectedScheduleId(created.id);
                  setDraft(created);
                }}
                className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
              >
                <Plus className="h-4 w-4" />
                {t('add', 'Add')}
              </button>
            </div>
            <div className="mt-4 flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
              <Search className="h-4 w-4 text-muted-foreground" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t('assistantSearchSchedules', '搜索任务...')}
                className="w-full bg-transparent text-sm outline-none"
              />
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-4">
            {loading ? (
              <div className="text-sm text-muted-foreground">{t('loading', 'Loading...')}</div>
            ) : (
              <div className="space-y-2">
                {filteredSchedules.map((schedule) => (
                  <button
                    key={schedule.id || schedule.name}
                    type="button"
                    onClick={() => setSelectedScheduleId(schedule.id)}
                    className={`w-full rounded-xl border px-3 py-3 text-left transition-colors ${
                      draft?.id === schedule.id ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-medium">{schedule.name}</div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          {agents.find((agent) => agent.id === schedule.agent_id)?.name || schedule.agent_id || t('assistantUnassignedAgent', 'Unassigned')}
                        </div>
                      </div>
                      <span
                        className={`rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${
                          schedule.enabled ? 'text-primary border-primary/30' : 'text-muted-foreground'
                        }`}
                      >
                        {schedule.enabled ? 'ON' : 'OFF'}
                      </span>
                    </div>
                    <div className="mt-2 flex items-center justify-between text-[11px] text-muted-foreground">
                      <span>{schedule.trigger.kind}</span>
                      <span>{formatTimestamp(schedule.next_run_at)}</span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="min-h-0 rounded-2xl border bg-card">
          <div className="border-b px-6 py-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-lg font-semibold">{draft?.name || t('assistantEmptySchedule', '选择一个定时任务')}</div>
                <div className="text-sm text-muted-foreground">
                  {t('assistantScheduleEditorDesc', 'Bind a saved Agent, a trigger rule, and a target conversation for scheduled runs.')}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {message ? <span className="text-xs text-muted-foreground">{message}</span> : null}
                <button
                  type="button"
                  onClick={() => void handleDelete()}
                  disabled={!draft?.id}
                  className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                >
                  <Trash2 className="h-4 w-4" />
                  {t('delete', 'Delete')}
                </button>
                <button
                  type="button"
                  onClick={() => void handleSave()}
                  disabled={!draft}
                  className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  <Save className="h-4 w-4" />
                  {t('saveCurrentTab', 'Save')}
                </button>
              </div>
            </div>
          </div>

          <div className="min-h-0 overflow-y-auto px-6 py-5">
            {draft ? (
              <div className="space-y-6">
                <div className="grid gap-4 md:grid-cols-2">
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('name', 'Name')}</span>
                    <input
                      value={draft.name}
                      onChange={(e) => setDraft((current) => (current ? { ...current, name: e.target.value } : current))}
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('assistantBoundAgent', '绑定 Agent')}</span>
                    <select
                      value={draft.agent_id}
                      onChange={(e) => setDraft((current) => (current ? { ...current, agent_id: e.target.value } : current))}
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    >
                      <option value="">{t('assistantSelectAgent', 'Select agent')}</option>
                      {agents.map((agent) => (
                        <option key={agent.id} value={agent.id}>
                          {agent.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>

                <label className="space-y-2 block">
                  <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('prompt', 'Prompt')}</span>
                  <textarea
                    value={draft.prompt}
                    onChange={(e) => setDraft((current) => (current ? { ...current, prompt: e.target.value } : current))}
                    className="min-h-[140px] w-full rounded-xl border bg-background px-3 py-3 text-sm leading-6"
                    placeholder={t('assistantSchedulePromptPlaceholder', 'Describe what this schedule should ask the Agent to do each time it runs.')}
                  />
                </label>

                <div className="grid gap-4 md:grid-cols-3">
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('type', 'Type')}</span>
                    <select
                      value={draft.trigger.kind}
                      onChange={(e) =>
                        setDraft((current) =>
                          current
                            ? {
                                ...current,
                                trigger: {
                                  ...current.trigger,
                                  kind: e.target.value,
                                },
                              }
                            : current,
                        )
                      }
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    >
                      <option value="interval">interval</option>
                      <option value="daily">daily</option>
                      <option value="weekly">weekly</option>
                    </select>
                  </label>
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('assistantTimeOfDay', 'Time')}</span>
                    <input
                      value={draft.trigger.time_of_day || ''}
                      onChange={(e) =>
                        setDraft((current) =>
                          current
                            ? {
                                ...current,
                                trigger: {
                                  ...current.trigger,
                                  time_of_day: e.target.value,
                                },
                              }
                            : current,
                        )
                      }
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                      placeholder="09:00"
                    />
                  </label>
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('assistantIntervalMinutes', 'Interval')}</span>
                    <input
                      type="number"
                      min="1"
                      step="1"
                      value={draft.trigger.interval_minutes || 30}
                      onChange={(e) =>
                        setDraft((current) =>
                          current
                            ? {
                                ...current,
                                trigger: {
                                  ...current.trigger,
                                  interval_minutes: Number(e.target.value),
                                },
                              }
                            : current,
                        )
                      }
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                </div>

                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 text-sm font-semibold">{t('assistantScheduleStatus', '状态与执行')}</div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <label className="flex items-center justify-between rounded-xl border bg-background px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('enabled', 'Enabled')}</div>
                        <div className="text-xs text-muted-foreground">{t('assistantScheduleEnabledDesc', 'Will continue running while the app stays alive in tray mode.')}</div>
                      </div>
                      <input
                        type="checkbox"
                        checked={draft.enabled}
                        onChange={(e) => {
                          setDraft((current) => (current ? { ...current, enabled: e.target.checked } : current));
                          void handleToggle(e.target.checked);
                        }}
                      />
                    </label>
                    <label className="flex items-center justify-between rounded-xl border bg-background px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('assistantWebSearch', '联网搜索')}</div>
                        <div className="text-xs text-muted-foreground">{t('assistantScheduleSearchDesc', 'Enable web search before each scheduled agent run.')}</div>
                      </div>
                      <input
                        type="checkbox"
                        checked={draft.web_search_enabled}
                        onChange={(e) => setDraft((current) => (current ? { ...current, web_search_enabled: e.target.checked } : current))}
                      />
                    </label>
                  </div>
                  <div className="mt-4 flex flex-wrap items-center gap-3">
                    <button
                      type="button"
                      onClick={() => void handleRunNow()}
                      disabled={!draft.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-4 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Play className="h-4 w-4" />
                      {t('runNow', '立即执行')}
                    </button>
                    <div className="inline-flex items-center gap-2 rounded-lg border bg-background px-4 py-2 text-sm text-muted-foreground">
                      <Clock3 className="h-4 w-4" />
                      {t('nextRun', '下次运行')}: {formatTimestamp(draft.next_run_at)}
                    </div>
                    <div className="inline-flex items-center gap-2 rounded-lg border bg-background px-4 py-2 text-sm text-muted-foreground">
                      {draft.enabled ? <Play className="h-4 w-4" /> : <PauseCircle className="h-4 w-4" />}
                      {draft.last_status || 'idle'}
                    </div>
                  </div>
                </div>

                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 text-sm font-semibold">{t('recentRuns', '最近运行')}</div>
                  <div className="space-y-2">
                    {(schedules.find((item) => item.id === draft.id)?.recent_runs || []).map((run) => (
                      <div key={run.id} className="rounded-xl border bg-background px-4 py-3">
                        <div className="flex items-center justify-between gap-3">
                          <div className="text-sm font-medium">{run.status}</div>
                          <div className="text-xs text-muted-foreground">{formatTimestamp(run.started_at)}</div>
                        </div>
                        {run.summary ? <div className="mt-2 text-xs text-muted-foreground">{run.summary}</div> : null}
                        {run.error_message ? <div className="mt-2 text-xs text-destructive">{run.error_message}</div> : null}
                      </div>
                    ))}
                    {(schedules.find((item) => item.id === draft.id)?.recent_runs || []).length === 0 ? (
                      <div className="text-sm text-muted-foreground">{t('assistantScheduleNoRuns', '暂无运行记录')}</div>
                    ) : null}
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-sm text-muted-foreground">{t('assistantSelectScheduleHint', '从左侧选择一个任务，或创建一个新的调度任务。')}</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
