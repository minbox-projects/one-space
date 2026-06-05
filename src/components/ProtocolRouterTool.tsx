import { useCallback, useEffect, useEffectEvent, useMemo, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  CheckCircle2,
  Clock3,
  Copy,
  PlugZap,
  Route as RouteIcon,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { errorToMessage } from "@/lib/messages";
import {
  protocolRouterGetConfig,
  protocolRouterSaveConfig,
  protocolRouterStats,
  protocolRouterStatus,
  protocolRouterTestConnection,
  type ProtocolRoute,
  type ProtocolRouterCallRecord,
  type ProtocolRouterConfig,
  type ProtocolRouterStatsSummary,
  type ProtocolRouterStatus,
} from "@/lib/protocolRouter";

const DEFAULT_CONFIG: ProtocolRouterConfig = {
  enabled: false,
  port: 17687,
  token: "",
  retention_days: 30,
  routes: [],
};

type MessageState = { type: "success" | "error" | ""; text: string };
type RouteConnectionStatus = "connected" | "flaky" | "failed" | "inactive";

interface ProtocolRouterRouteViewModel {
  route: ProtocolRoute;
  status: RouteConnectionStatus;
  lastLatencyMs: number | null;
  totalTokens: number;
  callCount: number;
  lastCalledAt: number | null;
  errorSummary: string | null;
}

interface TrendBucket {
  label: string;
  calls: number;
  tokens: number;
}

function normalizeConfig(config?: ProtocolRouterConfig): ProtocolRouterConfig {
  const merged = { ...DEFAULT_CONFIG, ...(config || {}) };
  return {
    ...merged,
    port: Number(merged.port) || DEFAULT_CONFIG.port,
    retention_days: Math.min(365, Math.max(1, Number(merged.retention_days) || 30)),
    routes: merged.routes || [],
  };
}

function routeUrl(port: number, route: ProtocolRoute) {
  return `http://127.0.0.1:${port}/anthropic/${route.id}/v1`;
}

function isFailedCall(call: ProtocolRouterCallRecord) {
  return call.status >= 400 || !!call.error_summary;
}

function safelyNumber(value: number | null | undefined) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function summarizeRouteCalls(calls: ProtocolRouterCallRecord[]): Omit<ProtocolRouterRouteViewModel, "route"> {
  const sorted = [...calls].sort((a, b) => b.ts - a.ts);
  const lastCall = sorted[0];
  const totalTokens = sorted.reduce((sum, call) => sum + call.total_tokens, 0);
  const recentTen = sorted.slice(0, 10);
  const failedRecent = recentTen.filter(isFailedCall).length;
  const failedRate = recentTen.length > 0 ? failedRecent / recentTen.length : 0;

  let status: RouteConnectionStatus = "inactive";
  if (!lastCall) {
    status = "inactive";
  } else if (isFailedCall(lastCall)) {
    status = "failed";
  } else if (failedRate > 0.2) {
    status = "flaky";
  } else {
    status = "connected";
  }

  return {
    status,
    lastLatencyMs: safelyNumber(lastCall?.latency_ms),
    totalTokens,
    callCount: sorted.length,
    lastCalledAt: lastCall?.ts ?? null,
    errorSummary: lastCall?.error_summary || null,
  };
}

function buildTrendBuckets(calls: ProtocolRouterCallRecord[], days: number): TrendBucket[] {
  if (days <= 1) {
    const now = new Date();
    const currentHour = now.getHours();
    const startHour = Math.floor(currentHour / 3) * 3;
    return Array.from({ length: 8 }, (_, index) => {
      const bucketStartHour = (startHour - (7 - index) * 3 + 24) % 24;
      const matching = calls.filter((call) => {
        const date = new Date(call.ts * 1000);
        return date.getDate() === now.getDate() && Math.floor(date.getHours() / 3) * 3 === bucketStartHour;
      });
      return {
        label: `${String(bucketStartHour).padStart(2, "0")}:00`,
        calls: matching.length,
        tokens: matching.reduce((sum, call) => sum + call.total_tokens, 0),
      };
    });
  }

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  return Array.from({ length: days }, (_, index) => {
    const day = new Date(today);
    day.setDate(today.getDate() - (days - 1 - index));
    const start = day.getTime();
    const end = start + 24 * 60 * 60 * 1000;
    const matching = calls.filter((call) => {
      const ts = call.ts * 1000;
      return ts >= start && ts < end;
    });
    return {
      label: `${day.getMonth() + 1}/${day.getDate()}`,
      calls: matching.length,
      tokens: matching.reduce((sum, call) => sum + call.total_tokens, 0),
    };
  });
}

function formatCompactNumber(value: number) {
  return new Intl.NumberFormat(undefined, {
    notation: value >= 1000 ? "compact" : "standard",
    maximumFractionDigits: value >= 1000 ? 1 : 0,
  }).format(value);
}

function TrendChart({
  buckets,
  t,
}: {
  buckets: TrendBucket[];
  t: (key: string, fallback: string) => string;
}) {
  const width = 640;
  const height = 220;
  const paddingX = 18;
  const paddingTop = 18;
  const paddingBottom = 28;
  const innerHeight = height - paddingTop - paddingBottom;
  const innerWidth = width - paddingX * 2;
  const maxTokens = Math.max(...buckets.map((bucket) => bucket.tokens), 1);
  const stepX = buckets.length > 1 ? innerWidth / (buckets.length - 1) : innerWidth;

  const points = buckets.map((bucket, index) => {
    const x = paddingX + stepX * index;
    const y = paddingTop + innerHeight - (bucket.tokens / maxTokens) * innerHeight;
    return { x, y, bucket };
  });

  const linePath = points
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ");
  const areaPath = `${linePath} L ${paddingX + innerWidth} ${height - paddingBottom} L ${paddingX} ${height - paddingBottom} Z`;

  return (
    <div className="rounded-[24px] border bg-card p-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="text-sm font-medium text-foreground">{t("protocolRouterTrendTitle", "Traffic trend")}</div>
          <div className="text-xs text-muted-foreground">{t("protocolRouterTrendDesc", "Daily token usage within the selected time window.")}</div>
        </div>
        <div className="rounded-full border bg-muted/40 px-3 py-1 text-xs text-muted-foreground">
          {t("tokens", "Tokens")}
        </div>
      </div>

      <div className="mt-5">
        <svg viewBox={`0 0 ${width} ${height}`} className="h-56 w-full">
          {[0, 0.5, 1].map((ratio) => {
            const y = paddingTop + innerHeight - innerHeight * ratio;
            return (
              <line
                key={ratio}
                x1={paddingX}
                x2={paddingX + innerWidth}
                y1={y}
                y2={y}
                className="stroke-border"
                strokeDasharray="4 6"
                strokeWidth="1"
              />
            );
          })}
          <path d={areaPath} fill="hsl(var(--chart-1) / 0.14)" />
          <path d={linePath} fill="none" stroke="hsl(var(--chart-1))" strokeWidth="3" strokeLinejoin="round" strokeLinecap="round" />
          {points.map((point) => (
            <g key={point.bucket.label}>
              <circle cx={point.x} cy={point.y} r="4" fill="hsl(var(--chart-1))" />
              <text x={point.x} y={height - 8} textAnchor="middle" className="fill-muted-foreground text-[10px]">
                {point.bucket.label}
              </text>
            </g>
          ))}
        </svg>
      </div>

      <div className="mt-3 grid gap-3 sm:grid-cols-3">
        {buckets.slice(-3).map((bucket) => (
          <div key={bucket.label} className="rounded-2xl border bg-muted/20 px-4 py-3">
            <div className="text-xs text-muted-foreground">{bucket.label}</div>
            <div className="mt-1 text-sm font-semibold">{formatCompactNumber(bucket.tokens)} {t("tokens", "tokens")}</div>
            <div className="text-xs text-muted-foreground">{bucket.calls} {t("calls", "calls")}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

function routeStatusPresentation(t: (key: string, fallback: string) => string, status: RouteConnectionStatus) {
  switch (status) {
    case "connected":
      return {
        label: t("protocolRouterRouteStatusConnected", "Connected"),
        dotClass: "bg-emerald-500",
        textClass: "text-emerald-700",
      };
    case "flaky":
      return {
        label: t("protocolRouterRouteStatusFlaky", "Flaky"),
        dotClass: "bg-amber-500",
        textClass: "text-amber-700",
      };
    case "failed":
      return {
        label: t("protocolRouterRouteStatusFailed", "Failed"),
        dotClass: "bg-rose-500",
        textClass: "text-rose-700",
      };
    default:
      return {
        label: t("protocolRouterRouteStatusInactive", "Inactive"),
        dotClass: "bg-zinc-300",
        textClass: "text-muted-foreground",
      };
  }
}

async function safelyUnlisten(unlisten: () => void | Promise<void>) {
  try {
    await unlisten();
  } catch (err) {
    console.warn("Failed to unlisten protocol-router-status-update", err);
  }
}

export function ProtocolRouterTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<ProtocolRouterConfig>(DEFAULT_CONFIG);
  const [status, setStatus] = useState<ProtocolRouterStatus | null>(null);
  const [stats, setStats] = useState<ProtocolRouterStatsSummary | null>(null);
  const [statsDays, setStatsDays] = useState(7);
  const [selectedRouteId, setSelectedRouteId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [testingRouteId, setTestingRouteId] = useState<string | null>(null);
  const [message, setMessage] = useState<MessageState>({ type: "", text: "" });

  const isTauri = "__TAURI_INTERNALS__" in window;

  const load = useCallback(async () => {
    if (!isTauri) return;
    try {
      const [nextConfig, nextStatus, nextStats] = await Promise.all([
        protocolRouterGetConfig(),
        protocolRouterStatus(),
        protocolRouterStats(statsDays),
      ]);
      setConfig(normalizeConfig(nextConfig));
      setStatus(nextStatus);
      setStats(nextStats);
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    }
  }, [isTauri, statsDays]);

  useEffect(() => {
    if (!isVisible) return;
    void load();
  }, [isVisible, load]);

  useEffect(() => {
    if (!selectedRouteId) return;
    if (!config.routes.some((route) => route.id === selectedRouteId)) {
      setSelectedRouteId(null);
    }
  }, [config.routes, selectedRouteId]);

  const handleRouterRefresh = useEffectEvent(() => {
    if (!isVisible) return;
    void load();
  });

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let statusTeardown: (() => void | Promise<void>) | null = null;
    let countsTeardown: (() => void | Promise<void>) | null = null;

    listen("protocol-router-status-update", () => {
      if (!disposed) {
        handleRouterRefresh();
      }
    })
      .then((unlisten) => {
        if (disposed) {
          void safelyUnlisten(unlisten);
          return;
        }
        statusTeardown = unlisten;
      })
      .catch((err) => {
        console.error("Failed to subscribe to protocol-router-status-update", err);
      });

    listen("refresh-counts", () => {
      if (!disposed) {
        handleRouterRefresh();
      }
    })
      .then((unlisten) => {
        if (disposed) {
          void safelyUnlisten(unlisten);
          return;
        }
        countsTeardown = unlisten;
      })
      .catch((err) => {
        console.error("Failed to subscribe to refresh-counts for protocol router", err);
      });

    return () => {
      disposed = true;
      if (statusTeardown) {
        void safelyUnlisten(statusTeardown);
      }
      if (countsTeardown) {
        void safelyUnlisten(countsTeardown);
      }
    };
  }, [handleRouterRefresh, isTauri]);

  const routeViewModels = useMemo<ProtocolRouterRouteViewModel[]>(() => {
    const calls = stats?.calls || [];
    return config.routes.map((route) => ({
      route,
      ...summarizeRouteCalls(calls.filter((call) => call.route_id === route.id)),
    }));
  }, [config.routes, stats?.calls]);

  const totalErrors = useMemo(
    () => (stats?.calls || []).filter(isFailedCall).length,
    [stats?.calls],
  );
  const successRate = useMemo(() => {
    const totalCalls = stats?.calls.length || 0;
    if (totalCalls === 0) return null;
    return Math.round(((totalCalls - totalErrors) / totalCalls) * 100);
  }, [stats?.calls.length, totalErrors]);

  const selectedRoute = useMemo(
    () => routeViewModels.find((item) => item.route.id === selectedRouteId) || null,
    [routeViewModels, selectedRouteId],
  );

  const filteredCalls = useMemo(() => {
    const allCalls = [...(stats?.calls || [])].sort((a, b) => b.ts - a.ts);
    return selectedRouteId ? allCalls.filter((call) => call.route_id === selectedRouteId) : allCalls;
  }, [selectedRouteId, stats?.calls]);

  const trendBuckets = useMemo(
    () => buildTrendBuckets(filteredCalls, statsDays),
    [filteredCalls, statsDays],
  );

  const toggleRunning = async (enabled: boolean) => {
    if (!isTauri) return;
    setBusy(true);
    setMessage({ type: "", text: "" });
    try {
      const latest = normalizeConfig(await protocolRouterGetConfig());
      const saved = normalizeConfig(await protocolRouterSaveConfig({ ...latest, enabled }));
      const [nextStatus, nextStats] = await Promise.all([
        protocolRouterStatus(),
        protocolRouterStats(statsDays),
      ]);
      setConfig(saved);
      setStatus(nextStatus);
      setStats(nextStats);
      await emit("protocol-router-status-update").catch(() => {});
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
      await load();
    } finally {
      setBusy(false);
    }
  };

  const testRoute = async (route: ProtocolRoute) => {
    setTestingRouteId(route.id);
    setMessage({ type: "", text: "" });
    try {
      const result = await protocolRouterTestConnection({ route_id: route.id, model: route.default_model || null });
      setMessage({
        type: "success",
        text: t("protocolRouterRouteTestResult", {
          name: route.name,
          status: result.status,
          latency: result.latency_ms,
          tokens: result.total_tokens,
          defaultValue: `${route.name}: HTTP ${result.status}, ${result.latency_ms}ms, ${result.total_tokens} tokens`,
        }),
      });
      setStats(await protocolRouterStats(statsDays));
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    } finally {
      setTestingRouteId(null);
    }
  };

  const copyEndpoint = async (route: ProtocolRoute) => {
    try {
      await navigator.clipboard.writeText(routeUrl(config.port, route));
      setMessage({ type: "success", text: t("copiedToClipboard", "Copied to clipboard") });
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    }
  };

  const activeRoutes = routeViewModels.filter((item) => item.callCount > 0).length;
  const selectedViewLabel = selectedRoute?.route.name || t("protocolRouterAllRoutes", "All routes");

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-7xl space-y-6 p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="space-y-1">
            <h1 className="text-2xl font-semibold tracking-tight">{t("protocolRouter", "Protocol Router")}</h1>
            <p className="max-w-3xl text-sm text-muted-foreground">
              {t(
                "protocolRouterWorkspaceDesc",
                "Monitor the local protocol router runtime, inspect derived routes, and drill into recent traffic without editing configuration here.",
              )}
            </p>
          </div>
          <div className="flex items-center gap-3 rounded-full border bg-card px-4 py-3">
            <div className="text-right">
              <div className="text-sm font-medium">{t("protocolRouterRuntimeSwitch", "Router runtime")}</div>
              <div className="text-xs text-muted-foreground">
                {status?.running
                  ? t("launcherProtocolRouterRunningAria", {
                      port: status.port,
                      routes: status.route_count,
                      defaultValue: `Running on port ${status.port} with ${status.route_count} route(s)`,
                    })
                  : t("protocolRouterRuntimeStoppedHint", "Persisted runtime switch also controls auto-start.")}
              </div>
            </div>
            <Switch checked={!!config.enabled} onCheckedChange={(checked) => void toggleRunning(checked)} disabled={busy} />
          </div>
        </div>

        {message.text ? (
          <div
            className={`rounded-2xl border px-4 py-3 text-sm ${
              message.type === "error"
                ? "border-destructive/30 bg-destructive/5 text-destructive"
                : "border-emerald-500/30 bg-emerald-500/5 text-emerald-700"
            }`}
          >
            {message.text}
          </div>
        ) : null}

        <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          <div className="rounded-[24px] border bg-card p-5">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("status", "Status")}</div>
            <div className="mt-3 text-xl font-semibold">{status?.running ? t("running", "Running") : t("stopped", "Stopped")}</div>
            <div className="mt-1 text-sm text-muted-foreground">{t("protocolRouterPortValue", { port: config.port, defaultValue: `Port ${config.port}` })}</div>
          </div>
          <div className="rounded-[24px] border bg-card p-5">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("protocolRouterActiveRoutes", "Active Routes")}</div>
            <div className="mt-3 text-xl font-semibold">{activeRoutes}</div>
            <div className="mt-1 text-sm text-muted-foreground">
              {t("protocolRouterConfiguredRoutes", {
                count: config.routes.length,
                defaultValue: `${config.routes.length} configured`,
              })}
            </div>
          </div>
          <div className="rounded-[24px] border bg-card p-5">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("protocolRouterPeriodTokens", "Period Tokens")}</div>
            <div className="mt-3 text-xl font-semibold">{formatCompactNumber(stats?.total_tokens || 0)}</div>
            <div className="mt-1 text-sm text-muted-foreground">
              {(stats?.total_calls || 0).toLocaleString()} {t("calls", "calls")}
            </div>
          </div>
          <div className="rounded-[24px] border bg-card p-5">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("protocolRouterSuccessAndErrors", "Success / Errors")}</div>
            <div className="mt-3 text-xl font-semibold">
              {successRate === null ? "--" : `${successRate}%`}
            </div>
            <div className="mt-1 text-sm text-muted-foreground">
              {t("protocolRouterErrorCount", {
                count: totalErrors,
                defaultValue: `${totalErrors} exception(s)`,
              })}
            </div>
          </div>
        </section>

        <section className="grid gap-6 xl:grid-cols-[minmax(0,1.4fr)_360px]">
          <div className="space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2 className="text-base font-semibold">{t("protocolRouterTrendSection", "Trend and traffic")}</h2>
                <p className="text-sm text-muted-foreground">
                  {t("protocolRouterSelectedView", {
                    name: selectedViewLabel,
                    defaultValue: `Current view: ${selectedViewLabel}`,
                  })}
                </p>
              </div>
              <div className="inline-flex rounded-full border bg-card p-1">
                {[1, 7, 30].map((days) => (
                  <button
                    key={days}
                    type="button"
                    onClick={() => setStatsDays(days)}
                    className={`rounded-full px-3 py-1.5 text-sm transition ${
                      statsDays === days ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted"
                    }`}
                  >
                    {days === 1 ? t("today", "Today") : `${days} ${t("days", "days")}`}
                  </button>
                ))}
              </div>
            </div>
            <TrendChart buckets={trendBuckets} t={(key, fallback) => t(key, fallback)} />
          </div>

          <div className="rounded-[24px] border bg-card p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-base font-semibold">{t("protocolRouterRecentRequests", "Recent requests")}</h2>
                <p className="text-sm text-muted-foreground">
                  {t("protocolRouterRecentRequestsDesc", "Latest requests for the current chart filter.")}
                </p>
              </div>
              <div className="rounded-full border bg-muted/40 px-3 py-1 text-xs text-muted-foreground">
                {selectedViewLabel}
              </div>
            </div>

            <div className="mt-4 space-y-3">
              {filteredCalls.slice(0, 8).map((call) => {
                const failed = isFailedCall(call);
                return (
                  <div key={`${call.ts}-${call.route_id}-${call.latency_ms}`} className="rounded-2xl border bg-muted/10 px-4 py-3">
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-medium">{call.model || call.route_id}</div>
                        <div className="truncate text-xs text-muted-foreground">{call.endpoint}</div>
                      </div>
                      <div className={`rounded-full px-2 py-1 text-xs ${failed ? "bg-rose-500/10 text-rose-700" : "bg-emerald-500/10 text-emerald-700"}`}>
                        HTTP {call.status}
                      </div>
                    </div>
                    <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground">
                      <span>{new Date(call.ts * 1000).toLocaleString()}</span>
                      <span>{call.total_tokens.toLocaleString()} {t("tokens", "tokens")}</span>
                      <span>{call.latency_ms}ms</span>
                    </div>
                    {call.error_summary ? (
                      <div className="mt-2 text-xs text-rose-700">{call.error_summary}</div>
                    ) : null}
                  </div>
                );
              })}
              {filteredCalls.length === 0 ? (
                <div className="rounded-2xl border border-dashed px-4 py-6 text-sm text-muted-foreground">
                  {t("protocolRouterNoRequestsForView", "No requests in the selected view yet.")}
                </div>
              ) : null}
            </div>
          </div>
        </section>

        <section className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 className="text-base font-semibold">{t("protocolRouterRoutes", "Derived Routes")}</h2>
              <p className="text-sm text-muted-foreground">
                {t("protocolRouterRoutesDesc", "Select a route card to filter the chart and request feed.")}
              </p>
            </div>
            <button
              type="button"
              onClick={() => setSelectedRouteId(null)}
              className={`rounded-full border px-3 py-1.5 text-sm transition ${
                selectedRouteId === null ? "bg-primary text-primary-foreground" : "bg-card text-muted-foreground hover:bg-muted"
              }`}
            >
              {t("protocolRouterAllRoutes", "All routes")}
            </button>
          </div>

          {routeViewModels.length === 0 ? (
            <div className="rounded-[24px] border border-dashed p-6 text-sm text-muted-foreground">
              {t("protocolRouterNoRoutes", "No Claude service providers are using the protocol router. Enable it in a Claude service provider and choose an OpenAI-compatible API format.")}
            </div>
          ) : null}

          <div className="grid gap-4 lg:grid-cols-2">
            {routeViewModels.map((item) => {
              const presentation = routeStatusPresentation((key, fallback) => t(key, fallback), item.status);
              const selected = selectedRouteId === item.route.id;
              return (
                <div
                  key={item.route.id}
                  role="button"
                  tabIndex={0}
                  onClick={() => setSelectedRouteId(selected ? null : item.route.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelectedRouteId(selected ? null : item.route.id);
                    }
                  }}
                  className={`rounded-[24px] border p-5 transition ${
                    selected ? "border-primary bg-muted/20 shadow-sm" : "bg-card hover:bg-muted/10"
                  }`}
                >
                  <div className="flex items-start justify-between gap-4">
                    <div className="min-w-0 space-y-2">
                      <div className="flex items-center gap-2">
                        <RouteIcon className="h-4 w-4 text-muted-foreground" />
                        <h3 className="truncate text-base font-semibold">{item.route.name}</h3>
                      </div>
                      <div className="text-sm text-muted-foreground">{item.route.claude_provider_name}</div>
                    </div>
                    <div className={`inline-flex items-center gap-2 rounded-full border px-2.5 py-1 text-xs ${presentation.textClass}`}>
                      <span className={`h-2 w-2 rounded-full ${presentation.dotClass}`} />
                      {presentation.label}
                    </div>
                  </div>

                  <div className="mt-4 grid gap-3 md:grid-cols-2">
                    <div className="rounded-2xl border bg-muted/10 px-4 py-3">
                      <div className="text-xs text-muted-foreground">{t("wireApi", "Wire API Format")}</div>
                      <div className="mt-1 text-sm font-medium">
                        {item.route.wire_api === "open_ai_responses"
                          ? t("protocolRouterWireApiOpenAiResponses", "OpenAI Responses")
                          : t("protocolRouterWireApiOpenAiChat", "OpenAI Chat")}
                      </div>
                    </div>
                    <div className="rounded-2xl border bg-muted/10 px-4 py-3">
                      <div className="text-xs text-muted-foreground">{t("endpoint", "Endpoint")}</div>
                      <div className="mt-1 truncate font-mono text-xs text-foreground">{routeUrl(config.port, item.route)}</div>
                    </div>
                  </div>

                  <div className="mt-4 grid gap-3 sm:grid-cols-3">
                    <div>
                      <div className="text-xs text-muted-foreground">{t("protocolRouterLastLatency", "Last latency")}</div>
                      <div className="mt-1 text-sm font-medium">{item.lastLatencyMs === null ? "--" : `${item.lastLatencyMs}ms`}</div>
                    </div>
                    <div>
                      <div className="text-xs text-muted-foreground">{t("tokens", "Tokens")}</div>
                      <div className="mt-1 text-sm font-medium">{item.totalTokens.toLocaleString()}</div>
                    </div>
                    <div>
                      <div className="text-xs text-muted-foreground">{t("calls", "Calls")}</div>
                      <div className="mt-1 text-sm font-medium">{item.callCount.toLocaleString()}</div>
                    </div>
                  </div>

                  <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t pt-4">
                    <div className="flex min-w-0 items-center gap-3 text-xs text-muted-foreground">
                      <span className="inline-flex items-center gap-1.5">
                        <Clock3 className="h-3.5 w-3.5" />
                        {item.lastCalledAt
                          ? new Date(item.lastCalledAt * 1000).toLocaleString()
                          : t("protocolRouterNeverCalled", "No recent traffic")}
                      </span>
                      {item.errorSummary ? (
                        <span className="inline-flex items-center gap-1.5 text-rose-700">
                          <AlertCircle className="h-3.5 w-3.5" />
                          <span className="truncate">{item.errorSummary}</span>
                        </span>
                      ) : item.callCount > 0 ? (
                        <span className="inline-flex items-center gap-1.5 text-emerald-700">
                          <CheckCircle2 className="h-3.5 w-3.5" />
                          {t("protocolRouterHealthyRoute", "Recent requests healthy")}
                        </span>
                      ) : null}
                    </div>
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          void copyEndpoint(item.route);
                        }}
                        className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted"
                      >
                        <Copy className="h-4 w-4" />
                        {t("copy", "Copy")}
                      </button>
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          void testRoute(item.route);
                        }}
                        disabled={testingRouteId === item.route.id || !item.route.enabled}
                        className="inline-flex items-center gap-2 rounded-xl bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                      >
                        <PlugZap className={`h-4 w-4 ${testingRouteId === item.route.id ? "animate-pulse" : ""}`} />
                        {t("testConnection", "Test Connection")}
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      </div>
    </div>
  );
}
