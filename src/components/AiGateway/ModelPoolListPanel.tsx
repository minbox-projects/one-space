import { memo, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Filter, Layers, Plus, RotateCcw } from "lucide-react";
import type {
  GatewayModelPool,
  GatewayModelPoolStatus,
  GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { ModelPoolCard } from "./ModelPoolCard";

export type ModelPoolStatusFilter = "all" | "enabled" | "disabled";

export type ModelPoolListPanelProps = {
  modelPools: GatewayModelPool[];
  providers: GatewayUpstreamProvider[];
  poolStatuses?: GatewayModelPoolStatus[];
  busy?: boolean;
  isActive?: boolean;
  onAddPool?: () => void;
  onTogglePoolEnabled?: (poolId: string, enabled: boolean) => void;
  onEditPool?: (pool: GatewayModelPool) => void;
  onDeletePool?: (poolId: string) => void;
  onOpenPoolLogs?: (model: string) => void;
};

function ModelPoolListPanelComponent({
  modelPools,
  providers,
  poolStatuses = [],
  busy = false,
  isActive = true,
  onAddPool,
  onTogglePoolEnabled,
  onEditPool,
  onDeletePool,
  onOpenPoolLogs,
}: ModelPoolListPanelProps) {
  const { t } = useTranslation();
  const [statusFilter, setStatusFilter] = useState<ModelPoolStatusFilter>("all");

  const poolCounts = useMemo(() => {
    if (!isActive) return { all: 0, enabled: 0, disabled: 0 };
    let enabled = 0;
    let disabled = 0;
    for (const pool of modelPools) {
      if (pool.enabled) {
        enabled++;
      } else {
        disabled++;
      }
    }
    return {
      all: modelPools.length,
      enabled,
      disabled,
    };
  }, [isActive, modelPools]);

  const filteredModelPools = useMemo(() => {
    if (!isActive) return [];
    return modelPools.filter((pool) => {
      if (statusFilter === "enabled" && !pool.enabled) return false;
      if (statusFilter === "disabled" && pool.enabled) return false;
      return true;
    });
  }, [isActive, modelPools, statusFilter]);

  // While inactive the visited panel stays mounted (CSS-hidden by its tab
  // container) for its filter state; the card grid above is empty and cheap.

  return (
    <section className="space-y-4" data-testid="ai-gateway-model-pools-panel">
      {/* 头部标题与全局操作 */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between pb-1 border-b border-border/40">
        <div className="flex items-center gap-2.5">
          <div className="rounded-lg p-2 bg-primary/10 text-primary">
            <Layers className="h-5 w-5" />
          </div>
          <div className="space-y-0.5">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold text-foreground">
                {t("aiGatewayModelPoolsTab", "Model pools")}
              </h2>
              <span
                data-testid="ai-gateway-model-pools-count"
                className="rounded-full bg-muted px-2 py-0.5 text-xs font-semibold text-muted-foreground"
              >
                {modelPools.length}
              </span>
            </div>
            <p className="text-xs text-muted-foreground">
              {t(
                "aiGatewayModelPoolsDesc",
                "Aggregate upstream models into unified public model pools with failover.",
              )}
            </p>
          </div>
        </div>

        {onAddPool ? (
          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={onAddPool}
              disabled={busy}
              data-testid="ai-gateway-add-model-pool"
              className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
            >
              <Plus className="h-3.5 w-3.5" />
              <span>{t("aiGatewayModelPoolCreate", "Add model pool")}</span>
            </button>
          </div>
        ) : null}
      </div>

      {/* 状态筛选工具条 */}
      {modelPools.length > 0 ? (
        <div
          data-testid="ai-gateway-model-pools-toolbar"
          className="flex flex-wrap items-center justify-between gap-2.5 pt-0.5"
        >
          <div className="flex flex-wrap items-center gap-2">
            <div
              role="group"
              aria-label={t("aiGatewayFilterAria", "Filter model pools by status")}
              data-testid="ai-gateway-pool-status-filter"
              className="inline-flex items-center rounded-lg border border-border/50 bg-muted/60 p-0.5 text-xs shadow-inner"
            >
              <button
                type="button"
                data-testid="filter-pool-status-all"
                aria-pressed={statusFilter === "all"}
                onClick={() => setStatusFilter("all")}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  statusFilter === "all"
                    ? "bg-background text-foreground shadow-sm border border-border/40"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                }`}
              >
                <span>{t("aiGatewayFilterAll", "All")}</span>
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                    statusFilter === "all"
                      ? "bg-muted text-foreground"
                      : "bg-background/60 text-muted-foreground"
                  }`}
                >
                  {poolCounts.all}
                </span>
              </button>
              <button
                type="button"
                data-testid="filter-pool-status-enabled"
                aria-pressed={statusFilter === "enabled"}
                onClick={() => setStatusFilter("enabled")}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  statusFilter === "enabled"
                    ? "bg-background text-foreground shadow-sm border border-border/40"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                }`}
              >
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                <span>{t("aiGatewayFilterEnabled", "Enabled")}</span>
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                    statusFilter === "enabled"
                      ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 font-semibold"
                      : "bg-background/60 text-muted-foreground"
                  }`}
                >
                  {poolCounts.enabled}
                </span>
              </button>
              <button
                type="button"
                data-testid="filter-pool-status-disabled"
                aria-pressed={statusFilter === "disabled"}
                onClick={() => setStatusFilter("disabled")}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  statusFilter === "disabled"
                    ? "bg-background text-foreground shadow-sm border border-border/40"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                }`}
              >
                <span className="h-1.5 w-1.5 rounded-full bg-rose-500" />
                <span>{t("aiGatewayFilterDisabled", "Disabled")}</span>
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                    statusFilter === "disabled"
                      ? "bg-rose-500/15 text-rose-700 dark:text-rose-300 font-semibold"
                      : "bg-background/60 text-muted-foreground"
                  }`}
                >
                  {poolCounts.disabled}
                </span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* 内容卡片区 */}
      {modelPools.length === 0 ? (
        <div
          data-testid="ai-gateway-model-pools-empty"
          className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-12 text-center"
        >
          <div className="rounded-full bg-muted/60 p-3 text-muted-foreground">
            <Layers className="h-6 w-6" />
          </div>
          <h4 className="mt-3 text-sm font-medium text-foreground">
            {t("aiGatewayNoModelPools", "No model pools yet.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "aiGatewayNoModelPoolsGuide",
              "Create a model pool to combine models from different providers into a single virtual model.",
            )}
          </p>
          {onAddPool ? (
            <button
              type="button"
              onClick={onAddPool}
              disabled={busy}
              className="mt-4 inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-3.5 text-xs font-medium shadow-xs transition hover:bg-muted"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("aiGatewayModelPoolCreate", "Add model pool")}
            </button>
          ) : null}
        </div>
      ) : filteredModelPools.length === 0 ? (
        <div
          data-testid="ai-gateway-model-pools-filter-empty"
          className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/40 px-6 py-12 text-center"
        >
          <div className="rounded-full bg-muted/60 p-3 text-muted-foreground">
            <Filter className="h-6 w-6 opacity-70" />
          </div>
          <h4 className="mt-3 text-sm font-medium text-foreground">
            {t("aiGatewayNoMatchingModelPools", "No matching model pools")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "aiGatewayNoMatchingModelPoolsGuide",
              "No model pools match the selected status filter.",
            )}
          </p>
          <button
            type="button"
            onClick={() => setStatusFilter("all")}
            className="mt-4 inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-3.5 text-xs font-medium shadow-xs transition hover:bg-muted"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("aiGatewayClearFilter", "Show all")}
          </button>
        </div>
      ) : (
        <div
          data-testid="ai-gateway-model-pools"
          className="grid gap-3 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3 items-start"
        >
          {filteredModelPools.map((pool) => (
            <ModelPoolCard
              key={pool.id}
              pool={pool}
              providers={providers}
              status={poolStatuses.find((item) => item.id === pool.id) ?? null}
              busy={busy}
              onToggleEnabled={(poolId, enabled) =>
                onTogglePoolEnabled?.(poolId, enabled)
              }
              onEdit={(target) => onEditPool?.(target)}
              onDelete={(poolId) => onDeletePool?.(poolId)}
              onOpenLogs={(model) => onOpenPoolLogs?.(model)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

export const ModelPoolListPanel = memo(ModelPoolListPanelComponent);
