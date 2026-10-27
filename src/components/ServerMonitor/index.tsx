import { useCallback, useEffect, useMemo, useState } from "react";
import { Activity, Pencil, Plus, Settings2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useConfirmDialog } from "@/components/ConfirmDialogProvider";
import { useToast } from "@/components/ToastProvider";
import { Button } from "@/components/ui/button";
import { ToolEmptyState } from "@/components/toolbox/ToolEmptyState";
import { ToolShell } from "@/components/toolbox/ToolShell";
import {
  SERVER_MONITOR_UPDATED_EVENT,
  formatMonitorTimestamp,
  isServerMonitorAvailable,
  latestGoodSample,
  normalizeServerMonitorSettings,
  sampleMetrics,
  sampleOutcome,
  selectNetworkInterface,
  serverMonitorDelete,
  serverMonitorHistory,
  serverMonitorRefresh,
  serverMonitorSetEnabled,
  serverMonitorSnapshot,
  serverMonitorTestConnection,
  serverMonitorUpsert,
  type ServerMonitorHistoryRange,
  type ServerMonitorSample,
  type ServerMonitorState,
  type ServerMonitorTargetView,
} from "@/lib/serverMonitor";
import { useTauriEvent } from "@/toolbox/useTauriEvent";

import { HistoryChart } from "./HistoryChart";
import { MetricCards } from "./MetricCards";
import { ServerFormDialog } from "./ServerFormDialog";

type PillTone = "success" | "warning" | "error" | "neutral";

const PILL_CLASSES: Record<PillTone, string> = {
  success: "border-emerald-500/20 bg-emerald-500/10 text-emerald-600",
  warning: "border-amber-500/20 bg-amber-500/10 text-amber-600",
  error: "border-destructive/20 bg-destructive/10 text-destructive",
  neutral: "border-border bg-muted text-muted-foreground",
};

function useBi() {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";
  const alt = i18n.getFixedT(alternate);
  const text = (key: string, values?: Record<string, string | number>) =>
    `${t(key, values)} ${alt(key, values)}`;
  return { t, alt, text };
}

function BiText({
  translationKey,
  values,
}: {
  translationKey: string;
  values?: Record<string, string | number>;
}) {
  const { t, alt } = useBi();
  const primary = t(translationKey, values);
  const secondary = alt(translationKey, values);
  return (
    <>
      {primary}
      {secondary && secondary !== primary ? (
        <span className="sr-only"> {secondary}</span>
      ) : null}
    </>
  );
}

function StatusPill({
  translationKey,
  tone,
}: {
  translationKey: string;
  tone: PillTone;
}) {
  return (
    <span
      className={`inline-flex items-center rounded-full border px-2 py-0.5 text-[11px] font-medium ${PILL_CLASSES[tone]}`}
    >
      <BiText translationKey={translationKey} />
    </span>
  );
}

/**
 * Deletion still uses the shared confirmation dialog when the provider is
 * present; the fallback keeps the tool usable in isolated previews where the
 * global provider is not mounted.
 */
function useSafeConfirmDialog() {
  try {
    return useConfirmDialog();
  } catch {
    return (message: string, options?: Parameters<ReturnType<typeof useConfirmDialog>>[1]) => {
      void options;
      return Promise.resolve(
        typeof window !== "undefined" && typeof window.confirm === "function"
          ? window.confirm(message)
          : false,
      );
    };
  }
}

function detailStatusKey(
  target: ServerMonitorTargetView,
  dailyMode: boolean,
  hasSample: boolean,
): { key: string; tone: PillTone } {
  if (!target.enabled) return { key: "serverMonitorStatusPaused", tone: "neutral" };
  if (target.blocked) return { key: "serverMonitorStatusBlocked", tone: "error" };
  if (target.collecting) return { key: "serverMonitorStatusCollecting", tone: "warning" };
  if (target.lastError) return { key: "serverMonitorStatusError", tone: "error" };
  if (dailyMode && !hasSample) return { key: "serverMonitorStatusWaiting", tone: "neutral" };
  return { key: "serverMonitorStatusActive", tone: "success" };
}

export interface ServerMonitorToolProps {
  isVisible?: boolean;
}

export function ServerMonitorTool({ isVisible = true }: ServerMonitorToolProps) {
  const { t, text } = useBi();
  const { pushToast } = useToast();
  const confirmDialog = useSafeConfirmDialog();

  const available = isServerMonitorAvailable();
  const [state, setState] = useState<ServerMonitorState | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [samples, setSamples] = useState<ServerMonitorSample[]>([]);
  const [range, setRange] = useState<ServerMonitorHistoryRange>("twentyFourHours");
  const [selectedInterface, setSelectedInterface] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [historyRefreshToken, setHistoryRefreshToken] = useState(0);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingTarget, setEditingTarget] = useState<ServerMonitorTargetView | null>(null);

  const applyState = useCallback((next: ServerMonitorState) => {
    setState(next);
    setError(null);
    setHistoryRefreshToken((token) => token + 1);
  }, []);

  const loadSnapshot = useCallback(async () => {
    if (!isServerMonitorAvailable()) return;
    setLoading(true);
    try {
      const next = await serverMonitorSnapshot();
      applyState(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [applyState]);

  useEffect(() => {
    if (!available || !isVisible) return;
    void loadSnapshot();
  }, [available, isVisible, loadSnapshot]);

  useEffect(() => {
    if (!isVisible || !state) return;
    const targets = Array.isArray(state.targets) ? state.targets : [];
    if (targets.length === 0) {
      setSelectedId(null);
      return;
    }
    setSelectedId((previous) =>
      previous && targets.some((target) => target.id === previous)
        ? previous
        : targets[0].id,
    );
  }, [isVisible, state]);

  useEffect(() => {
    setSelectedInterface(null);
  }, [selectedId, range]);

  useEffect(() => {
    setSamples([]);
  }, [selectedId]);

  useEffect(() => {
    if (!available || !isVisible || !selectedId) return;
    let cancelled = false;
    void (async () => {
      try {
        const list = await serverMonitorHistory(selectedId, range);
        if (cancelled) return;
        setSamples(Array.isArray(list) ? list : []);
      } catch {
        if (!cancelled) setSamples([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [available, isVisible, selectedId, range, historyRefreshToken]);

  const handleEvent = useCallback(
    (payload: unknown) => {
      if (!isVisible) return;
      if (payload && typeof payload === "object" && "targets" in payload) {
        applyState(payload as ServerMonitorState);
        return;
      }
      void loadSnapshot();
    },
    [applyState, isVisible, loadSnapshot],
  );

  useTauriEvent(SERVER_MONITOR_UPDATED_EVENT, handleEvent, available && isVisible, {
    respectVisibility: true,
  });

  const settings = useMemo(
    () => normalizeServerMonitorSettings(state?.settings),
    [state?.settings],
  );
  const targets = useMemo(
    () => (Array.isArray(state?.targets) ? state?.targets ?? [] : []),
    [state?.targets],
  );
  const selectedTarget = useMemo(
    () => targets.find((target) => target.id === selectedId) ?? targets[0] ?? null,
    [targets, selectedId],
  );

  const orderedSamples = useMemo(
    () => [...samples].sort((a, b) => b.collectedAtMs - a.collectedAtMs),
    [samples],
  );
  const latestSample = useMemo(
    () => latestGoodSample(orderedSamples),
    [orderedSamples],
  );
  const metrics = useMemo(() => sampleMetrics(latestSample), [latestSample]);
  const networks = useMemo(() => {
    if (Array.isArray(metrics?.networks) && metrics.networks.length > 0) {
      return metrics.networks;
    }
    return metrics?.network ? [metrics.network] : [];
  }, [metrics]);
  const selectedNetwork = useMemo(
    () => selectNetworkInterface(latestSample, selectedInterface),
    [latestSample, selectedInterface],
  );
  const partial = useMemo(() => {
    if (!metrics) return false;
    return (
      sampleOutcome(latestSample) === "partial" ||
      !metrics.cpu ||
      !metrics.memory ||
      !Array.isArray(metrics.disks) ||
      metrics.disks.length === 0 ||
      networks.length === 0
    );
  }, [metrics, latestSample, networks.length]);

  const nextCollectionAt =
    selectedTarget?.nextCollectionAtMs ?? state?.nextCollectionAtMs ?? null;
  const lastSuccessAt =
    selectedTarget?.lastSuccessAtMs ?? state?.lastSuccessAtMs ?? null;
  const status = selectedTarget
    ? detailStatusKey(selectedTarget, settings.scheduleMode === "daily", Boolean(metrics))
    : null;

  const openAdd = () => {
    setEditingTarget(null);
    setDialogOpen(true);
  };

  const openEdit = (target: ServerMonitorTargetView) => {
    setEditingTarget(target);
    setDialogOpen(true);
  };

  const handleCollectNow = async (target: ServerMonitorTargetView) => {
    try {
      const next = await serverMonitorRefresh(target.id);
      if (next && Array.isArray(next.targets)) {
        applyState(next);
      } else {
        await loadSnapshot();
      }
      pushToast({
        title: t("serverMonitorCollected"),
        description: text("serverMonitorCollected"),
        kind: "success",
      });
    } catch (reason) {
      pushToast({
        title: t("serverMonitorCollectFailed"),
        description: reason instanceof Error ? reason.message : String(reason),
        kind: "error",
      });
    }
  };

  const handleToggleEnabled = async (target: ServerMonitorTargetView) => {
    try {
      const next = await serverMonitorSetEnabled(target.id, !target.enabled);
      if (next && Array.isArray(next.targets)) {
        applyState(next);
      } else {
        await loadSnapshot();
      }
    } catch (reason) {
      pushToast({
        title: t("serverMonitorSaveFailed"),
        description: reason instanceof Error ? reason.message : String(reason),
        kind: "error",
      });
    }
  };

  const handleDelete = async (target: ServerMonitorTargetView) => {
    const confirmed = await confirmDialog(text("serverMonitorDeleteConfirm", { name: target.name }), {
      title: text("serverMonitorDeleteTitle"),
      okLabel: text("delete"),
      cancelLabel: text("cancel"),
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      const next = await serverMonitorDelete(target.id);
      if (next && Array.isArray(next.targets)) {
        applyState(next);
      } else {
        await loadSnapshot();
      }
      pushToast({
        title: t("serverMonitorDeleted"),
        description: text("serverMonitorDeleted"),
        kind: "success",
      });
    } catch (reason) {
      pushToast({
        title: t("serverMonitorDeleteFailed"),
        description: reason instanceof Error ? reason.message : String(reason),
        kind: "error",
      });
    }
  };

  const handleSaveTarget = async (draft: Parameters<typeof serverMonitorUpsert>[0]) => {
    try {
      const next = await serverMonitorUpsert(draft);
      if (next && Array.isArray(next.targets)) {
        applyState(next);
      } else {
        await loadSnapshot();
      }
      pushToast({
        title: t("serverMonitorSaved"),
        description: text("serverMonitorSaved"),
        kind: "success",
      });
    } catch (reason) {
      pushToast({
        title: t("serverMonitorSaveFailed"),
        description: reason instanceof Error ? reason.message : String(reason),
        kind: "error",
      });
      throw reason;
    }
  };

  const openSettings = () => {
    const appWindow = window as typeof window & {
      setSettingsTab?: (tab: string) => void;
      setSettingsOpen?: (open: boolean) => void;
    };
    appWindow.setSettingsTab?.("server-monitor");
    appWindow.setSettingsOpen?.(true);
  };

  if (!available) {
    return (
      <ToolShell
        title={t("serverMonitor")}
        description={t("serverMonitorDesc")}
        icon={
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
            <Activity className="h-5 w-5" />
          </div>
        }
      >
        <div className="rounded-xl border border-dashed border-border p-8 text-center">
          <p className="text-sm font-medium text-foreground">
            <BiText translationKey="serverMonitorUnavailableTitle" />
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            <BiText translationKey="serverMonitorUnavailableDesc" />
          </p>
        </div>
      </ToolShell>
    );
  }

  return (
    <ToolShell
      title={t("serverMonitor")}
      description={t("serverMonitorDesc")}
      icon={
        <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
          <Activity className="h-5 w-5" />
        </div>
      }
      error={null}
      actions={
        <>
          <Button type="button" size="sm" onClick={openAdd}>
            <Plus className="h-3.5 w-3.5" />
            <BiText translationKey="serverMonitorAdd" />
          </Button>
          {selectedTarget ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => void handleCollectNow(selectedTarget)}
            >
              <Activity className="h-3.5 w-3.5" />
              <BiText translationKey="serverMonitorCollectNow" />
            </Button>
          ) : null}
          <Button type="button" size="sm" variant="outline" onClick={openSettings}>
            <Settings2 className="h-3.5 w-3.5" />
            <BiText translationKey="serverMonitorSettings" />
          </Button>
        </>
      }
    >
      {error ? (
        <div className="space-y-3">
          <div
            role="alert"
            className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive break-words"
          >
            {error}
          </div>
          <Button type="button" size="sm" variant="outline" onClick={() => void loadSnapshot()}>
            <BiText translationKey="serverMonitorRetry" />
          </Button>
        </div>
      ) : null}

      {!error && !state && loading ? (
        <p role="status" className="text-sm text-muted-foreground">
          <BiText translationKey="serverMonitorLoading" />
        </p>
      ) : null}

      {!error && state && targets.length === 0 ? (
        <div>
          <ToolEmptyState
            title={t("serverMonitorEmptyTitle")}
            description={t("serverMonitorEmptyDesc")}
          />
          <span className="sr-only">
            {" "}
            <BiText translationKey="serverMonitorEmptyTitle" />{" "}
            <BiText translationKey="serverMonitorEmptyDesc" />
          </span>
        </div>
      ) : null}

      {targets.length > 0 ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
          <aside className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              <BiText translationKey="serverMonitorListTitle" />
            </h3>
            <div className="flex gap-2 overflow-x-auto pb-1 lg:flex-col lg:overflow-visible lg:pb-0">
              {targets.map((target) => {
                const active = selectedTarget?.id === target.id;
                const listKey = target.blocked
                  ? "serverMonitorStatusError"
                  : !target.enabled
                    ? "serverMonitorStatusPaused"
                    : target.collecting
                      ? "serverMonitorStatusCollecting"
                      : "serverMonitorStatusActive";
                return (
                  <button
                    key={target.id}
                    type="button"
                    aria-pressed={active}
                    onClick={() => setSelectedId(target.id)}
                    className={`min-w-[160px] rounded-xl border px-3 py-2 text-left transition-colors lg:min-w-0 ${
                      active
                        ? "border-primary/50 bg-primary/10"
                        : "border-border/70 bg-card hover:border-primary/30"
                    }`}
                  >
                    <span className="block truncate text-sm font-medium text-foreground">
                      {target.name}
                    </span>
                    <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                      {target.host ?? target.savedHostName ?? target.id}
                    </span>
                    <span className="mt-1 block text-[11px] text-muted-foreground">
                      <BiText translationKey={listKey} />
                    </span>
                  </button>
                );
              })}
            </div>
          </aside>

          <section className="min-w-0 space-y-4">
            {selectedTarget ? (
              <>
                <div className="flex flex-wrap items-start justify-between gap-3 rounded-xl border border-border/70 bg-card p-4 shadow-sm">
                  <div className="min-w-0">
                    <p className="truncate font-mono text-sm text-foreground">
                      {selectedTarget.host ??
                        selectedTarget.savedHostName ??
                        selectedTarget.id}
                      {selectedTarget.user ? ` · ${selectedTarget.user}` : ""}
                      {selectedTarget.port ? ` · ${selectedTarget.port}` : ""}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {settings.scheduleMode === "daily" ? (
                        <BiText
                          translationKey="serverMonitorScheduleDaily"
                          values={{ times: settings.dailyTimes.join(", ") }}
                        />
                      ) : (
                        <BiText
                          translationKey="serverMonitorScheduleInterval"
                          values={{ minutes: settings.intervalMinutes }}
                        />
                      )}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {status ? <StatusPill translationKey={status.key} tone={status.tone} /> : null}
                    {partial ? (
                      <StatusPill translationKey="serverMonitorPartial" tone="warning" />
                    ) : null}
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                  <div className="rounded-xl border border-border/70 bg-card p-3">
                    <p className="text-[11px] uppercase tracking-wider text-muted-foreground">
                      <BiText translationKey="serverMonitorNextCollection" />
                    </p>
                    <p className="mt-1 text-sm font-medium text-foreground">
                      {nextCollectionAt ? (
                        formatMonitorTimestamp(nextCollectionAt)
                      ) : (
                        <BiText translationKey="serverMonitorNever" />
                      )}
                    </p>
                  </div>
                  <div className="rounded-xl border border-border/70 bg-card p-3">
                    <p className="text-[11px] uppercase tracking-wider text-muted-foreground">
                      <BiText translationKey="serverMonitorLastSuccess" />
                    </p>
                    <p className="mt-1 text-sm font-medium text-foreground">
                      {lastSuccessAt ? (
                        formatMonitorTimestamp(lastSuccessAt)
                      ) : (
                        <BiText translationKey="serverMonitorNever" />
                      )}
                    </p>
                  </div>
                  <div className="rounded-xl border border-border/70 bg-card p-3">
                    <p className="text-[11px] uppercase tracking-wider text-muted-foreground">
                      <BiText translationKey="serverMonitorLastGood" />
                    </p>
                    <p className="mt-1 text-sm font-medium text-foreground">
                      {latestSample ? (
                        formatMonitorTimestamp(latestSample.collectedAtMs)
                      ) : (
                        <BiText translationKey="serverMonitorNever" />
                      )}
                    </p>
                  </div>
                </div>

                {selectedTarget.lastError ? (
                  <div
                    role="alert"
                    className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive break-words"
                  >
                    {selectedTarget.lastError}
                  </div>
                ) : null}

                <MetricCards
                  metrics={metrics}
                  networks={networks}
                  selectedInterface={selectedInterface}
                  selectedNetwork={selectedNetwork}
                  onSelectInterface={setSelectedInterface}
                />

                <div className="space-y-2 rounded-xl border border-border/70 bg-card p-4">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <h3 className="text-sm font-semibold text-foreground">
                      <BiText translationKey="serverMonitorHistory" />
                    </h3>
                    <div className="flex items-center gap-1.5">
                      <Button
                        type="button"
                        size="sm"
                        variant={range === "twentyFourHours" ? "default" : "outline"}
                        onClick={() => setRange("twentyFourHours")}
                      >
                        <BiText translationKey="serverMonitorRange24h" />
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant={range === "sevenDays" ? "default" : "outline"}
                        onClick={() => setRange("sevenDays")}
                      >
                        <BiText translationKey="serverMonitorRange7d" />
                      </Button>
                    </div>
                  </div>
                  <HistoryChart
                    samples={samples}
                    range={range}
                    expectedIntervalSeconds={latestSample?.expectedIntervalSeconds ?? null}
                  />
                </div>

                <div className="flex flex-wrap gap-2">
                  <Button type="button" size="sm" variant="outline" onClick={() => openEdit(selectedTarget)}>
                    <Pencil className="h-3.5 w-3.5" />
                    <BiText translationKey="serverMonitorEdit" />
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => void handleToggleEnabled(selectedTarget)}
                  >
                    <BiText
                      translationKey={
                        selectedTarget.enabled ? "serverMonitorPause" : "serverMonitorResume"
                      }
                    />
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => void handleDelete(selectedTarget)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    <BiText translationKey="serverMonitorDelete" />
                  </Button>
                </div>
              </>
            ) : (
              <ToolEmptyState title={t("serverMonitorNoSelection")} />
            )}
          </section>
        </div>
      ) : null}

      <div className="space-y-2 rounded-xl border border-border/60 bg-muted/20 p-4">
        <h3 className="text-sm font-semibold text-foreground">
          <BiText translationKey="serverMonitorGuidanceTitle" />
        </h3>
        <ul className="list-disc space-y-1 pl-5 text-xs leading-relaxed text-muted-foreground">
          <li><BiText translationKey="serverMonitorGuidanceCommands" /></li>
          <li><BiText translationKey="serverMonitorGuidanceAccess" /></li>
          <li><BiText translationKey="serverMonitorGuidanceNoAgent" /></li>
          <li><BiText translationKey="serverMonitorGuidanceLifetime" /></li>
          <li><BiText translationKey="serverMonitorGuidanceTime" /></li>
          <li><BiText translationKey="serverMonitorGuidanceSampling" /></li>
          <li><BiText translationKey="serverMonitorGuidanceHost" /></li>
        </ul>
      </div>

      <ServerFormDialog
        isOpen={dialogOpen}
        target={editingTarget}
        onClose={() => setDialogOpen(false)}
        onSave={handleSaveTarget}
        onTestConnection={serverMonitorTestConnection}
      />
    </ToolShell>
  );
}

export default ServerMonitorTool;
