import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw } from "lucide-react";
import {
  aiGatewayProviderGoUsage,
  type GatewayUpstreamProvider,
  type GoUsageWindow,
  type ProviderGoUsage,
} from "@/lib/aiGateway";

const GO_USAGE_I18N_KEYS = {
  rolling: "aiGatewayGoUsageRolling",
  weekly: "aiGatewayGoUsageWeekly",
  monthly: "aiGatewayGoUsageMonthly",
} as const;

type ProviderGoUsageBlockProps = {
  provider: GatewayUpstreamProvider;
  baseNow?: number | Date;
};

type UsageState =
  | { status: "loading" }
  | { status: "success"; data: ProviderGoUsage }
  | { status: "error"; reason: string };

function formatResetTime(
  resetAt: string | number | null | undefined,
  defaultOffsetHours?: number,
  baseNow?: number | Date,
): string | null {
  if (resetAt === null || resetAt === undefined || resetAt === "") return null;

  let isZero = false;
  let parsed: Date | null = null;

  if (typeof resetAt === "number") {
    if (!Number.isFinite(resetAt) || resetAt <= 0) {
      isZero = true;
    } else {
      parsed = new Date(resetAt < 10_000_000_000 ? resetAt * 1000 : resetAt);
    }
  } else {
    const trimmed = resetAt.trim();
    if (trimmed === "" || trimmed === "0") {
      isZero = true;
    } else {
      const num = Number(trimmed);
      if (!Number.isNaN(num)) {
        if (num <= 0) {
          isZero = true;
        } else {
          parsed = new Date(num < 10_000_000_000 ? num * 1000 : num);
        }
      } else {
        parsed = new Date(trimmed);
      }
    }
  }

  if (parsed && (Number.isNaN(parsed.getTime()) || parsed.getFullYear() < 2000)) {
    if (!Number.isNaN(parsed.getTime()) && parsed.getFullYear() < 2000) {
      isZero = true;
    } else {
      return null;
    }
  }

  if (isZero) {
    if (defaultOffsetHours === undefined) return null;
    const nowMs = baseNow instanceof Date
      ? baseNow.getTime()
      : typeof baseNow === "number"
        ? baseNow
        : Date.now();
    parsed = new Date(nowMs + defaultOffsetHours * 3600 * 1000);
  }

  if (!parsed || Number.isNaN(parsed.getTime())) return null;
  return new Intl.DateTimeFormat(undefined, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(parsed);
}

function formatUsageError(
  reason: string,
  t: ReturnType<typeof useTranslation>["t"],
): string {
  if (/no api key/i.test(reason)) return t("aiGatewayGoUsageErrorNoKey");
  if (/timed out/i.test(reason)) return t("aiGatewayGoUsageErrorTimeout");
  if (/HTTP 403|EntitlementError|subscription required/i.test(reason)) {
    return t("aiGatewayGoUsageErrorNoSubscription");
  }
  const httpStatus = reason.match(/HTTP (\d{3})/);
  if (httpStatus) return t("aiGatewayGoUsageErrorHttp", { status: httpStatus[1] });
  if (/invalid.*usage|missing.*usage|usage.*(?:invalid|missing)/i.test(reason)) {
    return t("aiGatewayGoUsageErrorInvalid");
  }
  return t("aiGatewayGoUsageError", { reason });
}

function UsageWindowLine({
  window,
  label,
  testId,
  t,
  resetOffsetHours,
  baseNow,
}: {
  window: GoUsageWindow;
  label: string;
  testId: string;
  t: ReturnType<typeof useTranslation>["t"];
  resetOffsetHours: number;
  baseNow?: number | Date;
}) {
  const percent = Number.isFinite(window.percent) ? window.percent : 0;
  const width = Math.min(100, Math.max(0, percent));
  const resetTime = formatResetTime(window.resetsAt, resetOffsetHours, baseNow);
  const color = percent < 50
    ? "bg-emerald-500"
    : percent < 80
      ? "bg-amber-500"
      : "bg-destructive";

  return (
    <div data-testid={testId} className="space-y-1">
      <div className="flex flex-wrap items-baseline justify-between gap-x-1.5 gap-y-0.5 text-[11px]">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate font-medium text-foreground">{label}</span>
          {resetTime ? (
            <span className="shrink-0 text-[10px] text-muted-foreground">
              {t("aiGatewayGoUsageReset", { time: resetTime })}
            </span>
          ) : null}
          {window.status === "rate-limited" ? (
            <span className="shrink-0 rounded bg-amber-500/15 px-1 py-0.2 text-[10px] font-medium text-amber-700 dark:text-amber-400">
              {t("aiGatewayGoUsageRateLimited")}
            </span>
          ) : null}
        </div>
        <span className="shrink-0 font-medium text-foreground">{window.percent}%</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={`h-full rounded-full transition-all duration-300 ${color}`}
          style={{ width: `${width}%` }}
        />
      </div>
    </div>
  );
}

export function ProviderGoUsageBlock({ provider, baseNow }: ProviderGoUsageBlockProps) {
  const { t } = useTranslation();
  const [state, setState] = useState<UsageState>({ status: "loading" });
  const [refreshNow, setRefreshNow] = useState<number | null>(null);

  const loadUsage = useCallback(async (forceRefresh = false) => {
    setState({ status: "loading" });
    if (forceRefresh) setRefreshNow(Date.now());
    try {
      const data = await aiGatewayProviderGoUsage(provider.id, forceRefresh);
      if (!data?.usage?.rolling || !data.usage.weekly || !data.usage.monthly) {
        throw new Error("invalid or missing usage response");
      }
      setState({ status: "success", data });
    } catch (error) {
      setState({
        status: "error",
        reason: error instanceof Error ? error.message : String(error),
      });
    }
  }, [provider.id]);

  useEffect(() => {
    void loadUsage();
  }, [loadUsage]);

  const id = provider.id;
  const usage = state.status === "success" ? state.data.usage : null;
  const effectiveNow = refreshNow ?? baseNow;

  return (
    <section
      data-testid={`ai-gateway-provider-go-usage-${id}`}
      aria-label={t("aiGatewayGoUsageTitle")}
      className="mt-2 space-y-1.5 rounded-lg border border-border/60 bg-muted/20 p-2 text-xs"
    >
      <div className="flex items-center justify-between gap-2">
        <div
          data-testid={`ai-gateway-provider-go-usage-summary-${id}`}
          className="text-[11px] font-medium text-muted-foreground"
        >
          {t("aiGatewayGoUsageTitle")}
        </div>
        {state.status !== "loading" ? (
          <button
            type="button"
            data-testid={`ai-gateway-provider-go-usage-refresh-${id}`}
            aria-label={t("aiGatewayGoUsageRefreshAria")}
            title={t("aiGatewayGoUsageRefresh")}
            onClick={() => void loadUsage(true)}
            className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground transition hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <RotateCcw className="h-3 w-3" />
            <span>{t("aiGatewayGoUsageRefresh")}</span>
          </button>
        ) : null}
      </div>

      {state.status === "loading" ? (
        <p
          data-testid={`ai-gateway-provider-go-usage-loading-${id}`}
          role="status"
          aria-live="polite"
          className="text-[11px] text-muted-foreground"
        >
          {t("aiGatewayGoUsageLoading")}
        </p>
      ) : null}

      {state.status === "error" ? (
        <p
          data-testid={`ai-gateway-provider-go-usage-error-${id}`}
          role="status"
          className="text-[11px] text-destructive"
        >
          {formatUsageError(state.reason, t)}
        </p>
      ) : null}

      {usage ? (
        <div className="space-y-1.5 border-t border-border/40 pt-0.5">
          <UsageWindowLine
            window={usage.rolling}
            label={t(GO_USAGE_I18N_KEYS.rolling)}
            testId={`ai-gateway-provider-go-usage-rolling-${id}`}
            t={t}
            resetOffsetHours={5}
            baseNow={effectiveNow}
          />
          <UsageWindowLine
            window={usage.weekly}
            label={t(GO_USAGE_I18N_KEYS.weekly)}
            testId={`ai-gateway-provider-go-usage-weekly-${id}`}
            t={t}
            resetOffsetHours={7 * 24}
            baseNow={effectiveNow}
          />
          <UsageWindowLine
            window={usage.monthly}
            label={t(GO_USAGE_I18N_KEYS.monthly)}
            testId={`ai-gateway-provider-go-usage-monthly-${id}`}
            t={t}
            resetOffsetHours={30 * 24}
            baseNow={effectiveNow}
          />
        </div>
      ) : null}
    </section>
  );
}
