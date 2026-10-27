import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  ChevronDown,
  ChevronUp,
  Info,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Server,
  Settings2,
  Trash2,
} from "lucide-react";
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
  success: "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
  warning: "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400",
  error: "border-rose-500/30 bg-rose-500/10 text-rose-600 dark:text-rose-400",
  neutral: "border-slate-300/60 bg-slate-100 text-slate-600 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300",
};

const STATUS_RING_CLASSES: Record<PillTone, string> = {
  success: "bg-emerald-500 ring-4 ring-emerald-500/20",
  warning: "bg-amber-500 ring-4 ring-amber-500/20",
  error: "bg-rose-500 ring-4 ring-rose-500/20",
  neutral: "bg-slate-400 ring-4 ring-slate-400/20",
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
      className={`inline-flex items-center rounded-full border px-2.5 py-0.5 text-[11px] font-semibold ${PILL_CLASSES[tone]}`}
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
  const [searchQuery, setSearchQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "active" | "error" | "paused">("all");
  const [guidanceOpen, setGuidanceOpen] = useState(false);

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

  const totalCount = targets.length;
  const activeCount = useMemo(
    () => targets.filter((t) => t.enabled && !t.blocked && !t.lastError).length,
    [targets],
  );
  const errorCount = useMemo(
    () => targets.filter((t) => t.blocked || Boolean(t.lastError)).length,
    [targets],
  );
  const pausedCount = useMemo(
    () => targets.filter((t) => !t.enabled).length,
    [targets],
  );

  const filteredTargets = useMemo(() => {
    return targets.filter((target) => {
      if (statusFilter === "active" && (!target.enabled || target.blocked || target.lastError)) {
        return false;
      }
      if (statusFilter === "error" && !target.blocked && !target.lastError) {
        return false;
      }
      if (statusFilter === "paused" && target.enabled) {
        return false;
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchName = target.name.toLowerCase().includes(q);
        const matchHost = (target.host || "").toLowerCase().includes(q);
        const matchSaved = (target.savedHostName || "").toLowerCase().includes(q);
        const matchUser = (target.user || "").toLowerCase().includes(q);
        return matchName || matchHost || matchSaved || matchUser;
      }
      return true;
    });
  }, [targets, statusFilter, searchQuery]);

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
        <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
          <Activity className="h-5 w-5" />
        </div>
      }
      error={null}
      actions={
        <>
          <Button
            type="button"
            size="sm"
            onClick={openAdd}
            className="rounded-xl bg-slate-900 text-white shadow-sm hover:bg-slate-800 dark:bg-emerald-600 dark:hover:bg-emerald-700"
          >
            <Plus className="h-3.5 w-3.5" />
            <BiText translationKey="serverMonitorAdd" />
          </Button>
          {selectedTarget ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => void handleCollectNow(selectedTarget)}
              className="rounded-xl border-slate-200 dark:border-slate-800"
            >
              <Activity className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
              <BiText translationKey="serverMonitorCollectNow" />
            </Button>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={openSettings}
            className="rounded-xl border-slate-200 dark:border-slate-800"
          >
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
            className="rounded-xl border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive break-words"
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
        <div data-testid="server-monitor-main" className="w-full max-w-full min-w-0 space-y-4 pr-1 sm:pr-2">
          {/* 顶部统计卡片仪表板 */}
          <div className="grid min-w-0 grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="min-w-0 overflow-hidden rounded-2xl border border-slate-200/80 bg-white p-4 shadow-sm transition hover:shadow-md dark:border-slate-800 dark:bg-slate-900">
              <div className="flex items-center justify-between">
                <span className="truncate text-xs font-medium text-slate-500 dark:text-slate-400">
                  <BiText translationKey="serverMonitorTotalServers" />
                </span>
                <span className="h-2 w-2 shrink-0 rounded-full bg-slate-400 ring-4 ring-slate-400/20" />
              </div>
              <span className="mt-2 block font-mono text-2xl font-bold tracking-tight text-slate-900 dark:text-white truncate">
                {totalCount}
              </span>
            </div>
            <div className="min-w-0 overflow-hidden rounded-2xl border border-slate-200/80 bg-white p-4 shadow-sm transition hover:shadow-md dark:border-slate-800 dark:bg-slate-900">
              <div className="flex items-center justify-between">
                <span className="truncate text-xs font-medium text-emerald-600 dark:text-emerald-400">
                  <BiText translationKey="serverMonitorRunningServers" />
                </span>
                <span className="h-2 w-2 shrink-0 rounded-full bg-emerald-500 ring-4 ring-emerald-500/20" />
              </div>
              <span className="mt-2 block font-mono text-2xl font-bold tracking-tight text-emerald-600 dark:text-emerald-400 truncate">
                {activeCount}
              </span>
            </div>
            <div className="min-w-0 overflow-hidden rounded-2xl border border-slate-200/80 bg-white p-4 shadow-sm transition hover:shadow-md dark:border-slate-800 dark:bg-slate-900">
              <div className="flex items-center justify-between">
                <span className="truncate text-xs font-medium text-rose-600 dark:text-rose-400">
                  <BiText translationKey="serverMonitorErrorServers" />
                </span>
                <span className="h-2 w-2 shrink-0 rounded-full bg-rose-500 ring-4 ring-rose-500/20" />
              </div>
              <span className="mt-2 block font-mono text-2xl font-bold tracking-tight text-rose-600 dark:text-rose-400 truncate">
                {errorCount}
              </span>
            </div>
            <div className="min-w-0 overflow-hidden rounded-2xl border border-slate-200/80 bg-white p-4 shadow-sm transition hover:shadow-md dark:border-slate-800 dark:bg-slate-900">
              <div className="flex items-center justify-between">
                <span className="truncate text-xs font-medium text-slate-500 dark:text-slate-400">
                  <BiText translationKey="serverMonitorPausedServers" />
                </span>
                <span className="h-2 w-2 shrink-0 rounded-full bg-slate-300 ring-4 ring-slate-300/20 dark:bg-slate-600" />
              </div>
              <span className="mt-2 block font-mono text-2xl font-bold tracking-tight text-slate-900 dark:text-white truncate">
                {pausedCount}
              </span>
            </div>
          </div>

          {/* 筛选与搜索工具条 */}
          <div className="flex flex-wrap items-center justify-between gap-3 min-w-0">
            <div className="flex flex-wrap items-center rounded-xl bg-slate-100 p-1 dark:bg-slate-800/80">
              <button
                type="button"
                data-testid="filter-all"
                onClick={() => setStatusFilter("all")}
                className={`rounded-lg px-3 py-1 text-xs transition-all ${
                  statusFilter === "all"
                    ? "bg-white font-semibold text-slate-900 shadow-sm dark:bg-slate-700 dark:text-white"
                    : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
                }`}
              >
                <BiText translationKey="serverMonitorFilterAll" /> ({totalCount})
              </button>
              <button
                type="button"
                data-testid="filter-active"
                onClick={() => setStatusFilter("active")}
                className={`rounded-lg px-3 py-1 text-xs transition-all ${
                  statusFilter === "active"
                    ? "bg-emerald-600 font-semibold text-white shadow-sm"
                    : "text-slate-500 hover:text-emerald-600 dark:text-slate-400 dark:hover:text-emerald-400"
                }`}
              >
                <BiText translationKey="serverMonitorFilterActive" /> ({activeCount})
              </button>
              <button
                type="button"
                data-testid="filter-error"
                onClick={() => setStatusFilter("error")}
                className={`rounded-lg px-3 py-1 text-xs transition-all ${
                  statusFilter === "error"
                    ? "bg-rose-600 font-semibold text-white shadow-sm"
                    : "text-slate-500 hover:text-rose-600 dark:text-slate-400 dark:hover:text-rose-400"
                }`}
              >
                <BiText translationKey="serverMonitorFilterError" /> ({errorCount})
              </button>
              <button
                type="button"
                data-testid="filter-paused"
                onClick={() => setStatusFilter("paused")}
                className={`rounded-lg px-3 py-1 text-xs transition-all ${
                  statusFilter === "paused"
                    ? "bg-slate-600 font-semibold text-white shadow-sm dark:bg-slate-600"
                    : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
                }`}
              >
                <BiText translationKey="serverMonitorFilterPaused" /> ({pausedCount})
              </button>
            </div>

            <div className="relative w-full sm:w-auto sm:min-w-[220px]">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-slate-400" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("serverMonitorSearchPlaceholder")}
                className="w-full rounded-xl border border-slate-200 bg-white py-1.5 pl-8 pr-3 text-xs text-slate-800 placeholder:text-slate-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200"
              />
            </div>
          </div>

          <div className="grid min-w-0 max-w-full grid-cols-1 gap-4 lg:grid-cols-[280px_minmax(0,1fr)]">
            <aside className="min-w-0 space-y-2">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
                <BiText translationKey="serverMonitorListTitle" />
              </h3>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-1">
                {filteredTargets.length > 0 ? (
                  filteredTargets.map((target) => {
                    const active = selectedTarget?.id === target.id;
                    const listKey = target.blocked
                      ? "serverMonitorStatusError"
                      : !target.enabled
                        ? "serverMonitorStatusPaused"
                        : target.collecting
                          ? "serverMonitorStatusCollecting"
                          : "serverMonitorStatusActive";
                    const tone: PillTone = target.blocked
                      ? "error"
                      : !target.enabled
                        ? "neutral"
                        : target.collecting
                          ? "warning"
                          : "success";

                    return (
                      <button
                        key={target.id}
                        type="button"
                        aria-pressed={active}
                        onClick={() => setSelectedId(target.id)}
                        className={`w-full min-w-0 rounded-2xl border p-3.5 text-left transition-all ${
                          active
                            ? "border-emerald-500/60 bg-emerald-50/50 shadow-sm ring-1 ring-emerald-500/20 dark:border-emerald-500/50 dark:bg-emerald-950/20"
                            : "border-slate-200/80 bg-white hover:border-slate-300 hover:bg-slate-50/80 dark:border-slate-800 dark:bg-slate-900 dark:hover:border-slate-700"
                        }`}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="block truncate text-sm font-semibold text-slate-900 dark:text-white">
                            {target.name}
                          </span>
                          <span
                            className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${STATUS_RING_CLASSES[tone]}`}
                          />
                        </div>
                        <span className="mt-1 block truncate font-mono text-[11px] text-slate-500 dark:text-slate-400">
                          {target.host ?? target.savedHostName ?? target.id}
                          {target.user ? ` · ${target.user}` : ""}
                        </span>
                        <div className="mt-2 flex items-center justify-between text-[11px]">
                          <span className="font-medium text-slate-500 dark:text-slate-400">
                            <BiText translationKey={listKey} />
                          </span>
                          {target.savedHostName ? (
                            <span className="rounded-md bg-slate-100 px-1.5 py-0.5 text-[10px] font-mono text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                              SSH
                            </span>
                          ) : null}
                        </div>
                      </button>
                    );
                  })
                ) : (
                  <div className="rounded-2xl border border-dashed border-slate-200 p-6 text-center text-xs text-slate-400 dark:border-slate-800">
                    未找到匹配的服务器
                  </div>
                )}
              </div>
            </aside>

            <section className="min-w-0 max-w-full space-y-4">
              {selectedTarget ? (
                <>
                  <div className="relative overflow-hidden rounded-2xl border border-slate-200/80 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2.5">
                          {status ? (
                            <span
                              className={`flex h-3 w-3 shrink-0 rounded-full ${STATUS_RING_CLASSES[status.tone]}`}
                            />
                          ) : null}
                          <p className="truncate font-mono text-base sm:text-lg font-bold tracking-tight text-slate-900 dark:text-white">
                            {selectedTarget.host ??
                              selectedTarget.savedHostName ??
                              selectedTarget.id}
                            {selectedTarget.user ? ` · ${selectedTarget.user}` : ""}
                            {selectedTarget.port ? ` · ${selectedTarget.port}` : ""}
                          </p>
                          {status ? (
                            <StatusPill translationKey={status.key} tone={status.tone} />
                          ) : null}
                          {partial ? (
                            <StatusPill translationKey="serverMonitorPartial" tone="warning" />
                          ) : null}
                        </div>
                        <p className="mt-2 text-xs text-slate-500 dark:text-slate-400">
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
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => openEdit(selectedTarget)}
                          className="rounded-xl border-slate-200 dark:border-slate-800"
                        >
                          <Pencil className="h-3.5 w-3.5" />
                          <BiText translationKey="serverMonitorEdit" />
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => void handleToggleEnabled(selectedTarget)}
                          className="rounded-xl border-slate-200 dark:border-slate-800"
                        >
                          <BiText
                            translationKey={
                              selectedTarget.enabled
                                ? "serverMonitorPause"
                                : "serverMonitorResume"
                            }
                          />
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => void handleDelete(selectedTarget)}
                          className="rounded-xl border-rose-200 bg-rose-50/50 text-rose-600 hover:bg-rose-100 hover:text-rose-700 dark:border-rose-900/50 dark:bg-rose-950/20 dark:text-rose-400 dark:hover:bg-rose-950/40"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          <BiText translationKey="serverMonitorDelete" />
                        </Button>
                      </div>
                    </div>
                  </div>

                  <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                    <div className="rounded-2xl border border-slate-200/80 bg-white p-3.5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
                      <p className="text-[11px] font-medium uppercase tracking-wider text-slate-500 dark:text-slate-400">
                        <BiText translationKey="serverMonitorNextCollection" />
                      </p>
                      <p className="mt-1 font-mono text-sm font-semibold text-slate-900 dark:text-slate-100">
                        {nextCollectionAt ? (
                          formatMonitorTimestamp(nextCollectionAt)
                        ) : (
                          <BiText translationKey="serverMonitorNever" />
                        )}
                      </p>
                    </div>
                    <div className="rounded-2xl border border-slate-200/80 bg-white p-3.5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
                      <p className="text-[11px] font-medium uppercase tracking-wider text-slate-500 dark:text-slate-400">
                        <BiText translationKey="serverMonitorLastSuccess" />
                      </p>
                      <p className="mt-1 font-mono text-sm font-semibold text-slate-900 dark:text-slate-100">
                        {lastSuccessAt ? (
                          formatMonitorTimestamp(lastSuccessAt)
                        ) : (
                          <BiText translationKey="serverMonitorNever" />
                        )}
                      </p>
                    </div>
                    <div className="rounded-2xl border border-slate-200/80 bg-white p-3.5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
                      <p className="text-[11px] font-medium uppercase tracking-wider text-slate-500 dark:text-slate-400">
                        <BiText translationKey="serverMonitorLastGood" />
                      </p>
                      <p className="mt-1 font-mono text-sm font-semibold text-slate-900 dark:text-slate-100">
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
                      className="rounded-2xl border border-rose-200 bg-rose-50/70 p-3.5 text-xs text-rose-800 dark:border-rose-900/50 dark:bg-rose-950/30 dark:text-rose-300 break-words"
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

                  <div className="space-y-3 rounded-2xl border border-slate-200/80 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <h3 className="text-sm font-semibold text-slate-900 dark:text-white">
                        <BiText translationKey="serverMonitorHistory" />
                      </h3>
                      <div className="flex items-center rounded-xl bg-slate-100 p-0.5 dark:bg-slate-800">
                        <button
                          type="button"
                          onClick={() => setRange("twentyFourHours")}
                          className={`rounded-lg px-2.5 py-1 text-xs font-medium transition ${
                            range === "twentyFourHours"
                              ? "bg-white font-semibold text-slate-900 shadow-sm dark:bg-slate-700 dark:text-white"
                              : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
                          }`}
                        >
                          <BiText translationKey="serverMonitorRange24h" />
                        </button>
                        <button
                          type="button"
                          onClick={() => setRange("sevenDays")}
                          className={`rounded-lg px-2.5 py-1 text-xs font-medium transition ${
                            range === "sevenDays"
                              ? "bg-white font-semibold text-slate-900 shadow-sm dark:bg-slate-700 dark:text-white"
                              : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
                          }`}
                        >
                          <BiText translationKey="serverMonitorRange7d" />
                        </button>
                      </div>
                    </div>
                    <HistoryChart
                      samples={samples}
                      range={range}
                      expectedIntervalSeconds={latestSample?.expectedIntervalSeconds ?? null}
                    />
                  </div>
                </>
              ) : (
                <ToolEmptyState title={t("serverMonitorNoSelection")} />
              )}
            </section>
          </div>
        </div>
      ) : null}

      <div className="rounded-xl border border-slate-200/70 bg-slate-50/50 p-2.5 text-xs dark:border-slate-800/70 dark:bg-slate-900/30">
        <button
          type="button"
          onClick={() => setGuidanceOpen(!guidanceOpen)}
          className="group flex w-full items-center justify-between text-left"
        >
          <div className="flex items-center gap-2">
            <Info className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
            <h3 className="text-xs font-semibold text-slate-700 dark:text-slate-300">
              <BiText translationKey="serverMonitorGuidanceTitle" />
            </h3>
            <span className="hidden text-[11px] text-slate-400 sm:inline dark:text-slate-500">
              (只读采集 · 无需 sudo / agent)
            </span>
          </div>
          <span className="flex items-center gap-1 text-[11px] font-medium text-slate-400 transition group-hover:text-slate-600 dark:text-slate-500 dark:group-hover:text-slate-300">
            <BiText
              translationKey={
                guidanceOpen
                  ? "serverMonitorGuidanceHide"
                  : "serverMonitorGuidanceShow"
              }
            />
            {guidanceOpen ? (
              <ChevronUp className="h-3 w-3" />
            ) : (
              <ChevronDown className="h-3 w-3" />
            )}
          </span>
        </button>

        <div className={`mt-2 border-t border-slate-200/60 pt-2 dark:border-slate-800/60 ${guidanceOpen ? "block" : "hidden"}`}>
          <ul className="list-disc space-y-1 pl-5 text-[11px] leading-relaxed text-slate-500 dark:text-slate-400">
            <li>
              <BiText translationKey="serverMonitorGuidanceCommands" />
            </li>
            <li>
              <BiText translationKey="serverMonitorGuidanceAccess" />
            </li>
            <li>
              <BiText translationKey="serverMonitorGuidanceNoAgent" />
            </li>
            <li>
              <BiText translationKey="serverMonitorGuidanceLifetime" />
            </li>
            <li>
              <BiText translationKey="serverMonitorGuidanceTime" />
            </li>
            <li>
              <BiText translationKey="serverMonitorGuidanceSampling" />
            </li>
            <li>
              <BiText translationKey="serverMonitorGuidanceHost" />
            </li>
          </ul>
        </div>
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
