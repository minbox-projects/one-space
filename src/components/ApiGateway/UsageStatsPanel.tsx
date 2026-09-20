import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  BarChart3,
  ChevronDown,
  ChevronUp,
  Cpu,
  Database,
  DollarSign,
  Gauge,
  HardDriveUpload,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import {
  apiGatewayUsageStats,
  formatGatewayTokens,
  formatUsageAmount,
  formatUsageRowAmount,
  USAGE_RANGE_KEYS,
  usageRangeToDays,
  type UsageBucket,
  type UsageMetrics,
  type UsageRangeKey,
  type UsageStats,
} from "@/lib/apiGateway";
import { errorToMessage } from "@/lib/messages";
import { SelectDropdown } from "./SelectDropdown";

function formatCount(value: number): string {
  return new Intl.NumberFormat().format(value);
}

const RANGE_LABEL_KEYS: Record<UsageRangeKey, string> = {
  today: "apiGatewayRangeToday",
  "7d": "apiGatewayRange7d",
  "15d": "apiGatewayRange15d",
  "30d": "apiGatewayRange30d",
  all: "apiGatewayRangeAll",
};

const RANGE_LABEL_FALLBACKS: Record<UsageRangeKey, string> = {
  today: "Today",
  "7d": "7d",
  "15d": "15d",
  "30d": "30d",
  all: "All",
};

type TrendMetric = "tokens" | "requests" | "cost";

function selectBuckets(stats: UsageStats) {
  if (stats.granularity === "hour") {
    return stats.buckets.filter((bucket) => bucket.request_count > 0).slice(0, 24);
  }
  return stats.buckets;
}

function formatBucketLabel(label: string): string {
  if (label.includes("-")) {
    const parts = label.split("-");
    if (parts.length === 3) return `${parts[1]}-${parts[2]}`;
  }
  return label;
}

function formatBucketTooltip(
  bucket: UsageBucket,
  t: ReturnType<typeof useTranslation>["t"],
): string {
  return [
    bucket.label,
    `${t("apiGatewayUsageTokens", "Tokens")}: ${formatGatewayTokens(bucket.total_tokens)} (${formatCount(bucket.total_tokens)})`,
    `${t("apiGatewayUsageRequests", "Requests")}: ${formatCount(bucket.request_count)}`,
    `${t("apiGatewayUsageCost", "Cost")}: ${formatUsageRowAmount(bucket)}`,
    `${t("apiGatewayUsageInput", "Input")}: ${formatGatewayTokens(bucket.input_tokens)}`,
    `${t("apiGatewayUsageCacheRead", "Cache read")}: ${formatGatewayTokens(bucket.cache_read_tokens)}`,
    `${t("apiGatewayUsageCacheWrite", "Cache write")}: ${formatGatewayTokens(bucket.cache_write_tokens)}`,
    `${t("apiGatewayUsageOutput", "Output")}: ${formatGatewayTokens(bucket.output_tokens)}`,
  ].join("\n");
}

function UsageAnalysisRow({
  label,
  metrics,
  indent = false,
  sharePercent,
}: {
  label: string;
  metrics: UsageMetrics;
  indent?: boolean;
  sharePercent?: number;
}) {
  const { t } = useTranslation();
  const cacheBase = metrics.input_tokens + metrics.cache_read_tokens;
  const cacheHitRate = cacheBase > 0 ? (metrics.cache_read_tokens / cacheBase) * 100 : 0;
  const cacheHitText = metrics.request_count > 0 ? `${Math.round(cacheHitRate)}%` : "-";

  return (
    <tr
      className="border-t"
      data-testid={indent ? "api-gateway-usage-provider-row" : "api-gateway-usage-model-row"}
    >
      <td className={`px-3 py-2 ${indent ? "pl-8 text-muted-foreground" : "font-medium"}`}>
        <div className="flex items-center justify-between gap-3">
          <span className="truncate">{label}</span>
          {sharePercent !== undefined && (
            <div
              className="flex items-center shrink-0"
              title={`${t("apiGatewayUsageShare", "Share")}: ${Math.round(sharePercent)}%`}
              aria-label={`${Math.round(sharePercent)}%`}
              data-testid="api-gateway-usage-share-bar"
            >
              <div className="h-1.5 w-14 rounded-full bg-muted/80 overflow-hidden">
                <div
                  className={`h-full rounded-full transition-all ${
                    indent ? "bg-primary/50" : "bg-primary"
                  }`}
                  style={{ width: `${Math.min(100, Math.max(0, sharePercent))}%` }}
                />
              </div>
            </div>
          )}
        </div>
      </td>
      <td className="px-3 py-2 text-right">{formatCount(metrics.request_count)}</td>
      <td className="px-3 py-2 text-right" title={formatCount(metrics.input_tokens)}>
        {formatGatewayTokens(metrics.input_tokens)}
      </td>
      <td className="px-3 py-2 text-right" title={formatCount(metrics.cache_read_tokens)}>
        {formatGatewayTokens(metrics.cache_read_tokens)}
      </td>
      <td className="px-3 py-2 text-right" title={formatCount(metrics.cache_write_tokens)}>
        {formatGatewayTokens(metrics.cache_write_tokens)}
      </td>
      <td className="px-3 py-2 text-right" title={formatCount(metrics.output_tokens)}>
        {formatGatewayTokens(metrics.output_tokens)}
      </td>
      <td
        className="px-3 py-2 text-right"
        title={
          cacheBase > 0
            ? `${formatGatewayTokens(metrics.cache_read_tokens)} / ${formatGatewayTokens(cacheBase)}`
            : undefined
        }
        data-testid={indent ? "api-gateway-usage-provider-cache-hit" : "api-gateway-usage-model-cache-hit"}
      >
        {cacheHitText}
      </td>
      <td className="px-3 py-2 text-right">{formatUsageRowAmount(metrics)}</td>
    </tr>
  );
}

export function UsageStatsPanel({ isActive = true }: { isActive?: boolean }) {
  const { t } = useTranslation();
  const [range, setRange] = useState<UsageRangeKey>("today");
  const [trendMetric, setTrendMetric] = useState<TrendMetric>("tokens");
  const [showBucketTable, setShowBucketTable] = useState(true);
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const requestSeqRef = useRef(0);

  const load = useCallback(
    async (options?: { refresh?: boolean }) => {
      const seq = requestSeqRef.current + 1;
      requestSeqRef.current = seq;
      setError("");
      if (options?.refresh) setRefreshing(true);
      else setLoading(true);
      try {
        const next = await apiGatewayUsageStats(usageRangeToDays(range));
        if (requestSeqRef.current !== seq) return;
        setStats(next);
      } catch (err) {
        if (requestSeqRef.current !== seq) return;
        setError(errorToMessage(err));
      } finally {
        if (requestSeqRef.current === seq) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    },
    [range],
  );

  useEffect(() => {
    if (!isActive) return;
    void load();
  }, [isActive, load]);

  const buckets = useMemo(() => (stats ? selectBuckets(stats) : []), [stats]);

  const peakBucket = useMemo(() => {
    if (buckets.length === 0) return null;
    return buckets.reduce((prev, curr) => {
      if (trendMetric === "tokens") {
        return curr.total_tokens > prev.total_tokens ? curr : prev;
      }
      if (trendMetric === "requests") {
        return curr.request_count > prev.request_count ? curr : prev;
      }
      return curr.amount > prev.amount ? curr : prev;
    }, buckets[0]);
  }, [buckets, trendMetric]);

  const peakValue = useMemo(() => {
    if (!peakBucket) return 0;
    if (trendMetric === "tokens") return peakBucket.total_tokens;
    if (trendMetric === "requests") return peakBucket.request_count;
    return peakBucket.amount;
  }, [peakBucket, trendMetric]);

  const maxMetricVal = useMemo(() => {
    if (buckets.length === 0) return 1;
    if (trendMetric === "tokens") {
      return Math.max(1, ...buckets.map((b) => b.total_tokens));
    }
    if (trendMetric === "requests") {
      return Math.max(1, ...buckets.map((b) => b.request_count));
    }
    return Math.max(0.0001, ...buckets.map((b) => b.amount));
  }, [buckets, trendMetric]);

  const cacheHitRate = useMemo(() => {
    if (!stats) return 0;
    const base = stats.input_tokens + stats.cache_read_tokens;
    return base > 0 ? (stats.cache_read_tokens / base) * 100 : 0;
  }, [stats]);

  return (
    <div className="rounded-2xl border bg-card p-5" data-testid="api-gateway-usage-stats">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <h2 className="text-base font-semibold text-foreground">
            {t("apiGatewayUsageTab", "Usage")}
          </h2>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <SelectDropdown
            value={range}
            options={USAGE_RANGE_KEYS.map((key) => ({
              value: key,
              label: t(RANGE_LABEL_KEYS[key], RANGE_LABEL_FALLBACKS[key]),
            }))}
            onChange={(nextRange) => setRange(nextRange)}
            testId="api-gateway-usage-range"
            ariaLabel={t("apiGatewayRangeToday", "Time range")}
          />
          <button
            type="button"
            onClick={() => void load({ refresh: true })}
            disabled={refreshing}
            aria-label={t("apiGatewayRefresh", "Refresh")}
            title={t("apiGatewayRefresh", "Refresh")}
            className="inline-flex h-8 w-8 items-center justify-center rounded-lg border bg-background transition hover:bg-muted disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />
          </button>
        </div>
      </div>

      {refreshing ? (
        <div role="status" className="mt-3 text-xs text-muted-foreground">
          {t("apiGatewayRefreshing", "Refreshing...")}
        </div>
      ) : null}

      {error ? (
        <div
          role="alert"
          className="mt-3 rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
        >
          {error}
        </div>
      ) : null}

      {!stats ? (
        loading ? (
          <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
            {t("loading", "Loading...")}
          </p>
        ) : null
      ) : stats.request_count === 0 ? (
        <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
          {t("apiGatewayUsageEmpty", "No usage records in this range.")}
        </p>
      ) : (
        <div className="mt-4 space-y-5">
          {/* 指标概览卡片区域（3个核心 + 4个细分 + 命中率） */}
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-tokens"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageTokens", "Tokens")}
                </span>
                <Cpu className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div
                className="mt-1.5 text-lg font-bold tracking-tight text-foreground"
                title={formatCount(stats.total_tokens)}
              >
                {formatGatewayTokens(stats.total_tokens)}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-requests"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageRequests", "Requests")}
                </span>
                <Activity className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div className="mt-1.5 text-lg font-bold tracking-tight text-foreground">
                {formatCount(stats.request_count)}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-cost"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageCost", "Cost")}
                </span>
                <DollarSign className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div className="mt-1.5 text-lg font-bold tracking-tight text-foreground">
                {formatUsageAmount(stats.amount)}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-cache-hit"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageAvgCacheHit", "Avg Cache Hit")}
                </span>
                <Gauge className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div className="mt-1.5 text-lg font-bold tracking-tight text-foreground">
                {`${Math.round(cacheHitRate)}%`}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-input"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageInput", "Input")}
                </span>
                <ArrowDownLeft className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div
                className="mt-1.5 text-base font-bold tracking-tight text-foreground"
                title={formatCount(stats.input_tokens)}
              >
                {formatGatewayTokens(stats.input_tokens)}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-cache-read"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageCacheRead", "Cache read")}
                </span>
                <Database className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div
                className="mt-1.5 text-base font-bold tracking-tight text-foreground"
                title={formatCount(stats.cache_read_tokens)}
              >
                {formatGatewayTokens(stats.cache_read_tokens)}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-cache-write"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageCacheWrite", "Cache write")}
                </span>
                <HardDriveUpload className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div
                className="mt-1.5 text-base font-bold tracking-tight text-foreground"
                title={formatCount(stats.cache_write_tokens)}
              >
                {formatGatewayTokens(stats.cache_write_tokens)}
              </div>
            </div>
            <div
              className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30"
              data-testid="api-gateway-usage-card-output"
            >
              <div className="flex items-center justify-between text-muted-foreground">
                <span className="text-[11px] font-medium uppercase tracking-wider">
                  {t("apiGatewayUsageOutput", "Output")}
                </span>
                <ArrowUpRight className="h-3.5 w-3.5 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
              </div>
              <div
                className="mt-1.5 text-base font-bold tracking-tight text-foreground"
                title={formatCount(stats.output_tokens)}
              >
                {formatGatewayTokens(stats.output_tokens)}
              </div>
            </div>
          </div>

          {stats.unpriced_count > 0 ? (
            <div
              data-testid="api-gateway-usage-unpriced-hint"
              className="rounded-lg border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-700"
            >
              {t(
                "apiGatewayUsageUnpricedHint",
                "{{count}} requests have no configured price and are excluded from the total.",
                { count: stats.unpriced_count },
              )}
            </div>
          ) : null}

          {/* 时间趋势柱状图卡片 */}
          {buckets.length > 0 ? (
            <div
              className="rounded-xl border bg-card p-4 space-y-3.5"
              data-testid="api-gateway-usage-trend-card"
            >
              <div className="flex flex-col gap-2.5 sm:flex-row sm:items-center sm:justify-between">
                <div className="flex items-center gap-2 flex-wrap">
                  <div className="flex items-center gap-1.5 text-muted-foreground">
                    <BarChart3 className="h-3.5 w-3.5 text-muted-foreground/70" />
                    <span className="text-xs font-semibold uppercase tracking-wider">
                      {t("apiGatewayUsageTrend", "Usage Trend")}
                    </span>
                  </div>
                  {peakBucket && peakValue > 0 && (
                    <span
                      className="inline-flex items-center gap-1 rounded-md border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground"
                      data-testid="api-gateway-usage-peak-badge"
                    >
                      <Sparkles className="h-3 w-3 shrink-0 text-muted-foreground/70" />
                      <span>
                        {t("apiGatewayUsagePeakDetail", {
                          time: peakBucket.label,
                          tokens: formatGatewayTokens(peakBucket.total_tokens),
                          requests: formatCount(peakBucket.request_count),
                        })}
                      </span>
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-1 self-end sm:self-auto">
                  <div
                    className="inline-flex rounded-lg border bg-muted/50 p-0.5 text-xs"
                    role="group"
                    aria-label={t("apiGatewayUsageTrend", "Usage Trend")}
                  >
                    <button
                      type="button"
                      onClick={() => setTrendMetric("tokens")}
                      className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                        trendMetric === "tokens"
                          ? "bg-background text-foreground shadow-xs font-semibold"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                      data-testid="api-gateway-trend-view-tokens"
                    >
                      {t("apiGatewayUsageViewTokens", "Tokens")}
                    </button>
                    <button
                      type="button"
                      onClick={() => setTrendMetric("requests")}
                      className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                        trendMetric === "requests"
                          ? "bg-background text-foreground shadow-xs font-semibold"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                      data-testid="api-gateway-trend-view-requests"
                    >
                      {t("apiGatewayUsageViewRequests", "Requests")}
                    </button>
                    <button
                      type="button"
                      onClick={() => setTrendMetric("cost")}
                      className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                        trendMetric === "cost"
                          ? "bg-background text-foreground shadow-xs font-semibold"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                      data-testid="api-gateway-trend-view-cost"
                    >
                      {t("apiGatewayUsageViewCost", "Cost")}
                    </button>
                  </div>
                </div>
              </div>

              {/* 柱状条展示 */}
              <div className="pt-2">
                <div
                  className="flex h-32 items-end gap-1.5 overflow-x-auto pb-1 px-1"
                  data-testid="api-gateway-usage-bars-container"
                >
                  {buckets.map((bucket) => {
                    const val =
                      trendMetric === "tokens"
                        ? bucket.total_tokens
                        : trendMetric === "requests"
                          ? bucket.request_count
                          : bucket.amount;
                    const height =
                      maxMetricVal > 0
                        ? Math.max(4, Math.round((val / maxMetricVal) * 100))
                        : 4;
                    return (
                      <div
                        key={`chart-bar-${bucket.label}`}
                        className="group flex min-w-[1.25rem] flex-1 flex-col items-center gap-1.5"
                      >
                        <div className="flex h-24 w-full items-end justify-center">
                          <div
                            className={`w-full max-w-[2.5rem] rounded-t transition-all group-hover:opacity-100 ${
                              val > 0
                                ? trendMetric === "cost"
                                  ? "bg-emerald-600/70 group-hover:bg-emerald-600"
                                  : "bg-primary/70 group-hover:bg-primary"
                                : "bg-muted/40"
                            }`}
                            style={{ height: `${height}%` }}
                            title={formatBucketTooltip(bucket, t)}
                            data-testid="api-gateway-usage-trend-bar"
                          />
                        </div>
                        <span className="truncate text-[10px] text-muted-foreground group-hover:text-foreground">
                          {formatBucketLabel(bucket.label)}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : null}

          {/* 时间分布详细表格 */}
          {buckets.length > 0 ? (
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">
                  {t("apiGatewayUsageTimeDistribution", "Time distribution")}
                </h3>
                <button
                  type="button"
                  onClick={() => setShowBucketTable((prev) => !prev)}
                  className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  data-testid="api-gateway-toggle-bucket-table"
                >
                  <span>
                    {showBucketTable
                      ? t("apiGatewayUsageHideTable", "Hide Details")
                      : t("apiGatewayUsageShowTable", "Show Details")}
                  </span>
                  {showBucketTable ? (
                    <ChevronUp className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronDown className="h-3.5 w-3.5" />
                  )}
                </button>
              </div>
              {showBucketTable && (
                <div className="overflow-x-auto rounded-lg border">
                  <table
                    className="w-full text-left text-xs"
                    data-testid="api-gateway-usage-buckets"
                  >
                    <thead className="bg-muted/50 text-muted-foreground">
                      <tr>
                        <th className="px-3 py-2 font-medium">
                          {stats.granularity === "hour"
                            ? t("apiGatewayLogsTimeColumn", "Time")
                            : t("date", "Date")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("apiGatewayUsageRequests", "Requests")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("apiGatewayUsageTokens", "Tokens")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("apiGatewayUsageCostColumn", "Cost ($)")}
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {buckets.map((bucket) => (
                        <tr
                          key={bucket.label}
                          className="border-t"
                          data-testid="api-gateway-usage-bucket-row"
                        >
                          <td className="px-3 py-2 font-mono">{bucket.label}</td>
                          <td className="px-3 py-2 text-right">
                            {formatCount(bucket.request_count)}
                          </td>
                          <td
                            className="px-3 py-2 text-right"
                            title={formatCount(bucket.total_tokens)}
                          >
                            {formatGatewayTokens(bucket.total_tokens)}
                          </td>
                          <td className="px-3 py-2 text-right">
                            {formatUsageRowAmount(bucket)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          ) : null}

          {/* 模型用量分析（含占比 Share % 可视化） */}
          <div>
            <h3 className="mb-2 text-sm font-semibold">
              {t("apiGatewayUsageAnalysis", "Usage analysis")}
            </h3>
            <div className="overflow-x-auto rounded-lg border">
              <table
                className="w-full text-left text-xs"
                data-testid="api-gateway-usage-models"
              >
                <thead className="bg-muted/50 text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">
                      {t("apiGatewayUsageModelColumn", "Model")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageRequests", "Requests")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageInput", "Input")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageCacheRead", "Cache read")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageCacheWrite", "Cache write")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageOutput", "Output")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageCacheHit", "Cache hit")}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t("apiGatewayUsageCostColumn", "Cost ($)")}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {stats.models.map((model) => {
                    const modelShare =
                      stats.total_tokens > 0
                        ? (model.total_tokens / stats.total_tokens) * 100
                        : 0;
                    return (
                      <Fragment key={model.local_model}>
                        <UsageAnalysisRow
                          label={model.local_model}
                          metrics={model}
                          sharePercent={modelShare}
                        />
                        {model.providers.map((provider) => {
                          const providerShare =
                            model.total_tokens > 0
                              ? (provider.total_tokens / model.total_tokens) * 100
                              : 0;
                          return (
                            <UsageAnalysisRow
                              key={`${model.local_model}-${provider.provider_id}`}
                              label={provider.provider_name}
                              metrics={provider}
                              indent
                              sharePercent={providerShare}
                            />
                          );
                        })}
                      </Fragment>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

