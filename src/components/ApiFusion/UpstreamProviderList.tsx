import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  ArrowRightLeft,
  Globe,
  Pencil,
  Plus,
  RotateCcw,
  Server,
  Trash2,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  formatFusionTimestamp,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";

type UpstreamProviderListProps = {
  providers: FusionUpstreamProvider[];
  selectedProviderId: string | null;
  busy: boolean;
  onSelect: (providerId: string) => void;
  onToggleEnabled: (provider: FusionUpstreamProvider, enabled: boolean) => void;
  onReenable: (providerId: string) => void;
  onAdd: () => void;
  onDelete?: (providerId: string) => void;
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
}: UpstreamProviderListProps) {
  const { t } = useTranslation();

  return (
    <section className="space-y-3.5" data-testid="api-fusion-providers">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <Server className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("apiFusionProviders", "Upstream providers")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
              {providers.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "apiFusionProvidersDesc",
              "Manage remote AI provider endpoints, protocols, and model route mappings.",
            )}
          </p>
        </div>

        <button
          type="button"
          onClick={onAdd}
          disabled={busy}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
        >
          <Plus className="h-3.5 w-3.5" />
          {t("apiFusionAddProvider", "Add provider")}
        </button>
      </div>

      {providers.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
          <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
            <Server className="h-5 w-5" />
          </div>
          <h4 className="mt-2.5 text-xs font-medium text-foreground">
            {t("apiFusionNoProviders", "No upstream providers yet.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "apiFusionNoProvidersGuide",
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
            {t("apiFusionAddProvider", "Add provider")}
          </button>
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {providers.map((provider) => {
            const isSelected = provider.id === selectedProviderId;
            const disabledAt = formatFusionTimestamp(provider.disabled_at);
            const mappingCount = provider.mappings?.length ?? 0;
            const isChatProtocol = provider.protocol !== "responses";

            return (
              <div
                key={provider.id}
                data-testid={`api-fusion-provider-${provider.id}`}
                className={`group relative flex flex-col justify-between rounded-xl border bg-card p-3.5 shadow-sm transition-all hover:border-primary/40 hover:shadow-md ${
                  isSelected ? "ring-2 ring-primary/20 border-primary" : ""
                }`}
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
                        aria-label={t("apiFusionToggleProviderAria", {
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
                          ? t("apiFusionMappingCount", {
                              count: mappingCount,
                              defaultValue: `${mappingCount} mappings configured`,
                            })
                          : t("apiFusionNoMappingsShort", "No mappings (default routing)")}
                      </span>
                    </div>
                  </div>

                  {/* 自动禁用警告栏 */}
                  {provider.auto_disabled ? (
                    <div
                      className="mt-2.5 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400"
                      data-testid={`api-fusion-auto-disabled-${provider.id}`}
                    >
                      <div className="min-w-0 space-y-0.5">
                        <div className="flex items-center gap-1 font-medium">
                          <AlertTriangle className="h-3 w-3 shrink-0" />
                          <span>{t("apiFusionAutoDisabled", "Auto-disabled")}</span>
                        </div>
                        {provider.disabled_reason ? (
                          <div className="truncate text-[10px]">
                            {t("apiFusionDisabledReason", {
                              reason: provider.disabled_reason,
                              defaultValue: `Reason: ${provider.disabled_reason}`,
                            })}
                          </div>
                        ) : null}
                        {disabledAt ? (
                          <div className="text-[10px] opacity-80">
                            {t("apiFusionDisabledAt", {
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
                        {t("apiFusionReenable", "Re-enable")}
                      </button>
                    </div>
                  ) : null}
                </div>

                {/* 卡片底栏操作按钮 */}
                <div className="mt-3 flex items-center justify-between border-t pt-2.5">
                  <span className="text-[11px] text-muted-foreground">
                    {provider.enabled
                      ? t("apiFusionEnabled", "Enabled")
                      : t("apiFusionDisabled", "Disabled")}
                  </span>
                  <div className="flex items-center gap-1">
                    {onDelete ? (
                      <button
                        type="button"
                        onClick={() => onDelete(provider.id)}
                        disabled={busy}
                        aria-label={t("apiFusionDeleteProviderAria", {
                          name: provider.name,
                          defaultValue: `Delete provider ${provider.name}`,
                        })}
                        title={t("apiFusionDelete", "Delete")}
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
                      {t("apiFusionEdit", "Edit")}
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
