import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  Download,
  Key,
  PlugZap,
  Plus,
  RefreshCw,
  Route as RouteIcon,
  Trash2,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { errorToMessage } from "@/lib/messages";
import {
  protocolProxyFetchModels,
  protocolProxyGetConfig,
  protocolProxyRotateToken,
  protocolProxySaveConfig,
  protocolProxyStats,
  protocolProxyStatus,
  protocolProxyTestConnection,
  type ModelCatalogSource,
  type ProtocolProxyConfig,
  type ProtocolProxyStatsSummary,
  type ProtocolProxyStatus,
  type ProtocolRoute,
  type ProtocolWireApi,
} from "@/lib/protocolProxy";

const DEFAULT_PROTOCOL_PROXY_CONFIG: ProtocolProxyConfig = {
  enabled: false,
  port: 17687,
  token: "",
  retention_days: 30,
  routes: [],
  catalog_sources: [],
};

const PROTOCOL_WIRE_OPTIONS: { value: ProtocolWireApi; label: string }[] = [
  { value: "open_ai_chat", label: "OpenAI Chat Completions" },
  { value: "open_ai_responses", label: "OpenAI Responses" },
  { value: "anthropic_messages", label: "Anthropic Messages" },
];

function FieldShell({
  label,
  description,
  children,
}: {
  label: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <label className="space-y-2">
      <div className="space-y-1">
        <div className="text-sm font-medium text-foreground">{label}</div>
        <p className="text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
      {children}
    </label>
  );
}

function normalizeProtocolProxyConfigForUi(
  config?: ProtocolProxyConfig,
): ProtocolProxyConfig {
  const merged = {
    ...DEFAULT_PROTOCOL_PROXY_CONFIG,
    ...(config || {}),
    catalog_sources:
      config?.catalog_sources || DEFAULT_PROTOCOL_PROXY_CONFIG.catalog_sources,
    routes: config?.routes || [],
  };
  return {
    ...merged,
    port: Number(merged.port) || DEFAULT_PROTOCOL_PROXY_CONFIG.port,
    retention_days: Math.min(
      365,
      Math.max(1, Number(merged.retention_days) || 30),
    ),
    catalog_sources: merged.catalog_sources.map((source) => ({
      ...source,
      base_url: source.base_url || "",
      api_key: source.api_key || "",
      auth_header: source.auth_header || "Authorization",
      model_id_prefix: source.model_id_prefix || "",
      default_wire_api: source.default_wire_api || "open_ai_chat",
      cached_models: source.cached_models || [],
      enabled: source.enabled !== false,
    })),
    routes: merged.routes.map((route) => ({
      ...route,
      api_key: route.api_key || "",
      auth_header: route.auth_header || "Authorization",
      wire_api: route.wire_api || "open_ai_chat",
      default_model: route.default_model || "",
      mappings: route.mappings || [],
      enabled: route.enabled !== false,
    })),
  };
}

export function ProtocolProxyTool({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<ProtocolProxyConfig>(
    DEFAULT_PROTOCOL_PROXY_CONFIG,
  );
  const [savedConfig, setSavedConfig] = useState<ProtocolProxyConfig>(
    DEFAULT_PROTOCOL_PROXY_CONFIG,
  );
  const [status, setStatus] = useState<ProtocolProxyStatus | null>(null);
  const [stats, setStats] = useState<ProtocolProxyStatsSummary | null>(null);
  const [statsDays, setStatsDays] = useState(7);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ type: "success" | "error" | ""; text: string }>({
    type: "",
    text: "",
  });
  const [testResult, setTestResult] = useState("");

  const isTauri = "__TAURI_INTERNALS__" in window;

  const dirty = useMemo(
    () => JSON.stringify(config) !== JSON.stringify(savedConfig),
    [config, savedConfig],
  );

  const loadProtocolProxy = useCallback(async () => {
    if (!isTauri) return;
    try {
      const [latestConfig, latestStatus, latestStats] = await Promise.all([
        protocolProxyGetConfig(),
        protocolProxyStatus(),
        protocolProxyStats(statsDays),
      ]);
      const normalized = normalizeProtocolProxyConfigForUi(latestConfig);
      setConfig(normalized);
      setSavedConfig(normalized);
      setStatus(latestStatus);
      setStats(latestStats);
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    }
  }, [isTauri, statsDays]);

  useEffect(() => {
    if (!isVisible) return;
    void loadProtocolProxy();
  }, [isVisible]);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let teardown: (() => void) | null = null;

    listen("protocol-proxy-status-update", () => {
      if (disposed) return;
      void protocolProxyStatus()
        .then(setStatus)
        .catch(() => {});
    })
      .then((unlisten) => {
        if (disposed) {
          void unlisten();
          return;
        }
        teardown = unlisten;
      })
      .catch(() => {});

    return () => {
      disposed = true;
      if (teardown) void teardown();
    };
  }, [isTauri]);

  const updateProtocolProxy = (patch: Partial<ProtocolProxyConfig>) => {
    setConfig((prev) => ({ ...prev, ...patch }));
  };

  const updateCatalogSourceAt = (
    index: number,
    patch: Partial<ModelCatalogSource>,
  ) => {
    setConfig((prev) => ({
      ...prev,
      catalog_sources: prev.catalog_sources.map((source, sourceIndex) =>
        sourceIndex === index ? { ...source, ...patch } : source,
      ),
    }));
  };

  const updateProtocolRoute = (
    routeId: string,
    patch: Partial<ProtocolRoute>,
  ) => {
    setConfig((prev) => ({
      ...prev,
      routes: prev.routes.map((route) =>
        route.id === routeId ? { ...route, ...patch } : route,
      ),
    }));
  };

  const saveProtocolProxy = async () => {
    setBusy(true);
    setMessage({ type: "", text: "" });
    try {
      const saved = await protocolProxySaveConfig(config);
      const normalized = normalizeProtocolProxyConfigForUi(saved);
      setConfig(normalized);
      setSavedConfig(normalized);
      setStatus(await protocolProxyStatus());
      setStats(await protocolProxyStats(statsDays));
      setMessage({
        type: "success",
        text: t("currentSectionSavedSuccess", "Current section saved."),
      });
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const addProtocolCatalogSource = () => {
    const id = `custom-${Date.now()}`;
    setConfig((prev) => ({
      ...prev,
      catalog_sources: [
        ...prev.catalog_sources,
        {
          id,
          name: t("customProvider", "Custom Provider"),
          models_url: "",
          base_url: "",
          auth_header: "Authorization",
          api_key: "",
          model_id_prefix: "",
          default_wire_api: "open_ai_chat",
          enabled: true,
          last_loaded_at: null,
          cached_models: [],
        },
      ],
    }));
  };

  const removeProtocolCatalogSource = (sourceId: string) => {
    setConfig((prev) => ({
      ...prev,
      catalog_sources: prev.catalog_sources.filter(
        (source) => source.id !== sourceId,
      ),
      routes: prev.routes.filter((route) => route.provider_id !== sourceId),
    }));
  };

  const addProtocolRouteFromSource = (source: ModelCatalogSource) => {
    const model = source.cached_models[0]?.id || "";
    const idBase = source.id || `route-${Date.now()}`;
    const routeId = config.routes.some((route) => route.id === idBase)
      ? `${idBase}-${config.routes.length + 1}`
      : idBase;
    setConfig((prev) => ({
      ...prev,
      routes: [
        ...prev.routes,
        {
          id: routeId,
          name: source.name,
          provider_id: source.id,
          provider_name: source.name,
          base_url: source.base_url,
          auth_header: source.auth_header || "Authorization",
          api_key: source.api_key || "",
          wire_api: source.default_wire_api,
          default_model: model,
          mappings: [],
          enabled: true,
        },
      ],
    }));
  };

  const fetchProtocolModels = async (sourceId: string) => {
    setBusy(true);
    setMessage({ type: "", text: "" });
    try {
      await protocolProxySaveConfig(config);
      const models = await protocolProxyFetchModels(sourceId);
      const latest = await protocolProxyGetConfig();
      const normalized = normalizeProtocolProxyConfigForUi(latest);
      setConfig(normalized);
      setSavedConfig(normalized);
      setMessage({
        type: "success",
        text: t("modelsLoadedWithCount", {
          count: models.length,
          defaultValue: `Models loaded (${models.length})`,
        }),
      });
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const testProtocolRoute = async (route: ProtocolRoute) => {
    setBusy(true);
    setTestResult("");
    try {
      await protocolProxySaveConfig(config);
      const result = await protocolProxyTestConnection({
        route_id: route.id,
        model: route.default_model || null,
      });
      setTestResult(
        `${route.id}: HTTP ${result.status}, ${result.latency_ms}ms, ${result.total_tokens} ${t("tokens", "tokens")}`,
      );
      setStats(await protocolProxyStats(statsDays));
    } catch (err) {
      setTestResult(errorToMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const rotateToken = async () => {
    setBusy(true);
    setMessage({ type: "", text: "" });
    try {
      const next = await protocolProxyRotateToken();
      const normalized = normalizeProtocolProxyConfigForUi(next);
      setConfig(normalized);
      setSavedConfig(normalized);
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const refreshStats = async () => {
    try {
      setStats(await protocolProxyStats(statsDays));
      setStatus(await protocolProxyStatus());
    } catch (err) {
      setMessage({ type: "error", text: errorToMessage(err) });
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-7xl space-y-6 p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="space-y-1">
            <h1 className="text-2xl font-semibold tracking-tight">
              {t("protocolProxy", "Protocol Proxy")}
            </h1>
            <p className="text-sm text-muted-foreground">
              {t(
                "protocolProxyToolDesc",
                "Manage provider catalogs, routes, model mappings, connection tests, and request usage for the local protocol converter.",
              )}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void loadProtocolProxy()}
              className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted"
            >
              <RefreshCw className="h-4 w-4" />
              {t("refresh", "Refresh")}
            </button>
            <button
              type="button"
              onClick={() => void saveProtocolProxy()}
              disabled={busy || !dirty}
              className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {t("save", "Save")}
            </button>
          </div>
        </div>

        {message.text && (
          <div
            className={`rounded-xl border px-4 py-3 text-sm ${
              message.type === "error"
                ? "border-destructive/30 bg-destructive/5 text-destructive"
                : "border-emerald-500/30 bg-emerald-500/5 text-emerald-600"
            }`}
          >
            {message.text}
          </div>
        )}

        <section className="grid gap-4 md:grid-cols-4">
          <div className="rounded-xl border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
              {t("status", "Status")}
            </div>
            <div className="mt-2 text-lg font-semibold">
              {status?.running ? t("running", "Running") : t("stopped", "Stopped")}
            </div>
          </div>
          <div className="rounded-xl border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
              {t("port", "Port")}
            </div>
            <div className="mt-2 text-lg font-semibold">{config.port}</div>
          </div>
          <div className="rounded-xl border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
              {t("routes", "Routes")}
            </div>
            <div className="mt-2 text-lg font-semibold">{config.routes.length}</div>
          </div>
          <div className="rounded-xl border bg-card p-4">
            <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
              {t("tokens", "Tokens")}
            </div>
            <div className="mt-2 text-lg font-semibold">
              {stats?.total_tokens || 0}
            </div>
          </div>
        </section>

        <section className="space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-base font-semibold">
              {t("modelCatalogSources", "Model Catalog Sources")}
            </h2>
            <button
              type="button"
              onClick={addProtocolCatalogSource}
              className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted"
            >
              <Plus className="h-4 w-4" />
              {t("addProvider", "Add Provider")}
            </button>
          </div>

          <div className="space-y-3">
            {config.catalog_sources.length === 0 && (
              <div className="rounded-xl border border-dashed p-6 text-sm text-muted-foreground">
                {t(
                  "protocolProxyNoProviders",
                  "No providers yet. Add a provider with a models URL and base URL before creating routes.",
                )}
              </div>
            )}
            {config.catalog_sources.map((source, sourceIndex) => (
              <div
                key={source.id}
                className="rounded-xl border bg-card p-5 shadow-sm space-y-4"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="font-semibold">{source.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {source.cached_models.length} {t("models", "models")}
                      {source.last_loaded_at
                        ? ` · ${new Date(source.last_loaded_at * 1000).toLocaleString()}`
                        : ""}
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => void fetchProtocolModels(source.id)}
                      disabled={busy}
                      className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      {busy ? (
                        <RefreshCw className="h-4 w-4 animate-spin" />
                      ) : (
                        <Download className="h-4 w-4" />
                      )}
                      {t("loadModels", "Load Models")}
                    </button>
                    <button
                      type="button"
                      onClick={() => addProtocolRouteFromSource(source)}
                      className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted"
                    >
                      <Plus className="h-4 w-4" />
                      {t("createRoute", "Create Route")}
                    </button>
                    <button
                      type="button"
                      onClick={() => removeProtocolCatalogSource(source.id)}
                      className="inline-flex items-center gap-2 rounded-xl border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5"
                    >
                      <Trash2 className="h-4 w-4" />
                      {t("delete", "Delete")}
                    </button>
                  </div>
                </div>

                <div className="grid gap-4 md:grid-cols-2">
                  <FieldShell
                    label={t("providerIdLabel", "Provider ID")}
                    description={t(
                      "providerIdDesc",
                      "Stable local identifier used by routes. Use lowercase letters, numbers, or dashes.",
                    )}
                  >
                    <input
                      value={source.id}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, { id: event.target.value })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm font-mono"
                      placeholder={t("providerIdPlaceholder", "Provider ID")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("providerNameLabel", "Provider Name")}
                    description={t(
                      "providerNameDesc",
                      "Display name shown in the protocol conversion workspace and route selectors.",
                    )}
                  >
                    <input
                      value={source.name}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          name: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("providerNamePlaceholder", "Provider name")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("modelsUrlLabel", "Models URL")}
                    description={t(
                      "modelsUrlDesc",
                      "OpenAI-compatible /models endpoint used to load and cache available models.",
                    )}
                  >
                    <input
                      value={source.models_url}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          models_url: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("modelsUrlPlaceholder", "Models URL")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("baseUrlLabel", "Base URL")}
                    description={t(
                      "providerBaseUrlDesc",
                      "Upstream API base URL used when creating routes from this provider.",
                    )}
                  >
                    <input
                      value={source.base_url}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          base_url: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("baseUrlPlaceholder", "Base URL")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("protocolProxyWireApiLabel", "Wire API")}
                    description={t(
                      "protocolProxyWireApiDesc",
                      "Default upstream protocol used by new routes created from this provider.",
                    )}
                  >
                    <select
                      value={source.default_wire_api}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          default_wire_api: event.target.value as ProtocolWireApi,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                    >
                      {PROTOCOL_WIRE_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </FieldShell>
                  <FieldShell
                    label={t("protocolProxyAuthHeaderLabel", "Auth Header")}
                    description={t(
                      "protocolProxyAuthHeaderDesc",
                      "Header name used for provider authentication. Most OpenAI-compatible providers use Authorization.",
                    )}
                  >
                    <input
                      value={source.auth_header || "Authorization"}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          auth_header: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("authHeaderPlaceholder", "Auth header")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("modelIdPrefixLabel", "Model ID Prefix")}
                    description={t(
                      "modelIdPrefixDesc",
                      "Optional prefix added to loaded model IDs to avoid naming collisions.",
                    )}
                  >
                    <input
                      value={source.model_id_prefix || ""}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          model_id_prefix: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("modelIdPrefixPlaceholder", "Model ID prefix")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("protocolProxyApiKeyLabel", "API Key")}
                    description={t(
                      "providerApiKeyDesc",
                      "Provider key stored locally in OneSpace secrets. It is not written to Claude profiles.",
                    )}
                  >
                    <input
                      type="password"
                      value={source.api_key}
                      onChange={(event) =>
                        updateCatalogSourceAt(sourceIndex, {
                          api_key: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("apiKeyPlaceholder", "API key")}
                    />
                  </FieldShell>
                </div>
                {source.cached_models.length > 0 && (
                  <div className="flex flex-wrap gap-2">
                    {source.cached_models.slice(0, 12).map((model) => (
                      <span
                        key={model.id}
                        className="rounded-full border px-2 py-1 text-xs text-muted-foreground"
                      >
                        {model.id}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </section>

        <section className="space-y-4">
          <h2 className="text-base font-semibold">
            {t("protocolRoutes", "Routes")}
          </h2>
          <div className="space-y-3">
            {config.routes.map((route) => (
              <div
                key={route.id}
                className="rounded-xl border bg-card p-5 shadow-sm space-y-4"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="font-semibold">{route.name}</div>
                    <div className="font-mono text-xs text-muted-foreground">
                      http://127.0.0.1:{config.port}/anthropic/{route.id}/v1
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Switch
                      checked={route.enabled}
                      onCheckedChange={(checked) =>
                        updateProtocolRoute(route.id, { enabled: checked })
                      }
                    />
                    <button
                      type="button"
                      onClick={() =>
                        updateProtocolProxy({
                          routes: config.routes.filter(
                            (item) => item.id !== route.id,
                          ),
                        })
                      }
                      className="rounded-xl border border-destructive/30 p-2 text-destructive hover:bg-destructive/5"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>

                <div className="grid gap-4 md:grid-cols-2">
                  <FieldShell
                    label={t("routeNameLabel", "Route Name")}
                    description={t(
                      "routeNameDesc",
                      "Human-readable name for this Claude-facing local route.",
                    )}
                  >
                    <input
                      value={route.name}
                      onChange={(event) =>
                        updateProtocolRoute(route.id, { name: event.target.value })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("routeNamePlaceholder", "Route name")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("protocolProxyDefaultModelLabel", "Default Model")}
                    description={t(
                      "protocolProxyDefaultModelDesc",
                      "Upstream model used when Claude sends an empty or unmapped model name.",
                    )}
                  >
                    <select
                      value={route.default_model || ""}
                      onChange={(event) =>
                        updateProtocolRoute(route.id, {
                          default_model: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                    >
                      <option value="">{t("selectModel", "Select model")}</option>
                      {(config.catalog_sources.find(
                        (source) => source.id === route.provider_id,
                      )?.cached_models || []).map((model) => (
                        <option key={model.id} value={model.id}>
                          {model.id}
                        </option>
                      ))}
                    </select>
                  </FieldShell>
                  <FieldShell
                    label={t("baseUrlLabel", "Base URL")}
                    description={t(
                      "routeBaseUrlDesc",
                      "Upstream endpoint for this route. It can differ from the provider default.",
                    )}
                  >
                    <input
                      value={route.base_url}
                      onChange={(event) =>
                        updateProtocolRoute(route.id, {
                          base_url: event.target.value,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      placeholder={t("baseUrlPlaceholder", "Base URL")}
                    />
                  </FieldShell>
                  <FieldShell
                    label={t("protocolProxyWireApiLabel", "Wire API")}
                    description={t(
                      "routeWireApiDesc",
                      "Upstream protocol used when forwarding Claude-compatible requests.",
                    )}
                  >
                    <select
                      value={route.wire_api}
                      onChange={(event) =>
                        updateProtocolRoute(route.id, {
                          wire_api: event.target.value as ProtocolWireApi,
                        })
                      }
                      className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                    >
                      {PROTOCOL_WIRE_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </FieldShell>
                </div>

                <div className="space-y-2">
                  <div className="space-y-1">
                    <div className="text-sm font-medium">
                      {t("modelMappings", "Model Mappings")}
                    </div>
                    <p className="text-xs leading-5 text-muted-foreground">
                      {t(
                        "modelMappingsDesc",
                        "Optional aliases that translate Claude-facing model names to upstream model IDs.",
                      )}
                    </p>
                  </div>
                  {(route.mappings || []).length === 0 && (
                    <div className="rounded-xl border border-dashed px-4 py-3 text-xs text-muted-foreground">
                      {t(
                        "modelMappingsEmpty",
                        "No mappings yet. Requests use the same model name unless a mapping or default model applies.",
                      )}
                    </div>
                  )}
                  {(route.mappings || []).map((mapping, index) => (
                    <div
                      key={`${route.id}-mapping-${index}`}
                      className="grid gap-2 md:grid-cols-[1fr_1fr_auto]"
                    >
                      <FieldShell
                        label={t("claudeFacingModelLabel", "Claude-facing Model")}
                        description={t(
                          "claudeFacingModelDesc",
                          "Model name Claude tools will send to the local proxy.",
                        )}
                      >
                        <input
                          value={mapping.claude_model}
                          onChange={(event) => {
                            const mappings = [...route.mappings];
                            mappings[index] = {
                              ...mapping,
                              claude_model: event.target.value,
                            };
                            updateProtocolRoute(route.id, { mappings });
                          }}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          placeholder={t(
                            "claudeFacingModelPlaceholder",
                            "Claude-facing model",
                          )}
                        />
                      </FieldShell>
                      <FieldShell
                        label={t("upstreamModelLabel", "Upstream Model")}
                        description={t(
                          "upstreamModelDesc",
                          "Actual model ID sent to the upstream provider.",
                        )}
                      >
                        <input
                          value={mapping.upstream_model}
                          onChange={(event) => {
                            const mappings = [...route.mappings];
                            mappings[index] = {
                              ...mapping,
                              upstream_model: event.target.value,
                            };
                            updateProtocolRoute(route.id, { mappings });
                          }}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          placeholder={t("upstreamModelPlaceholder", "Upstream model")}
                        />
                      </FieldShell>
                      <button
                        type="button"
                        onClick={() =>
                          updateProtocolRoute(route.id, {
                            mappings: route.mappings.filter(
                              (_, itemIndex) => itemIndex !== index,
                            ),
                          })
                        }
                        className="rounded-xl border p-2 hover:bg-muted"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() =>
                      updateProtocolRoute(route.id, {
                        mappings: [
                          ...(route.mappings || []),
                          {
                            claude_model: "sonnet",
                            upstream_model: route.default_model || "",
                          },
                        ],
                      })
                    }
                    className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted"
                  >
                    <Plus className="h-4 w-4" />
                    {t("addMapping", "Add Mapping")}
                  </button>
                  <button
                    type="button"
                    onClick={() => void testProtocolRoute(route)}
                    disabled={busy || !route.default_model}
                    className="ml-2 inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                  >
                    <PlugZap className="h-4 w-4" />
                    {t("testConnection", "Test Connection")}
                  </button>
                </div>
              </div>
            ))}
            {config.routes.length === 0 && (
              <div className="rounded-xl border border-dashed p-6 text-sm text-muted-foreground">
                {t("protocolProxyNoRoutes", "No routes yet. Create one from a catalog source.")}
              </div>
            )}
          </div>
          {testResult && (
            <div className="rounded-xl border bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
              {testResult}
            </div>
          )}
        </section>

        <section className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold">
              {t("usageStats", "Usage Stats")}
            </h2>
            <div className="flex items-center gap-2">
              <select
                value={statsDays}
                onChange={(event) => setStatsDays(parseInt(event.target.value) || 7)}
                className="rounded-xl border bg-background px-3 py-2 text-sm"
              >
                <option value={1}>{t("today", "Today")}</option>
                <option value={7}>7 {t("days", "days")}</option>
                <option value={30}>30 {t("days", "days")}</option>
              </select>
              <input
                type="number"
                min={1}
                max={365}
                value={statsDays}
                onChange={(event) =>
                  setStatsDays(
                    Math.min(365, Math.max(1, parseInt(event.target.value) || 7)),
                  )
                }
                className="w-24 rounded-xl border bg-background px-3 py-2 text-sm"
              />
              <button
                type="button"
                onClick={() => void refreshStats()}
                className="inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-sm hover:bg-muted"
              >
                <RefreshCw className="h-4 w-4" />
                {t("refresh", "Refresh")}
              </button>
            </div>
          </div>

          <div className="rounded-xl border bg-card p-5 shadow-sm">
            <div className="mb-4 grid gap-3 md:grid-cols-2">
              {(stats?.by_provider || []).slice(0, 6).map((row) => (
                <div
                  key={`provider-${row.key}`}
                  className="rounded-xl border bg-muted/10 p-3"
                >
                  <div className="truncate text-sm font-medium">{row.key}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {row.calls} {t("calls", "calls")} · {row.total_tokens}{" "}
                    {t("tokens", "tokens")}
                  </div>
                </div>
              ))}
              {(stats?.by_model || []).slice(0, 6).map((row) => (
                <div
                  key={`model-${row.key}`}
                  className="rounded-xl border bg-muted/10 p-3"
                >
                  <div className="truncate text-sm font-medium">{row.key}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {row.calls} {t("calls", "calls")} · {row.total_tokens}{" "}
                    {t("tokens", "tokens")}
                  </div>
                </div>
              ))}
            </div>
            <div className="grid gap-3 md:grid-cols-3">
              {(stats?.by_route || []).slice(0, 6).map((row) => (
                <div key={row.key} className="rounded-xl border bg-muted/10 p-3">
                  <div className="truncate text-sm font-medium">{row.key}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {row.calls} {t("calls", "calls")} · {row.total_tokens}{" "}
                    {t("tokens", "tokens")}
                  </div>
                </div>
              ))}
            </div>
            <div className="mt-4 overflow-hidden rounded-xl border">
              <table className="w-full text-left text-sm">
                <thead className="bg-muted/50 text-xs text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2">{t("time", "Time")}</th>
                    <th className="px-3 py-2">{t("route", "Route")}</th>
                    <th className="px-3 py-2">{t("model", "Model")}</th>
                    <th className="px-3 py-2">{t("tokens", "Tokens")}</th>
                    <th className="px-3 py-2">{t("status", "Status")}</th>
                  </tr>
                </thead>
                <tbody>
                  {(stats?.calls || [])
                    .slice()
                    .reverse()
                    .slice(0, 12)
                    .map((call) => (
                      <tr
                        key={`${call.ts}-${call.route_id}-${call.latency_ms}`}
                        className="border-t"
                      >
                        <td className="px-3 py-2 text-xs text-muted-foreground">
                          {new Date(call.ts * 1000).toLocaleString()}
                        </td>
                        <td className="px-3 py-2">{call.route_id}</td>
                        <td className="px-3 py-2">{call.model}</td>
                        <td className="px-3 py-2">{call.total_tokens}</td>
                        <td className="px-3 py-2">{call.status}</td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>

        <section className="rounded-xl border bg-card p-5 shadow-sm space-y-4">
          <div className="flex items-center gap-2">
            <RouteIcon className="h-4 w-4 text-primary" />
            <h2 className="text-base font-semibold">
              {t("protocolProxyGlobalSettings", "Global Settings")}
            </h2>
          </div>
          <div className="grid gap-4 md:grid-cols-4">
            <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3 md:col-span-2">
              <span className="text-sm font-medium">
                {t("enableProtocolProxy", "Enable Protocol Proxy")}
              </span>
              <Switch
                checked={config.enabled}
                onCheckedChange={(checked) =>
                  updateProtocolProxy({ enabled: checked })
                }
              />
            </label>
            <label className="space-y-2">
              <span className="text-sm font-medium">{t("port", "Port")}</span>
              <input
                type="number"
                min={1}
                max={65535}
                value={config.port}
                onChange={(event) =>
                  updateProtocolProxy({
                    port: parseInt(event.target.value) || 17687,
                  })
                }
                className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
              />
            </label>
            <label className="space-y-2">
              <span className="text-sm font-medium">
                {t("retentionDays", "Retention Days")}
              </span>
              <input
                type="number"
                min={1}
                max={365}
                value={config.retention_days}
                onChange={(event) =>
                  updateProtocolProxy({
                    retention_days: Math.min(
                      365,
                      Math.max(1, parseInt(event.target.value) || 30),
                    ),
                  })
                }
                className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
              />
            </label>
          </div>

          <div className="grid gap-3 md:grid-cols-[1fr_auto]">
            <input
              readOnly
              value={config.token}
              className="w-full rounded-xl border bg-muted/50 px-4 py-2.5 font-mono text-xs"
            />
            <button
              type="button"
              onClick={() => void rotateToken()}
              disabled={busy}
              className="inline-flex items-center justify-center gap-2 rounded-xl border px-4 py-2.5 text-sm hover:bg-muted disabled:opacity-50"
            >
              <Key className="h-4 w-4" />
              {t("rotateToken", "Rotate Token")}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
