import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw, Tags } from "lucide-react";
import {
  apiGatewayUsageStats,
  formatUsageAmount,
  formatUsageRowAmount,
  USAGE_RANGE_KEYS,
  usageRangeToDays,
  type UsageMetrics,
  type UsageRangeKey,
  type UsageStats,
} from "@/lib/apiGateway";
import { errorToMessage } from "@/lib/messages";
import { ModelPriceDialog } from "./ModelPriceDialog";
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

function selectBuckets(stats: UsageStats) {
  if (stats.granularity === "hour") {
    return stats.buckets.filter((bucket) => bucket.request_count > 0).slice(0, 24);
  }
  return stats.buckets;
}

function UsageAnalysisRow({
  label,
  metrics,
  indent = false,
}: {
  label: string;
  metrics: UsageMetrics;
  indent?: boolean;
}) {
  return (
    <tr
      className="border-t"
      data-testid={indent ? "api-gateway-usage-provider-row" : "api-gateway-usage-model-row"}
    >
      <td className={`px-3 py-2 ${indent ? "pl-8 text-muted-foreground" : "font-medium"}`}>
        {label}
      </td>
      <td className="px-3 py-2 text-right">{formatCount(metrics.request_count)}</td>
      <td className="px-3 py-2 text-right">{formatCount(metrics.input_tokens)}</td>
      <td className="px-3 py-2 text-right">{formatCount(metrics.cache_read_tokens)}</td>
      <td className="px-3 py-2 text-right">{formatCount(metrics.cache_write_tokens)}</td>
      <td className="px-3 py-2 text-right">{formatCount(metrics.output_tokens)}</td>
      <td className="px-3 py-2 text-right">{formatUsageRowAmount(metrics)}</td>
    </tr>
  );
}

export function UsageStatsPanel({ isActive = true }: { isActive?: boolean }) {
  const { t } = useTranslation();
  const [range, setRange] = useState<UsageRangeKey>("today");
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [pricesOpen, setPricesOpen] = useState(false);
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

  const buckets = stats ? selectBuckets(stats) : [];

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
          <button
            type="button"
            onClick={() => setPricesOpen(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium transition hover:bg-muted"
          >
            <Tags className="h-3.5 w-3.5" />
            {t("apiGatewayModelPrices", "Model prices")}
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
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <div
              className="rounded-xl border bg-muted/20 px-4 py-3"
              data-testid="api-gateway-usage-card-tokens"
            >
              <div className="text-[11px] font-medium uppercase text-muted-foreground">
                {t("apiGatewayUsageTokens", "Tokens")}
              </div>
              <div className="mt-1 text-lg font-semibold">
                {formatCount(stats.total_tokens)}
              </div>
            </div>
            <div
              className="rounded-xl border bg-muted/20 px-4 py-3"
              data-testid="api-gateway-usage-card-requests"
            >
              <div className="text-[11px] font-medium uppercase text-muted-foreground">
                {t("apiGatewayUsageRequests", "Requests")}
              </div>
              <div className="mt-1 text-lg font-semibold">
                {formatCount(stats.request_count)}
              </div>
            </div>
            <div
              className="rounded-xl border bg-muted/20 px-4 py-3"
              data-testid="api-gateway-usage-card-cost"
            >
              <div className="text-[11px] font-medium uppercase text-muted-foreground">
                {t("apiGatewayUsageCost", "Cost")}
              </div>
              <div className="mt-1 text-lg font-semibold">
                {formatUsageAmount(stats.amount)}
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

          {buckets.length > 0 ? (
            <div>
              <h3 className="mb-2 text-sm font-semibold">
                {t("apiGatewayUsageTimeDistribution", "Time distribution")}
              </h3>
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
                        <td className="px-3 py-2 text-right">
                          {formatCount(bucket.total_tokens)}
                        </td>
                        <td className="px-3 py-2 text-right">
                          {formatUsageRowAmount(bucket)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          ) : null}

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
                      {t("apiGatewayUsageCostColumn", "Cost ($)")}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {stats.models.map((model) => (
                    <Fragment key={model.local_model}>
                      <UsageAnalysisRow label={model.local_model} metrics={model} />
                      {model.providers.map((provider) => (
                        <UsageAnalysisRow
                          key={`${model.local_model}-${provider.provider_id}`}
                          label={provider.provider_name}
                          metrics={provider}
                          indent
                        />
                      ))}
                    </Fragment>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      <ModelPriceDialog
        open={pricesOpen}
        onOpenChange={setPricesOpen}
        onSaved={() => void load()}
      />
    </div>
  );
}
