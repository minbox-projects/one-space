import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Network } from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { useConfirmDialog } from "@/components/ConfirmDialogProvider";
import { errorToMessage } from "@/lib/messages";
import { publishRuntimeStatus, useAppVisibility } from "@/lib/runtimeStatus";
import { useVisibleInterval } from "@/toolbox/useVisibleInterval";
import {
  AI_GATEWAY_CONFIG_UPDATED_EVENT,
  AI_GATEWAY_DEFAULT_PORT,
  AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
  AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS,
  aggregateModels,
  aiGatewayGetConfig,
  aiGatewayModelPoolsStatus,
  aiGatewayProviderTemplates,
  aiGatewayRequestLogs,
  aiGatewayStatus,
  aiGatewayTerminalTargets,
  aiGatewayUsageStats,
  localBaseUrl,
  type GatewayConfig,
  type GatewayKeyAuthFailedEvent,
  type GatewayModelPoolStatus,
  type GatewayModelPool,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateView,
  type GatewayStatus,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
  type UsageStats,
} from "@/lib/aiGateway";
import type { AiGatewayTab, UsageSubTab } from "../helpers/pageTypes";

export function useAiGatewayPageData(isVisible: boolean) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const confirmDialog = useConfirmDialog();
  const ToolIcon = Network;
  const iconClassName = "bg-primary/10 text-primary";

  const [activeTab, setActiveTab] = useState<AiGatewayTab>("providers");
  const [visitedTabs, setVisitedTabs] = useState<
    ReadonlySet<AiGatewayTab>
  >(() => new Set<AiGatewayTab>(["providers"]));
  const [visitedUsageSubTabs, setVisitedUsageSubTabs] = useState<
    ReadonlySet<UsageSubTab>
  >(() => new Set<UsageSubTab>());
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
  const [anthropicAddressCopied, setAnthropicAddressCopied] = useState(false);
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
  const [poolStatuses, setPoolStatuses] = useState<GatewayModelPoolStatus[]>([]);
  const [isPoolDialogOpen, setIsPoolDialogOpen] = useState(false);
  const [editingPool, setEditingPool] = useState<GatewayModelPool | null>(null);
  const [poolDialogKey, setPoolDialogKey] = useState(0);
  const [usageFocusModel, setUsageFocusModel] = useState<string | null>(null);
  const [usageFocusProviderId, setUsageFocusProviderId] = useState<string | null>(
    null,
  );
  const [usageFocusProviderName, setUsageFocusProviderName] = useState<string | null>(
    null,
  );
  const [usageFocusDay, setUsageFocusDay] = useState<string | null>(null);
  const [usageFocusToken, setUsageFocusToken] = useState(0);

  const isTauri = "__TAURI_INTERNALS__" in window;
  const appVisible = useAppVisibility();
  const pageVisible = isVisible && appVisible;

  // Page data generations: a hidden view, a newer configuration read or a
  // newer request supersedes every earlier reply so it can neither apply nor
  // restore an obsolete view.
  const configGenerationRef = useRef(0);
  const todaySeqRef = useRef(0);
  const statusSeqRef = useRef(0);
  const poolSeqRef = useRef(0);
  const configInFlightRef = useRef(false);
  const configPendingRef = useRef(false);
  const isVisibleRef = useRef(pageVisible);
  const previousVisibleRef = useRef(pageVisible);
  const hasLoadedRef = useRef(false);
  const visibilityCatchUpSkipRef = useRef(0);

  useEffect(() => {
    isVisibleRef.current = pageVisible;
  }, [pageVisible]);

  const selectTab = useCallback((tab: AiGatewayTab) => {
    setActiveTab(tab);
    setVisitedTabs((previous) => {
      if (previous.has(tab)) return previous;
      const next = new Set(previous);
      next.add(tab);
      return next;
    });
  }, []);

  const selectUsageSubTab = useCallback((tab: UsageSubTab) => {
    setUsageSubTab(tab);
    setVisitedUsageSubTabs((previous) => {
      if (previous.has(tab)) return previous;
      const next = new Set(previous);
      next.add(tab);
      return next;
    });
  }, []);

  const navigateToProviders = useCallback(
    () => selectTab("providers"),
    [selectTab],
  );
  const navigateToModelPools = useCallback(
    () => selectTab("model-pools"),
    [selectTab],
  );

  useEffect(() => {
    if (activeTab !== "usage") return;
    setVisitedUsageSubTabs((previous) => {
      if (previous.has(usageSubTab)) return previous;
      const next = new Set(previous);
      next.add(usageSubTab);
      return next;
    });
  }, [activeTab, usageSubTab]);

  const applyTodayStats = useCallback((next: UsageStats | null) => {
    setTodayStats(next);
    setTodayFailedRequests(
      next
        ? Math.max(0, next.request_count - next.successful_request_count)
        : 0,
    );
  }, []);

  const applyConfigMutation = useCallback((next: GatewayConfig) => {
    configGenerationRef.current += 1;
    statusSeqRef.current += 1;
    poolSeqRef.current += 1;
    setConfig(next);
  }, []);

  const allTemplatesExpanded = useMemo(() => {
    if (templates.length === 0) return false;
    return templates.every((view) => Boolean(templateExpandedIds[view.template.id]));
  }, [templates, templateExpandedIds]);

  // Tab counts are derived from the loaded configuration only; header/status
  // refreshes must not repeat the aggregation or pool filtering work.
  const modelPoolsForView = useMemo(() => config?.model_pools ?? [], [config]);
  const aggregatedModelCount = useMemo(
    () => aggregateModels(config?.providers ?? []).length,
    [config],
  );
  const modelListTabCount = useMemo(
    () =>
      aggregatedModelCount +
      modelPoolsForView.filter((pool) => pool.enabled).length,
    [aggregatedModelCount, modelPoolsForView],
  );

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
      setPoolStatuses([]);
      return;
    }

    setLoadError(null);
    setTemplatesLoadError(null);
    const configGeneration = ++configGenerationRef.current;
    const todaySeq = ++todaySeqRef.current;
    const statusSeq = ++statusSeqRef.current;
    const poolSeq = ++poolSeqRef.current;
    const templatesPromise = aiGatewayProviderTemplates()
      .then((value) => ({ ok: true as const, value: value ?? [] }))
      .catch((err: unknown) => ({ ok: false as const, error: err }));
    const statsPromise = aiGatewayUsageStats("today", { fresh: true }).catch(
      () => null,
    );
    const logsPromise = aiGatewayRequestLogs(
      { range: "today" },
      { fresh: true },
    ).catch(() => null);
    try {
      const [
        nextConfig,
        nextStatus,
        nextTargets,
        nextPoolStatuses,
        templatesResult,
        nextTodayStats,
        nextTodayLogs,
      ] = await Promise.all([
        aiGatewayGetConfig(),
        aiGatewayStatus(),
        aiGatewayTerminalTargets(),
        aiGatewayModelPoolsStatus().catch(() => [] as GatewayModelPoolStatus[]),
        templatesPromise,
        statsPromise,
        logsPromise,
      ]);
      if (configGeneration !== configGenerationRef.current) return;
      setConfig(nextConfig);
      setTargets(nextTargets ?? []);
      if (statusSeq === statusSeqRef.current) {
        setStatus(nextStatus);
        publishRuntimeStatus("gateway", nextStatus);
      }
      if (poolSeq === poolSeqRef.current) {
        setPoolStatuses(nextPoolStatuses ?? []);
      }
      if (todaySeq === todaySeqRef.current) {
        // Today's failure count always comes from the same statistics snapshot.
        // The plain log read only covers the degraded case where that snapshot
        // is unavailable; it never contributes a failure count alongside stats.
        if (nextTodayStats) {
          applyTodayStats(nextTodayStats);
        } else {
          const fallbackFailures = (nextTodayLogs?.groups ?? []).reduce(
            (total, group) => total + (group.error_count || 0),
            0,
          );
          setTodayStats(null);
          setTodayFailedRequests(fallbackFailures);
        }
      }
      if (templatesResult.ok) {
        setTemplates(templatesResult.value);
      } else {
        // 模板数据获取失败不得阻塞上游服务商页签；仅内联提示，不弹 toast。
        setTemplates([]);
        setTemplatesLoadError(errorToMessage(templatesResult.error));
      }
    } catch (err) {
      if (configGeneration !== configGenerationRef.current) return;
      const msg = errorToMessage(err);
      setLoadError(msg);
      pushToast({
        title: t("aiGatewayLoadFailed", "Failed to load AI Gateway configuration."),
        description: msg,
        kind: "error",
      });
    }
  }, [applyTodayStats, isTauri, pushToast, t]);

  // Configuration-update broadcast: refresh only the data a configuration
  // change can affect (config, runtime status counters, terminal targets,
  // templates and pool statuses). Usage stats and request logs stay on the
  // visible freshness path. A burst starts at most one current read plus one
  // follow-up, and a hidden page only records that work is pending.
  const runConfigScoped = useCallback(
    async (allowFollowUp: boolean) => {
      if (!isTauri) return;
      const configGeneration = ++configGenerationRef.current;
      const statusSeq = ++statusSeqRef.current;
      const poolSeq = ++poolSeqRef.current;
      setLoadError(null);
      setTemplatesLoadError(null);
      try {
        const templatesPromise = aiGatewayProviderTemplates()
          .then((value) => ({ ok: true as const, value: value ?? [] }))
          .catch((err: unknown) => ({ ok: false as const, error: err }));
        const [
          nextConfig,
          nextStatus,
          nextTargets,
          nextPoolStatuses,
          templatesResult,
        ] = await Promise.all([
          aiGatewayGetConfig(),
          aiGatewayStatus(),
          aiGatewayTerminalTargets(),
          aiGatewayModelPoolsStatus().catch(() => [] as GatewayModelPoolStatus[]),
          templatesPromise,
        ]);
        if (configGeneration !== configGenerationRef.current) return;
        setConfig(nextConfig);
        setTargets(nextTargets ?? []);
        if (statusSeq === statusSeqRef.current) {
          setStatus(nextStatus);
          publishRuntimeStatus("gateway", nextStatus);
        }
        if (poolSeq === poolSeqRef.current) {
          setPoolStatuses(nextPoolStatuses ?? []);
        }
        if (templatesResult.ok) {
          setTemplates(templatesResult.value);
        } else {
          setTemplates([]);
          setTemplatesLoadError(errorToMessage(templatesResult.error));
        }
      } catch (err) {
        if (configGeneration !== configGenerationRef.current) return;
        setLoadError(errorToMessage(err));
      } finally {
        configInFlightRef.current = false;
        if (
          allowFollowUp &&
          configPendingRef.current &&
          isVisibleRef.current &&
          configGeneration === configGenerationRef.current
        ) {
          configPendingRef.current = false;
          void runConfigScoped(false);
        } else {
          configPendingRef.current = false;
        }
      }
    },
    [isTauri],
  );

  const loadConfigScoped = useCallback(() => {
    if (!isTauri) return;
    if (!isVisibleRef.current || configInFlightRef.current) {
      configPendingRef.current = true;
      return;
    }
    configInFlightRef.current = true;
    configPendingRef.current = false;
    void runConfigScoped(true);
  }, [isTauri, runConfigScoped]);

  useEffect(() => {
    const wasVisible = previousVisibleRef.current;
    previousVisibleRef.current = pageVisible;
    if (!pageVisible) {
      // A hidden page stops page reads and invalidates every earlier reply.
      configGenerationRef.current += 1;
      todaySeqRef.current += 1;
      statusSeqRef.current += 1;
      poolSeqRef.current += 1;
      return;
    }
    if (!hasLoadedRef.current) {
      hasLoadedRef.current = true;
      if (!wasVisible) visibilityCatchUpSkipRef.current += 1;
      void load();
      return;
    }
    if (!wasVisible) {
      // The visible catch-up is one coherent page read; the interval's own
      // catch-up callback is consumed by refreshTodayUsage below.
      visibilityCatchUpSkipRef.current += 1;
      void load();
    }
  }, [load, pageVisible]);

  // 订阅后端的运行时状态广播：结算翻转 auto_disabled 后刷新受影响的配置数据，使
  // 服务商卡片、模型列表与运行时状态卡无需切页或重启即可反映新状态。用量统计与
  // 请求日志不走此路径，由可见时的轮询维护新鲜度。非 Tauri 环境跳过。
  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen(AI_GATEWAY_CONFIG_UPDATED_EVENT, () => {
      void loadConfigScoped();
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
  }, [isTauri, loadConfigScoped]);

  // 订阅密钥鉴权失败广播：同一服务商在聚合窗口内到达多次只推送一条带计数的
  // warning toast，窗口过期后再次到达则开新窗口并推送新 toast；事件不含密钥值。
  useEffect(() => {
    if (!isTauri) return;
    const pending = new Map<
      string,
      { providerName: string; count: number; timer: number }
    >();
    let disposed = false;
    let unlisten: (() => void) | null = null;

    const flush = (providerId: string) => {
      const entry = pending.get(providerId);
      if (!entry) return;
      pending.delete(providerId);
      pushToast({
        kind: "warning",
        title: t("aiGatewayKeyAuthFailedToastTitle", "Key authentication failed"),
        description: t("aiGatewayKeyAuthFailedToastDescription", {
          provider: entry.providerName,
          count: entry.count,
        }),
      });
    };

    void listen<GatewayKeyAuthFailedEvent>(
      AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
      (event) => {
        const { provider_id, provider_name } = event.payload;
        const existing = pending.get(provider_id);
        if (existing) {
          existing.count += 1;
          return;
        }
        const entry = {
          providerName: provider_name,
          count: 1,
          timer: window.setTimeout(
            () => flush(provider_id),
            AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS,
          ),
        };
        pending.set(provider_id, entry);
      },
    )
      .then((release) => {
        if (disposed) {
          release();
        } else {
          unlisten = release;
        }
      })
      .catch(() => {});

    return () => {
      disposed = true;
      unlisten?.();
      for (const entry of pending.values()) {
        window.clearTimeout(entry.timer);
      }
      pending.clear();
    };
  }, [isTauri, pushToast, t]);

  const refreshTodayUsage = useCallback(
    async (silent = true) => {
      if (!isTauri) return;
      if (visibilityCatchUpSkipRef.current > 0) {
        // The visible transition already performed one coherent page read.
        visibilityCatchUpSkipRef.current -= 1;
        return;
      }
      const todaySeq = ++todaySeqRef.current;
      const statusSeq = ++statusSeqRef.current;
      const poolSeq = ++poolSeqRef.current;
      if (!silent) {
        setRefreshingToday(true);
      }
      try {
        const [nextTodayStats, nextStatus, nextPoolStatuses] = await Promise.all([
          aiGatewayUsageStats("today", { fresh: !silent }).catch(() => null),
          aiGatewayStatus().catch(() => null),
          aiGatewayModelPoolsStatus().catch(() => null),
        ]);
        if (todaySeq === todaySeqRef.current) {
          applyTodayStats(nextTodayStats);
        }
        if (nextStatus && statusSeq === statusSeqRef.current) {
          setStatus(nextStatus);
          publishRuntimeStatus("gateway", nextStatus);
        }
        if (nextPoolStatuses && poolSeq === poolSeqRef.current) {
          setPoolStatuses(nextPoolStatuses);
        }
      } catch {
        // 静默刷新异常不干扰用户体验
      } finally {
        if (!silent) {
          setRefreshingToday(false);
        }
      }
    },
    [applyTodayStats, isTauri],
  );

  const handleManualRefresh = useCallback(async () => {
    await refreshTodayUsage(false);
  }, [refreshTodayUsage]);

  useEffect(() => {
    if (!pageVisible) return;
    const handleFocus = () => {
      void refreshTodayUsage(true);
    };
    window.addEventListener("focus", handleFocus);
    return () => {
      window.removeEventListener("focus", handleFocus);
    };
  }, [pageVisible, refreshTodayUsage]);

  // 今日用量新鲜度轮询：仅在页签可见且网关运行时按间隔刷新；原生窗口或文档
  // 隐藏时暂停，hidden -> visible 时恰好追赶一次。
  useVisibleInterval(
    () => {
      void refreshTodayUsage(true);
    },
    5000,
    pageVisible && Boolean(status?.running),
  );

  const applyConfig = useCallback(async (next: GatewayConfig) => {
    // A user mutation supersedes every page read that started before it.
    const configGeneration = ++configGenerationRef.current;
    const statusSeq = ++statusSeqRef.current;
    setConfig(next);
    const [nextStatus, nextTargets] = await Promise.all([

      aiGatewayStatus(),
      aiGatewayTerminalTargets(),
    ]);
    if (configGeneration !== configGenerationRef.current) return;
    if (statusSeq === statusSeqRef.current) {
      setStatus(nextStatus);
    }
    setTargets(nextTargets);
  }, []);

  const runAction = useCallback(
    async (
      action: () => Promise<void>,
      successTitle: string,
      failureTitle: string,
      successDescription?: string,
    ): Promise<boolean> => {
      setBusy(true);
      try {
        await action();
        pushToast(
          successDescription
            ? {
                title: successTitle,
                description: successDescription,
                kind: "success",
              }
            : { title: successTitle, kind: "success" },
        );
        return true;
      } catch (err) {
        pushToast({
          title: failureTitle,
          description: errorToMessage(err),
          kind: "error",
        });
        return false;
      } finally {
        setBusy(false);
      }
    },
    [pushToast],
  );
  return {
    t,
    pushToast,
    confirmDialog,
    ToolIcon,
    iconClassName,
    activeTab,
    setActiveTab,
    visitedTabs,
    setVisitedTabs,
    visitedUsageSubTabs,
    setVisitedUsageSubTabs,
    config,
    setConfig,
    status,
    setStatus,
    targets,
    setTargets,
    selectedProviderId,
    setSelectedProviderId,
    editingProvider,
    setEditingProvider,
    isDialogOpen,
    setIsDialogOpen,
    isKeyDialogOpen,
    setIsKeyDialogOpen,
    busy,
    setBusy,
    syncingTools,
    setSyncingTools,
    addressCopied,
    setAddressCopied,
    anthropicAddressCopied,
    setAnthropicAddressCopied,
    todayStats,
    setTodayStats,
    todayFailedRequests,
    setTodayFailedRequests,
    refreshingToday,
    setRefreshingToday,
    copiedKeyId,
    setCopiedKeyId,
    loadError,
    setLoadError,
    templates,
    setTemplates,
    templatesLoadError,
    setTemplatesLoadError,
    syncingTemplates,
    setSyncingTemplates,
    isTemplatePickerOpen,
    setIsTemplatePickerOpen,
    isTemplateEditOpen,
    setIsTemplateEditOpen,
    editingTemplate,
    setEditingTemplate,
    creatingTemplate,
    setCreatingTemplate,
    isTemplateManageOpen,
    setIsTemplateManageOpen,
    templateExpandedIds,
    setTemplateExpandedIds,
    usageSubTab,
    setUsageSubTab,
    poolStatuses,
    setPoolStatuses,
    isPoolDialogOpen,
    setIsPoolDialogOpen,
    editingPool,
    setEditingPool,
    poolDialogKey,
    setPoolDialogKey,
    usageFocusModel,
    setUsageFocusModel,
    usageFocusProviderId,
    setUsageFocusProviderId,
    usageFocusProviderName,
    setUsageFocusProviderName,
    usageFocusDay,
    setUsageFocusDay,
    usageFocusToken,
    setUsageFocusToken,
    isTauri,
    pageVisible,
    selectTab,
    selectUsageSubTab,
    navigateToProviders,
    navigateToModelPools,
    applyTodayStats,
    applyConfigMutation,
    allTemplatesExpanded,
    modelPoolsForView,
    aggregatedModelCount,
    modelListTabCount,
    handleToggleTemplateExpand,
    handleToggleAllTemplates,
    load,
    runConfigScoped,
    loadConfigScoped,
    refreshTodayUsage,
    handleManualRefresh,
    applyConfig,
    runAction,
  };
}

export type AiGatewayPageData = ReturnType<typeof useAiGatewayPageData>;
