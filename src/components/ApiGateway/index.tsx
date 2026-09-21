import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit } from "@tauri-apps/api/event";
import {
  BarChart3,
  Boxes,
  ChevronsUpDown,
  KeyRound,
  Network,
  Plus,
  RotateCcw,
  ScrollText,
  Server,
  Sparkles,
  TerminalSquare,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { errorToMessage } from "@/lib/messages";
import {
  API_GATEWAY_STATUS_UPDATED_EVENT,
  aggregateModels,
  apiGatewayConfigureTerminal,
  apiGatewayCreateProviderFromTemplate,
  apiGatewayDeleteKey,
  apiGatewayDeleteProvider,
  apiGatewayDeleteProviderModel,
  apiGatewayDeleteProviderTemplate,
  apiGatewayGetConfig,
  apiGatewayProviderTemplates,
  apiGatewayReenableProviderModel,
  apiGatewayReenableProviderModels,
  apiGatewayResetProviderTemplates,
  apiGatewayRestoreProviderModel,
  apiGatewaySetDefaultKey,
  apiGatewaySetProviderEnabled,
  apiGatewayStart,
  apiGatewayStatus,
  apiGatewayStop,
  apiGatewaySyncProviderTemplate,
  apiGatewaySyncTerminal,
  apiGatewayTerminalTargets,
  apiGatewayUpsertKey,
  apiGatewayUpsertProvider,
  apiGatewayUpsertProviderTemplate,
  localBaseUrl,
  resolveDefaultKeyId,
  type CreateProviderFromTemplateRequest,
  type GatewayConfig,
  type GatewayKey,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateView,
  type GatewayStatus,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
  type ModelPrice,
} from "@/lib/apiGateway";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { RuntimeStatusCard } from "./RuntimeStatusCard";
import { ModelListPanel } from "./ModelListPanel";
import { UpstreamProviderList } from "./UpstreamProviderList";
import { ProviderDetailDialog } from "./ProviderDetailDialog";
import { ProviderTemplateSection } from "./ProviderTemplateSection";
import { ProviderTemplatePickerDialog } from "./ProviderTemplatePickerDialog";
import { ProviderTemplateEditDialog } from "./ProviderTemplateEditDialog";
import { TemplateCreateDialog } from "./TemplateCreateDialog";
import { AggregatedModelsDialog } from "./AggregatedModelsDialog";
import { LocalKeyDialog } from "./LocalKeyDialog";
import { LocalKeyList } from "./LocalKeyList";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import { UsageStatsPanel } from "./UsageStatsPanel";
import { UsageLogsPanel } from "./UsageLogsPanel";

type ApiGatewayTab =
  | "providers"
  | "models"
  | "keys"
  | "terminals"
  | "usage";

type UsageSubTab = "stats" | "logs";

function emptyProvider(): GatewayUpstreamProvider {
  return {
    id: "",
    name: "",
    base_url: "",
    api_key: "",
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
  };
}

export function ApiGateway({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const ToolIcon = Network;
  const iconClassName = "bg-primary/10 text-primary";

  const [activeTab, setActiveTab] = useState<ApiGatewayTab>("providers");
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [targets, setTargets] = useState<GatewayTerminalTarget[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [editingProvider, setEditingProvider] =
    useState<GatewayUpstreamProvider | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [isKeyDialogOpen, setIsKeyDialogOpen] = useState(false);
  const [isModelsDialogOpen, setIsModelsDialogOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [syncingTools, setSyncingTools] = useState<Record<string, boolean>>({});
  const [addressCopied, setAddressCopied] = useState(false);
  const [copiedKeyId, setCopiedKeyId] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [templates, setTemplates] = useState<GatewayProviderTemplateView[]>([]);
  const [templatesLoadError, setTemplatesLoadError] = useState<string | null>(null);
  const [syncingTemplates, setSyncingTemplates] = useState<Record<string, boolean>>(
    {},
  );
  const [isTemplatePickerOpen, setIsTemplatePickerOpen] = useState(false);
  const [isTemplateEditOpen, setIsTemplateEditOpen] = useState(false);
  const [editingTemplate, setEditingTemplate] =
    useState<GatewayProviderTemplate | null>(null);
  const [creatingTemplate, setCreatingTemplate] =
    useState<GatewayProviderTemplate | null>(null);
  const [isTemplateManageOpen, setIsTemplateManageOpen] = useState(false);
  const [templateExpandedIds, setTemplateExpandedIds] = useState<
    Record<string, boolean>
  >({});
  const [usageSubTab, setUsageSubTab] = useState<UsageSubTab>("stats");

  const allTemplatesExpanded = useMemo(() => {
    if (templates.length === 0) return false;
    return templates.every((view) => Boolean(templateExpandedIds[view.template.id]));
  }, [templates, templateExpandedIds]);

  const handleToggleTemplateExpand = useCallback((templateId: string) => {
    setTemplateExpandedIds((prev) => ({
      ...prev,
      [templateId]: !prev[templateId],
    }));
  }, []);

  const handleToggleAllTemplates = useCallback(() => {
    if (allTemplatesExpanded) {
      setTemplateExpandedIds({});
    } else {
      const next: Record<string, boolean> = {};
      for (const view of templates) {
        next[view.template.id] = true;
      }
      setTemplateExpandedIds(next);
    }
  }, [allTemplatesExpanded, templates]);

  const isTauri = "__TAURI_INTERNALS__" in window;

  const load = useCallback(async () => {
    if (!isTauri) {
      // 在非 Tauri 浏览器预览环境下提供默认初始配置，避免页面永久 Loading
      setConfig({
        enabled: false,
        port: 17688,
        providers: [],
        keys: [],
        default_key_id: null,
        terminal_syncs: [],
      });
      setStatus({
        running: false,
        enabled: false,
        port: 17688,
        local_base_url: "http://127.0.0.1:17688/v1",
        provider_count: 0,
        auto_disabled_count: 0,
        key_count: 0,
        default_key_id: null,
      });
      setTargets([]);
      setTemplates([]);
      setTemplatesLoadError(null);
      setLoadError(null);
      return;
    }

    setLoadError(null);
    setTemplatesLoadError(null);
    const templatesPromise = apiGatewayProviderTemplates()
      .then((value) => ({ ok: true as const, value: value ?? [] }))
      .catch((err: unknown) => ({ ok: false as const, error: err }));
    try {
      const [nextConfig, nextStatus, nextTargets, templatesResult] =
        await Promise.all([
          apiGatewayGetConfig(),
          apiGatewayStatus(),
          apiGatewayTerminalTargets(),
          templatesPromise,
        ]);
      setConfig(nextConfig);
      setStatus(nextStatus);
      setTargets(nextTargets ?? []);
      if (templatesResult.ok) {
        setTemplates(templatesResult.value);
      } else {
        // 模板数据获取失败不得阻塞上游服务商页签；仅内联提示，不弹 toast。
        setTemplates([]);
        setTemplatesLoadError(errorToMessage(templatesResult.error));
      }
    } catch (err) {
      const msg = errorToMessage(err);
      setLoadError(msg);
      pushToast({
        title: t("apiGatewayLoadFailed", "Failed to load API Gateway configuration."),
        description: msg,
        kind: "error",
      });
    }
  }, [isTauri, pushToast, t]);

  useEffect(() => {
    if (!isVisible) return;
    void load();
  }, [isVisible, load]);

  const applyConfig = useCallback(async (next: GatewayConfig) => {
    setConfig(next);
    const [nextStatus, nextTargets] = await Promise.all([
      apiGatewayStatus(),
      apiGatewayTerminalTargets(),
    ]);
    setStatus(nextStatus);
    setTargets(nextTargets);
  }, []);

  const runAction = useCallback(
    async (
      action: () => Promise<void>,
      successTitle: string,
    ): Promise<boolean> => {
      setBusy(true);
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
        return true;
      } catch (err) {
        pushToast({
          title: t("apiGatewayActionFailed", "Action failed"),
          description: errorToMessage(err),
          kind: "error",
        });
        return false;
      } finally {
        setBusy(false);
      }
    },
    [pushToast, t],
  );

  const handleToggleService = () =>
    runAction(async () => {
      const running = Boolean(status?.running);
      const nextStatus = running ? await apiGatewayStop() : await apiGatewayStart();
      setStatus(nextStatus);
      setConfig(await apiGatewayGetConfig());
      await emit(API_GATEWAY_STATUS_UPDATED_EVENT).catch(() => {});
    }, t("apiGatewaySaved", "Saved."));

  const handleToggleProviderEnabled = (provider: GatewayUpstreamProvider, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await apiGatewaySetProviderEnabled(provider.id, enabled));
    }, t("apiGatewaySaved", "Saved."));

  const handleReenableProviderModel = (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) =>
    runAction(async () => {
      const next = await apiGatewayReenableProviderModel(
        providerId,
        localModel,
        upstreamModel,
      );
      await applyConfig(next);
      setEditingProvider(
        next.providers.find((provider) => provider.id === providerId) ?? null,
      );
    }, t("apiGatewaySaved", "Saved."));

  const handleReenableProviderModels = (providerId: string) =>
    runAction(async () => {
      const next = await apiGatewayReenableProviderModels(providerId);
      await applyConfig(next);
      setEditingProvider(
        next.providers.find((provider) => provider.id === providerId) ?? null,
      );
    }, t("apiGatewaySaved", "Saved."));

  const handleSaveProvider = (
    draft: GatewayUpstreamProvider,
    prices: ModelPrice[],
  ) =>
    runAction(async () => {
      const next = await apiGatewayUpsertProvider(draft, prices);
      setConfig(next);
      const saved = draft.id
        ? next.providers.find((provider) => provider.id === draft.id) ?? null
        : null;
      setEditingProvider(saved);
      setSelectedProviderId(saved?.id ?? null);
      const [nextStatus, nextTargets] = await Promise.all([
        apiGatewayStatus(),
        apiGatewayTerminalTargets(),
      ]);
      setStatus(nextStatus);
      setTargets(nextTargets);
    }, t("apiGatewayProviderSaved", "Provider saved."));

  const handleDeleteProvider = (providerId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewayDeleteProvider(providerId));
      setEditingProvider(null);
      setSelectedProviderId(null);
      setIsDialogOpen(false);
    }, t("apiGatewayDeleted", "Deleted."));

  const syncEditingProvider = (
    next: GatewayConfig,
    providerId: string,
  ) => {
    const updated =
      next.providers.find((provider) => provider.id === providerId) ?? null;
    setEditingProvider((prev) =>
      prev && prev.id === providerId ? updated : prev,
    );
  };

  const handleDeleteProviderModel = (providerId: string, upstreamModel: string) =>
    runAction(async () => {
      const next = await apiGatewayDeleteProviderModel(providerId, upstreamModel);
      await applyConfig(next);
      syncEditingProvider(next, providerId);
    }, t("apiGatewayMappingDeleted", "Model deleted."));

  const handleRestoreProviderModel = (providerId: string, upstreamModel: string) =>
    runAction(async () => {
      const next = await apiGatewayRestoreProviderModel(providerId, upstreamModel);
      await applyConfig(next);
      syncEditingProvider(next, providerId);
    }, t("apiGatewayModelRestored", "Model restored."));

  const handleSaveKey = (key: GatewayKey): Promise<boolean> =>
    runAction(async () => {
      await applyConfig(await apiGatewayUpsertKey(key));
    }, t("apiGatewayKeySaved", "Key saved."));

  const handleDeleteKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewayDeleteKey(keyId));
    }, t("apiGatewayDeleted", "Deleted."));

  const handleToggleKeyEnabled = (key: GatewayKey, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await apiGatewayUpsertKey({ ...key, enabled }));
    }, t("apiGatewaySaved", "Saved."));

  const handleSetDefaultKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewaySetDefaultKey(keyId));
    }, t("apiGatewaySaved", "Saved."));

  const runTerminalAction = useCallback(
    async (tool: string, action: () => Promise<void>, successTitle: string) => {
      setSyncingTools((prev) => ({ ...prev, [tool]: true }));
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
      } catch (err) {
        pushToast({
          title: t("apiGatewayActionFailed", "Action failed"),
          description: errorToMessage(err),
          kind: "error",
        });
      } finally {
        setSyncingTools((prev) => {
          const next = { ...prev };
          delete next[tool];
          return next;
        });
      }
    },
    [pushToast, t],
  );

  const handleConfigureTool = (tool: string) =>
    void runTerminalAction(
      tool,
      async () => {
        await apiGatewayConfigureTerminal([tool]);
        await applyConfig(await apiGatewayGetConfig());
      },
      t("apiGatewayConfigureSuccess", "Terminal targets configured."),
    );

  const handleSyncTool = (tool: string) =>
    void runTerminalAction(
      tool,
      async () => {
        await apiGatewaySyncTerminal([tool]);
        await applyConfig(await apiGatewayGetConfig());
      },
      t("apiGatewaySyncSuccess", "Terminal targets synced."),
    );

  const handleSyncTemplate = useCallback(
    (templateId: string) => {
      setSyncingTemplates((prev) => ({ ...prev, [templateId]: true }));
      void (async () => {
        try {
          const updated = await apiGatewaySyncProviderTemplate(templateId);
          setTemplates((prev) =>
            prev.map((view) =>
              view.template.id === templateId ? updated : view,
            ),
          );
          await applyConfig(await apiGatewayGetConfig());
          pushToast({
            title: t("apiGatewayTemplateSyncSuccess", "Provider template synced."),
            kind: "success",
          });
        } catch (err) {
          pushToast({
            title: t("apiGatewayActionFailed", "Action failed"),
            description: errorToMessage(err),
            kind: "error",
          });
        } finally {
          setSyncingTemplates((prev) => {
            const next = { ...prev };
            delete next[templateId];
            return next;
          });
        }
      })();
    },
    [applyConfig, pushToast, t],
  );

  const handleCreateProviderFromTemplate = useCallback(
    async (request: CreateProviderFromTemplateRequest): Promise<boolean> => {
      try {
        const created = await apiGatewayCreateProviderFromTemplate(request);
        const existingIds = new Set(
          (config?.providers ?? []).map((provider) => provider.id),
        );
        const newProvider =
          created.providers.find((provider) => !existingIds.has(provider.id)) ?? null;
        await applyConfig(await apiGatewayGetConfig());
        if (newProvider) {
          setSelectedProviderId(newProvider.id);
          setEditingProvider(newProvider);
          setIsDialogOpen(true);
        }
        pushToast({
          title: t(
            "apiGatewayTemplateProviderCreated",
            "Provider created from template.",
          ),
          kind: "success",
        });
        return true;
      } catch (err) {
        pushToast({
          title: t(
            "apiGatewayTemplateCreateFailed",
            "Failed to create provider from template.",
          ),
          description: errorToMessage(err),
          kind: "error",
        });
        return false;
      }
    },
    [applyConfig, config, pushToast, t],
  );

  const handleUpsertTemplate = async (
    template: GatewayProviderTemplate,
  ): Promise<boolean> => {
    try {
      const nextViews = await apiGatewayUpsertProviderTemplate(template);
      setTemplates(nextViews);
      pushToast({
        title: t("apiGatewayTemplateSaved", "Template saved."),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t("apiGatewayActionFailed", "Action failed"),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleDeleteTemplate = async (templateId: string): Promise<boolean> => {
    try {
      const nextViews = await apiGatewayDeleteProviderTemplate(templateId);
      setTemplates(nextViews);
      pushToast({
        title: t("apiGatewayTemplateDeleted", "Template deleted."),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t("apiGatewayActionFailed", "Action failed"),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleResetBuiltinTemplates = async (): Promise<boolean> => {
    try {
      const nextViews = await apiGatewayResetProviderTemplates();
      setTemplates(nextViews);
      pushToast({
        title: t(
          "apiGatewayTemplateResetSuccess",
          "Built-in templates restored.",
        ),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t("apiGatewayActionFailed", "Action failed"),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleCopyAddress = async () => {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(localBaseUrl(config.port));
      setAddressCopied(true);
      pushToast({ title: t("apiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("apiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const handleCopyKey = async (key: GatewayKey) => {
    try {
      await navigator.clipboard.writeText(key.value);
      setCopiedKeyId(key.id);
      pushToast({ title: t("apiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("apiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const defaultKeyId = config
    ? resolveDefaultKeyId(config.keys, config.default_key_id)
    : null;

  if (!config) {
    return (
      <div className="h-full overflow-y-auto">
        <div className="mx-auto max-w-7xl p-6">
          {loadError ? (
            <div className="flex flex-col items-center justify-center rounded-2xl border border-destructive/20 bg-destructive/5 p-8 text-center">
              <div className="rounded-full bg-destructive/10 p-3 text-destructive">
                <Network className="h-6 w-6" />
              </div>
              <h3 className="mt-3 text-base font-semibold text-foreground">
                {t("apiGatewayLoadFailed", "Failed to load API Gateway configuration.")}
              </h3>
              <p className="mt-1 max-w-md text-xs text-muted-foreground">{loadError}</p>
              <button
                type="button"
                onClick={() => void load()}
                className="mt-4 inline-flex items-center rounded-lg bg-primary px-4 py-2 text-xs font-medium text-primary-foreground shadow-sm hover:bg-primary/90"
              >
                {t("retry", "Retry")}
              </button>
            </div>
          ) : (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <span className="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent" />
              <span>{t("loading", "Loading...")}</span>
            </div>
          )}
        </div>
      </div>
    );
  }

  const pendingSyncCount = (targets ?? []).filter(
    (target) => target.pending_sync,
  ).length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;

  const tabs: Array<{
    id: ApiGatewayTab;
    label: string;
    icon: typeof Server;
    count?: number;
    hasAlert?: boolean;
  }> = [
    {
      id: "providers",
      label: t("apiGatewayProviders", "Upstream providers"),
      icon: Server,
      count: config?.providers?.length ?? 0,
      hasAlert: autoDisabledCount > 0,
    },
    {
      id: "models",
      label: t("apiGatewayModelListTab", "Model list"),
      icon: Boxes,
      count: aggregateModels(config.providers).length,
    },
    {
      id: "keys",
      label: t("apiGatewayKeys", "API Keys"),
      icon: KeyRound,
      count: config?.keys?.length ?? 0,
    },
    {
      id: "terminals",
      label: t("apiGatewayTerminalSync", "AI terminal integration"),
      icon: TerminalSquare,
      hasAlert: pendingSyncCount > 0,
    },
    {
      id: "usage",
      label: t("apiGatewayUsageAndLogsTab", "Usage & Logs"),
      icon: BarChart3,
    },
  ];

  return (
    <div className="h-full overflow-y-auto" data-testid="api-gateway-console">
      <div className="mx-auto max-w-7xl space-y-4 p-6">
        {/* 头部标题与简介（对齐 AiEnvironments 规范） */}
        <header className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div className="space-y-0.5">
            <h1 className="text-xl font-bold tracking-tight text-foreground">
              {t("apiGateway", "API Gateway")}
            </h1>
            <p className="max-w-3xl text-xs text-muted-foreground">
              {t(
                "apiGatewayWorkspaceDesc",
                "Run a local OpenAI-compatible relay across multiple upstream providers, manage local keys, and push the local endpoint to OpenCode / Codex.",
              )}
            </p>
          </div>
        </header>

        {/* 常驻运行时服务状态卡片 */}
        <RuntimeStatusCard
          status={status}
          config={config}
          busy={busy}
          addressCopied={addressCopied}
          targets={targets}
          onSelectTab={setActiveTab}
          onShowModels={() => setIsModelsDialogOpen(true)}
          onStart={handleToggleService}
          onStop={handleToggleService}
          onCopyAddress={() => void handleCopyAddress()}
        />

        {/* 工作区 Tabs 标签页导航 */}
        <div
          role="tablist"
          aria-label={t("apiGatewayWorkspaceTabs", "API Gateway tabs")}
          className="flex flex-wrap items-center gap-1.5 rounded-xl border bg-muted/50 p-1.5 shadow-xs"
        >
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => setActiveTab(tab.id)}
                className={`group inline-flex h-9 items-center gap-2 rounded-lg px-3.5 text-xs font-medium transition-all ${
                  isActive
                    ? "bg-background text-foreground font-semibold shadow-xs ring-1 ring-border/80 dark:ring-border"
                    : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
                }`}
              >
                <Icon
                  className={`h-4 w-4 transition-colors ${
                    isActive
                      ? "text-primary"
                      : "text-muted-foreground/70 group-hover:text-foreground"
                  }`}
                />
                <span>{tab.label}</span>
                {tab.count !== undefined ? (
                  <span
                    className={`rounded-full px-2 py-0.5 text-[11px] font-semibold transition-colors ${
                      isActive
                        ? "bg-primary/10 text-primary"
                        : "bg-muted text-muted-foreground group-hover:bg-muted/80 group-hover:text-foreground"
                    }`}
                  >
                    {tab.count}
                  </span>
                ) : null}
                {tab.hasAlert ? (
                  <span
                    className="relative flex h-2 w-2"
                    title={t("apiGatewayHasPendingItems", "Has items needing attention")}
                  >
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
                    <span className="relative inline-flex h-2 w-2 rounded-full bg-amber-500" />
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>

        {/* Tab 1: 上游服务商 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayProviders", "Upstream providers")}
          className={activeTab === "providers" ? "block" : "hidden"}
        >
          <UpstreamProviderList
            providers={config.providers}
            templates={templates}
            selectedProviderId={selectedProviderId}
            busy={busy}
            onSelect={(providerId) => {
              setSelectedProviderId(providerId);
              const found =
                config.providers.find((provider) => provider.id === providerId) ?? null;
              setEditingProvider(found);
              setIsDialogOpen(true);
            }}
            onToggleEnabled={(provider, enabled) =>
              void handleToggleProviderEnabled(provider, enabled)
            }
            onAdd={() => setIsTemplatePickerOpen(true)}
            onDelete={(providerId) => void handleDeleteProvider(providerId)}
            onManageTemplates={() => setIsTemplateManageOpen(true)}
          />
        </div>

        {/* Tab 2: 模型列表（面板常驻以保留搜索状态） */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayModelListTab", "Model list")}
          className={activeTab === "models" ? "block" : "hidden"}
        >
          <ModelListPanel
            providers={config.providers}
            port={config.port}
            onNavigateProviders={() => setActiveTab("providers")}
          />
        </div>

        {/* Tab 3: 本地密钥 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayKeys", "API Keys")}
          className={activeTab === "keys" ? "block" : "hidden"}
        >
          <LocalKeyList
            keys={config.keys}
            defaultKeyId={defaultKeyId}
            busy={busy}
            copiedKeyId={copiedKeyId}
            onAdd={() => setIsKeyDialogOpen(true)}
            onDelete={(keyId) => void handleDeleteKey(keyId)}
            onSetDefault={(keyId) => void handleSetDefaultKey(keyId)}
            onToggleEnabled={(key, enabled) => void handleToggleKeyEnabled(key, enabled)}
            onCopy={(key) => void handleCopyKey(key)}
          />
        </div>

        {/* Tab 4: AI 终端集成 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayTerminalSync", "AI terminal integration")}
          className={activeTab === "terminals" ? "block" : "hidden"}
        >
          <TerminalSyncPanel
            targets={targets}
            config={config}
            gatewayRunning={Boolean(status?.running)}
            onStartGateway={handleToggleService}
            startingGateway={busy}
            syncingTools={syncingTools}
            onConfigureTool={(tool) => handleConfigureTool(tool)}
            onSyncTool={(tool) => handleSyncTool(tool)}
          />
        </div>

        {/* Tab 5: 用量与日志 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayUsageAndLogsTab", "Usage & Logs")}
          className={activeTab === "usage" ? "space-y-4 block" : "hidden"}
        >
          {/* 二级子标签切换（用量统计 / 请求日志） */}
          <div className="flex items-center justify-between border-b pb-3">
            <div
              role="tablist"
              aria-label={t("apiGatewayUsageSubTabs", "Usage and logs subtabs")}
              className="inline-flex items-center rounded-lg border bg-muted/40 p-1 text-xs"
            >
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "stats"}
                onClick={() => setUsageSubTab("stats")}
                data-testid="api-gateway-subtab-usage-stats"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "stats"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <BarChart3 className="h-3.5 w-3.5" />
                <span>{t("apiGatewayUsageStatsSubTab", "Usage stats")}</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "logs"}
                onClick={() => setUsageSubTab("logs")}
                data-testid="api-gateway-subtab-usage-logs"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "logs"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <ScrollText className="h-3.5 w-3.5" />
                <span>{t("apiGatewayUsageLogsSubTab", "Request logs")}</span>
              </button>
            </div>
          </div>

          <div className={usageSubTab === "stats" ? "block" : "hidden"}>
            <UsageStatsPanel isActive={activeTab === "usage" && usageSubTab === "stats"} />
          </div>

          <div className={usageSubTab === "logs" ? "block" : "hidden"}>
            <UsageLogsPanel isActive={activeTab === "usage" && usageSubTab === "logs"} />
          </div>
        </div>

        {/* 服务商新增与编辑模态弹窗 */}
        <ProviderDetailDialog
          open={isDialogOpen}
          onOpenChange={(open) => {
            setIsDialogOpen(open);
            if (!open) {
              setSelectedProviderId(null);
              setEditingProvider(null);
            }
          }}
          provider={editingProvider}
          prices={config.model_prices ?? []}
          busy={busy}
          onSave={(draft, prices) => void handleSaveProvider(draft, prices)}
          onDelete={(providerId) => void handleDeleteProvider(providerId)}
          templates={templates}
          onDeleteModel={(providerId, upstreamModel) =>
            void handleDeleteProviderModel(providerId, upstreamModel)
          }
          onRestoreModel={(providerId, upstreamModel) =>
            void handleRestoreProviderModel(providerId, upstreamModel)
          }
          onReenableModel={(providerId, localModel, upstreamModel) =>
            void handleReenableProviderModel(
              providerId,
              localModel,
              upstreamModel,
            )
          }
          onReenableModels={(providerId) =>
            void handleReenableProviderModels(providerId)
          }
        />

        {/* 本地密钥新增模态弹窗 */}
        <LocalKeyDialog
          open={isKeyDialogOpen}
          onOpenChange={setIsKeyDialogOpen}
          busy={busy}
          onSave={handleSaveKey}
        />

        {/* 聚合模型列表模态弹窗 */}
        <AggregatedModelsDialog
          open={isModelsDialogOpen}
          onOpenChange={setIsModelsDialogOpen}
          providers={config.providers}
        />

        {/* 预设服务商模板选择器 */}
        <ProviderTemplatePickerDialog
          open={isTemplatePickerOpen}
          onOpenChange={setIsTemplatePickerOpen}
          templates={templates}
          providers={config.providers}
          busy={busy}
          onSelectBlank={() => {
            setIsTemplatePickerOpen(false);
            setSelectedProviderId(null);
            setEditingProvider(emptyProvider());
            setIsDialogOpen(true);
          }}
          onSelectTemplate={(tpl) => {
            setIsTemplatePickerOpen(false);
            setCreatingTemplate(tpl);
          }}
          onEditTemplate={(tpl) => {
            setEditingTemplate(tpl);
            setIsTemplateEditOpen(true);
          }}
          onNewTemplate={() => {
            setEditingTemplate(null);
            setIsTemplateEditOpen(true);
          }}
        />

        {/* 服务商模板维护与编辑模态弹窗 */}
        <ProviderTemplateEditDialog
          open={isTemplateEditOpen}
          onOpenChange={setIsTemplateEditOpen}
          template={editingTemplate}
          providers={config.providers}
          busy={busy}
          onSave={handleUpsertTemplate}
          onDelete={handleDeleteTemplate}
        />

        {/* 从模板创建上游服务商模态弹窗 */}
        <TemplateCreateDialog
          open={Boolean(creatingTemplate)}
          onOpenChange={(open) => {
            if (!open) setCreatingTemplate(null);
          }}
          template={creatingTemplate}
          busy={busy}
          onConfirm={async (req) => {
            const ok = await handleCreateProviderFromTemplate(req);
            if (ok) setCreatingTemplate(null);
            return ok;
          }}
        />

        {/* 服务商模板管理模态弹窗 */}
        <Dialog open={isTemplateManageOpen} onOpenChange={setIsTemplateManageOpen}>
          <DialogContent
            className="max-h-[90vh] w-full sm:max-w-5xl lg:max-w-6xl overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
            data-testid="api-gateway-templates-dialog"
          >
            <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
                    <Sparkles className="h-4.5 w-4.5" />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                        {t("apiGatewayTemplateTab", "Provider Templates")}
                      </DialogTitle>
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-semibold text-primary">
                        {t("apiGatewayAvailableTemplatesCount", {
                          count: templates.length,
                          defaultValue: `共 ${templates.length} 个可用模板`,
                        })}
                      </span>
                    </div>
                    <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                      {t(
                        "apiGatewayTemplatesDialogDesc",
                        "Manage official and custom templates. Sync official models and add upstream providers with one click.",
                      )}
                    </DialogDescription>
                  </div>
                </div>

                {/* 顶部操作栏常驻按钮 */}
                <div className="flex items-center gap-2 shrink-0">
                  {templates.length > 0 && (
                    <button
                      type="button"
                      data-testid="template-section-toggle-all-btn"
                      onClick={handleToggleAllTemplates}
                      disabled={busy}
                      className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground shadow-2xs transition hover:bg-muted active:scale-98 disabled:opacity-50"
                    >
                      <ChevronsUpDown className="h-3.5 w-3.5 text-muted-foreground" />
                      <span>
                        {allTemplatesExpanded
                          ? t("apiGatewayTemplateCollapseAll", "Collapse all")
                          : t("apiGatewayTemplateExpandAll", "Expand all")}
                      </span>
                    </button>
                  )}

                  <button
                    type="button"
                    data-testid="template-section-reset-btn"
                    onClick={() => void handleResetBuiltinTemplates()}
                    disabled={busy}
                    title={t("apiGatewayTemplateResetBuiltin", "Restore built-in presets")}
                    className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground shadow-2xs transition hover:bg-muted active:scale-98 disabled:opacity-50"
                  >
                    <RotateCcw className="h-3.5 w-3.5 text-muted-foreground" />
                    <span>{t("apiGatewayTemplateResetBuiltin", "Restore built-in presets")}</span>
                  </button>

                  <button
                    type="button"
                    data-testid="template-section-new-btn"
                    onClick={() => {
                      setEditingTemplate(null);
                      setIsTemplateEditOpen(true);
                    }}
                    disabled={busy}
                    className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-2xs transition hover:bg-primary/90 active:scale-98 disabled:opacity-50"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    <span>{t("apiGatewayNewTemplate", "New template")}</span>
                  </button>
                </div>
              </div>
            </DialogHeader>
            {templatesLoadError ? (
              <div
                data-testid="api-gateway-templates-load-error"
                title={templatesLoadError}
                className="mx-6 mt-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3.5 py-2.5 text-xs text-amber-700 dark:text-amber-400"
              >
                {t(
                  "apiGatewayTemplatesLoadFailed",
                  "Failed to load provider templates.",
                )}
              </div>
            ) : null}
            <div className="flex-1 overflow-y-auto p-6">
              <ProviderTemplateSection
                templates={templates}
                hideHeader
                expandedIds={templateExpandedIds}
                onToggleExpand={handleToggleTemplateExpand}
                busy={busy}
                syncingTemplateIds={syncingTemplates}
                onSync={handleSyncTemplate}
                onCreateProvider={async (req) => {
                  const ok = await handleCreateProviderFromTemplate(req);
                  if (ok) {
                    setIsTemplateManageOpen(false);
                  }
                  return ok;
                }}
                onEditTemplate={(tpl) => {
                  setEditingTemplate(tpl);
                  setIsTemplateEditOpen(true);
                }}
                onNewTemplate={() => {
                  setEditingTemplate(null);
                  setIsTemplateEditOpen(true);
                }}
                onResetBuiltin={() => void handleResetBuiltinTemplates()}
              />
            </div>
          </DialogContent>
        </Dialog>
      </div>
    </div>
  );
}
