import { useState, useMemo } from "react";
import {
  CheckCircle2,
  AlertTriangle,
  Layers,
  ShieldCheck,
  Loader2,
  CloudUpload,
  ChevronDown,
  ChevronRight,
  Cpu,
  Search,
  X,
  ArrowRight,
  Laptop,
  Cloud,
  Lock,
  Copy,
  Check,
} from "lucide-react";
import type { TFunction } from "i18next";
import type {
  CloudAgentStatus,
  GatewaySyncResult,
  CloudGatewayProvider,
} from "@/lib/cloudAgent";
import { formatContextWindowDisplay } from "@/lib/aiGateway/context";
import { ProviderTemplateAvatar } from "@/components/AiGateway/ProviderTemplateIcon";

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
  const [copiedModelId, setCopiedModelId] = useState<string | null>(null);

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

  const handleCopyModel = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedModelId(text);
    setTimeout(() => {
      setCopiedModelId((curr) => (curr === text ? null : curr));
    }, 1500);
  };

  const isHealthy =
    status?.gateway?.status === "ok" || status?.gateway?.status === "healthy";
  const modelPools = status?.gateway?.model_pools || [];
  const providers: CloudGatewayProvider[] = status?.gateway?.providers || [];
  const gatewayPort = status?.gateway?.port || 17688;

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
      .filter(Boolean) as CloudGatewayProvider[];
  }, [providers, searchQuery]);

  // 控制全部展开/收起
  const allExpanded = useMemo(() => {
    if (providers.length === 0) return false;
    return providers.every((p) => expandedProviders[p.id]);
  }, [providers, expandedProviders]);

  const toggleExpandAll = () => {
    if (allExpanded) {
      setExpandedProviders({});
    } else {
      const all: Record<string, boolean> = {};
      providers.forEach((p) => {
        all[p.id] = true;
      });
      setExpandedProviders(all);
    }
  };

  return (
    <div className="space-y-4">
      {/* 1. 双端拓扑交互与同步控制栏 */}
      <div className="rounded-xl border border-border bg-card p-4 shadow-sm transition-all">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
          {/* 左侧：本地与云端交互流程示意 */}
          <div className="flex-1 min-w-0">
            <div className="grid grid-cols-1 items-center gap-2.5 sm:grid-cols-7">
              {/* 节点 1：本地桌面网关节点 */}
              <div className="sm:col-span-3 flex items-center gap-2.5 rounded-lg border border-border/80 bg-muted/20 p-2.5 shadow-2xs">
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-sky-500/10 text-sky-600 dark:bg-sky-500/20 dark:text-sky-400">
                  <Laptop className="h-4.5 w-4.5" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs font-semibold text-foreground truncate">
                      本地 OneSpace 网关
                    </span>
                    <span className="rounded bg-sky-500/10 px-1.5 py-0.2 text-[9px] font-mono text-sky-600 dark:text-sky-400">
                      源配置
                    </span>
                  </div>
                  <p className="text-[11px] text-muted-foreground truncate">
                    桌面配置中心 · 多服务商与模型池策略
                  </p>
                </div>
              </div>

              {/* 连接通道：加密流向 */}
              <div className="sm:col-span-1 flex flex-col items-center justify-center py-0.5">
                <div className="flex items-center gap-1 text-muted-foreground/60">
                  <span className="h-0.5 w-2 bg-border sm:block hidden" />
                  <div className="flex h-5 w-5 items-center justify-center rounded-full border border-border bg-muted/60 text-primary shadow-2xs">
                    <ArrowRight className="h-3 w-3" />
                  </div>
                  <span className="h-0.5 w-2 bg-border sm:block hidden" />
                </div>
                <span className="text-[9px] font-mono text-muted-foreground/70 mt-0.5 flex items-center gap-0.5">
                  <Lock className="h-2 w-2" />
                  AES-GCM
                </span>
              </div>

              {/* 节点 2：云端 Linux 网关节点 */}
              <div className="sm:col-span-3 flex items-center gap-2.5 rounded-lg border border-border/80 bg-muted/20 p-2.5 shadow-2xs">
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                  <Cloud className="h-4.5 w-4.5" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs font-semibold text-foreground truncate">
                      云端 Linux 网关
                    </span>
                    <span className="rounded bg-primary/10 px-1.5 py-0.2 text-[9px] font-mono text-primary">
                      Port {gatewayPort}
                    </span>
                    <span
                      className={`inline-flex items-center gap-1 rounded-full px-1.5 py-0.2 text-[9px] font-semibold border ${
                        isHealthy
                          ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                          : "border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-400"
                      }`}
                    >
                      {isHealthy ? (
                        <CheckCircle2 className="h-2.5 w-2.5" />
                      ) : (
                        <AlertTriangle className="h-2.5 w-2.5" />
                      )}
                      {isHealthy
                        ? t("healthy", "健康运行中")
                        : t("offlineOrStarting", "离线或启动中")}
                    </span>
                  </div>
                  <p className="text-[11px] text-muted-foreground truncate">
                    {providers.length} 个服务商 · {modelPools.length} 个模型池已同步
                  </p>
                </div>
              </div>
            </div>
          </div>

          {/* 右侧：一键同步操作按钮 */}
          <div className="flex items-center shrink-0 justify-end">
            <button
              type="button"
              disabled={isSyncing}
              onClick={handleSync}
              className="inline-flex h-8 items-center gap-2 rounded-lg bg-primary px-3.5 text-xs font-medium text-primary-foreground shadow-2xs hover:bg-primary/90 active:scale-98 transition-all disabled:opacity-50 cursor-pointer"
            >
              {isSyncing ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>{t("syncingGateway", "正在同步网关...")}</span>
                </>
              ) : (
                <>
                  <CloudUpload className="h-3.5 w-3.5" />
                  <span>{t("syncGatewayToCloudBtn", "同步至云端")}</span>
                </>
              )}
            </button>
          </div>
        </div>

        {/* 同步结果反馈条 */}
        {syncResult && (
          <div
            className={`mt-3 rounded-lg border p-2.5 text-xs transition-all animate-in fade-in slide-in-from-top-1 ${
              syncResult.success
                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-800 dark:text-emerald-300"
                : "border-destructive/30 bg-destructive/10 text-destructive"
            }`}
          >
            <div className="flex items-center justify-between">
              <span className="font-semibold flex items-center gap-1.5">
                {syncResult.success ? (
                  <CheckCircle2 className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
                ) : (
                  <AlertTriangle className="h-4 w-4" />
                )}
                {syncResult.message}
              </span>
              {syncResult.success && (
                <span className="text-[11px] opacity-90 font-mono">
                  同步已生效：{syncResult.providers_synced} 个服务商 /{" "}
                  {syncResult.model_pools_synced} 个虚拟模型池
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      {/* 2. 当前生效的虚拟模型池（Model Pools） */}
      <div className="rounded-xl border border-border bg-card p-4 shadow-sm space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="flex h-6 w-6 items-center justify-center rounded-md bg-primary/10 text-primary">
              <Layers className="h-3.5 w-3.5" />
            </div>
            <h4 className="text-xs font-semibold text-foreground uppercase tracking-wider">
              {t("activeModelPools", "云端当前生效的模型池")}
            </h4>
          </div>
          <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-semibold text-muted-foreground">
            {modelPools.length} 个模型池
          </span>
        </div>

        {modelPools.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/80 bg-muted/20 p-6 text-center text-xs text-muted-foreground">
            {t(
              "noModelPoolsSynced",
              "云端暂无模型池，请点击右上角「同步至云端」完成配置下发。"
            )}
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-2.5 pt-1">
            {modelPools.map((poolName) => (
              <div
                key={poolName}
                className="group relative flex items-center justify-between rounded-lg border border-border/60 bg-muted/20 p-3 shadow-2xs hover:border-border hover:bg-muted/30 hover:shadow-xs transition-all"
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-background text-primary">
                    <Cpu className="h-4 w-4" />
                  </div>
                  <div className="min-w-0">
                    <span className="font-mono text-xs font-semibold text-foreground truncate block">
                      {poolName}
                    </span>
                    <span className="text-[10px] text-muted-foreground">
                      虚拟聚合模型池
                    </span>
                  </div>
                </div>
                <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[9px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                  可用
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 3. 云端服务商与模型明细 */}
      <div className="rounded-xl border border-border bg-card p-4 shadow-sm space-y-3">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h4 className="text-sm font-semibold text-foreground">
              {t("cloudProvidersAndModels", "云端服务商与模型列表")}
            </h4>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {t("cloudProvidersCount", { count: providers.length }) ||
                `${providers.length} 个服务商`}{" "}
              ·{" "}
              {t("cloudModelsCount", { count: totalModelsCount }) ||
                `${totalModelsCount} 个模型`}
            </p>
          </div>

          <div className="flex items-center gap-2">
            {/* 搜索框 */}
            <div className="relative w-48 sm:w-60">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("searchProvidersOrModelsPlaceholder", "搜索服务商或模型...")}
                className="w-full rounded-lg border border-input bg-background pl-8 pr-7 py-1 text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
              {searchQuery && (
                <button
                  type="button"
                  onClick={() => setSearchQuery("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                >
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>

            {/* 全部展开/收起按钮 */}
            {providers.length > 0 && (
              <button
                type="button"
                onClick={toggleExpandAll}
                className="rounded-lg border border-border/80 bg-background px-2.5 py-1 text-xs font-medium text-foreground hover:bg-muted transition-colors cursor-pointer shadow-2xs"
              >
                {allExpanded ? t("collapseAll", "全部收起") : t("expandAll", "全部展开")}
              </button>
            )}
          </div>
        </div>

        {providers.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/80 bg-muted/20 p-8 text-center text-xs text-muted-foreground">
            {t(
              "noProvidersSynced",
              "云端暂无服务商与模型映射，请点击右上角「同步至云端」完成配置下发。"
            )}
          </div>
        ) : filteredProviders.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/80 bg-muted/20 p-6 text-center text-xs text-muted-foreground">
            {t("noMatchingResults", "未找到匹配的服务商或模型")}
          </div>
        ) : (
          <div className="space-y-3 pt-1">
            {filteredProviders.map((provider) => {
              const isExpanded = searchQuery.trim()
                ? (expandedProviders[provider.id] ?? true)
                : Boolean(expandedProviders[provider.id]);

              return (
                <div
                  key={provider.id}
                  className="rounded-xl border border-border/80 bg-card shadow-2xs overflow-hidden transition-all hover:border-border"
                >
                  {/* 服务商卡片头部（带官方品牌 Logo） */}
                  <div
                    onClick={() => toggleProviderExpand(provider.id)}
                    className="flex items-center justify-between p-3 hover:bg-muted/30 transition-colors cursor-pointer select-none"
                  >
                    <div className="flex items-center gap-3 min-w-0">
                      {isExpanded ? (
                        <ChevronDown className="h-4 w-4 text-muted-foreground shrink-0" />
                      ) : (
                        <ChevronRight className="h-4 w-4 text-muted-foreground shrink-0" />
                      )}

                      {/* 官方品牌图标 */}
                      <ProviderTemplateAvatar
                        templateId={provider.id}
                        templateName={provider.name}
                        baseUrl={provider.base_url}
                        size={32}
                      />

                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-semibold text-foreground truncate">
                            {provider.name}
                          </span>
                          <span className="font-mono text-[11px] text-muted-foreground truncate">
                            ({provider.id})
                          </span>
                        </div>
                        <span className="font-mono text-[11px] text-muted-foreground/80 truncate block max-w-sm mt-0.5">
                          {provider.base_url}
                        </span>
                      </div>
                    </div>

                    <div className="flex items-center gap-2.5 shrink-0">
                      <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold text-muted-foreground">
                        {provider.models?.length || 0} 个模型
                      </span>
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${
                          provider.is_active
                            ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                            : "bg-muted text-muted-foreground"
                        }`}
                      >
                        {provider.is_active
                          ? t("providerStatusActive", "已启用")
                          : t("providerStatusDisabled", "已禁用")}
                      </span>
                    </div>
                  </div>

                  {/* 展开的模型高保真列表 */}
                  {isExpanded && (
                    <div className="border-t border-border/60 bg-muted/15 p-3 space-y-2">
                      <div className="grid grid-cols-1 gap-2">
                        {(provider.models || []).map((model) => {
                          const contextLimitDisplay = formatContextWindowDisplay(
                            model.context_window
                          );
                          const isCopied = copiedModelId === model.id;

                          return (
                            <div
                              key={model.id}
                              className="group flex flex-col gap-2 rounded-lg border border-border/60 bg-card p-2.5 shadow-2xs sm:flex-row sm:items-center sm:justify-between text-xs transition-colors hover:border-border hover:bg-card"
                            >
                              {/* 左侧：本地模型标识 ➔ 上游映射 */}
                              <div className="space-y-1.5 min-w-0 flex-1">
                                <div className="flex flex-wrap items-center gap-2">
                                  <div className="flex items-center gap-1 font-mono font-semibold text-foreground">
                                    <span>{model.id}</span>
                                    <button
                                      type="button"
                                      onClick={() => handleCopyModel(model.id)}
                                      title="复制模型标识"
                                      className="inline-flex h-3.5 w-3.5 items-center justify-center rounded text-muted-foreground/60 hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
                                    >
                                      {isCopied ? (
                                        <Check className="h-3 w-3 text-emerald-500" />
                                      ) : (
                                        <Copy className="h-3 w-3" />
                                      )}
                                    </button>
                                  </div>

                                  {model.display_name && (
                                    <span className="rounded-md bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                                      {model.display_name}
                                    </span>
                                  )}
                                </div>

                                <div className="flex items-center gap-1.5 font-mono text-[11px] text-muted-foreground">
                                  <span>{t("upstreamModel", "上游模型")}:</span>
                                  <span className="text-muted-foreground/40">➔</span>
                                  <code className="rounded bg-muted/50 px-1 py-0.2 text-foreground/90">
                                    {model.upstream_model}
                                  </code>
                                </div>
                              </div>

                              {/* 右侧：上下文与推理强度指标（对齐 AI 网关模型列表界面样式） */}
                              <div className="flex flex-wrap items-center gap-2.5 shrink-0 pt-1 sm:pt-0">
                                {/* 上下文长度限制 tag */}
                                {contextLimitDisplay ? (
                                  <span
                                    className="inline-flex items-center gap-1 rounded-md border border-cyan-500/25 bg-cyan-500/10 px-1.5 py-0.5 font-mono text-[10px] font-medium text-cyan-700 dark:text-cyan-300 shadow-2xs"
                                    title={t("contextLimit", "上下文长度")}
                                  >
                                    <Layers className="h-2.5 w-2.5 text-cyan-600/80 dark:text-cyan-400/80 shrink-0" />
                                    <span>{contextLimitDisplay}</span>
                                  </span>
                                ) : (
                                  <span
                                    className="inline-flex items-center gap-1 rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 py-0.5 font-mono text-[10px] font-medium text-amber-700 dark:text-amber-400 shadow-2xs"
                                    title={t("unlimitedContext", "未限制")}
                                  >
                                    <Layers className="h-2.5 w-2.5 text-amber-600/80 dark:text-amber-400/80 shrink-0" />
                                    <span>{t("unlimitedContext", "未限制")}</span>
                                  </span>
                                )}

                                {/* 推理强度 tag 组 */}
                                {model.reasoning_efforts &&
                                  model.reasoning_efforts.length > 0 && (
                                    <div className="flex flex-wrap items-center gap-1">
                                      <span
                                        title={t("reasoningEfforts", "推理强度")}
                                        className="text-[10px] font-medium text-muted-foreground"
                                      >
                                        {t("reasoningEfforts", "推理强度")}:
                                      </span>
                                      {model.reasoning_efforts.map((effort) => (
                                        <span
                                          key={effort}
                                          className="inline-flex items-center rounded border border-border/80 bg-muted/60 px-1.5 py-0.2 font-mono text-[10px] font-medium text-foreground/85 leading-tight shadow-2xs"
                                        >
                                          {effort}
                                        </span>
                                      ))}
                                    </div>
                                  )}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* 4. 网关安全同步架构说明条 */}
      <div className="flex items-start gap-2.5 rounded-lg border border-border/80 bg-muted/30 p-3 text-xs text-muted-foreground shadow-2xs">
        <ShieldCheck className="h-4 w-4 shrink-0 text-primary mt-0.5" />
        <div className="space-y-0.5">
          <span className="font-semibold text-foreground">
            {t("cloudSyncArchitectureTitle", "网关安全同步机制与双端协同")}
          </span>
          <p className="leading-relaxed">
            云端 AI 网关采用端到端配置安全同步：本地 OneSpace AI
            网关所配置的供应商上游、虚拟模型池策略、API 密钥均会安全脱敏后加密同步至云端
            Linux Agent 守护进程，使云端 OpenCode 会话和 Issues
            自动实施流水线可以直接享有与桌面端完全一致的模型路由与自动重试保活体验。
          </p>
        </div>
      </div>
    </div>
  );
}
