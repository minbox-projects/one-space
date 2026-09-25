import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
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
  AI_GATEWAY_CONFIG_UPDATED_EVENT,
  AI_GATEWAY_DEFAULT_PORT,
  AI_GATEWAY_STATUS_UPDATED_EVENT,
  aggregateModels,
  aiGatewayConfigureTerminal,
  aiGatewayCreateProviderFromTemplate,
  aiGatewayDeleteKey,
  aiGatewayDeleteProvider,
  aiGatewayDeleteProviderModel,
  aiGatewayDeleteProviderTemplate,
  aiGatewayGetConfig,
  aiGatewayProviderTemplates,
  aiGatewayReenableProviderKey,
  aiGatewayReenableProviderModel,
  aiGatewayReenableProviderModels,
  aiGatewayRequestLogs,
  aiGatewayResetProviderTemplates,
  aiGatewayRestoreProviderModel,
  aiGatewaySetDefaultKey,
  aiGatewaySetProviderEnabled,
  aiGatewayStart,
  aiGatewayStatus,
  aiGatewayStop,
  aiGatewaySyncProviderTemplate,
  aiGatewaySyncTerminal,
  aiGatewayTerminalTargets,
  aiGatewayUpsertKey,
  aiGatewayUpsertProvider,
  aiGatewayUpsertProviderTemplate,
  aiGatewayUsageStats,
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
  type GatewayUpstreamProviderWithKeys,
  type ModelPrice,
  type UsageStats,
} from "@/lib/aiGateway";
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
import { LocalKeyDialog } from "./LocalKeyDialog";
import { LocalKeyList } from "./LocalKeyList";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import { UsageStatsPanel } from "./UsageStatsPanel";
import { UsageLogsPanel } from "./UsageLogsPanel";
import {
  setTemplateAutoRefreshFailure,
  setTemplateSyncInFlight,
} from "./useTemplateAutoRefresh";

type AiGatewayTab =
  | "providers"
  | "models"
  | "keys"
  | "terminals"
  | "usage";

type UsageSubTab = "stats" | "logs";

function emptyProvider(): GatewayUpstreamProviderWithKeys {
  return {
    id: "",
    name: "",
    base_url: "",
    keys: [],
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
  };
}

export function AiGateway({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const ToolIcon = Network;
  const iconClassName = "bg-primary/10 text-primary";

  const [activeTab, setActiveTab] = useState<AiGatewayTab>("providers");
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [targets, setTargets] = useState<GatewayTerminalTarget[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [editingProvider, setEditingProvider] =
    useState<GatewayUpstreamProvider | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [isKeyDialogOpen, setIsKeyDialogOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [syncingTools, setSyncingTools] = useState<Record<string, boolean>>({});
  const [addressCopied, setAddressCopied] = useState(false);
  const [defaultKeyCopied, setDefaultKeyCopied] = useState(false);
  const [todayStats, setTodayStats] = useState<UsageStats | null>(null);
  const [todayFailedRequests, setTodayFailedRequests] = useState(0);
  const [refreshingToday, setRefreshingToday] = useState(false);
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
        port: AI_GATEWAY_DEFAULT_PORT,
        providers: [],
        keys: [],
        default_key_id: null,
        terminal_syncs: [],
      });
      setStatus({
        running: false,
        enabled: false,
        port: AI_GATEWAY_DEFAULT_PORT,
        local_base_url: localBaseUrl(AI_GATEWAY_DEFAULT_PORT),
        provider_count: 0,
        auto_disabled_count: 0,
        key_count: 0,
        default_key_id: null,
      });
      setTargets([]);
      setTemplates([]);
      setTemplatesLoadError(null);
      setLoadError(null);
      setTodayStats(null);
      setTodayFailedRequests(0);
      return;
    }

    setLoadError(null);
    setTemplatesLoadError(null);
    const templatesPromise = aiGatewayProviderTemplates()
      .then((value) => ({ ok: true as const, value: value ?? [] }))
      .catch((err: unknown) => ({ ok: false as const, error: err }));
    const statsPromise = aiGatewayUsageStats("today").catch(() => null);
    const logsPromise = aiGatewayRequestLogs({
      range: "today",
      groupBy: "day",
    }).catch(() => null);
    try {
      const [
        nextConfig,
        nextStatus,
        nextTargets,
        templatesResult,
        nextTodayStats,
        nextTodayLogs,
      ] = await Promise.all([
        aiGatewayGetConfig(),
        aiGatewayStatus(),
        aiGatewayTerminalTargets(),
        templatesPromise,
        statsPromise,
        logsPromise,
      ]);
      setConfig(nextConfig);
      setStatus(nextStatus);
      setTargets(nextTargets ?? []);
      setTodayStats(nextTodayStats);
      const failedCount = (nextTodayLogs?.groups ?? []).reduce(
        (acc, g) => acc + (g.error_count || 0),
        0,
      );
      setTodayFailedRequests(failedCount);
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
        title: t("aiGatewayLoadFailed", "Failed to load AI Gateway configuration."),
        description: msg,
        kind: "error",
      });
    }
  }, [isTauri, pushToast, t]);

  useEffect(() => {
    if (!isVisible) return;
    void load();
  }, [isVisible, load]);

  // 订阅后端的运行时状态广播：结算翻转 auto_disabled 后刷新配置，使服务商卡片、
  // 模型列表与运行时状态卡无需切页或重启即可反映新状态。非 Tauri 环境跳过。
  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen(AI_GATEWAY_CONFIG_UPDATED_EVENT, () => {
      void load();
    })
      .then((release) => {
        if (disposed) {
          // 组件已卸载后才解析出 unlisten：立即释放，避免泄漏。
          release();
        } else {
          unlisten = release;
        }
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isTauri, load]);

  const refreshTodayUsage = useCallback(
    async (silent = true) => {
      if (!isTauri) return;
      if (!silent) {
        setRefreshingToday(true);
      }
      try {
        const [nextTodayStats, nextTodayLogs, nextStatus] = await Promise.all([
          aiGatewayUsageStats("today").catch(() => null),
          aiGatewayRequestLogs({ range: "today", groupBy: "day" }).catch(() => null),
          aiGatewayStatus().catch(() => null),
        ]);
        setTodayStats(nextTodayStats);
        const failedCount = (nextTodayLogs?.groups ?? []).reduce(
          (acc, g) => acc + (g.error_count || 0),
          0,
        );
        setTodayFailedRequests(failedCount);
        if (nextStatus) {
          setStatus(nextStatus);
        }
      } catch {
        // 静默刷新异常不干扰用户体验
      } finally {
        if (!silent) {
          setRefreshingToday(false);
        }
      }
    },
    [isTauri],
  );

  const handleManualRefresh = useCallback(async () => {
    await refreshTodayUsage(false);
  }, [refreshTodayUsage]);

  useEffect(() => {
    if (!isVisible) return;
    const handleFocus = () => {
      void refreshTodayUsage(true);
    };
    window.addEventListener("focus", handleFocus);
    return () => {
      window.removeEventListener("focus", handleFocus);
    };
  }, [isVisible, refreshTodayUsage]);

  useEffect(() => {
    if (!isVisible || !status?.running) return;
    const timer = setInterval(() => {
      void refreshTodayUsage(true);
    }, 5000);
    return () => {
      clearInterval(timer);
    };
  }, [isVisible, status?.running, refreshTodayUsage]);

  const applyConfig = useCallback(async (next: GatewayConfig) => {
    setConfig(next);
    const [nextStatus, nextTargets] = await Promise.all([
      aiGatewayStatus(),
      aiGatewayTerminalTargets(),
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
          title: t("aiGatewayActionFailed", "Action failed"),
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
      const nextStatus = running ? await aiGatewayStop() : await aiGatewayStart();
      setStatus(nextStatus);
      setConfig(await aiGatewayGetConfig());
      await emit(AI_GATEWAY_STATUS_UPDATED_EVENT).catch(() => {});
      void refreshTodayUsage(true);
    }, t("aiGatewaySaved", "Saved."));

  const handleToggleProviderEnabled = (provider: GatewayUpstreamProvider, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await aiGatewaySetProviderEnabled(provider.id, enabled));
    }, t("aiGatewaySaved", "Saved."));

  const handleReenableProviderModel = (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) =>
    runAction(async () => {
      const next = await aiGatewayReenableProviderModel(
        providerId,
        localModel,
        upstreamModel,
      );
      await applyConfig(next);
      setEditingProvider(
        next.providers.find((provider) => provider.id === providerId) ?? null,
      );
    }, t("aiGatewaySaved", "Saved."));

  const handleReenableProviderModels = (providerId: string) =>
    runAction(async () => {
      const next = await aiGatewayReenableProviderModels(providerId);
      await applyConfig(next);
      setEditingProvider(
        next.providers.find((provider) => provider.id === providerId) ?? null,
      );
    }, t("aiGatewaySaved", "Saved."));

  const handleReenableProviderKey = (providerId: string, keyId: string) =>
    runAction(async () => {
      const next = await aiGatewayReenableProviderKey(providerId, keyId);
      await applyConfig(next);
      setEditingProvider(
        next.providers.find((provider) => provider.id === providerId) ?? null,
      );
    }, t("aiGatewaySaved", "Saved."));

  const handleSaveProvider = (
    draft: GatewayUpstreamProviderWithKeys,
    prices: ModelPrice[],
  ) =>
    runAction(async () => {
      const next = await aiGatewayUpsertProvider(draft, prices);
      setConfig(next);
      const saved = draft.id
        ? next.providers.find((provider) => provider.id === draft.id) ?? null
        : null;
      setEditingProvider(saved);
      setSelectedProviderId(saved?.id ?? null);
      const [nextStatus, nextTargets] = await Promise.all([
        aiGatewayStatus(),
        aiGatewayTerminalTargets(),
      ]);
      setStatus(nextStatus);
      setTargets(nextTargets);
    }, t("aiGatewayProviderSaved", "Provider saved."));

  const handleDeleteProvider = (providerId: string) =>
    runAction(async () => {
      await applyConfig(await aiGatewayDeleteProvider(providerId));
      setEditingProvider(null);
      setSelectedProviderId(null);
      setIsDialogOpen(false);
    }, t("aiGatewayDeleted", "Deleted."));

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
      const next = await aiGatewayDeleteProviderModel(providerId, upstreamModel);
      await applyConfig(next);
      syncEditingProvider(next, providerId);
    }, t("aiGatewayMappingDeleted", "Model deleted."));

  const handleRestoreProviderModel = (providerId: string, upstreamModel: string) =>
    runAction(async () => {
      const next = await aiGatewayRestoreProviderModel(providerId, upstreamModel);
      await applyConfig(next);
      syncEditingProvider(next, providerId);
    }, t("aiGatewayModelRestored", "Model restored."));

  const handleSaveKey = (key: GatewayKey): Promise<boolean> =>
    runAction(async () => {
      await applyConfig(await aiGatewayUpsertKey(key));
    }, t("aiGatewayKeySaved", "Key saved."));

  const handleDeleteKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await aiGatewayDeleteKey(keyId));
    }, t("aiGatewayDeleted", "Deleted."));

  const handleToggleKeyEnabled = (key: GatewayKey, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await aiGatewayUpsertKey({ ...key, enabled }));
    }, t("aiGatewaySaved", "Saved."));

  const handleSetDefaultKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await aiGatewaySetDefaultKey(keyId));
    }, t("aiGatewaySaved", "Saved."));

  const runTerminalAction = useCallback(
    async (tool: string, action: () => Promise<void>, successTitle: string) => {
      setSyncingTools((prev) => ({ ...prev, [tool]: true }));
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
      } catch (err) {
        pushToast({
          title: t("aiGatewayActionFailed", "Action failed"),
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
        await aiGatewayConfigureTerminal([tool]);
        await applyConfig(await aiGatewayGetConfig());
      },
      t("aiGatewayConfigureSuccess", "Terminal targets configured."),
    );

  const handleSyncTool = (tool: string) =>
    void runTerminalAction(
      tool,
      async () => {
        await aiGatewaySyncTerminal([tool]);
        await applyConfig(await aiGatewayGetConfig());
      },
      t("aiGatewaySyncSuccess", "Terminal targets synced."),
    );

  const handleSyncTemplate = useCallback(
    (templateId: string) => {
      setSyncingTemplates((prev) => ({ ...prev, [templateId]: true }));
      setTemplateSyncInFlight(templateId, true);
      void (async () => {
        try {
          const updated = await aiGatewaySyncProviderTemplate(templateId);
          setTemplates((prev) =>
            prev.map((view) =>
              view.template.id === templateId ? updated : view,
            ),
          );
          // A manual success clears any prior automatic-refresh failure.
          setTemplateAutoRefreshFailure(templateId, null);
          await applyConfig(await aiGatewayGetConfig());
          pushToast({
            title: t("aiGatewayTemplateSyncSuccess", "Provider template synced."),
            kind: "success",
          });
        } catch (err) {
          pushToast({
            title: t("aiGatewayActionFailed", "Action failed"),
            description: errorToMessage(err),
            kind: "error",
          });
        } finally {
          setTemplateSyncInFlight(templateId, false);
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
        const created = await aiGatewayCreateProviderFromTemplate(request);
        const existingIds = new Set(
          (config?.providers ?? []).map((provider) => provider.id),
        );
        const newProvider =
          created.providers.find((provider) => !existingIds.has(provider.id)) ?? null;
        await applyConfig(await aiGatewayGetConfig());
        if (newProvider) {
          setSelectedProviderId(newProvider.id);
          setEditingProvider(newProvider);
          setIsDialogOpen(true);
        }
        pushToast({
          title: t(
            "aiGatewayTemplateProviderCreated",
            "Provider created from template.",
          ),
          kind: "success",
        });
        return true;
      } catch (err) {
        pushToast({
          title: t(
            "aiGatewayTemplateCreateFailed",
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
      const nextViews = await aiGatewayUpsertProviderTemplate(template);
      setTemplates(nextViews);
      pushToast({
        title: t("aiGatewayTemplateSaved", "Template saved."),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t("aiGatewayActionFailed", "Action failed"),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleDeleteTemplate = async (templateId: string): Promise<boolean> => {
    try {
      const nextViews = await aiGatewayDeleteProviderTemplate(templateId);
      setTemplates(nextViews);
      pushToast({
        title: t("aiGatewayTemplateDeleted", "Template deleted."),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t("aiGatewayActionFailed", "Action failed"),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleResetBuiltinTemplates = async (): Promise<boolean> => {
    try {
      const nextViews = await aiGatewayResetProviderTemplates();
      setTemplates(nextViews);
      pushToast({
        title: t(
          "aiGatewayTemplateResetSuccess",
          "Built-in templates restored.",
        ),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t("aiGatewayActionFailed", "Action failed"),
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
      setTimeout(() => setAddressCopied(false), 2000);
      pushToast({ title: t("aiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("aiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const handleCopyDefaultKey = async () => {
    if (!config) return;
    const effectiveId = resolveDefaultKeyId(config.keys, config.default_key_id);
    const key = config.keys.find((k) => k.id === effectiveId);
    if (!key) return;
    try {
      await navigator.clipboard.writeText(key.value);
      setDefaultKeyCopied(true);
      setTimeout(() => setDefaultKeyCopied(false), 2000);
      pushToast({ title: t("aiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("aiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const handleCopyKey = async (key: GatewayKey) => {
    try {
      await navigator.clipboard.writeText(key.value);
      setCopiedKeyId(key.id);
      pushToast({ title: t("aiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("aiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const defaultKeyId = config
    ? resolveDefaultKeyId(config.keys, config.default_key_id)
    : null;

  // 全部服务商已使用的标签，作为编辑弹窗「推荐标签」的候选，让用户能复用其他
  // 服务商已有的标签，而不是只能手输。
  const availableProviderTags = useMemo(() => {
    const set = new Set<string>();
    for (const provider of config?.providers ?? []) {
      for (const tag of provider.tags ?? []) {
        const trimmed = tag.trim();
        if (trimmed) set.add(trimmed);
      }
    }
    return Array.from(set).sort();
  }, [config?.providers]);

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
                {t("aiGatewayLoadFailed", "Failed to load AI Gateway configuration.")}
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
  const syncedTargetsCount = (targets ?? []).filter(
    (target) => target.synced,
  ).length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;

  const tabs: Array<{
    id: AiGatewayTab;
    label: string;
    icon: typeof Server;
    count?: number | string;
    hasAlert?: boolean;
  }> = [
    {
      id: "providers",
      label: t("aiGatewayProviders", "Upstream providers"),
      icon: Server,
      count: config?.providers?.length ?? 0,
      hasAlert: autoDisabledCount > 0,
    },
    {
      id: "models",
      label: t("aiGatewayModelListTab", "Model list"),
      icon: Boxes,
      count: aggregateModels(config.providers).length,
    },
    {
      id: "keys",
      label: t("aiGatewayKeys", "API Keys"),
      icon: KeyRound,
      count: config?.keys?.length ?? 0,
    },
    {
      id: "terminals",
      label: t("aiGatewayTerminalSync", "AI terminal integration"),
      icon: TerminalSquare,
      count:
        (targets ?? []).length > 0
          ? `${syncedTargetsCount}/${targets.length}`
          : undefined,
      hasAlert: pendingSyncCount > 0,
    },
    {
      id: "usage",
      label: t("aiGatewayUsageAndLogsTab", "Usage & Logs"),
      icon: BarChart3,
    },
  ];

  // 传给详情弹窗的最新运行时快照：取已加载 config 中与编辑目标同 id 的服务商对象，
  // 供弹窗只合并运行时字段（未找到或新建服务商时为 null）。
  const runtimeProvider = editingProvider?.id
    ? config?.providers.find((provider) => provider.id === editingProvider.id) ?? null
    : null;

  return (
    <div className="h-full overflow-y-auto" data-testid="ai-gateway-console">
      <div className="mx-auto max-w-7xl space-y-4 p-6">
        {/* 头部标题与简介（对齐 AiEnvironments 规范） */}
        <header className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div className="space-y-0.5">
            <h1 className="text-xl font-bold tracking-tight text-foreground">
              {t("aiGateway", "AI Gateway")}
            </h1>
            <p className="max-w-3xl text-xs text-muted-foreground">
              {t(
                "aiGatewayWorkspaceDesc",
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
          defaultKeyCopied={defaultKeyCopied}
          todayStats={todayStats}
          todayFailedRequests={todayFailedRequests}
          refreshing={refreshingToday}
          onSelectTab={setActiveTab}
          onStart={handleToggleService}
          onStop={handleToggleService}
          onCopyAddress={() => void handleCopyAddress()}
          onCopyDefaultKey={() => void handleCopyDefaultKey()}
          onRefresh={() => void handleManualRefresh()}
        />

        {/* 工作区 Tabs 标签页导航 */}
        <div
          role="tablist"
          aria-label={t("aiGatewayWorkspaceTabs", "AI Gateway tabs")}
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
                    title={t("aiGatewayHasPendingItems", "Has items needing attention")}
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
          aria-label={t("aiGatewayProviders", "Upstream providers")}
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
            onManageTemplates={() => setIsTemplateManageOpen(true)}
          />
        </div>

        {/* Tab 2: 模型列表（面板常驻以保留搜索状态） */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayModelListTab", "Model list")}
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
          aria-label={t("aiGatewayKeys", "API Keys")}
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
          aria-label={t("aiGatewayTerminalSync", "AI terminal integration")}
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
          aria-label={t("aiGatewayUsageAndLogsTab", "Usage & Logs")}
          className={activeTab === "usage" ? "space-y-4 block" : "hidden"}
        >
          {/* 二级子标签切换（用量统计 / 请求日志） */}
          <div className="flex items-center justify-between border-b pb-3">
            <div
              role="tablist"
              aria-label={t("aiGatewayUsageSubTabs", "Usage and logs subtabs")}
              className="inline-flex items-center rounded-lg border bg-muted/40 p-1 text-xs"
            >
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "stats"}
                onClick={() => setUsageSubTab("stats")}
                data-testid="ai-gateway-subtab-usage-stats"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "stats"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <BarChart3 className="h-3.5 w-3.5" />
                <span>{t("aiGatewayUsageStatsSubTab", "Usage stats")}</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "logs"}
                onClick={() => setUsageSubTab("logs")}
                data-testid="ai-gateway-subtab-usage-logs"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "logs"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <ScrollText className="h-3.5 w-3.5" />
                <span>{t("aiGatewayUsageLogsSubTab", "Request logs")}</span>
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
          runtimeProvider={runtimeProvider}
          availableTags={availableProviderTags}
          prices={config.model_prices ?? []}
          busy={busy}
          onSave={(draft, prices) => void handleSaveProvider(draft, prices)}
          onDelete={(providerId) => handleDeleteProvider(providerId)}
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
          onReenableKey={(providerId, keyId) =>
            void handleReenableProviderKey(providerId, keyId)
          }
        />

        {/* 本地密钥新增模态弹窗 */}
        <LocalKeyDialog
          open={isKeyDialogOpen}
          onOpenChange={setIsKeyDialogOpen}
          busy={busy}
          onSave={handleSaveKey}
        />


        {/* 预设服务商模板选择器 */}
        <ProviderTemplatePickerDialog
          open={isTemplatePickerOpen}
          onOpenChange={setIsTemplatePickerOpen}
          templates={templates}
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
            data-testid="ai-gateway-templates-dialog"
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
                        {t("aiGatewayTemplateTab", "Provider Templates")}
                      </DialogTitle>
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-semibold text-primary">
                        {t("aiGatewayAvailableTemplatesCount", {
                          count: templates.length,
                          defaultValue: `共 ${templates.length} 个可用模板`,
                        })}
                      </span>
                    </div>
                    <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                      {t(
                        "aiGatewayTemplatesDialogDesc",
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
                          ? t("aiGatewayTemplateCollapseAll", "Collapse all")
                          : t("aiGatewayTemplateExpandAll", "Expand all")}
                      </span>
                    </button>
                  )}

                  <button
                    type="button"
                    data-testid="template-section-reset-btn"
                    onClick={() => void handleResetBuiltinTemplates()}
                    disabled={busy}
                    title={t("aiGatewayTemplateResetBuiltin", "Restore built-in presets")}
                    className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground shadow-2xs transition hover:bg-muted active:scale-98 disabled:opacity-50"
                  >
                    <RotateCcw className="h-3.5 w-3.5 text-muted-foreground" />
                    <span>{t("aiGatewayTemplateResetBuiltin", "Restore built-in presets")}</span>
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
                    <span>{t("aiGatewayNewTemplate", "New template")}</span>
                  </button>
                </div>
              </div>
            </DialogHeader>
            {templatesLoadError ? (
              <div
                data-testid="ai-gateway-templates-load-error"
                title={templatesLoadError}
                className="mx-6 mt-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3.5 py-2.5 text-xs text-amber-700 dark:text-amber-400"
              >
                {t(
                  "aiGatewayTemplatesLoadFailed",
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
