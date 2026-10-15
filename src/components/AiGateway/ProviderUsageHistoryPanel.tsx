import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  BarChart3,
  History as HistoryIcon,
  RefreshCw,
} from "lucide-react";
import {
  aiGatewayProviderUsageHistory,
  aiGatewayUsageProviders,
  formatGatewayTokens,
  formatUsageAmount,
  type ProviderDailyEntry,
  type ProviderUsageHistory,
  type ProviderUsageIdentity,
} from "@/lib/aiGateway";
import { errorToMessage } from "@/lib/messages";
import { SelectDropdown } from "./SelectDropdown";

/**
 * One request/day action handed to the parent so it can open the existing
 * request-log view with this exact provider ID and UTC+8 day.
 */
export interface ProviderHistoryOpenDayRequest {
  providerId: string;
  day: string;
}

/** The additive metric the operator is trending in the daily chart. */
type ProviderHistoryMetric = "tokens" | "requests" | "cost";

const METRIC_OPTIONS: Array<{
  value: ProviderHistoryMetric;
  labelKey: string;
  fallback: string;
}> = [
  { value: "tokens", labelKey: "aiGatewayUsageViewTokens", fallback: "Tokens" },
  {
    value: "requests",
    labelKey: "aiGatewayUsageViewRequests",
    fallback: "Requests",
  },
  { value: "cost", labelKey: "aiGatewayUsageViewCost", fallback: "Cost" },
];

const COVERAGE_LABEL_KEYS = {
  continuous: "aiGatewayProviderHistoryCoverageContinuous",
  retained_historical: "aiGatewayProviderHistoryCoverageRetained",
  unavailable: "aiGatewayProviderHistoryCoverageUnavailable",
} as const;

const COVERAGE_LABEL_FALLBACKS = {
  continuous: "Continuous",
  retained_historical: "Retained history",
  unavailable: "Unavailable",
} as const;

/** Additive counts render as grouped numbers; an unavailable day is `—`. */
function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  return new Intl.NumberFormat().format(value);
}

/**
 * Recorded USD amounts reuse the established formatter but drop padded trailing
 * zeros so a recorded `1.5` reads `1.5` rather than `1.5000`.
 */
function formatAmount(amount: number | null | undefined): string {
  const formatted = formatUsageAmount(amount);
  if (formatted === "—") return formatted;
  return formatted.replace(/\.?0+$/, "") || "0";
}

export function ProviderUsageHistoryPanel({
  isActive = true,
  onOpenDay,
}: {
  isActive?: boolean;
  onOpenDay?: (request: ProviderHistoryOpenDayRequest) => void;
}) {
  const { t } = useTranslation();
  const [identities, setIdentities] = useState<ProviderUsageIdentity[]>([]);
  const [providersError, setProvidersError] = useState("");
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [history, setHistory] = useState<ProviderUsageHistory | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyRefreshing, setHistoryRefreshing] = useState(false);
  const [historyError, setHistoryError] = useState("");
  const [metric, setMetric] = useState<ProviderHistoryMetric>("tokens");
  const providersSeqRef = useRef(0);
  const historySeqRef = useRef(0);

  const loadProviders = useCallback(async () => {
    const seq = providersSeqRef.current + 1;
    providersSeqRef.current = seq;
    setProvidersError("");
    try {
      const list = await aiGatewayUsageProviders();
      if (providersSeqRef.current !== seq) return;
      setIdentities(list ?? []);
    } catch (err) {
      if (providersSeqRef.current !== seq) return;
      setIdentities([]);
      setProvidersError(errorToMessage(err));
    }
  }, []);

  // An inactive mount issues no provider or history query; each activation
  // re-reads the current provider identities.
  useEffect(() => {
    if (!isActive) return;
    void loadProviders();
  }, [isActive, loadProviders]);

  const loadHistory = useCallback(
    async (options?: { refresh?: boolean }) => {
      if (selectedProviderId === null) return;
      const seq = historySeqRef.current + 1;
      historySeqRef.current = seq;
      setHistoryError("");
      if (options?.refresh) setHistoryRefreshing(true);
      else setHistoryLoading(true);
      try {
        const next = await aiGatewayProviderUsageHistory({
          providerId: selectedProviderId,
        });
        if (historySeqRef.current !== seq) return;
        setHistory(next);
      } catch (err) {
        if (historySeqRef.current !== seq) return;
        setHistory(null);
        setHistoryError(errorToMessage(err));
      } finally {
        if (historySeqRef.current === seq) {
          setHistoryLoading(false);
          setHistoryRefreshing(false);
        }
      }
    },
    [selectedProviderId],
  );

  // On a provider change or reactivation the previous snapshot is cleared and a
  // fresh read is requested; leaving the view invalidates any in-flight response
  // so a hidden or superseded answer can never commit.
  useEffect(() => {
    if (!isActive || selectedProviderId === null) return;
    setHistory(null);
    setHistoryError("");
    void loadHistory();
    return () => {
      historySeqRef.current += 1;
    };
  }, [isActive, selectedProviderId, loadHistory]);

  const handleRefresh = useCallback(() => {
    void loadProviders();
    void loadHistory({ refresh: true });
  }, [loadHistory, loadProviders]);

  const duplicateNameCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const identity of identities) {
      const base = identity.provider_name || identity.provider_id;
      counts.set(base, (counts.get(base) ?? 0) + 1);
    }
    return counts;
  }, [identities]);

  const providerLabel = useCallback(
    (identity: ProviderUsageIdentity): string => {
      const base = identity.provider_name || identity.provider_id;
      let label = base;
      if ((duplicateNameCounts.get(base) ?? 0) > 1) {
        label = `${base} (${identity.provider_id})`;
      }
      if (identity.deleted) {
        label = `${label} · ${t("aiGatewayProviderHistoryDeletedTag", "Deleted")}`;
      }
      return label;
    },
    [duplicateNameCounts, t],
  );

  const providerOptions = useMemo(
    () => [
      {
        value: "",
        label: t("aiGatewayProviderHistorySelectProvider", "Select a provider"),
      },
      ...identities.map((identity) => ({
        value: identity.provider_id,
        label: providerLabel(identity),
      })),
    ],
    [identities, providerLabel, t],
  );

  // Only the current selection's own snapshot is rendered, so a late answer can
  // never paint one provider's numbers under another provider's label.
  const activeHistory =
    history && history.provider_id === selectedProviderId ? history : null;

  const hasHistoricalGap = useMemo(() => {
    if (!activeHistory || activeHistory.earliest_available_day === null) {
      return false;
    }
    if (
      activeHistory.days.some((entry) => entry.coverage === "retained_historical")
    ) {
      return true;
    }
    return (
      activeHistory.continuity_start_day !== null &&
      activeHistory.earliest_available_day < activeHistory.continuity_start_day
    );
  }, [activeHistory]);

  const trendValue = useCallback(
    (entry: ProviderDailyEntry): number | null => {
      if (!entry.metrics) return null;
      switch (metric) {
        case "tokens":
          return entry.metrics.total_tokens;
        case "requests":
          return entry.metrics.request_count;
        case "cost":
          return entry.metrics.amount;
      }
    },
    [metric],
  );

  const chartMax = useMemo(() => {
    if (!activeHistory) return 1;
    let max = 0;
    for (const entry of activeHistory.days) {
      const value = trendValue(entry);
      if (value !== null) max = Math.max(max, value);
    }
    return Math.max(1, max);
  }, [activeHistory, trendValue]);

  const selectedProviderValue = selectedProviderId ?? "";

  const renderMetricCell = (
    entry: ProviderDailyEntry,
    pick: (metrics: NonNullable<ProviderDailyEntry["metrics"]>) => string,
  ): string => (entry.metrics ? pick(entry.metrics) : "—");

  const coverageLabel = (entry: ProviderDailyEntry): string =>
    t(
      COVERAGE_LABEL_KEYS[entry.coverage],
      COVERAGE_LABEL_FALLBACKS[entry.coverage],
    );

  const rateText = (rate: number | null | undefined): string => {
    if (rate === null || rate === undefined || !Number.isFinite(rate)) {
      return t("aiGatewayProviderHistoryRateUnavailable", "N/A");
    }
    return `${Math.round(rate)}%`;
  };

  return (
    <div
      className="rounded-2xl border bg-card p-5"
      data-testid="ai-gateway-provider-usage-history"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2.5">
          <div className="rounded-lg bg-primary/10 p-2 text-primary">
            <HistoryIcon className="h-4.5 w-4.5" />
          </div>
          <div className="space-y-0.5">
            <h2 className="text-base font-semibold text-foreground">
              {t("aiGatewayProviderHistorySubTab", "Provider history")}
            </h2>
            <p className="text-xs text-muted-foreground">
              {t(
                "aiGatewayProviderHistoryDesc",
                "Inspect one provider's daily usage through today, with retained-history coverage.",
              )}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <SelectDropdown
            value={selectedProviderValue}
            options={providerOptions}
            onChange={(next) => setSelectedProviderId(next === "" ? null : next)}
            testId="ai-gateway-provider-history-provider"
            ariaLabel={t("aiGatewayProviderHistorySelectProvider", "Select a provider")}
          />
          {selectedProviderId !== null ? (
            <button
              type="button"
              data-testid="ai-gateway-provider-history-refresh"
              onClick={handleRefresh}
              disabled={historyRefreshing}
              aria-label={t("aiGatewayRefresh", "Refresh")}
              title={t("aiGatewayRefresh", "Refresh")}
              className="inline-flex h-8 w-8 items-center justify-center rounded-lg border bg-background transition hover:bg-muted disabled:opacity-50"
            >
              <RefreshCw
                className={`h-4 w-4 ${historyRefreshing ? "animate-spin" : ""}`}
              />
            </button>
          ) : null}
        </div>
      </div>

      {providersError ? (
        <div
          data-testid="ai-gateway-provider-history-providers-error"
          role="alert"
          className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
        >
          <span>
            {t(
              "aiGatewayProviderHistoryProvidersError",
              "Failed to load the provider list.",
            )}
          </span>
          <button
            type="button"
            data-testid="ai-gateway-provider-history-providers-retry"
            onClick={() => void loadProviders()}
            className="rounded-md border border-destructive/30 bg-background px-2.5 py-1 font-medium text-destructive transition hover:bg-destructive/10"
          >
            {t("retry", "Retry")}
          </button>
        </div>
      ) : null}

      {historyRefreshing ? (
        <div role="status" className="mt-3 text-xs text-muted-foreground">
          {t("aiGatewayRefreshing", "Refreshing...")}
        </div>
      ) : null}

      {historyError ? (
        <div
          role="alert"
          className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
        >
          <span
            data-testid="ai-gateway-provider-history-error"
            className="flex items-center gap-1.5"
          >
            <AlertCircle className="h-3.5 w-3.5" />
            <span>
              {t(
                "aiGatewayProviderHistoryError",
                "Failed to load provider history.",
              )}
            </span>
          </span>
          <button
            type="button"
            data-testid="ai-gateway-provider-history-retry"
            onClick={() => void loadHistory()}
            className="rounded-md border border-destructive/30 bg-background px-2.5 py-1 font-medium text-destructive transition hover:bg-destructive/10"
          >
            {t("retry", "Retry")}
          </button>
        </div>
      ) : null}

      {selectedProviderId === null ? (
        <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
          {t(
            "aiGatewayProviderHistorySelectPrompt",
            "Select a provider to inspect its daily usage history.",
          )}
        </p>
      ) : historyLoading && activeHistory === null && !historyError ? (
        <div
          role="status"
          className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground"
        >
          {t("loading", "Loading...")}
        </div>
      ) : activeHistory && activeHistory.earliest_available_day === null ? (
        <p
          data-testid="ai-gateway-provider-history-empty"
          className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground"
        >
          {t(
            "aiGatewayProviderHistoryEmpty",
            "No usage history for this provider yet.",
          )}
        </p>
      ) : activeHistory ? (
        <div className="mt-4 space-y-5">
          <div
            data-testid="ai-gateway-provider-history-coverage"
            className="flex flex-wrap items-center gap-x-5 gap-y-1.5 rounded-lg border border-border/60 bg-muted/20 px-3.5 py-2.5 text-xs"
          >
            <span className="font-medium text-muted-foreground">
              {t("aiGatewayProviderHistoryCoverageLabel", "Available range")}
            </span>
            <span className="inline-flex items-baseline gap-1.5">
              <span className="text-muted-foreground">
                {t("aiGatewayProviderHistoryEarliestLabel", "Earliest available")}
              </span>
              <span className="font-mono font-medium text-foreground">
                {activeHistory.earliest_available_day}
              </span>
            </span>
            {activeHistory.continuity_start_day ? (
              <span className="inline-flex items-baseline gap-1.5">
                <span className="text-muted-foreground">
                  {t("aiGatewayProviderHistoryContinuityLabel", "Continuous since")}
                </span>
                <span className="font-mono font-medium text-foreground">
                  {activeHistory.continuity_start_day}
                </span>
              </span>
            ) : null}
            {activeHistory.end_day ? (
              <span className="inline-flex items-baseline gap-1.5">
                <span className="text-muted-foreground">
                  {t("aiGatewayProviderHistoryEndLabel", "Through")}
                </span>
                <span className="font-mono font-medium text-foreground">
                  {activeHistory.end_day}
                </span>
              </span>
            ) : null}
          </div>

          {hasHistoricalGap ? (
            <p
              data-testid="ai-gateway-provider-history-historical-note"
              className="rounded-lg border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-400"
            >
              {t(
                "aiGatewayProviderHistoryHistoricalNote",
                "Earlier retained history describes available records only and may be incomplete; missing dates are unavailable.",
              )}
            </p>
          ) : null}

          {activeHistory.cumulative ? (
            <div
              data-testid="ai-gateway-provider-history-cumulative"
              className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-4"
            >
              {[
                {
                  key: "requests",
                  label: t("aiGatewayProviderHistoryRequests", "Requests"),
                  value: formatCount(activeHistory.cumulative.request_count),
                },
                {
                  key: "success",
                  label: t("aiGatewayProviderHistorySuccess", "Success"),
                  value: formatCount(
                    activeHistory.cumulative.successful_request_count,
                  ),
                },
                {
                  key: "failure",
                  label: t("aiGatewayProviderHistoryFailure", "Failure"),
                  value: formatCount(
                    activeHistory.cumulative.request_count -
                      activeHistory.cumulative.successful_request_count,
                  ),
                },
                {
                  key: "input",
                  label: t("aiGatewayProviderHistoryInput", "Input"),
                  value: formatGatewayTokens(activeHistory.cumulative.input_tokens),
                },
                {
                  key: "cacheRead",
                  label: t("aiGatewayProviderHistoryCacheRead", "Cache read"),
                  value: formatGatewayTokens(
                    activeHistory.cumulative.cache_read_tokens,
                  ),
                },
                {
                  key: "cacheWrite",
                  label: t("aiGatewayProviderHistoryCacheWrite", "Cache write"),
                  value: formatGatewayTokens(
                    activeHistory.cumulative.cache_write_tokens,
                  ),
                },
                {
                  key: "output",
                  label: t("aiGatewayProviderHistoryOutput", "Output"),
                  value: formatGatewayTokens(activeHistory.cumulative.output_tokens),
                },
                {
                  key: "stored",
                  label: t("aiGatewayProviderHistoryStoredTotal", "Stored total"),
                  value: formatGatewayTokens(activeHistory.cumulative.total_tokens),
                },
                {
                  key: "amount",
                  label: t("aiGatewayProviderHistoryAmount", "Amount ($)"),
                  value: formatAmount(activeHistory.cumulative.amount),
                },
                {
                  key: "unpriced",
                  label: t("aiGatewayProviderHistoryUnpriced", "Unpriced"),
                  value: formatCount(activeHistory.cumulative.unpriced_count),
                },
                {
                  key: "rate",
                  label: t("aiGatewayProviderHistoryCacheRate", "Cache hit rate"),
                  value: rateText(activeHistory.cumulative.cache_hit_rate_percent),
                },
                {
                  key: "coverage",
                  label: t("aiGatewayProviderHistoryCacheCoverage", "Rate coverage"),
                  value: `${formatCount(
                    activeHistory.cumulative.cache_rate_eligible_count,
                  )} / ${formatCount(
                    activeHistory.cumulative.successful_request_count,
                  )}`,
                },
              ].map((card) => (
                <div
                  key={card.key}
                  className="flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3"
                >
                  <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                    {card.label}
                  </span>
                  <span className="mt-1.5 text-lg font-bold tracking-tight text-foreground">
                    {card.value}
                  </span>
                </div>
              ))}
            </div>
          ) : null}

          {activeHistory.days.length > 0 ? (
            <div
              data-testid="ai-gateway-provider-history-chart"
              className="rounded-xl border bg-card p-4"
            >
              <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-1.5 text-muted-foreground">
                  <BarChart3 className="h-3.5 w-3.5" />
                  <span className="text-xs font-semibold uppercase tracking-wider">
                    {t("aiGatewayProviderHistoryChartTitle", "Daily trend")}
                  </span>
                </div>
                <SelectDropdown
                  value={metric}
                  options={METRIC_OPTIONS.map((option) => ({
                    value: option.value,
                    label: t(option.labelKey, option.fallback),
                  }))}
                  onChange={(next) => setMetric(next)}
                  testId="ai-gateway-provider-history-metric"
                  ariaLabel={t(
                    "aiGatewayProviderHistoryMetricLabel",
                    "Trend metric",
                  )}
                />
              </div>
              <div className="overflow-x-auto pb-1">
                <div className="flex h-32 items-end gap-1.5 px-1">
                  {activeHistory.days.map((entry) => {
                    const value = trendValue(entry);
                    const valueText =
                      value === null
                        ? t(
                            "aiGatewayProviderHistoryCoverageUnavailable",
                            "Unavailable",
                          )
                        : metric === "cost"
                          ? formatAmount(value)
                          : formatCount(value);
                    const height =
                      value === null
                        ? 4
                        : Math.max(4, Math.round((value / chartMax) * 100));
                    return (
                      <div
                        key={`chart-${entry.day}`}
                        className="flex min-w-[3.25rem] flex-1 flex-col items-center gap-1.5"
                      >
                        <div className="flex h-24 w-full items-end justify-center">
                          <div
                            role="img"
                            aria-label={`${entry.day}: ${valueText}`}
                            title={`${entry.day}: ${valueText}`}
                            data-testid="ai-gateway-provider-history-chart-bar"
                            className={`w-full max-w-[2rem] rounded-t transition-all ${
                              value !== null && value > 0
                                ? "bg-primary/70"
                                : "bg-muted/40"
                            }`}
                            style={{ height: `${height}%` }}
                          />
                        </div>
                        <span className="whitespace-nowrap text-[10px] text-muted-foreground">
                          {entry.day}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : null}

          {activeHistory.days.length > 0 ? (
            <div>
              <h3 className="mb-2 text-sm font-semibold">
                {t("aiGatewayProviderHistoryDaysTitle", "Daily history")}
              </h3>
              <div className="overflow-x-auto rounded-lg border">
                <table
                  data-testid="ai-gateway-provider-history-days"
                  className="w-full text-left text-xs"
                >
                  <thead className="bg-muted/50 text-muted-foreground">
                    <tr>
                      <th className="px-3 py-2 font-medium">
                        {t("aiGatewayProviderHistoryDateColumn", "Date")}
                      </th>
                      <th className="px-3 py-2 font-medium">
                        {t("aiGatewayProviderHistoryCoverageColumn", "Coverage")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryRequests", "Requests")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistorySuccess", "Success")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryFailure", "Failure")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryInput", "Input")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryCacheRead", "Cache read")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryCacheWrite", "Cache write")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryOutput", "Output")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryStoredTotal", "Stored total")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryAmount", "Amount ($)")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryUnpriced", "Unpriced")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        {t("aiGatewayProviderHistoryCacheRate", "Cache hit rate")}
                      </th>
                      <th className="px-3 py-2 text-right font-medium">
                        <span className="sr-only">
                          {t("aiGatewayProviderHistoryOpenDayColumn", "Open day")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {activeHistory.days.map((entry) => (
                      <tr
                        key={entry.day}
                        data-testid={`ai-gateway-provider-history-day-${entry.day}`}
                        data-coverage={entry.coverage}
                        data-ongoing={entry.ongoing ? "true" : "false"}
                        className="border-t"
                      >
                        <td className="px-3 py-2 whitespace-nowrap">
                          <span className="font-mono">{entry.day}</span>
                          {entry.ongoing ? (
                            <span className="ml-2 rounded-full border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
                              {t("aiGatewayProviderHistoryOngoing", "Ongoing")}
                            </span>
                          ) : null}
                        </td>
                        <td className="px-3 py-2 whitespace-nowrap text-muted-foreground">
                          {coverageLabel(entry)}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.request_count),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.successful_request_count),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.request_count - m.successful_request_count),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.input_tokens),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.cache_read_tokens),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.cache_write_tokens),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.output_tokens),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.total_tokens),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) => formatAmount(m.amount))}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.unpriced_count),
                          )}
                        </td>
                        <td className="px-3 py-2 text-right whitespace-nowrap">
                          {renderMetricCell(
                            entry,
                            (m) =>
                              `${rateText(m.cache_hit_rate_percent)} (${formatCount(
                                m.cache_rate_eligible_count,
                              )} / ${formatCount(m.successful_request_count)})`,
                          )}
                        </td>
                        <td className="px-3 py-2 text-right">
                          <button
                            type="button"
                            data-testid={`ai-gateway-provider-history-open-day-${entry.day}`}
                            onClick={() =>
                              onOpenDay?.({
                                providerId: activeHistory.provider_id,
                                day: entry.day,
                              })
                            }
                            aria-label={t(
                              "aiGatewayProviderHistoryOpenDayAria",
                              "Open requests for {{day}}",
                              { day: entry.day },
                            )}
                            className="inline-flex h-7 items-center rounded-md border bg-background px-2.5 text-xs font-medium transition hover:bg-muted"
                          >
                            {t("aiGatewayProviderHistoryViewDay", "View")}
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
