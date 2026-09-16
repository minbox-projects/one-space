import { useTranslation } from "react-i18next";
import { Check, Copy, Power, Radio } from "lucide-react";
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
    <section
      className="rounded-2xl border bg-card p-4 shadow-sm"
      data-testid="api-fusion-runtime"
    >
      <div className="grid gap-4 lg:grid-cols-[auto_1fr_auto] lg:items-center">
        {/* 左侧：运行状态指示与启停按钮 */}
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2.5 rounded-xl border bg-muted/20 px-3 py-2">
            <span className="relative flex h-3 w-3">
              {running ? (
                <>
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex h-3 w-3 rounded-full bg-emerald-500" />
                </>
              ) : (
                <span className="relative inline-flex h-3 w-3 rounded-full bg-muted-foreground/40" />
              )}
            </span>
            <div className="space-y-0.5">
              <div className="flex items-center gap-2">
                <span
                  className={`text-sm font-semibold leading-none ${
                    running ? "text-emerald-700 dark:text-emerald-400" : "text-muted-foreground"
                  }`}
                  data-testid="api-fusion-runtime-state"
                >
                  {running ? t("apiFusionRunning", "Running") : t("apiFusionStopped", "Stopped")}
                </span>
              </div>
              <div className="text-[11px] text-muted-foreground">
                {t("apiFusionPortValue", { port: config.port, defaultValue: `Port ${config.port}` })}
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={running ? onStop : onStart}
            disabled={busy}
            className={`inline-flex items-center gap-1.5 rounded-xl px-3.5 py-2 text-xs font-semibold shadow-sm transition disabled:opacity-50 ${
              running
                ? "border border-destructive/30 bg-destructive/10 text-destructive hover:bg-destructive/20"
                : "bg-primary text-primary-foreground hover:bg-primary/90"
            }`}
          >
            <Power className="h-3.5 w-3.5" />
            {running ? t("apiFusionStop", "Stop service") : t("apiFusionStart", "Start service")}
          </button>
        </div>

        {/* 中间：本地 API 地址展示与一键复制 */}
        <div className="flex min-w-0 flex-1 flex-col justify-center rounded-xl border bg-muted/15 px-3 py-2">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              <Radio className="h-3 w-3 text-indigo-500" />
              <span>{t("apiFusionLocalAddress", "Local API address")}</span>
            </div>
            {addressCopied ? (
              <span className="text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
                {t("apiFusionCopied", "Copied")}
              </span>
            ) : null}
          </div>
          <div className="mt-1 flex items-center justify-between gap-2">
            <code
              className="min-w-0 truncate font-mono text-xs font-medium text-foreground select-all"
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
              className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border bg-background text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              {addressCopied ? (
                <Check className="h-3.5 w-3.5 text-emerald-600" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </button>
          </div>
        </div>

        {/* 右侧：统计概览胶囊 */}
        <div className="flex items-center gap-2">
          <div className="rounded-xl border bg-muted/20 px-3 py-1.5 text-center min-w-[72px]">
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
              {t("apiFusionProviderCount", "Upstream providers")}
            </div>
            <div className="mt-0.5 text-base font-bold">{providerCount}</div>
          </div>
          <div
            className={`rounded-xl border px-3 py-1.5 text-center min-w-[72px] ${
              autoDisabledCount > 0
                ? "border-amber-500/40 bg-amber-500/10"
                : "bg-muted/20"
            }`}
          >
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
              {t("apiFusionAutoDisabledCount", "Auto-disabled")}
            </div>
            <div
              className={`mt-0.5 text-base font-bold ${
                autoDisabledCount > 0 ? "text-amber-600 dark:text-amber-400" : ""
              }`}
              data-testid="api-fusion-auto-disabled-count"
            >
              {autoDisabledCount}
            </div>
          </div>
          <div className="rounded-xl border bg-muted/20 px-3 py-1.5 text-center min-w-[72px]">
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
              {t("apiFusionKeyCount", "Local keys")}
            </div>
            <div className="mt-0.5 text-base font-bold">{keyCount}</div>
          </div>
        </div>
      </div>
    </section>
  );
}
