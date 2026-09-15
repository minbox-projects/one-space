import { useTranslation } from "react-i18next";
import { Check, Copy } from "lucide-react";
import { localBaseUrl, type FusionConfig, type FusionStatus } from "@/lib/apiFusion";

type RuntimeStatusCardProps = {
  status: FusionStatus | null;
  config: FusionConfig;
  busy: boolean;
  addressCopied: boolean;
  onStart: () => void;
  onStop: () => void;
  onCopyAddress: () => void;
};

export function RuntimeStatusCard({
  status,
  config,
  busy,
  addressCopied,
  onStart,
  onStop,
  onCopyAddress,
}: RuntimeStatusCardProps) {
  const { t } = useTranslation();
  const running = Boolean(status?.running);
  const address = localBaseUrl(config.port);
  const providerCount = status?.provider_count ?? config.providers.length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;
  const keyCount = status?.key_count ?? config.keys.length;

  return (
    <section className="rounded-[24px] border bg-card p-5" data-testid="api-fusion-runtime">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <h3 className="text-sm font-semibold">{t("apiFusionRuntime", "Local service")}</h3>
          <div className="flex items-center gap-2">
            <span
              className={`h-2.5 w-2.5 rounded-full ${
                running ? "bg-emerald-500" : "bg-muted-foreground/40"
              }`}
              aria-hidden="true"
            />
            <span
              className={`text-lg font-semibold ${
                running ? "text-emerald-700" : "text-muted-foreground"
              }`}
              data-testid="api-fusion-runtime-state"
            >
              {running ? t("apiFusionRunning", "Running") : t("apiFusionStopped", "Stopped")}
            </span>
          </div>
          <div className="text-sm text-muted-foreground">
            {t("apiFusionPortValue", { port: config.port, defaultValue: `Port ${config.port}` })}
          </div>
        </div>
        <button
          type="button"
          onClick={running ? onStop : onStart}
          disabled={busy}
          className="rounded-xl bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
        >
          {running ? t("apiFusionStop", "Stop service") : t("apiFusionStart", "Start service")}
        </button>
      </div>

      <div className="mt-4 grid gap-2 md:grid-cols-[128px_minmax(0,1fr)_40px] md:items-center">
        <div className="text-[10px] uppercase tracking-wide text-muted-foreground">
          {t("apiFusionLocalAddress", "Local API address")}
        </div>
        <code
          className="min-w-0 truncate rounded-md bg-muted/40 px-3 py-2 font-mono text-xs"
          title={address}
          data-testid="api-fusion-local-address"
        >
          {address}
        </code>
        <button
          type="button"
          onClick={onCopyAddress}
          aria-label={t("apiFusionCopyAddress", "Copy local API address")}
          title={t("apiFusionCopyAddress", "Copy local API address")}
          className="inline-flex h-9 w-9 items-center justify-center rounded-lg border bg-card text-muted-foreground transition hover:bg-muted"
        >
          {addressCopied ? (
            <Check className="h-4 w-4 text-emerald-600" />
          ) : (
            <Copy className="h-4 w-4" />
          )}
        </button>
      </div>

      <div className="mt-4 grid gap-3 sm:grid-cols-3">
        <div className="rounded-2xl border bg-muted/10 px-4 py-3">
          <div className="text-xs text-muted-foreground">
            {t("apiFusionProviderCount", "Upstream providers")}
          </div>
          <div className="mt-1 text-lg font-semibold">{providerCount}</div>
        </div>
        <div className="rounded-2xl border bg-muted/10 px-4 py-3">
          <div className="text-xs text-muted-foreground">
            {t("apiFusionAutoDisabledCount", "Auto-disabled")}
          </div>
          <div
            className={`mt-1 text-lg font-semibold ${
              autoDisabledCount > 0 ? "text-rose-700" : ""
            }`}
            data-testid="api-fusion-auto-disabled-count"
          >
            {autoDisabledCount}
          </div>
        </div>
        <div className="rounded-2xl border bg-muted/10 px-4 py-3">
          <div className="text-xs text-muted-foreground">{t("apiFusionKeyCount", "Local keys")}</div>
          <div className="mt-1 text-lg font-semibold">{keyCount}</div>
        </div>
      </div>
    </section>
  );
}
