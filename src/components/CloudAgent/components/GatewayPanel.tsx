import { useState, useMemo } from "react";
import {
  Network,
  CheckCircle2,
  AlertTriangle,
  Layers,
  ShieldCheck,
  Loader2,
  Sparkles,
  Server,
  ChevronDown,
  ChevronRight,
  Cpu,
  Search,
} from "lucide-react";
import type { TFunction } from "i18next";
import type {
  CloudAgentStatus,
  GatewaySyncResult,
  CloudGatewayProvider,
} from "@/lib/cloudAgent";
import { formatContextWindowDisplay } from "@/lib/aiGateway/context";
import { CloudEmptyState } from "./CloudEmptyState";

interface GatewayPanelProps {
  t: TFunction;
  status: CloudAgentStatus | null;
  loading?: boolean;
  onSyncGateway: () => Promise<GatewaySyncResult | undefined>;
}

export function GatewayPanel({
  t,
  status,
  onSyncGateway,
}: GatewayPanelProps) {
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncResult, setSyncResult] = useState<GatewaySyncResult | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [expandedProviders, setExpandedProviders] = useState<Record<string, boolean>>({});

  const handleSync = async () => {
    try {
      setIsSyncing(true);
      const res = await onSyncGateway();
      if (res) {
        setSyncResult(res);
      }
    } finally {
      setIsSyncing(false);
    }
  };

  const toggleProviderExpand = (providerId: string) => {
    setExpandedProviders((prev) => {
      const current = searchQuery.trim()
        ? (prev[providerId] ?? true)
        : Boolean(prev[providerId]);
      return {
        ...prev,
        [providerId]: !current,
      };
    });
  };

  const isHealthy =
    status?.gateway?.status === "ok" || status?.gateway?.status === "healthy";
  const modelPools = status?.gateway?.model_pools || [];
  const providers: CloudGatewayProvider[] = status?.gateway?.providers || [];

  const totalModelsCount = useMemo(() => {
    return providers.reduce((acc, p) => acc + (p.models?.length || 0), 0);
  }, [providers]);

  // 根据搜索关键字过滤服务商与模型
  const filteredProviders = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return providers;

    return providers
      .map((p) => {
        const matchesProvider =
          p.id.toLowerCase().includes(q) ||
          p.name.toLowerCase().includes(q) ||
          p.base_url.toLowerCase().includes(q);

        const matchedModels = (p.models || []).filter(
          (m) =>
            m.id.toLowerCase().includes(q) ||
            m.upstream_model.toLowerCase().includes(q) ||
            (m.display_name && m.display_name.toLowerCase().includes(q))
        );

        if (matchesProvider) {
          return p;
        }
        if (matchedModels.length > 0) {
          return { ...p, models: matchedModels };
        }
        return null;
      })
      .filter((p): p is CloudGatewayProvider => p !== null);
  }, [providers, searchQuery]);

  return (
    <div className="space-y-6 max-w-4xl">
      {/* 1. 网关状态总览卡片 */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <div className="flex items-center gap-3.5">
            <div
              className={`flex h-12 w-12 items-center justify-center rounded-2xl ${
                isHealthy
                  ? "bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400"
                  : "bg-amber-500/10 text-amber-600 dark:bg-amber-500/20 dark:text-amber-400"
              }`}
            >
              <Network className="h-6 w-6" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h3 className="text-base font-semibold text-slate-900 dark:text-white">
                  {t("cloudGatewayTitle", "Linux AI 网关 (Port 17688)")}
                </h3>
                {isHealthy ? (
                  <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
                    <CheckCircle2 className="h-3 w-3" />
                    {t("healthy", "健康运行中")}
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold text-amber-600 dark:bg-amber-500/20 dark:text-amber-400">
                    <AlertTriangle className="h-3 w-3" />
                    {t("offlineOrStarting", "离线或启动中")}
                  </span>
                )}
              </div>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                {t(
                  "cloudGatewayDesc",
                  "轻量纯 Python OpenAI 兼容代理，支持多 Provider 轮询、Key 自动冷却与虚拟模型池。"
                )}
              </p>
            </div>
          </div>

          <button
            type="button"
            disabled={isSyncing}
            onClick={handleSync}
            className="inline-flex items-center gap-2 rounded-xl bg-emerald-600 px-4 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all disabled:opacity-60 shrink-0 cursor-pointer"
          >
            {isSyncing ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t("syncingGateway", "正在同步网关...")}
              </>
            ) : (
              <>
                <Sparkles className="h-3.5 w-3.5" />
                {t("syncGatewayToCloudBtn", "一键同步网关至云端")}
              </>
            )}
          </button>
        </div>

        {syncResult && (
          <div className="mt-4 rounded-xl bg-emerald-50 dark:bg-emerald-950/30 p-3.5 text-xs text-emerald-800 dark:text-emerald-300 flex items-center gap-2">
            <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
            <span>{syncResult.message}</span>
          </div>
        )}
      </div>

      {/* 2. 云端服务商与模型列表 (核心新增模块) */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900 space-y-5">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div className="flex items-center gap-2.5">
            <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-sky-500/10 text-sky-600 dark:bg-sky-500/20 dark:text-sky-400">
              <Server className="h-4 w-4" />
            </div>
            <div>
              <h4 className="text-sm font-semibold text-slate-900 dark:text-white">
                {t("cloudProvidersAndModels", "云端服务商与模型列表")}
              </h4>
              <p className="text-[11px] text-slate-400">
                {t("cloudProvidersCount", {
                  count: providers.length,
                  defaultValue: `${providers.length} 个服务商`,
                })}{" "}
                ·{" "}
                {t("cloudModelsCount", {
                  count: totalModelsCount,
                  defaultValue: `${totalModelsCount} 个模型`,
                })}
              </p>
            </div>
          </div>

          {providers.length > 0 && (
            <div className="flex items-center gap-2 w-full sm:w-auto">
              <button
                type="button"
                onClick={() => {
                  const allExpanded = providers.every((p) =>
                    Boolean(expandedProviders[p.id])
                  );
                  const next: Record<string, boolean> = {};
                  providers.forEach((p) => {
                    next[p.id] = !allExpanded;
                  });
                  setExpandedProviders(next);
                }}
                className="rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-800/50 px-3 py-1.5 text-xs font-medium text-slate-600 dark:text-slate-300 hover:bg-white dark:hover:bg-slate-800 hover:border-slate-300 dark:hover:border-slate-700 transition-colors whitespace-nowrap cursor-pointer shadow-2xs"
              >
                {providers.every((p) => Boolean(expandedProviders[p.id]))
                  ? t("collapseAll", "全部收起")
                  : t("expandAll", "全部展开")}
              </button>

              <div className="relative flex-1 sm:w-56">
                <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-slate-400" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={t("searchPlaceholder", "搜索服务商或模型...")}
                  className="w-full rounded-xl border border-slate-200 bg-slate-50/70 py-1.5 pl-8 pr-3 text-xs text-slate-800 placeholder-slate-400 transition-colors focus:border-emerald-500 focus:bg-white focus:outline-none dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-200 dark:placeholder-slate-500 dark:focus:border-emerald-500 dark:focus:bg-slate-900"
                />
              </div>
            </div>
          )}
        </div>

        {providers.length === 0 ? (
          <CloudEmptyState
            icon={Cpu}
            title={t("cloudGatewayProvidersTitle", "云端服务商与模型映射")}
            description={t(
              "noProvidersSynced",
              "云端暂无服务商与模型映射，请点击右上角「一键同步网关至云端」完成配置下发。"
            )}
            action={{
              label: isSyncing
                ? t("syncingGateway", "正在同步网关...")
                : t("syncGatewayToCloudBtn", "一键同步网关至云端"),
              icon: Sparkles,
              onClick: handleSync,
              loading: isSyncing,
              disabled: isSyncing,
            }}
          />
        ) : filteredProviders.length === 0 ? (
          <p className="text-xs text-slate-400 py-3 text-center">
            {t("noMatchingModelsFound", "未找到匹配的服务商或模型")}
          </p>
        ) : (
          <div className="space-y-4">
            {filteredProviders.map((provider) => {
              const isExpanded = searchQuery.trim()
                ? (expandedProviders[provider.id] ?? true)
                : Boolean(expandedProviders[provider.id]);
              const isCollapsed = !isExpanded;
              const models = provider.models || [];

              return (
                <div
                  key={provider.id}
                  className="overflow-hidden rounded-xl border border-slate-100 bg-slate-50/50 transition-all dark:border-slate-800/80 dark:bg-slate-800/30"
                >
                  {/* 服务商头部 */}
                  <div
                    onClick={() => toggleProviderExpand(provider.id)}
                    className="flex items-center justify-between px-4 py-3 cursor-pointer select-none hover:bg-slate-100/50 dark:hover:bg-slate-800/60 transition-colors"
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <button
                        type="button"
                        aria-label={isCollapsed ? "展开" : "收起"}
                        className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 transition-transform"
                      >
                        {isCollapsed ? (
                          <ChevronRight className="h-4 w-4" />
                        ) : (
                          <ChevronDown className="h-4 w-4" />
                        )}
                      </button>

                      <div className="flex items-center gap-2 min-w-0">
                        <span className="font-semibold text-xs text-slate-900 dark:text-white truncate">
                          {provider.name || provider.id}
                        </span>
                        <span className="font-mono text-[10px] text-slate-400 bg-slate-200/60 dark:bg-slate-800 px-1.5 py-0.5 rounded shrink-0">
                          {provider.id}
                        </span>
                        {provider.is_active ? (
                          <span className="inline-flex items-center gap-1 text-[10px] text-emerald-600 dark:text-emerald-400 font-medium shrink-0">
                            <span className="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
                            {t("providerStatusActive", "已启用")}
                          </span>
                        ) : (
                          <span className="inline-flex items-center gap-1 text-[10px] text-slate-400 font-medium shrink-0">
                            <span className="h-1.5 w-1.5 rounded-full bg-slate-300 dark:bg-slate-600" />
                            {t("providerStatusDisabled", "已禁用")}
                          </span>
                        )}
                      </div>
                    </div>

                    <div className="flex items-center gap-3 shrink-0 text-[11px] text-slate-400">
                      {provider.base_url && (
                        <span className="hidden md:inline font-mono truncate max-w-xs text-[10px] opacity-70">
                          {provider.base_url}
                        </span>
                      )}
                      <span className="rounded-full bg-white dark:bg-slate-800 px-2 py-0.5 font-medium text-slate-600 dark:text-slate-300 shadow-2xs border border-slate-200/60 dark:border-slate-700/60 text-[10px]">
                        {models.length} 个模型
                      </span>
                    </div>
                  </div>

                  {/* 模型表格明细 */}
                  {!isCollapsed && (
                    <div className="border-t border-slate-100 bg-white/70 dark:border-slate-800/80 dark:bg-slate-900/60 overflow-x-auto">
                      {models.length === 0 ? (
                        <p className="p-4 text-xs text-slate-400 italic">
                          {t("noModelsInProvider", "该服务商下暂无可用的映射模型")}
                        </p>
                      ) : (
                        <table className="w-full text-left text-xs border-collapse">
                          <thead>
                            <tr className="border-b border-slate-100 dark:border-slate-800/60 text-[11px] text-slate-400 font-medium">
                              <th className="py-2.5 px-4 font-normal">
                                {t("modelId", "模型标识")}
                              </th>
                              <th className="py-2.5 px-3 font-normal">
                                {t("upstreamModel", "上游模型")}
                              </th>
                              <th className="py-2.5 px-3 font-normal whitespace-nowrap">
                                {t("contextLimit", "上下文长度")}
                              </th>
                              <th className="py-2.5 px-4 font-normal">
                                {t("reasoningEfforts", "推理强度")}
                              </th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-slate-100 dark:divide-slate-800/40">
                            {models.map((model) => {
                              const hasEfforts =
                                Boolean(model.reasoning_efforts && model.reasoning_efforts.length > 0);
                              const contextDisplay = formatContextWindowDisplay(
                                model.context_window
                              );

                              return (
                                <tr
                                  key={model.id}
                                  className="hover:bg-slate-50/60 dark:hover:bg-slate-800/30 transition-colors"
                                >
                                  {/* 本地模型标识 */}
                                  <td className="py-2.5 px-4">
                                    <div className="font-mono font-medium text-slate-800 dark:text-slate-200">
                                      {model.id}
                                    </div>
                                    {model.display_name &&
                                      model.display_name !== model.id && (
                                        <div className="text-[10px] text-slate-400 mt-0.5 truncate max-w-xs">
                                          {model.display_name}
                                        </div>
                                      )}
                                  </td>

                                  {/* 上游模型 */}
                                  <td className="py-2.5 px-3 font-mono text-[11px] text-slate-500 dark:text-slate-400">
                                    {model.upstream_model}
                                  </td>

                                  {/* 上下文长度限制 */}
                                  <td className="py-2.5 px-3 whitespace-nowrap">
                                    {contextDisplay ? (
                                      <span className="inline-flex items-center gap-1 rounded-md border border-sky-200/70 bg-sky-50 px-2 py-0.5 font-mono text-[11px] font-medium text-sky-700 dark:border-sky-800/50 dark:bg-sky-950/40 dark:text-sky-300 shadow-2xs">
                                        <Layers className="h-2.5 w-2.5 shrink-0 opacity-70" />
                                        <span>{contextDisplay}</span>
                                      </span>
                                    ) : (
                                      <span className="text-[11px] text-slate-400 font-mono">
                                        -
                                      </span>
                                    )}
                                  </td>

                                  {/* 推理强度 */}
                                  <td className="py-2.5 px-4">
                                    {hasEfforts ? (
                                      <div className="flex flex-wrap items-center gap-1.5">
                                        {model.reasoning_efforts!.map(
                                          (effort) => (
                                            <span
                                              key={effort}
                                              className="inline-flex items-center rounded-md border border-purple-200/70 bg-purple-50 px-1.5 py-0.5 font-mono text-[10px] font-medium text-purple-700 dark:border-purple-800/50 dark:bg-purple-950/40 dark:text-purple-300 shadow-2xs"
                                            >
                                              {effort}
                                            </span>
                                          )
                                        )}
                                      </div>
                                    ) : (
                                      <span className="text-[11px] text-slate-400 font-mono">
                                        -
                                      </span>
                                    )}
                                  </td>
                                </tr>
                              );
                            })}
                          </tbody>
                        </table>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* 3. 虚拟模型池 */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Layers className="h-4 w-4 text-emerald-500" />
            <h4 className="text-sm font-semibold text-slate-900 dark:text-white">
              {t("activeModelPools", "云端当前生效的模型池")}
            </h4>
          </div>
          <span className="text-xs text-slate-400">
            {modelPools.length} 个模型池
          </span>
        </div>

        {modelPools.length === 0 ? (
          <p className="text-xs text-slate-400 py-3">
            {t(
              "noModelPoolsSynced",
              "云端暂无模型池，请点击右上角「一键同步网关至云端」完成配置下发。"
            )}
          </p>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
            {modelPools.map((poolName) => (
              <div
                key={poolName}
                className="rounded-xl border border-slate-100 bg-slate-50/80 p-3 text-xs dark:border-slate-800 dark:bg-slate-800/40"
              >
                <div className="font-mono font-medium text-slate-800 dark:text-slate-200">
                  {poolName}
                </div>
                <div className="text-[11px] text-slate-400 mt-1">
                  Virtual Model Pool
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 4. 原理说明卡片 */}
      <div className="rounded-2xl border border-dashed border-slate-200 bg-slate-50/60 p-5 dark:border-slate-800 dark:bg-slate-900/30 text-xs text-slate-500 dark:text-slate-400 space-y-2">
        <div className="font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-1.5">
          <ShieldCheck className="h-4 w-4 text-emerald-500" />
          {t("cloudSyncArchitectureTitle", "网关安全同步机制与双端协同")}
        </div>
        <p>
          1. <strong>安全免密通道</strong>：同步通过本地 Tauri 后端建立的临时安全 SSH 通道写入，无需在云端开放公网管理端口。
        </p>
        <p>
          2. <strong>智能配置合流</strong>：仅覆盖云端的 <code>ai_gateway</code> 节，云端保留现有的 GitHub Token、监听仓库及通知 Webhook 配置。
        </p>
        <p>
          3. <strong>OpenCode 自动对齐</strong>：重载后，云端 OpenCode 自动将本地配置的模型映射为 <code>Cloud Agent Gateway</code> 提供商，立享一致的调用额度。
        </p>
      </div>
    </div>
  );
}
