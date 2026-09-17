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
  Radio,
  Server,
  Square,
  TerminalSquare,
} from "lucide-react";
import {
  localBaseUrl,
  type FusionConfig,
  type FusionStatus,
  type FusionTerminalTarget,
} from "@/lib/apiFusion";

type RuntimeStatusCardProps = {
  status: FusionStatus | null;
  config: FusionConfig;
  busy: boolean;
  addressCopied: boolean;
  targets?: FusionTerminalTarget[];
  onSelectTab?: (tab: "providers" | "keys" | "terminals") => void;
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

  // 2. 聚合模型数（去重统计所有有效服务商的映射及默认模型）
  const aggregatedModelsCount = useMemo(() => {
    const set = new Set<string>();
    config.providers.forEach((p) => {
      if (!p.enabled || p.auto_disabled) return;
      if (p.default_model && p.default_model.trim()) {
        set.add(p.default_model.trim());
      }
      p.mappings.forEach((m) => {
        if (m.local_model && m.local_model.trim()) {
          set.add(m.local_model.trim());
        }
      });
    });
    return set.size;
  }, [config.providers]);

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
      data-testid="api-fusion-runtime"
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
                data-testid="api-fusion-runtime-state"
              >
                {running ? t("apiFusionRunning", "Running") : t("apiFusionStopped", "Stopped")}
              </span>
              <span className="text-muted-foreground/50">·</span>
              <span className="font-mono text-muted-foreground">
                {t("apiFusionPortValue", { port: config.port, defaultValue: `Port ${config.port}` })}
              </span>
              <span className="text-muted-foreground/50">·</span>
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
            title={running ? t("apiFusionStop", "Stop service") : t("apiFusionStart", "Start service")}
            aria-label={running ? t("apiFusionStop", "Stop service") : t("apiFusionStart", "Start service")}
            data-testid="api-fusion-toggle-service"
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
            <span>{running ? t("apiFusionStop", "Stop service") : t("apiFusionStart", "Start service")}</span>
          </button>
        </div>
      </div>

      {/* 4 组核心数据指标卡 */}
      <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
        {/* 指标卡 1：服务商健康度 */}
        <div
          data-testid="api-fusion-metric-health"
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
              {t("apiFusionUpstreamHealth", "Provider health")}
            </span>
            <Server className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {totalProviders > 0 ? `${activeProviders}/${totalProviders}` : "0"}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("apiFusionRunning", "Online")}
            </span>
          </div>
          <div className="mt-1 flex items-center justify-between gap-1 text-[11px]">
            {autoDisabledCount > 0 ? (
              <span className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
                <AlertTriangle className="h-3 w-3 shrink-0" />
                <span data-testid="api-fusion-auto-disabled-count">{autoDisabledCount}</span>
                <span>{t("apiFusionAutoDisabledCount", "Auto-disabled")}</span>
              </span>
            ) : (
              <div className="flex items-center gap-1 text-muted-foreground">
                <span
                  data-testid="api-fusion-auto-disabled-count"
                  className="hidden"
                >
                  {autoDisabledCount}
                </span>
                <span>
                  {totalProviders > 0
                    ? t("apiFusionAllHealthy", "All online")
                    : t("apiFusionNoProviders", "No upstream providers yet.")}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* 指标卡 2：聚合模型数 */}
        <div
          data-testid="api-fusion-metric-models"
          onClick={() => onSelectTab?.("providers")}
          role={onSelectTab ? "button" : undefined}
          tabIndex={onSelectTab ? 0 : undefined}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              onSelectTab?.("providers");
            }
          }}
          className={`group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 ${
            onSelectTab ? "cursor-pointer" : ""
          }`}
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium">
              {t("apiFusionAggregatedModels", "Aggregated models")}
            </span>
            <Boxes className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {aggregatedModelsCount}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("apiFusionAggregatedModelsCount", { count: aggregatedModelsCount, defaultValue: `${aggregatedModelsCount} models` }).replace(String(aggregatedModelsCount), "").trim()}
            </span>
          </div>
          <div className="mt-1 text-[11px] text-muted-foreground truncate">
            {t("apiFusionAggregatedModelsDesc", "Unified local mappings")}
          </div>
        </div>

        {/* 指标卡 3：本地有效密钥 */}
        <div
          data-testid="api-fusion-metric-keys"
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
              {t("apiFusionActiveKeys", "Active keys")}
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
            {t("apiFusionKeyCount", "Local keys")}
          </div>
        </div>

        {/* 指标卡 4：AI 终端联动 */}
        <div
          data-testid="api-fusion-metric-terminals"
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
              {t("apiFusionTerminalIntegrationTitle", "AI terminals")}
            </span>
            <TerminalSquare className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {totalTargets > 0 ? `${syncedTargets}/${totalTargets}` : "0"}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("apiFusionTerminalsSynced", { synced: syncedTargets, total: totalTargets, defaultValue: `${syncedTargets}/${totalTargets} synced` }).replace(`${syncedTargets}/${totalTargets}`, "").trim()}
            </span>
          </div>
          <div className="mt-1 text-[11px] truncate">
            {pendingTargets > 0 ? (
              <span className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
                <span className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse" />
                <span>
                  {t("apiFusionTerminalPendingNotice", { count: pendingTargets, defaultValue: `${pendingTargets} pending sync` })}
                </span>
              </span>
            ) : (
              <span className="inline-flex items-center gap-1 text-muted-foreground">
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                <span>
                  {totalTargets > 0
                    ? t("apiFusionTerminalAllSyncedNotice", "All synced")
                    : t("apiFusionTerminalSync", "AI terminal integration")}
                </span>
              </span>
            )}
          </div>
        </div>
      </div>

      {/* 本地端点（Base URL）快捷复制栏 */}
      <div className="flex flex-col gap-2 rounded-lg border bg-muted/15 px-3 py-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <div className="flex shrink-0 items-center gap-1.5 text-xs font-semibold text-muted-foreground">
            <Radio className="h-3.5 w-3.5 text-indigo-500" />
            <span>{t("apiFusionLocalAddress", "Local API address")}</span>
          </div>
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <code
              className={`min-w-0 truncate rounded bg-background/80 px-2 py-0.5 font-mono text-xs font-medium select-all ${
                running ? "text-foreground" : "text-muted-foreground/70"
              }`}
              title={address}
              data-testid="api-fusion-local-address"
            >
              {address}
            </code>
            {!running && (
              <span className="hidden text-[11px] text-muted-foreground sm:inline">
                ({t("apiFusionServiceOfflineNotice", "Service is stopped. Local API is currently unreachable.")})
              </span>
            )}
          </div>
        </div>

        <div className="flex shrink-0 items-center justify-end gap-2">
          {addressCopied && (
            <span className="text-xs font-medium text-emerald-600 dark:text-emerald-400">
              {t("apiFusionCopied", "Copied")}
            </span>
          )}
          <button
            type="button"
            onClick={onCopyAddress}
            aria-label={t("apiFusionCopyAddress", "Copy local API address")}
            title={t("apiFusionCopyAddress", "Copy local API address")}
            className="inline-flex items-center gap-1.5 rounded-md border bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground shadow-xs transition hover:bg-muted hover:text-foreground"
          >
            {addressCopied ? (
              <Check className="h-3.5 w-3.5 text-emerald-600" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
            <span>{t("apiFusionCopyAddress", "Copy local API address")}</span>
          </button>
        </div>
      </div>
    </section>
  );
}
