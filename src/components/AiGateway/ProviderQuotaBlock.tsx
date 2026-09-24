import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw } from "lucide-react";
import {
  aiGatewayProviderQuota,
  type GatewayUpstreamProvider,
  type ProviderQuota,
  type QuotaWindow,
} from "@/lib/aiGateway";
import { formatResetTime } from "./gatewayShared";

type ProviderQuotaBlockProps = {
  provider: GatewayUpstreamProvider;
  baseNow?: number | Date;
};

type QuotaState =
  | { status: "loading" }
  | { status: "success"; quota: ProviderQuota }
  | { status: "error"; reason: string };

function formatCap(cap: number): string {
  return `$${new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 2,
  }).format(cap)}`;
}

function formatQuotaError(
  reason: string,
  t: ReturnType<typeof useTranslation>["t"],
): string {
  if (/no api key/i.test(reason)) return t("aiGatewayQuotaErrorNoKey");
  if (/timed out/i.test(reason)) return t("aiGatewayQuotaErrorTimeout");
  const httpStatus = reason.match(/HTTP (\d{3})/);
  if (httpStatus) return t("aiGatewayQuotaErrorHttp", { status: httpStatus[1] });
  if (/invalid quota response/i.test(reason)) return t("aiGatewayQuotaErrorInvalid");
  return t("aiGatewayQuotaError", { reason });
}

function QuotaWindowLine({
  window,
  label,
  testId,
  t,
  defaultOffsetHours,
  baseNow,
}: {
  window: QuotaWindow;
  label: string;
  testId: string;
  t: ReturnType<typeof useTranslation>["t"];
  defaultOffsetHours?: number;
  baseNow?: number | Date;
}) {
  const exceeded = window.exceeded === true || window.used > window.cap;
  const remaining = exceeded ? 0 : Math.max(window.cap - window.used, 0);
  const resetTime = formatResetTime(window.resetAt, defaultOffsetHours, baseNow);
  const remainingFraction = window.cap > 0 ? remaining / window.cap : 0;
  const remainingPercent = Math.min(100, Math.max(0, Math.round(remainingFraction * 100)));
  const isLow = remainingFraction < 0.2 || exceeded;
  const isMedium = remainingFraction < 0.5 && !isLow;

  return (
    <div data-testid={testId} className="space-y-1">
      <div className="flex flex-wrap items-baseline justify-between gap-x-1.5 gap-y-0.5 text-[11px]">
        <div className="flex items-center gap-1.5 min-w-0">
          <span className="font-medium text-foreground truncate">{label}</span>
          {resetTime ? (
            <span className="text-[10px] text-muted-foreground shrink-0">
              {t("aiGatewayQuotaReset", { time: resetTime })}
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <span
            className={
              exceeded
                ? "font-semibold text-destructive"
                : "font-medium text-foreground"
            }
          >
            ${remaining.toFixed(2)} / {formatCap(window.cap)}
          </span>
        </div>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={`h-full rounded-full transition-all duration-300 ${
            isLow
              ? "bg-destructive"
              : isMedium
                ? "bg-amber-500"
                : "bg-emerald-500"
          }`}
          style={{ width: `${remainingPercent}%` }}
        />
      </div>
    </div>
  );
}

export function ProviderQuotaBlock({ provider, baseNow }: ProviderQuotaBlockProps) {
  const { t } = useTranslation();
  const [state, setState] = useState<QuotaState>({ status: "loading" });
  const [refreshNow, setRefreshNow] = useState<number | null>(null);

  const loadQuota = useCallback(async (forceRefresh = false) => {
    setState({ status: "loading" });
    if (forceRefresh) {
      setRefreshNow(Date.now());
    }
    try {
      const quota = await aiGatewayProviderQuota(provider.id, forceRefresh);
      setState({ status: "success", quota });
    } catch (error) {
      setState({
        status: "error",
        reason: error instanceof Error ? error.message : String(error),
      });
    }
  }, [provider.id]);

  useEffect(() => {
    void loadQuota();
  }, [loadQuota]);

  const id = provider.id;
  const quota = state.status === "success" ? state.quota : null;
  const windowLimits = quota?.windowLimits;
  const effectiveNow = refreshNow ?? baseNow;

  return (
    <section
      data-testid={`ai-gateway-provider-quota-${id}`}
      aria-label={t("aiGatewayQuotaTitle")}
      className="mt-2 space-y-1.5 rounded-lg border border-border/60 bg-muted/20 p-2 text-xs"
    >
      <div className="flex items-center justify-between gap-2">
        {quota ? (
          <div
            data-testid={`ai-gateway-provider-quota-credits-${id}`}
            className="flex items-baseline gap-1 text-[11px]"
          >
            <span className="text-muted-foreground">{t("aiGatewayQuotaCredits")}:</span>
            <span className="font-semibold text-foreground">
              ${(quota.credits.monthlyCredits + quota.credits.purchasedCredits + quota.credits.freeCredits).toFixed(2)}
            </span>
            {quota.credits.belowThreshold ? (
              <span className="rounded bg-amber-500/15 px-1 py-0.2 text-[10px] font-medium text-amber-700 dark:text-amber-400">
                {t("aiGatewayQuotaLowBalance")}
              </span>
            ) : null}
          </div>
        ) : (
          <div />
        )}
        {state.status !== "loading" ? (
          <button
            type="button"
            data-testid={`ai-gateway-provider-quota-refresh-${id}`}
            aria-label={t("aiGatewayQuotaRefreshAria")}
            title={t("aiGatewayQuotaRefresh")}
            onClick={() => void loadQuota(true)}
            className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground transition hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring shrink-0"
          >
            <RotateCcw className="h-3 w-3" />
            <span>{t("aiGatewayQuotaRefresh")}</span>
          </button>
        ) : null}
      </div>

      {state.status === "loading" ? (
        <p
          data-testid={`ai-gateway-provider-quota-loading-${id}`}
          role="status"
          aria-live="polite"
          className="text-[11px] text-muted-foreground"
        >
          {t("aiGatewayQuotaLoading")}
        </p>
      ) : null}

      {state.status === "error" ? (
        <p
          data-testid={`ai-gateway-provider-quota-error-${id}`}
          role="status"
          className="text-[11px] text-destructive"
        >
          {formatQuotaError(state.reason, t)}
        </p>
      ) : null}

      {quota && windowLimits?.limited === true ? (
        <div className="space-y-1.5 pt-0.5 border-t border-border/40">
          {windowLimits.fiveHour && windowLimits.fiveHour.cap > 0 ? (
            <QuotaWindowLine
              window={windowLimits.fiveHour}
              label={t("aiGatewayQuotaWindow5h")}
              testId={`ai-gateway-provider-quota-window-5h-${id}`}
              t={t}
              defaultOffsetHours={5}
              baseNow={effectiveNow}
            />
          ) : null}
          {windowLimits.weekly && windowLimits.weekly.cap > 0 ? (
            <QuotaWindowLine
              window={windowLimits.weekly}
              label={t("aiGatewayQuotaWindowWeekly")}
              testId={`ai-gateway-provider-quota-window-weekly-${id}`}
              t={t}
              defaultOffsetHours={7 * 24}
              baseNow={effectiveNow}
            />
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
