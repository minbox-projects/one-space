import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  Boxes,
  Check,
  Copy,
  KeyRound,
  Loader2,
  Play,
  Server,
  Square,
  TerminalSquare,
} from "lucide-react";
import {
  aggregateModels,
  localBaseUrl,
  type GatewayConfig,
  type GatewayStatus,
  type GatewayTerminalTarget,
} from "@/lib/apiGateway";

type RuntimeStatusCardProps = {
  status: GatewayStatus | null;
  config: GatewayConfig;
  busy: boolean;
  addressCopied: boolean;
  targets?: GatewayTerminalTarget[];
  onSelectTab?: (tab: "providers" | "keys" | "terminals") => void;
  onShowModels?: () => void;
  onStart: () => void;
  onStop: () => void;
  onCopyAddress: () => void;
};

export function RuntimeStatusCard({
  status,
  config,
  busy,
  addressCopied,
  targets = [],
  onSelectTab,
  onShowModels,
  onStart,
  onStop,
  onCopyAddress,
}: RuntimeStatusCardProps) {
  const { t } = useTranslation();
  const running = Boolean(status?.running);
  const address = localBaseUrl(config.port);

  // 1. 服务商与健康度
  const totalProviders = status?.provider_count ?? config.providers.length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;
  const activeProviders = useMemo(() => {
    return config.providers.filter((p) => p.enabled && !p.auto_disabled).length;
  }, [config.providers]);

  // 2. 聚合模型数（与聚合模型弹框内容严格一致）
  const aggregatedModelsCount = useMemo(
    () => aggregateModels(config.providers).length,
    [config.providers],
  );

  // 3. 本地有效密钥
  const totalKeys = status?.key_count ?? config.keys.length;
  const activeKeys = useMemo(() => {
    return config.keys.filter((k) => k.enabled).length;
  }, [config.keys]);

  // 4. 终端同步就绪度
  const totalTargets = targets.length;
  const syncedTargets = useMemo(() => {
    return targets.filter((t) => t.synced).length;
  }, [targets]);
  const pendingTargets = useMemo(() => {
    return targets.filter((t) => t.pending_sync).length;
  }, [targets]);

  return (
    <section
      className="space-y-3 rounded-xl border bg-card p-4 shadow-sm"
      data-testid="api-gateway-runtime"
    >
      {/* 顶部主状态栏与启停按钮 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
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
                data-testid="api-gateway-runtime-state"
              >
                {running ? t("apiGatewayRunning", "Running") : t("apiGatewayStopped", "Stopped")}
              </span>
              <span className="text-muted-foreground/40">·</span>
              <div className="flex items-center gap-1">
                <code
                  className={`font-mono text-xs font-medium select-all ${
                    running ? "text-foreground" : "text-muted-foreground/70"
                  }`}
                  title={address}
                  data-testid="api-gateway-local-address"
                >
                  {address}
                </code>
                <button
                  type="button"
                  onClick={onCopyAddress}
                  aria-label={t("apiGatewayCopyAddress", "Copy local API address")}
                  title={
                    addressCopied
                      ? t("apiGatewayCopied", "Copied")
                      : t("apiGatewayCopyAddress", "Copy local API address")
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
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={running ? onStop : onStart}
            disabled={busy}
            title={running ? t("apiGatewayStop", "Stop service") : t("apiGatewayStart", "Start service")}
            aria-label={running ? t("apiGatewayStop", "Stop service") : t("apiGatewayStart", "Start service")}
            data-testid="api-gateway-toggle-service"
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
            <span>{running ? t("apiGatewayStop", "Stop service") : t("apiGatewayStart", "Start service")}</span>
          </button>
        </div>
      </div>

      {/* 4 组核心数据指标卡 */}
      <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
        {/* 指标卡 1：服务商健康度 */}
        <div
          data-testid="api-gateway-metric-health"
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
              {t("apiGatewayUpstreamHealth", "Provider health")}
            </span>
            <Server className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {totalProviders > 0 ? `${activeProviders}/${totalProviders}` : "0"}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("apiGatewayRunning", "Online")}
            </span>
          </div>
          <div className="mt-1 flex items-center justify-between gap-1 text-[11px]">
            {autoDisabledCount > 0 ? (
              <span className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
                <AlertTriangle className="h-3 w-3 shrink-0" />
                <span data-testid="api-gateway-auto-disabled-count">{autoDisabledCount}</span>
                <span>{t("apiGatewayAutoDisabledCount", "Auto-disabled")}</span>
              </span>
            ) : (
              <div className="flex items-center gap-1 text-muted-foreground">
                <span
                  data-testid="api-gateway-auto-disabled-count"
                  className="hidden"
                >
                  {autoDisabledCount}
                </span>
                <span>
                  {totalProviders > 0
                    ? t("apiGatewayAllHealthy", "All online")
                    : t("apiGatewayNoProviders", "No upstream providers yet.")}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* 指标卡 2：聚合模型数 */}
        <div
          data-testid="api-gateway-metric-models"
          onClick={() => onShowModels?.()}
          role={onShowModels ? "button" : undefined}
          tabIndex={onShowModels ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onShowModels?.();
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 ${
            onShowModels ? "cursor-pointer" : ""
          }`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("apiGatewayAggregatedModels", "Aggregated models")}
            </span>
            <Boxes className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {aggregatedModelsCount}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("apiGatewayAggregatedModelsCount", { count: aggregatedModelsCount, defaultValue: `${aggregatedModelsCount} models` }).replace(String(aggregatedModelsCount), "").trim()}
            </span>
          </div>
          <div className="mt-1 text-[11px] text-muted-foreground truncate">
            {t("apiGatewayAggregatedModelsDesc", "Unified local mappings")}
          </div>
        </div>

        {/* 指标卡 3：本地有效密钥 */}
        <div
          data-testid="api-gateway-metric-keys"
          onClick={() => onSelectTab?.("keys")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("keys");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 ${
            onSelectTab ? "cursor-pointer" : ""
          }`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("apiGatewayActiveKeys", "Active keys")}
            </span>
            <KeyRound className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {activeKeys}
            </span>
            <span className="text-xs text-muted-foreground">
              / {totalKeys}
            </span>
          </div>
          <div className="mt-1 text-[11px] text-muted-foreground truncate">
            {t("apiGatewayKeyCount", "Local keys")}
          </div>
        </div>

        {/* 指标卡 4：AI 终端联动 */}
        <div
          data-testid="api-gateway-metric-terminals"
          onClick={() => onSelectTab?.("terminals")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("terminals");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border p-3 transition-colors ${
            pendingTargets > 0
              ? "border-amber-500/30 bg-amber-500/5 hover:bg-amber-500/10"
              : "border-border/60 bg-muted/20 hover:border-border hover:bg-muted/30"
          } ${onSelectTab ? "cursor-pointer" : ""}`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("apiGatewayTerminalIntegrationTitle", "AI terminals")}
            </span>
            <TerminalSquare className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {totalTargets > 0 ? `${syncedTargets}/${totalTargets}` : "0"}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("apiGatewayTerminalsSynced", { synced: syncedTargets, total: totalTargets, defaultValue: `${syncedTargets}/${totalTargets} synced` }).replace(`${syncedTargets}/${totalTargets}`, "").trim()}
            </span>
          </div>
          <div className="mt-1 text-[11px] truncate">
            {pendingTargets > 0 ? (
              <span className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
                <span className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse" />
                <span>
                  {t("apiGatewayTerminalPendingNotice", { count: pendingTargets, defaultValue: `${pendingTargets} pending sync` })}
                </span>
              </span>
            ) : (
              <span className="inline-flex items-center gap-1 text-muted-foreground">
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                <span>
                  {totalTargets > 0
                    ? t("apiGatewayTerminalAllSyncedNotice", "All synced")
                    : t("apiGatewayTerminalSync", "AI terminal integration")}
                </span>
              </span>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
