import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import i18n from "@/i18n";
import {
  Activity,
  AlertCircle,
  ArrowDownRight,
  ArrowUpRight,
  BarChart3,
  CheckCircle2,
  Database,
  DollarSign,
  HardDrive,
  HelpCircle,
  Layers,
  RefreshCw,
  ShieldCheck,
  XCircle,
  Zap,
} from "lucide-react";
import {
  aiGatewayProviderUsageHistory,
  aiGatewayUsageProviders,
  formatGatewayTokens,
  formatUsageAmount,
  resolveProviderTemplate,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
  type ProviderDailyEntry,
  type ProviderUsageHistory,
  type ProviderUsageIdentity,
} from "@/lib/aiGateway";
import { errorToMessage } from "@/lib/messages";
import { ProviderTemplateIcon } from "./ProviderTemplateIcon";
import { SelectDropdown } from "./SelectDropdown";

/**
 * One request/day action handed to the parent so it can open the existing
 * request-log view with this exact provider ID and UTC+8 day.
 */
export interface ProviderHistoryOpenDayRequest {
  providerId: string;
  providerName?: string;
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

const countFormatters = new Map<string, Intl.NumberFormat>();

/** Additive counts render as grouped numbers; an unavailable day is `—`. */
function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  const locale = i18n.resolvedLanguage ?? i18n.language ?? "en";
  let formatter = countFormatters.get(locale);
  if (!formatter) {
    formatter = new Intl.NumberFormat(locale);
    countFormatters.set(locale, formatter);
  }
  return formatter.format(value);
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

function ProviderUsageHistoryPanelComponent({
  isActive = true,
  onOpenDay,
  providers,
  templates,
}: {
  isActive?: boolean;
  onOpenDay?: (request: ProviderHistoryOpenDayRequest) => void;
  providers?: GatewayUpstreamProvider[];
  templates?: GatewayProviderTemplateView[];
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

  const currentIdentity = useMemo(
    () =>
      identities.find((item) => item.provider_id === selectedProviderId) ?? null,
    [identities, selectedProviderId],
  );

  const currentConfigProvider = useMemo(
    () => providers?.find((p) => p.id === selectedProviderId) ?? null,
    [providers, selectedProviderId],
  );

  const boundTemplateView = useMemo(() => {
    if (!currentConfigProvider?.template_id) return null;
    return resolveProviderTemplate(templates, currentConfigProvider);
  }, [currentConfigProvider, templates]);

  const templateName = boundTemplateView?.template.name || null;
  const templateIcon = boundTemplateView?.template.icon || null;
  const templateId =
    boundTemplateView?.template.id || currentConfigProvider?.template_id || null;

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
      {/* 顶部主标题栏：与用量统计与请求日志保持完全一致的设计语言 */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <h2 className="text-base font-semibold text-foreground">
            {t("aiGatewayProviderHistorySubTab", "Provider history")}
          </h2>
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

      {/* 用量详情主内容区 */}
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
          {/* 服务商详情头部概览与可用范围 */}
          <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 border-b pb-4">
            <div className="space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="text-base font-bold text-foreground">
                  {currentIdentity?.provider_name || activeHistory.provider_id}
                </h3>
                {templateName ? (
                  <span
                    data-testid="ai-gateway-provider-history-template-tag"
                    className="inline-flex items-center gap-1.5 rounded-md border border-primary/20 bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary shadow-xs"
                    title={`Template: ${templateName}`}
                  >
                    <ProviderTemplateIcon
                      icon={templateIcon}
                      templateId={templateId}
                      templateName={templateName}
                      className="h-3.5 w-3.5 shrink-0"
                    />
                    <span>{templateName}</span>
                  </span>
                ) : null}
                {currentIdentity?.deleted ? (
                  <span className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
                    {t("aiGatewayProviderHistoryDeletedTag", "Deleted")}
                  </span>
                ) : null}
              </div>
              <div className="flex items-center gap-2 text-xs text-muted-foreground font-mono">
                <span>ID: {activeHistory.provider_id}</span>
              </div>
            </div>

            {/* 可用范围胶囊 */}
            <div
              data-testid="ai-gateway-provider-history-coverage"
              className="flex flex-wrap items-center gap-x-5 gap-y-1.5 rounded-lg border border-border/60 bg-muted/20 px-3.5 py-2 text-xs"
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

          {/* 累计指标概览卡片区域（与用量统计统一风格） */}
          {activeHistory.cumulative ? (
            <div
              data-testid="ai-gateway-provider-history-cumulative"
              className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6"
            >
              {[
                {
                  key: "requests",
                  label: t("aiGatewayProviderHistoryRequests", "Requests"),
                  value: formatCount(activeHistory.cumulative.request_count),
                  icon: Activity,
                },
                {
                  key: "success",
                  label: t("aiGatewayProviderHistorySuccess", "Success"),
                  value: formatCount(
                    activeHistory.cumulative.successful_request_count,
                  ),
                  icon: CheckCircle2,
                },
                {
                  key: "failure",
                  label: t("aiGatewayProviderHistoryFailure", "Failure"),
                  value: formatCount(
                    activeHistory.cumulative.request_count -
                      activeHistory.cumulative.successful_request_count,
                  ),
                  icon: XCircle,
                },
                {
                  key: "input",
                  label: t("aiGatewayProviderHistoryInput", "Input"),
                  value: formatGatewayTokens(activeHistory.cumulative.input_tokens),
                  icon: ArrowDownRight,
                },
                {
                  key: "cacheRead",
                  label: t("aiGatewayProviderHistoryCacheRead", "Cache read"),
                  value: formatGatewayTokens(
                    activeHistory.cumulative.cache_read_tokens,
                  ),
                  icon: Database,
                },
                {
                  key: "cacheWrite",
                  label: t("aiGatewayProviderHistoryCacheWrite", "Cache write"),
                  value: formatGatewayTokens(
                    activeHistory.cumulative.cache_write_tokens,
                  ),
                  icon: HardDrive,
                },
                {
                  key: "output",
                  label: t("aiGatewayProviderHistoryOutput", "Output"),
                  value: formatGatewayTokens(activeHistory.cumulative.output_tokens),
                  icon: ArrowUpRight,
                },
                {
                  key: "stored",
                  label: t("aiGatewayProviderHistoryStoredTotal", "Tokens"),
                  value: formatGatewayTokens(activeHistory.cumulative.total_tokens),
                  icon: Layers,
                },
                {
                  key: "amount",
                  label: t("aiGatewayProviderHistoryAmount", "Amount ($)"),
                  value: formatAmount(activeHistory.cumulative.amount),
                  icon: DollarSign,
                },
                {
                  key: "unpriced",
                  label: t("aiGatewayProviderHistoryUnpriced", "Unpriced"),
                  value: formatCount(activeHistory.cumulative.unpriced_count),
                  icon: HelpCircle,
                },
                {
                  key: "rate",
                  label: t("aiGatewayProviderHistoryCacheRate", "Cache hit rate"),
                  value: rateText(activeHistory.cumulative.cache_hit_rate_percent),
                  icon: Zap,
                },
                {
                  key: "coverage",
                  label: t("aiGatewayProviderHistoryCacheCoverage", "Rate coverage"),
                  value: `${formatCount(
                    activeHistory.cumulative.cache_rate_eligible_count,
                  )} / ${formatCount(
                    activeHistory.cumulative.successful_request_count,
                  )}`,
                  icon: ShieldCheck,
                },
              ].map((card) => (
                <div
                  key={card.key}
                  className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
                >
                  <div className="flex items-center justify-between text-muted-foreground">
                    <span className="text-[11px] font-medium uppercase tracking-wider truncate">
                      {card.label}
                    </span>
                    <card.icon className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground shrink-0" />
                  </div>
                  <span className="mt-1.5 text-lg font-bold tracking-tight text-foreground font-mono">
                    {card.value}
                  </span>
                </div>
              ))}
            </div>
          ) : null}

          {/* 每日用量趋势图表 */}
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
                <div className="flex h-32 items-end gap-1.5 px-1 min-w-[20rem]">
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
                          : metric === "tokens"
                            ? formatGatewayTokens(value)
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
                        <span className="whitespace-nowrap text-[10px] text-muted-foreground font-mono">
                          {entry.day}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : null}

          {/* 每日历史明细表格（彻底解决列换行，各列全部加 whitespace-nowrap 且支持平滑横向滚动） */}
          {activeHistory.days.length > 0 ? (
            <div>
              <h3 className="mb-2 text-sm font-semibold">
                {t("aiGatewayProviderHistoryDaysTitle", "Daily history")}
              </h3>
              <div className="overflow-x-auto rounded-lg border">
                <table
                  data-testid="ai-gateway-provider-history-days"
                  className="min-w-full text-left text-xs whitespace-nowrap"
                >
                  <thead className="bg-muted/50 text-muted-foreground">
                    <tr>
                      <th className="px-3.5 py-2.5 font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryDateColumn", "Date")}
                      </th>
                      <th className="px-3.5 py-2.5 font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryCoverageColumn", "Coverage")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryRequests", "Requests")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistorySuccess", "Success")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryFailure", "Failure")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryInput", "Input")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryCacheRead", "Cache read")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryCacheWrite", "Cache write")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryOutput", "Output")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryStoredTotal", "Tokens")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryAmount", "Amount ($)")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryUnpriced", "Unpriced")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        {t("aiGatewayProviderHistoryCacheRate", "Cache hit rate")}
                      </th>
                      <th className="px-3.5 py-2.5 text-right font-medium whitespace-nowrap">
                        <span className="sr-only">
                          {t("aiGatewayProviderHistoryOpenDayColumn", "Open day")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/60">
                    {activeHistory.days.map((entry) => (
                      <tr
                        key={entry.day}
                        data-testid={`ai-gateway-provider-history-day-${entry.day}`}
                        data-coverage={entry.coverage}
                        data-ongoing={entry.ongoing ? "true" : "false"}
                        className="hover:bg-muted/40 transition-colors"
                      >
                        <td className="px-3.5 py-2.5 whitespace-nowrap">
                          <span className="font-mono font-medium">{entry.day}</span>
                          {entry.ongoing ? (
                            <span className="ml-2 rounded-full border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary whitespace-nowrap">
                              {t("aiGatewayProviderHistoryOngoing", "Ongoing")}
                            </span>
                          ) : null}
                        </td>
                        <td className="px-3.5 py-2.5 whitespace-nowrap text-muted-foreground">
                          {coverageLabel(entry)}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.request_count),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.successful_request_count),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.request_count - m.successful_request_count),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.input_tokens),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.cache_read_tokens),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.cache_write_tokens),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.output_tokens),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatGatewayTokens(m.total_tokens),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) => formatAmount(m.amount))}
                        </td>
                        <td className="px-3.5 py-2.5 text-right font-mono tabular-nums whitespace-nowrap">
                          {renderMetricCell(entry, (m) =>
                            formatCount(m.unpriced_count),
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right whitespace-nowrap font-mono tabular-nums">
                          {renderMetricCell(
                            entry,
                            (m) =>
                              `${rateText(m.cache_hit_rate_percent)} (${formatCount(
                                m.cache_rate_eligible_count,
                              )} / ${formatCount(m.successful_request_count)})`,
                          )}
                        </td>
                        <td className="px-3.5 py-2.5 text-right whitespace-nowrap">
                          <button
                            type="button"
                            data-testid={`ai-gateway-provider-history-open-day-${entry.day}`}
                            onClick={() => {
                              const activeProviderName =
                                currentIdentity?.provider_name ??
                                currentConfigProvider?.name ??
                                activeHistory.provider_id;
                              onOpenDay?.({
                                providerId: activeHistory.provider_id,
                                providerName: activeProviderName,
                                day: entry.day,
                              });
                            }}
                            aria-label={t(
                              "aiGatewayProviderHistoryOpenDayAria",
                              "Open requests for {{day}}",
                              { day: entry.day },
                            )}
                            className="inline-flex h-7 items-center rounded-md border bg-background px-2.5 text-xs font-medium transition hover:bg-muted whitespace-nowrap"
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

export const ProviderUsageHistoryPanel = memo(ProviderUsageHistoryPanelComponent);
