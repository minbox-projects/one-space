import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  ArrowRightLeft,
  Filter,
  Globe,
  Pencil,
  Plus,
  RotateCcw,
  Server,
  Trash2,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  formatGatewayTimestamp,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";

export type ProviderStatusFilter = "all" | "enabled" | "disabled";

type UpstreamProviderListProps = {
  providers: GatewayUpstreamProvider[];
  selectedProviderId: string | null;
  busy: boolean;
  onSelect: (providerId: string) => void;
  onToggleEnabled: (provider: GatewayUpstreamProvider, enabled: boolean) => void;
  onReenable: (providerId: string) => void;
  onAdd: () => void;
  onDelete?: (providerId: string) => void;
  /** Optional content rendered above the provider list (e.g. the template area). */
  templateSection?: ReactNode;
};

export function UpstreamProviderList({
  providers,
  selectedProviderId,
  busy,
  onSelect,
  onToggleEnabled,
  onReenable,
  onAdd,
  onDelete,
  templateSection,
}: UpstreamProviderListProps) {
  const { t } = useTranslation();
  const [statusFilter, setStatusFilter] = useState<ProviderStatusFilter>("all");

  const counts = useMemo(() => {
    let enabled = 0;
    let disabled = 0;
    for (const p of providers) {
      if (p.enabled) {
        enabled++;
      } else {
        disabled++;
      }
    }
    return {
      all: providers.length,
      enabled,
      disabled,
    };
  }, [providers]);

  const filteredProviders = useMemo(() => {
    if (statusFilter === "enabled") {
      return providers.filter((p) => p.enabled);
    }
    if (statusFilter === "disabled") {
      return providers.filter((p) => !p.enabled);
    }
    return providers;
  }, [providers, statusFilter]);

  return (
    <section className="space-y-3.5" data-testid="api-gateway-providers">
      {templateSection}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <Server className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("apiGatewayProviders", "Upstream providers")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
              {providers.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "apiGatewayProvidersDesc",
              "Manage remote AI provider endpoints, protocols, and model route mappings.",
            )}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          {providers.length > 0 ? (
            <div
              role="group"
              aria-label={t("apiGatewayFilterAria", "Filter providers by status")}
              data-testid="api-gateway-provider-status-filter"
              className="inline-flex items-center rounded-lg border border-border/50 bg-muted/60 p-0.5 text-xs shadow-inner"
            >
              <button
                type="button"
                data-testid="filter-status-all"
                aria-pressed={statusFilter === "all"}
                onClick={() => setStatusFilter("all")}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  statusFilter === "all"
                    ? "bg-background text-foreground shadow-sm border border-border/40"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                }`}
              >
                <span>{t("apiGatewayFilterAll", "All")}</span>
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                    statusFilter === "all"
                      ? "bg-muted text-foreground"
                      : "bg-background/60 text-muted-foreground"
                  }`}
                >
                  {counts.all}
                </span>
              </button>

              <button
                type="button"
                data-testid="filter-status-enabled"
                aria-pressed={statusFilter === "enabled"}
                onClick={() => setStatusFilter("enabled")}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  statusFilter === "enabled"
                    ? "bg-background text-foreground shadow-sm border border-border/40"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                }`}
              >
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                <span>{t("apiGatewayFilterEnabled", "Enabled")}</span>
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                    statusFilter === "enabled"
                      ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 font-semibold"
                      : "bg-background/60 text-muted-foreground"
                  }`}
                >
                  {counts.enabled}
                </span>
              </button>

              <button
                type="button"
                data-testid="filter-status-disabled"
                aria-pressed={statusFilter === "disabled"}
                onClick={() => setStatusFilter("disabled")}
                className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  statusFilter === "disabled"
                    ? "bg-background text-foreground shadow-sm border border-border/40"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                }`}
              >
                <span className="h-1.5 w-1.5 rounded-full bg-rose-500" />
                <span>{t("apiGatewayFilterDisabled", "Disabled")}</span>
                <span
                  className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                    statusFilter === "disabled"
                      ? "bg-rose-500/15 text-rose-700 dark:text-rose-300 font-semibold"
                      : "bg-background/60 text-muted-foreground"
                  }`}
                >
                  {counts.disabled}
                </span>
              </button>
            </div>
          ) : null}

          <button
            type="button"
            onClick={onAdd}
            disabled={busy}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("apiGatewayAddProvider", "Add provider")}
          </button>
        </div>
      </div>

      {providers.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <Server className="h-5 w-5" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("apiGatewayNoProviders", "No upstream providers yet.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "apiGatewayNoProvidersGuide",
              "Add upstream providers like OpenAI, DeepSeek, or any OpenAI-compatible API to start proxying requests.",
            )}
          </p>
          <button
            type="button"
            onClick={onAdd}
            disabled={busy}
            className="mt-3.5 inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
          >
            <Plus className="h-3 w-3" />
            {t("apiGatewayAddProvider", "Add provider")}
          </button>
        </div>
      ) : filteredProviders.length === 0 ? (
        <div
          data-testid="api-gateway-providers-filter-empty"
          className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/40 px-6 py-10 text-center"
        >
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <Filter className="h-5 w-5 opacity-70" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("apiGatewayNoMatchingProviders", "No matching upstream providers")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "apiGatewayNoMatchingProvidersGuide",
              "No upstream providers match the selected status filter.",
            )}
          </p>
          <button
            type="button"
            onClick={() => setStatusFilter("all")}
            className="mt-3.5 inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
          >
            <RotateCcw className="h-3 w-3" />
            {t("apiGatewayClearFilter", "Show all providers")}
          </button>
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {filteredProviders.map((provider) => {
            const isSelected = provider.id === selectedProviderId;
            const isAutoDisabled = Boolean(provider.auto_disabled);
            const isEnabled = provider.enabled;
            const disabledAt = formatGatewayTimestamp(provider.disabled_at);
            const mappingCount = provider.mappings?.length ?? 0;
            const isChatProtocol = provider.protocol !== "responses";

            return (
              <div
                key={provider.id}
                data-testid={`api-gateway-provider-${provider.id}`}
                className={`group relative flex flex-col justify-between rounded-xl border bg-card p-3.5 shadow-sm transition-all hover:border-primary/40 hover:shadow-md ${
                  isSelected ? "ring-2 ring-primary/20 border-primary" : ""
                } ${!isEnabled ? "opacity-85 hover:opacity-100" : ""}`}
              >
                {/* 头部：名称、协议徽章、启用开关 */}
                <div>
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      <button
                        type="button"
                        onClick={() => onSelect(provider.id)}
                        className="truncate text-left text-sm font-semibold text-foreground hover:text-primary transition-colors block w-full leading-5"
                        title={provider.name}
                      >
                        {provider.name}
                      </button>
                      <div className="mt-1 flex flex-wrap items-center gap-1.5">
                        <span
                          className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-4 ${
                            isChatProtocol
                              ? "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                              : "bg-purple-500/10 text-purple-600 dark:text-purple-400"
                          }`}
                        >
                          {isChatProtocol ? "Chat" : "Responses"}
                        </span>
                        {provider.default_model ? (
                          <span
                            className="inline-flex max-w-[150px] truncate rounded-md border bg-background px-1.5 py-0.5 font-mono text-[11px] font-medium leading-4 text-muted-foreground"
                            title={`Default model: ${provider.default_model}`}
                          >
                            {provider.default_model}
                          </span>
                        ) : null}
                      </div>
                    </div>

                    <div className="flex items-center gap-1.5 shrink-0">
                      <Switch
                        aria-label={t("apiGatewayToggleProviderAria", {
                          name: provider.name,
                          defaultValue: `Enable provider ${provider.name}`,
                        })}
                        checked={provider.enabled}
                        disabled={busy}
                        onCheckedChange={(checked) => onToggleEnabled(provider, checked)}
                      />
                    </div>
                  </div>

                  {/* 中间信息：Base URL 与映射数量 */}
                  <div className="mt-2.5 space-y-1">
                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Globe className="h-3.5 w-3.5 shrink-0 opacity-70" />
                      <span className="truncate font-mono" title={provider.base_url}>
                        {provider.base_url}
                      </span>
                    </div>

                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <ArrowRightLeft className="h-3.5 w-3.5 shrink-0 opacity-70" />
                      <span>
                        {mappingCount > 0
                          ? t("apiGatewayMappingCount", {
                              count: mappingCount,
                              defaultValue: `${mappingCount} mappings configured`,
                            })
                          : t("apiGatewayNoMappingsShort", "No mappings (default routing)")}
                      </span>
                    </div>
                  </div>

                  {/* 自动禁用警告栏 */}
                  {provider.auto_disabled ? (
                    <div
                      className="mt-2.5 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400"
                      data-testid={`api-gateway-auto-disabled-${provider.id}`}
                    >
                      <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-1 font-medium">
                          <AlertTriangle className="h-3 w-3 shrink-0" />
                          <span>{t("apiGatewayAutoDisabled", "Auto-disabled")}</span>
                        </div>
                        {provider.disabled_reason ? (
                          <div className="truncate text-[10px]">
                            {t("apiGatewayDisabledReason", {
                              reason: provider.disabled_reason,
                              defaultValue: `Reason: ${provider.disabled_reason}`,
                            })}
                          </div>
                        ) : null}
                        {disabledAt ? (
                          <div className="text-[10px] opacity-80">
                            {t("apiGatewayDisabledAt", {
                              time: disabledAt,
                              defaultValue: `Disabled at ${disabledAt}`,
                            })}
                          </div>
                        ) : null}
                      </div>
                      <button
                        type="button"
                        onClick={() => onReenable(provider.id)}
                        disabled={busy}
                        className="inline-flex h-6 items-center gap-1 rounded-md border border-amber-500/40 bg-background px-2 text-[10px] font-medium shadow-sm transition hover:bg-amber-500/20 disabled:opacity-50"
                      >
                        <RotateCcw className="h-2.5 w-2.5" />
                        {t("apiGatewayReenable", "Re-enable")}
                      </button>
                    </div>
                  ) : null}
                </div>

                {/* 卡片底栏操作按钮 */}
                <div className="mt-3 flex items-center justify-between border-t pt-2.5">
                  {isAutoDisabled ? (
                    <span
                      data-testid={`api-gateway-status-badge-${provider.id}`}
                      className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/25 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400"
                    >
                      <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
                      <span>{t("apiGatewayAutoDisabled", "Auto-disabled")}</span>
                    </span>
                  ) : isEnabled ? (
                    <span
                      data-testid={`api-gateway-status-badge-${provider.id}`}
                      className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:text-emerald-300"
                    >
                      <span className="relative flex h-1.5 w-1.5">
                        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75 duration-1000" />
                        <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-emerald-500" />
                      </span>
                      <span>{t("apiGatewayEnabled", "Enabled")}</span>
                    </span>
                  ) : (
                    <span
                      data-testid={`api-gateway-status-badge-${provider.id}`}
                      className="inline-flex items-center gap-1.5 rounded-full border border-rose-500/25 bg-rose-500/10 px-2 py-0.5 text-[11px] font-medium text-rose-700 dark:text-rose-400"
                    >
                      <span className="h-1.5 w-1.5 rounded-full bg-rose-500" />
                      <span>{t("apiGatewayDisabled", "Disabled")}</span>
                    </span>
                  )}
                  <div className="flex items-center gap-1">
                    {onDelete ? (
                      <button
                        type="button"
                        onClick={() => onDelete(provider.id)}
                        disabled={busy}
                        aria-label={t("apiGatewayDeleteProviderAria", {
                          name: provider.name,
                          defaultValue: `Delete provider ${provider.name}`,
                        })}
                        title={t("apiGatewayDelete", "Delete")}
                        className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    ) : null}
                    <button
                      type="button"
                      onClick={() => onSelect(provider.id)}
                      className="inline-flex h-7 items-center gap-1 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted"
                    >
                      <Pencil className="h-3 w-3" />
                      {t("apiGatewayEdit", "Edit")}
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
