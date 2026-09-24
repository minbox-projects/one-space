import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  aiGatewayProviderQuota,
  type GatewayUpstreamProvider,
  type ProviderQuota,
  type QuotaWindow,
} from "@/lib/aiGateway";

type ProviderQuotaBlockProps = {
  provider: GatewayUpstreamProvider;
};

type QuotaState =
  | { status: "loading" }
  | { status: "success"; quota: ProviderQuota }
  | { status: "error"; reason: string };

function formatResetTime(resetAt: string | number | null | undefined): string | null {
  if (resetAt === null || resetAt === undefined || resetAt === "") return null;
  const parsed = new Date(resetAt);
  if (Number.isNaN(parsed.getTime())) return null;
  return new Intl.DateTimeFormat(undefined, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(parsed);
}

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
}: {
  window: QuotaWindow;
  label: string;
  testId: string;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const exceeded = window.exceeded === true || window.used > window.cap;
  const remaining = exceeded ? 0 : Math.max(window.cap - window.used, 0);
  const resetTime = formatResetTime(window.resetAt);

  return (
    <div data-testid={testId} className="flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
      <span className="text-muted-foreground">{label}</span>
      <span className={exceeded ? "font-medium text-amber-700 dark:text-amber-400" : "font-medium text-foreground"}>
        ${remaining.toFixed(2)} / {formatCap(window.cap)}
      </span>
      {exceeded ? (
        <span className="text-amber-700 dark:text-amber-400">{t("aiGatewayQuotaExceeded")}</span>
      ) : null}
      {resetTime ? (
        <span className="text-muted-foreground">
          {t("aiGatewayQuotaReset", { time: resetTime })}
        </span>
      ) : null}
    </div>
  );
}

export function ProviderQuotaBlock({ provider }: ProviderQuotaBlockProps) {
  const { t } = useTranslation();
  const [state, setState] = useState<QuotaState>({ status: "loading" });

  const loadQuota = useCallback(async (forceRefresh = false) => {
    setState({ status: "loading" });
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

  return (
    <section
      data-testid={`ai-gateway-provider-quota-${id}`}
      aria-label={t("aiGatewayQuotaTitle")}
      className="mt-2.5 space-y-1.5 rounded-lg border border-border/60 bg-muted/20 p-2.5 text-xs"
    >
      <div className="flex items-center justify-between gap-2">
        <h4 className="font-medium text-foreground">{t("aiGatewayQuotaTitle")}</h4>
        {state.status !== "loading" ? (
          <button
            type="button"
            data-testid={`ai-gateway-provider-quota-refresh-${id}`}
            aria-label={t("aiGatewayQuotaRefreshAria")}
            title={t("aiGatewayQuotaRefresh")}
            onClick={() => void loadQuota(true)}
            className="rounded px-1.5 py-0.5 text-primary hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {t("aiGatewayQuotaRefresh")}
          </button>
        ) : null}
      </div>

      {state.status === "loading" ? (
        <p
          data-testid={`ai-gateway-provider-quota-loading-${id}`}
          role="status"
          aria-live="polite"
          className="text-muted-foreground"
        >
          {t("aiGatewayQuotaLoading")}
        </p>
      ) : null}

      {state.status === "error" ? (
        <p
          data-testid={`ai-gateway-provider-quota-error-${id}`}
          role="status"
          className="text-destructive"
        >
          {formatQuotaError(state.reason, t)}
        </p>
      ) : null}

      {quota ? (
        <div className="space-y-1">
          <div
            data-testid={`ai-gateway-provider-quota-credits-${id}`}
            className="flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5"
          >
            <span className="text-muted-foreground">{t("aiGatewayQuotaCredits")}</span>
            <span className="font-medium text-foreground">
              ${(quota.credits.monthlyCredits + quota.credits.purchasedCredits + quota.credits.freeCredits).toFixed(2)}
            </span>
            {quota.credits.belowThreshold ? (
              <span className="text-amber-700 dark:text-amber-400">
                {t("aiGatewayQuotaLowBalance")}
              </span>
            ) : null}
          </div>
          {windowLimits?.limited === true && windowLimits.fiveHour && windowLimits.fiveHour.cap > 0 ? (
            <QuotaWindowLine
              window={windowLimits.fiveHour}
              label={t("aiGatewayQuotaWindow5h")}
              testId={`ai-gateway-provider-quota-window-5h-${id}`}
              t={t}
            />
          ) : null}
          {windowLimits?.limited === true && windowLimits.weekly && windowLimits.weekly.cap > 0 ? (
            <QuotaWindowLine
              window={windowLimits.weekly}
              label={t("aiGatewayQuotaWindowWeekly")}
              testId={`ai-gateway-provider-quota-window-weekly-${id}`}
              t={t}
            />
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
