import { useTranslation } from "react-i18next";
import { AlertTriangle, Plus, RotateCcw, Server } from "lucide-react";
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
};

export function UpstreamProviderList({
  providers,
  selectedProviderId,
  busy,
  onSelect,
  onToggleEnabled,
  onReenable,
  onAdd,
}: UpstreamProviderListProps) {
  const { t } = useTranslation();

  return (
    <section className="rounded-[24px] border bg-card p-5" data-testid="api-fusion-providers">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Server className="h-4 w-4 text-muted-foreground" />
          <h3 className="text-sm font-semibold">
            {t("apiFusionProviders", "Upstream providers")}
          </h3>
        </div>
        <button
          type="button"
          onClick={onAdd}
          disabled={busy}
          className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-sm transition hover:bg-muted disabled:opacity-50"
        >
          <Plus className="h-4 w-4" />
          {t("apiFusionAddProvider", "Add provider")}
        </button>
      </div>

      {providers.length === 0 ? (
        <p className="mt-4 rounded-2xl border border-dashed px-4 py-5 text-sm text-muted-foreground">
          {t("apiFusionNoProviders", "No upstream providers yet.")}
        </p>
      ) : (
        <ul className="mt-4 space-y-2">
          {providers.map((provider) => {
            const selected = provider.id === selectedProviderId;
            const disabledAt = formatFusionTimestamp(provider.disabled_at);
            return (
              <li
                key={provider.id}
                data-testid={`api-fusion-provider-${provider.id}`}
                className={`rounded-2xl border px-4 py-3 transition ${
                  selected ? "border-primary bg-muted/20" : "bg-muted/10"
                }`}
              >
                <div className="flex flex-wrap items-center gap-3">
                  <button
                    type="button"
                    onClick={() => onSelect(provider.id)}
                    className="min-w-0 flex-1 text-left"
                  >
                    <div className="truncate text-sm font-medium">{provider.name}</div>
                    <div className="truncate font-mono text-xs text-muted-foreground">
                      {provider.base_url}
                    </div>
                  </button>
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">
                      {provider.enabled
                        ? t("apiFusionEnabled", "Enabled")
                        : t("apiFusionDisabled", "Disabled")}
                    </span>
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

                {provider.auto_disabled ? (
                  <div
                    className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-xl border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-700"
                    data-testid={`api-fusion-auto-disabled-${provider.id}`}
                  >
                    <span className="inline-flex items-center gap-1.5 font-medium">
                      <AlertTriangle className="h-3.5 w-3.5" />
                      {t("apiFusionAutoDisabled", "Auto-disabled")}
                    </span>
                    {provider.disabled_reason ? (
                      <span>
                        {t("apiFusionDisabledReason", {
                          reason: provider.disabled_reason,
                          defaultValue: `Reason: ${provider.disabled_reason}`,
                        })}
                      </span>
                    ) : null}
                    {disabledAt ? (
                      <span>
                        {t("apiFusionDisabledAt", {
                          time: disabledAt,
                          defaultValue: `Disabled at ${disabledAt}`,
                        })}
                      </span>
                    ) : null}
                    <button
                      type="button"
                      onClick={() => onReenable(provider.id)}
                      disabled={busy}
                      className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/40 px-2.5 py-1 font-medium transition hover:bg-amber-500/10 disabled:opacity-50"
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                      {t("apiFusionReenable", "Re-enable")}
                    </button>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
