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
  Sparkles,
  Trash2,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { ProviderTemplateAvatar } from "./ProviderTemplateIcon";
import {
  formatGatewayTimestamp,
  isMappingDeprecated,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";

export type ProviderStatusFilter = "all" | "enabled" | "disabled";

type UpstreamProviderListProps = {
  providers: GatewayUpstreamProvider[];
  /** Loaded template views, used to resolve a provider's bound template icon and retired hint. */
  templates?: GatewayProviderTemplateView[];
  selectedProviderId: string | null;
  busy: boolean;
  onSelect: (providerId: string) => void;
  onToggleEnabled: (provider: GatewayUpstreamProvider, enabled: boolean) => void;
  onAdd: () => void;
  onDelete?: (providerId: string) => void;
  onManageTemplates?: () => void;
  /** Optional content rendered above the provider list (e.g. the template area). */
  templateSection?: ReactNode;
};

export function UpstreamProviderList({
  providers,
  templates,
  selectedProviderId,
  busy,
  onSelect,
  onToggleEnabled,
  onAdd,
  onDelete,
  onManageTemplates,
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
    <section className="space-y-3.5" data-testid="ai-gateway-providers">
      {templateSection}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <Server className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("aiGatewayProviders", "Upstream providers")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
              {providers.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "aiGatewayProvidersDesc",
              "Manage remote AI provider endpoints, protocols, and model route mappings.",
            )}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          {providers.length > 0 ? (
            <div
              role="group"
              aria-label={t("aiGatewayFilterAria", "Filter providers by status")}
              data-testid="ai-gateway-provider-status-filter"
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
                <span>{t("aiGatewayFilterAll", "All")}</span>
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
                <span>{t("aiGatewayFilterEnabled", "Enabled")}</span>
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
                <span>{t("aiGatewayFilterDisabled", "Disabled")}</span>
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

          {onManageTemplates ? (
            <button
              type="button"
              onClick={onManageTemplates}
              data-testid="ai-gateway-manage-templates"
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/70 bg-background px-3 text-xs font-medium text-foreground shadow-xs transition hover:bg-muted active:bg-muted/80"
            >
              <Sparkles className="h-3.5 w-3.5 text-primary" />
              <span>{t("aiGatewayProviderTemplatesButton", "Provider templates")}</span>
            </button>
          ) : null}

          <button
            type="button"
            onClick={onAdd}
            disabled={busy}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("aiGatewayAddProvider", "Add provider")}
          </button>
        </div>
      </div>

      {providers.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <Server className="h-5 w-5" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("aiGatewayNoProviders", "No upstream providers yet.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "aiGatewayNoProvidersGuide",
              "Add upstream providers like OpenAI, DeepSeek, or any OpenAI-compatible API to start proxying requests.",
            )}
          </p>
          <div className="mt-3.5 flex items-center gap-2">
            <button
              type="button"
              onClick={onAdd}
              disabled={busy}
              className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
            >
              <Plus className="h-3 w-3" />
              {t("aiGatewayAddProvider", "Add provider")}
            </button>
            {onManageTemplates ? (
              <button
                type="button"
                onClick={onManageTemplates}
                data-testid="ai-gateway-manage-templates-empty"
                className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border border-border/70 bg-background px-3 text-xs font-medium text-muted-foreground shadow-sm transition hover:bg-muted hover:text-foreground"
              >
                <Sparkles className="h-3 w-3 text-primary" />
                {t("aiGatewayProviderTemplatesButton", "Provider templates")}
              </button>
            ) : null}
          </div>
        </div>
      ) : filteredProviders.length === 0 ? (
        <div
          data-testid="ai-gateway-providers-filter-empty"
          className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/40 px-6 py-10 text-center"
        >
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <Filter className="h-5 w-5 opacity-70" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("aiGatewayNoMatchingProviders", "No matching upstream providers")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "aiGatewayNoMatchingProvidersGuide",
              "No upstream providers match the selected status filter.",
            )}
          </p>
          <button
            type="button"
            onClick={() => setStatusFilter("all")}
            className="mt-3.5 inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
          >
            <RotateCcw className="h-3 w-3" />
            {t("aiGatewayClearFilter", "Show all providers")}
          </button>
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {filteredProviders.map((provider) => {
            const isSelected = provider.id === selectedProviderId;
            const isEnabled = provider.enabled;
            const disabledAt = formatGatewayTimestamp(provider.disabled_at);
            const mappingCount = provider.mappings?.length ?? 0;
            const autoDisabledMappings = (provider.mappings ?? []).filter(
              (mapping) => mapping.auto_disabled === true,
            );
            const isChatProtocol = provider.protocol !== "responses";
            const templateView = provider.template_id
              ? templates?.find(
                  (view) => view.template.id === provider.template_id,
                )
              : undefined;
            const retiredMappings = templateView
              ? (provider.mappings ?? []).filter(
                  (mapping) =>
                    mapping.enabled === false &&
                    isMappingDeprecated(mapping, templateView.template),
                )
              : [];

            return (
              <div
                key={provider.id}
                data-testid={`ai-gateway-provider-${provider.id}`}
                className={`group relative flex flex-col justify-between rounded-xl border bg-card p-3.5 shadow-sm transition-all hover:border-primary/40 hover:shadow-md ${
                  isSelected ? "ring-2 ring-primary/20 border-primary" : ""
                } ${!isEnabled ? "opacity-85 hover:opacity-100" : ""}`}
              >
                {/* 头部：名称、协议徽章、启用开关 */}
                <div>
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-start gap-3 min-w-0 flex-1">
                      {templateView ? (
                        <span
                          data-testid={`ai-gateway-provider-template-icon-${provider.id}`}
                          title={t("aiGatewayProviderTemplateAvatarTitle", {
                            name: templateView.template.name,
                            defaultValue: `Created from template ${templateView.template.name}`,
                          })}
                          className="shrink-0 mt-0.5"
                        >
                          <ProviderTemplateAvatar
                            icon={templateView.template.icon}
                            templateId={templateView.template.id}
                            templateName={templateView.template.name}
                            size={36}
                          />
                        </span>
                      ) : (
                        <div
                          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-border/70 bg-muted/40 text-muted-foreground shadow-2xs mt-0.5"
                          aria-hidden="true"
                        >
                          <Server className="h-4.5 w-4.5 opacity-70" />
                        </div>
                      )}
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
                          <span
                            data-testid={`ai-gateway-weight-badge-${provider.id}`}
                            className="inline-flex items-center rounded-md border bg-background px-1.5 py-0.5 text-[11px] font-medium leading-4 text-muted-foreground"
                          >
                            {t("aiGateway.provider.weight", "Weight")}: {provider.weight ?? 1}
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
                    </div>

                    <div className="flex items-center gap-1.5 shrink-0 pt-0.5">
                      <Switch
                        aria-label={t("aiGatewayToggleProviderAria", {
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
                          ? t("aiGatewayMappingCount", {
                              count: mappingCount,
                              defaultValue: `${mappingCount} mappings configured`,
                            })
                          : t("aiGatewayNoMappingsShort", "No mappings (default routing)")}
                      </span>
                    </div>
                  </div>

                  {/* 退休映射提示：模板同步移除模型后其派生映射被自动禁用 */}
                  {retiredMappings.length > 0 ? (
                    <div
                      data-testid={`ai-gateway-provider-retired-mappings-${provider.id}`}
                      title={t("aiGatewayTemplateRetiredMappingsTooltip", {
                        models: retiredMappings
                          .map((mapping) => mapping.upstream_model)
                          .join(", "),
                        defaultValue: `Removed from the template and disabled: ${retiredMappings
                          .map((mapping) => mapping.upstream_model)
                          .join(", ")}`,
                      })}
                      className="mt-2 inline-flex items-center gap-1 rounded-full border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400"
                    >
                      <AlertTriangle className="h-3 w-3 shrink-0" />
                      <span>
                        {t("aiGatewayTemplateRetiredMappings", {
                          count: retiredMappings.length,
                          defaultValue: `${retiredMappings.length} mapping(s) removed from template`,
                        })}
                      </span>
                    </div>
                  ) : null}

                  {/* 旧版服务商级运行时状态（只读）：徽章与重启用按钮已移除，仅保留原因与时间信息 */}
                  {provider.auto_disabled &&
                  (provider.disabled_reason || disabledAt) ? (
                    <div className="mt-2.5 space-y-0.5 rounded-lg border border-border/60 bg-muted/20 p-2 text-[11px] text-muted-foreground">
                      {provider.disabled_reason ? (
                        <div className="truncate">
                          {t("aiGatewayDisabledReason", {
                            reason: provider.disabled_reason,
                            defaultValue: `Reason: ${provider.disabled_reason}`,
                          })}
                        </div>
                      ) : null}
                      {disabledAt ? (
                        <div className="opacity-80">
                          {t("aiGatewayDisabledAt", {
                            time: disabledAt,
                            defaultValue: `Disabled at ${disabledAt}`,
                          })}
                        </div>
                      ) : null}
                    </div>
                  ) : null}

                  {/* 逐行自动禁用读提示：健康失败累计后由后端按映射行自动禁用 */}
                  {autoDisabledMappings.length > 0 ? (
                    <div
                      data-testid={`ai-gateway-provider-auto-disabled-models-${provider.id}`}
                      className="mt-2 inline-flex items-center gap-1 rounded-full border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400"
                    >
                      <AlertTriangle className="h-3 w-3 shrink-0" />
                      <span>
                        {t("aiGatewayProviderAutoDisabledModelsHint", {
                          count: autoDisabledMappings.length,
                          defaultValue: `${autoDisabledMappings.length} mapping(s) auto-disabled`,
                        })}
                      </span>
                    </div>
                  ) : null}
                </div>

                {/* 卡片底栏操作按钮 */}
                <div className="mt-3 flex items-center justify-between border-t pt-2.5">
                  {isEnabled ? (
                    <span
                      data-testid={`ai-gateway-status-badge-${provider.id}`}
                      className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:text-emerald-300"
                    >
                      <span className="relative flex h-1.5 w-1.5">
                        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75 duration-1000" />
                        <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-emerald-500" />
                      </span>
                      <span>{t("aiGatewayEnabled", "Enabled")}</span>
                    </span>
                  ) : (
                    <span
                      data-testid={`ai-gateway-status-badge-${provider.id}`}
                      className="inline-flex items-center gap-1.5 rounded-full border border-rose-500/25 bg-rose-500/10 px-2 py-0.5 text-[11px] font-medium text-rose-700 dark:text-rose-400"
                    >
                      <span className="h-1.5 w-1.5 rounded-full bg-rose-500" />
                      <span>{t("aiGatewayDisabled", "Disabled")}</span>
                    </span>
                  )}
                  <div className="flex items-center gap-1">
                    {onDelete ? (
                      <button
                        type="button"
                        onClick={() => onDelete(provider.id)}
                        disabled={busy}
                        aria-label={t("aiGatewayDeleteProviderAria", {
                          name: provider.name,
                          defaultValue: `Delete provider ${provider.name}`,
                        })}
                        title={t("aiGatewayDelete", "Delete")}
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
                      {t("aiGatewayEdit", "Edit")}
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
