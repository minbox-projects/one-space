import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  Boxes,
  Check,
  CheckCircle2,
  Copy,
  KeyRound,
  Loader2,
  Play,
  RefreshCw,
  Server,
  Square,
  Zap,
} from "lucide-react";
import {
  aggregateModels,
  formatGatewayTokens,
  formatUsageCurrency,
  localBaseUrl,
  resolveDefaultKeyId,
  type GatewayConfig,
  type GatewayStatus,
  type UsageStats,
} from "@/lib/aiGateway";

type RuntimeStatusCardProps = {
  status: GatewayStatus | null;
  config: GatewayConfig;
  busy: boolean;
  addressCopied: boolean;
  defaultKeyCopied?: boolean;
  todayStats?: UsageStats | null;
  todayFailedRequests?: number;
  refreshing?: boolean;
  onSelectTab?: (tab: "providers" | "models" | "keys" | "terminals" | "usage") => void;
  onStart: () => void;
  onStop: () => void;
  onCopyAddress: () => void;
  onCopyDefaultKey?: () => void;
  onRefresh?: () => void;
};

export function RuntimeStatusCard({
  status,
  config,
  busy,
  addressCopied,
  defaultKeyCopied = false,
  todayStats = null,
  todayFailedRequests = 0,
  refreshing = false,
  onSelectTab,
  onStart,
  onStop,
  onCopyAddress,
  onCopyDefaultKey,
  onRefresh,
}: RuntimeStatusCardProps) {
  const { t } = useTranslation();
  const running = Boolean(status?.running);
  const address = localBaseUrl(config.port);

  // 1. 服务商与健康度
  const totalProviders = status?.provider_count ?? config.providers.length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;
  const activeProviders = useMemo(() => {
    return config.providers.filter((p) => p.enabled).length;
  }, [config.providers]);

  // 2. 聚合模型数（与聚合模型面板严格一致）
  const aggregatedModelsCount = useMemo(
    () => aggregateModels(config.providers).length,
    [config.providers],
  );

  // 3. 默认 API 密钥解析
  const effectiveDefaultKey = useMemo(() => {
    const effectiveId = resolveDefaultKeyId(config.keys, config.default_key_id);
    if (!effectiveId) return null;
    return config.keys.find((k) => k.id === effectiveId) ?? null;
  }, [config.keys, config.default_key_id]);

  // 4. 今日用量与消耗
  const todayRequests = todayStats?.request_count ?? 0;
  const todayTokens = todayStats?.total_tokens ?? 0;
  const todayAmount = todayStats?.amount ?? 0;

  return (
    <section
      className="space-y-3 rounded-xl border bg-card p-4 shadow-sm"
      data-testid="ai-gateway-runtime"
    >
      {/* 顶部主状态栏：运行状态、本地 API 地址、默认 Key 快速复制与启停按钮 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2.5">
          {/* 服务运行状态灯与基础地址 */}
          <div className="flex items-center gap-2 rounded-lg border bg-muted/20 px-3 py-1.5">
            <span className="relative flex h-2.5 w-2.5">
              {running ? (
                <>
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-emerald-500" />
                </>
              ) : (
                <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-muted-foreground/40" />
              )}
            </span>
            <div className="flex items-center gap-2 text-xs">
              <span
                className={`font-semibold leading-none ${
                  running ? "text-emerald-700 dark:text-emerald-400" : "text-muted-foreground"
                }`}
                data-testid="ai-gateway-runtime-state"
              >
                {running ? t("aiGatewayRunning", "Running") : t("aiGatewayStopped", "Stopped")}
              </span>
              <span className="text-muted-foreground/40">·</span>
              <div className="flex items-center gap-1">
                <code
                  className={`font-mono text-xs font-medium select-all ${
                    running ? "text-foreground" : "text-muted-foreground/70"
                  }`}
                  title={address}
                  data-testid="ai-gateway-local-address"
                >
                  {address}
                </code>
                <button
                  type="button"
                  onClick={onCopyAddress}
                  aria-label={t("aiGatewayCopyAddress", "Copy local API address")}
                  title={
                    addressCopied
                      ? t("aiGatewayCopied", "Copied")
                      : t("aiGatewayCopyAddress", "Copy local API address")
                  }
                  className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground transition hover:bg-muted hover:text-foreground"
                >
                  {addressCopied ? (
                    <Check className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                  ) : (
                    <Copy className="h-3 w-3" />
                  )}
                </button>
              </div>
              <span className="text-muted-foreground/40">·</span>
              <span className="rounded bg-muted/60 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                OpenAI
              </span>
            </div>
          </div>

          {/* 默认 API 密钥快速复制栏（高频快捷接入） */}
          <div className="flex items-center gap-1.5 rounded-lg border bg-muted/20 px-3 py-1.5 text-xs">
            <KeyRound className="h-3.5 w-3.5 text-muted-foreground/70 shrink-0" />
            <span className="text-muted-foreground/70 text-[11px] font-medium">
              {t("aiGatewayDefaultKey", "Default API key")}:
            </span>
            {effectiveDefaultKey ? (
              <div className="flex items-center gap-1">
                <code
                  className="font-mono text-xs font-medium select-all text-foreground"
                  title={effectiveDefaultKey.label || effectiveDefaultKey.value}
                  data-testid="ai-gateway-default-key-preview"
                >
                  {effectiveDefaultKey.value
                    ? `${effectiveDefaultKey.value.slice(0, 11)}••••`
                    : effectiveDefaultKey.label}
                </code>
                <button
                  type="button"
                  onClick={onCopyDefaultKey}
                  aria-label={t("aiGatewayCopyDefaultKey", "Copy default API key")}
                  title={
                    defaultKeyCopied
                      ? t("aiGatewayCopied", "Copied")
                      : t("aiGatewayCopyDefaultKey", "Copy default API key")
                  }
                  data-testid="ai-gateway-copy-default-key"
                  className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground transition hover:bg-muted hover:text-foreground"
                >
                  {defaultKeyCopied ? (
                    <Check className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                  ) : (
                    <Copy className="h-3 w-3" />
                  )}
                </button>
              </div>
            ) : (
              <span
                className="text-muted-foreground/60 text-[11px] cursor-pointer hover:underline"
                onClick={() => onSelectTab?.("keys")}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") onSelectTab?.("keys");
                }}
              >
                {t("aiGatewayNoDefaultKey", "No key configured")}
              </span>
            )}
          </div>
        </div>

        {/* 右侧服务操作按钮 */}
        <div className="flex items-center gap-2">
          {onRefresh && (
            <button
              type="button"
              onClick={onRefresh}
              disabled={busy || refreshing}
              title={t("aiGatewayRefreshData", "Refresh data")}
              aria-label={t("aiGatewayRefreshData", "Refresh data")}
              data-testid="ai-gateway-refresh-btn"
              className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-border/80 bg-background text-muted-foreground shadow-2xs transition hover:bg-muted hover:text-foreground active:scale-98 disabled:opacity-50"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`} />
            </button>
          )}

          <button
            type="button"
            onClick={running ? onStop : onStart}
            disabled={busy}
            title={running ? t("aiGatewayStop", "Stop service") : t("aiGatewayStart", "Start service")}
            aria-label={running ? t("aiGatewayStop", "Stop service") : t("aiGatewayStart", "Start service")}
            data-testid="ai-gateway-toggle-service"
            className={`inline-flex h-8 items-center gap-1.5 rounded-lg border px-3 text-xs font-medium shadow-sm transition disabled:opacity-50 ${
              running
                ? "border-destructive/30 bg-destructive/10 text-destructive hover:bg-destructive/20 active:bg-destructive/30"
                : "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 hover:bg-emerald-500/20 active:bg-emerald-500/30 dark:border-emerald-500/40 dark:bg-emerald-500/15 dark:text-emerald-400"
            }`}
          >
            {busy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : running ? (
              <Square className="h-3.5 w-3.5 fill-current" />
            ) : (
              <Play className="h-3.5 w-3.5 fill-current translate-x-0.5" />
            )}
            <span>{running ? t("aiGatewayStop", "Stop service") : t("aiGatewayStart", "Start service")}</span>
          </button>
        </div>
      </div>

      {/* 4 组核心数据指标卡：服务商健康、可用模型、今日请求量、今日消耗 */}
      <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
        {/* 指标卡 1：服务商健康度 */}
        <div
          data-testid="ai-gateway-metric-health"
          onClick={() => onSelectTab?.("providers")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("providers");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border p-3 transition-colors ${
            autoDisabledCount > 0
              ? "border-amber-500/30 bg-amber-500/5 hover:bg-amber-500/10"
              : "border-border/60 bg-muted/20 hover:border-border hover:bg-muted/30"
          } ${onSelectTab ? "cursor-pointer" : ""}`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("aiGatewayUpstreamHealth", "Provider health")}
            </span>
            <Server className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {totalProviders > 0 ? `${activeProviders}/${totalProviders}` : "0"}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("aiGatewayRunning", "Online")}
            </span>
          </div>
          <div className="mt-1 flex items-center justify-between gap-1 text-[11px]">
            {autoDisabledCount > 0 ? (
              <span className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
                <AlertTriangle className="h-3 w-3 shrink-0" />
                <span data-testid="ai-gateway-auto-disabled-count">{autoDisabledCount}</span>
                <span>{t("aiGatewayAutoDisabledCount", "Auto-disabled")}</span>
              </span>
            ) : (
              <div className="flex items-center gap-1 text-muted-foreground">
                <span
                  data-testid="ai-gateway-auto-disabled-count"
                  className="hidden"
                >
                  {autoDisabledCount}
                </span>
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                <span>
                  {totalProviders > 0
                    ? t("aiGatewayAllHealthy", "All online")
                    : t("aiGatewayNoProviders", "No upstream providers yet.")}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* 指标卡 2：聚合模型数 */}
        <div
          data-testid="ai-gateway-metric-models"
          onClick={() => onSelectTab?.("models")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("models");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 ${
            onSelectTab ? "cursor-pointer" : ""
          }`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("aiGatewayAggregatedModels", "Aggregated models")}
            </span>
            <Boxes className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {aggregatedModelsCount}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("aiGatewayAggregatedModelsCount", { count: aggregatedModelsCount, defaultValue: `${aggregatedModelsCount} models` }).replace(String(aggregatedModelsCount), "").trim()}
            </span>
          </div>
          <div className="mt-1 text-[11px] text-muted-foreground truncate">
            {t("aiGatewayAggregatedModelsDesc", "Unified local mappings")}
          </div>
        </div>

        {/* 指标卡 3：今日请求量（高频业务指标） */}
        <div
          data-testid="ai-gateway-metric-requests"
          onClick={() => onSelectTab?.("usage")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("usage");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border p-3 transition-colors ${
            todayFailedRequests > 0
              ? "border-amber-500/30 bg-amber-500/5 hover:bg-amber-500/10"
              : "border-border/60 bg-muted/20 hover:border-border hover:bg-muted/30"
          } ${onSelectTab ? "cursor-pointer" : ""}`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("aiGatewayTodayRequests", "Today's requests")}
            </span>
            <Activity className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {todayRequests}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("aiGatewayUsageRequests", "Requests")}
            </span>
          </div>
          <div className="mt-1 flex items-center gap-1 text-[11px] truncate">
            {todayRequests > 0 ? (
              todayFailedRequests > 0 ? (
                <span
                  className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400"
                  data-testid="ai-gateway-today-failed-requests"
                >
                  <AlertTriangle className="h-3 w-3 shrink-0" />
                  <span>
                    {t("aiGatewayTodayFailureCount", {
                      count: todayFailedRequests,
                      defaultValue: `${todayFailedRequests} failed`,
                    })}
                  </span>
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 font-medium text-emerald-600 dark:text-emerald-400">
                  <CheckCircle2 className="h-3 w-3 shrink-0" />
                  <span>{t("aiGatewayTodaySuccessRate", "100% success")}</span>
                </span>
              )
            ) : (
              <span className="text-muted-foreground">{t("aiGatewayUsageEmpty", "No usage records")}</span>
            )}
          </div>
        </div>

        {/* 指标卡 4：今日消耗与费用（高频消耗指标） */}
        <div
          data-testid="ai-gateway-metric-tokens"
          onClick={() => onSelectTab?.("usage")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("usage");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 ${
            onSelectTab ? "cursor-pointer" : ""
          }`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("aiGatewayTodayTokens", "Today's tokens")}
            </span>
            <Zap className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {formatGatewayTokens(todayTokens)}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("aiGatewayUsageTokens", "Tokens")}
            </span>
          </div>
          <div className="mt-1 text-[11px] text-muted-foreground truncate">
            {todayAmount > 0
              ? t("aiGatewayTodayCost", {
                  amount: formatUsageCurrency(todayAmount),
                  defaultValue: `Cost: ${formatUsageCurrency(todayAmount)}`,
                })
              : t("aiGatewayTodayNoCost", "No price configured")}
          </div>
        </div>
      </div>
    </section>
  );
}
