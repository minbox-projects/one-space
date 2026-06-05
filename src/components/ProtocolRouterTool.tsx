import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Key, PlugZap, RefreshCw, Route as RouteIcon } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { errorToMessage } from "@/lib/messages";
import {
  protocolRouterGetConfig,
  protocolRouterRotateToken,
  protocolRouterSaveConfig,
  protocolRouterStats,
  protocolRouterStatus,
  protocolRouterTestConnection,
  type ProtocolRoute,
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

export function ProtocolRouterTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<ProtocolRouterConfig>(DEFAULT_CONFIG);
  const [savedConfig, setSavedConfig] = useState<ProtocolRouterConfig>(DEFAULT_CONFIG);
  const [status, setStatus] = useState<ProtocolRouterStatus | null>(null);
  const [stats, setStats] = useState<ProtocolRouterStatsSummary | null>(null);
  const [statsDays, setStatsDays] = useState(7);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ type: "success" | "error" | ""; text: string }>({ type: "", text: "" });
  const [testResult, setTestResult] = useState("");

  const isTauri = "__TAURI_INTERNALS__" in window;
  const dirty = useMemo(
    () => JSON.stringify({ ...config, routes: [] }) !== JSON.stringify({ ...savedConfig, routes: [] }),
    [config, savedConfig],
  );

  const load = useCallback(async () => {
    if (!isTauri) return;
    try {
      const [nextConfig, nextStatus, nextStats] = await Promise.all([
        protocolRouterGetConfig(),
        protocolRouterStatus(),
        protocolRouterStats(statsDays),
      ]);
      const normalized = normalizeConfig(nextConfig);
      setConfig(normalized);
      setSavedConfig(normalized);
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
    if (!isTauri) return;
    let disposed = false;
    let teardown: (() => void) | null = null;
    listen("protocol-router-status-update", () => {
      if (!disposed) void load();
    }).then((unlisten) => {
      if (disposed) void unlisten();
      else teardown = unlisten;
    }).catch(() => {});
    return () => {
      disposed = true;
      if (teardown) void teardown();
    };
  }, [isTauri, load]);

  const save = async () => {
    setBusy(true);
    setMessage({ type: "", text: "" });
    try {
      const saved = await protocolRouterSaveConfig(config);
      const normalized = normalizeConfig(saved);
      setConfig(normalized);
      setSavedConfig(normalized);
      setStatus(await protocolRouterStatus());
      setMessage({ type: "success", text: t("currentSectionSavedSuccess", "Current section saved.") });
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const rotateToken = async () => {
    setBusy(true);
    try {
      const next = normalizeConfig(await protocolRouterRotateToken());
      setConfig(next);
      setSavedConfig(next);
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const testRoute = async (route: ProtocolRoute) => {
    setBusy(true);
    setTestResult("");
    try {
      const result = await protocolRouterTestConnection({ route_id: route.id, model: route.default_model || null });
      setTestResult(`${route.name}: HTTP ${result.status}, ${result.latency_ms}ms, ${result.total_tokens} ${t("tokens", "tokens")}`);
      setStats(await protocolRouterStats(statsDays));
    } catch (err) {
      setTestResult(errorToMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-7xl space-y-6 p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="space-y-1">
            <h1 className="text-2xl font-semibold tracking-tight">{t("protocolRouter", "Protocol Router")}</h1>
            <p className="text-sm text-muted-foreground">
              {t("protocolRouterToolDesc", "Expose local Anthropic-compatible routes for Claude service providers using each provider's own API key, base URL, API format, and model mappings.")}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button type="button" onClick={() => void load()} className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted">
              <RefreshCw className="h-4 w-4" />
              {t("refresh", "Refresh")}
            </button>
            <button type="button" onClick={() => void save()} disabled={busy || !dirty} className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">
              {t("save", "Save")}
            </button>
          </div>
        </div>

        {message.text ? (
          <div className={`rounded-lg border px-4 py-3 text-sm ${message.type === "error" ? "border-destructive/30 bg-destructive/5 text-destructive" : "border-emerald-500/30 bg-emerald-500/5 text-emerald-600"}`}>
            {message.text}
          </div>
        ) : null}

        <section className="grid gap-4 md:grid-cols-4">
          <div className="rounded-lg border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("status", "Status")}</div>
            <div className="mt-2 text-lg font-semibold">{status?.running ? t("running", "Running") : t("stopped", "Stopped")}</div>
          </div>
          <div className="rounded-lg border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("port", "Port")}</div>
            <div className="mt-2 text-lg font-semibold">{config.port}</div>
          </div>
          <div className="rounded-lg border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("routes", "Routes")}</div>
            <div className="mt-2 text-lg font-semibold">{config.routes.length}</div>
          </div>
          <div className="rounded-lg border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">{t("tokens", "Tokens")}</div>
            <div className="mt-2 text-lg font-semibold">{stats?.total_tokens || 0}</div>
          </div>
        </section>

        <section className="rounded-lg border bg-card p-5">
          <div className="grid gap-4 md:grid-cols-[1fr_1fr_auto]">
            <label className="space-y-2">
              <span className="text-sm font-medium">{t("enableProtocolRouter", "Enable Protocol Router")}</span>
              <div className="flex h-10 items-center">
                <Switch checked={config.enabled} onCheckedChange={(enabled) => setConfig((prev) => ({ ...prev, enabled }))} />
              </div>
            </label>
            <label className="space-y-2">
              <span className="text-sm font-medium">{t("port", "Port")}</span>
              <input className="w-full rounded-lg border bg-background px-3 py-2 text-sm" type="number" min={1} max={65535} value={config.port} onChange={(event) => setConfig((prev) => ({ ...prev, port: parseInt(event.target.value) || 17687 }))} />
            </label>
            <label className="space-y-2">
              <span className="text-sm font-medium">{t("retentionDays", "Retention Days")}</span>
              <input className="w-32 rounded-lg border bg-background px-3 py-2 text-sm" type="number" min={1} max={365} value={config.retention_days} onChange={(event) => setConfig((prev) => ({ ...prev, retention_days: parseInt(event.target.value) || 30 }))} />
            </label>
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <div className="min-w-0 flex-1 rounded-lg border bg-muted/30 px-3 py-2 font-mono text-xs">
              {config.token ? `${config.token.slice(0, 12)}...${config.token.slice(-6)}` : t("notGenerated", "Not generated")}
            </div>
            <button type="button" onClick={() => void rotateToken()} disabled={busy} className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50">
              <Key className="h-4 w-4" />
              {t("rotateToken", "Rotate Token")}
            </button>
          </div>
        </section>

        <section className="space-y-3">
          <h2 className="text-base font-semibold">{t("protocolRouterRoutes", "Derived Routes")}</h2>
          {config.routes.length === 0 ? (
            <div className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground">
              {t("protocolRouterNoRoutes", "No Claude service providers are using the protocol router. Enable it in a Claude service provider and choose an OpenAI-compatible API format.")}
            </div>
          ) : null}
          {config.routes.map((route) => (
            <div key={route.id} className="rounded-lg border bg-card p-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                  <div className="flex items-center gap-2 font-semibold">
                    <RouteIcon className="h-4 w-4" />
                    {route.claude_provider_name}
                  </div>
                  <div className="text-sm text-muted-foreground">{route.base_url || t("baseUrlMissing", "Base URL missing")}</div>
                  <div className="break-all font-mono text-xs text-muted-foreground">{routeUrl(config.port, route)}</div>
                </div>
                <button type="button" onClick={() => void testRoute(route)} disabled={busy || !route.enabled} className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50">
                  <PlugZap className="h-4 w-4" />
                  {t("testConnection", "Test Connection")}
                </button>
              </div>
              <div className="mt-4 grid gap-3 text-sm md:grid-cols-3">
                <div>
                  <div className="text-xs text-muted-foreground">{t("wireApi", "Wire API Format")}</div>
                  <div className="font-medium">{route.wire_api === "open_ai_responses" ? "OpenAI Responses" : "OpenAI Chat"}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground">{t("defaultModel", "Default Model")}</div>
                  <div className="font-medium">{route.default_model || "-"}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground">{t("modelMappings", "Model Mappings")}</div>
                  <div className="font-medium">{route.mappings.length}</div>
                </div>
              </div>
            </div>
          ))}
          {testResult ? <div className="rounded-lg border bg-muted/20 px-4 py-3 text-sm text-muted-foreground">{testResult}</div> : null}
        </section>

        <section className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold">{t("usageStats", "Usage Stats")}</h2>
            <select value={statsDays} onChange={(event) => setStatsDays(parseInt(event.target.value) || 7)} className="rounded-lg border bg-background px-3 py-2 text-sm">
              <option value={1}>{t("today", "Today")}</option>
              <option value={7}>7 {t("days", "days")}</option>
              <option value={30}>30 {t("days", "days")}</option>
            </select>
          </div>
          <div className="rounded-lg border bg-card">
            {(stats?.calls || []).slice(0, 12).map((call) => (
              <div key={`${call.ts}-${call.route_id}-${call.latency_ms}`} className="grid gap-2 border-b px-4 py-3 text-sm last:border-b-0 md:grid-cols-[1fr_auto_auto]">
                <div className="min-w-0">
                  <div className="truncate font-medium">{call.route_id} / {call.model}</div>
                  <div className="text-xs text-muted-foreground">{new Date(call.ts * 1000).toLocaleString()}</div>
                </div>
                <div className="font-mono text-xs text-muted-foreground">HTTP {call.status}</div>
                <div className="font-mono text-xs text-muted-foreground">{call.total_tokens} {t("tokens", "tokens")}</div>
              </div>
            ))}
            {(!stats || stats.calls.length === 0) ? (
              <div className="px-4 py-6 text-sm text-muted-foreground">{t("noUsageYet", "No usage recorded yet.")}</div>
            ) : null}
          </div>
        </section>
      </div>
    </div>
  );
}
