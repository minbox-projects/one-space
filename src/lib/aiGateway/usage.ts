import type { UsageGroupBy, UsageLogResult, UsageMetrics, UsageRangeKey } from "./types";

export const USAGE_RANGE_KEYS: readonly UsageRangeKey[] = [
  "today",
  "yesterday",
  "7d",
  "15d",
  "30d",
  "all",
];


const UTC8_OFFSET_MS = 8 * 60 * 60 * 1000;


function pad2(value: number): string {
  return String(value).padStart(2, "0");
}


/** Format a millisecond timestamp as `YYYY-MM-DD HH:mm:ss` in UTC+8. */
export function formatUtc8DateTime(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(
    date.getUTCDate(),
  )} ${pad2(date.getUTCHours())}:${pad2(date.getUTCMinutes())}:${pad2(date.getUTCSeconds())}`;
}


/** Format a millisecond timestamp as `YYYY-MM-DD` in UTC+8. */
export function formatUtc8Day(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(
    date.getUTCDate(),
  )}`;
}


/** Format a millisecond timestamp as `HH:00` in UTC+8. */
export function formatUtc8Hour(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${pad2(date.getUTCHours())}:00`;
}


/** Format an amount with 4 decimals; missing amounts render as `—`. */
export function formatUsageAmount(amount: number | null | undefined): string {
  if (amount === null || amount === undefined || Number.isNaN(amount)) return "—";
  return amount.toFixed(4);
}


/** Format an amount with 4 decimals and USD currency symbol ($); missing amounts render as `—`. */
export function formatUsageCurrency(amount: number | null | undefined): string {
  const formatted = formatUsageAmount(amount);
  return formatted === "—" ? "—" : `$${formatted}`;
}


/**
 * Format a per-attempt latency in milliseconds as value-only seconds with
 * three decimals (e.g. `5.000`); the unit is rendered separately by the caller
 * so it can stay muted and independent of the value color.
 * - Returns "—" for null, undefined, non-finite or negative values.
 */
export function formatGatewayDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return "—";
  }
  return (ms / 1000).toFixed(3);
}


/**
 * Returns Tailwind text color classes for request duration based on latency.
 * Each band carries a readable light-theme shade plus a matching dark: variant.
 * - < 3s (< 3000ms): green (text-emerald-600 dark:text-emerald-400)
 * - 3s ~ 15s (3000ms ~ 15000ms): amber/yellow (text-amber-600 dark:text-amber-400)
 * - > 15s (> 15000ms): red/rose (text-rose-600 dark:text-rose-400)
 * - null/undefined/negative: muted foreground (text-muted-foreground)
 */
export function getGatewayDurationColorClass(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return "text-muted-foreground";
  }
  if (ms < 3000) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (ms <= 15000) {
    return "text-amber-600 dark:text-amber-400";
  }
  return "text-rose-600 dark:text-rose-400";
}


/**
 * Format an average output rate in tokens per second as value-only two-decimal
 * text (e.g. `50.00`); the `t/s` unit is rendered separately by the caller.
 * - Returns "—" for null, undefined, non-finite or negative values, so an
 *   explicit `0` stays a measured value distinct from unavailable.
 */
export function formatGatewayTps(tps: number | null | undefined): string {
  if (tps === null || tps === undefined || !Number.isFinite(tps) || tps < 0) {
    return "—";
  }
  return tps.toFixed(2);
}


/**
 * Returns Tailwind text color classes for first-token latency, classified on
 * the raw millisecond value (never the display-rounded seconds).
 * - < 1000ms: green (text-emerald-600 dark:text-emerald-400)
 * - 1000ms ~ 3000ms inclusive: amber (text-amber-600 dark:text-amber-400)
 * - > 3000ms: rose (text-rose-600 dark:text-rose-400)
 * - null/undefined/non-finite/negative: muted foreground
 */
export function getGatewayFirstTokenColorClass(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return "text-muted-foreground";
  }
  if (ms < 1000) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (ms <= 3000) {
    return "text-amber-600 dark:text-amber-400";
  }
  return "text-rose-600 dark:text-rose-400";
}


/**
 * Returns Tailwind text color classes for average TPS, classified on the raw
 * value (never the display-rounded text).
 * - >= 60: green (text-emerald-600 dark:text-emerald-400)
 * - 20 ~ < 60: amber (text-amber-600 dark:text-amber-400)
 * - < 20: rose (text-rose-600 dark:text-rose-400)
 * - null/undefined/non-finite/negative: muted foreground
 */
export function getGatewayTpsColorClass(tps: number | null | undefined): string {
  if (tps === null || tps === undefined || !Number.isFinite(tps) || tps < 0) {
    return "text-muted-foreground";
  }
  if (tps >= 60) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (tps >= 20) {
    return "text-amber-600 dark:text-amber-400";
  }
  return "text-rose-600 dark:text-rose-400";
}


/**
 * Format a token count using compact Chinese units (万, 百万, 千万, 亿),
 * matching the formatting used in AI Usage Stats.
 * Numbers below 10,000 are formatted with thousand separators.
 * Null, undefined, NaN, or non-finite values default to "0".
 */
export function formatGatewayTokens(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "0";
  }
  const absValue = Math.abs(value);
  const units = [
    { threshold: 100_000_000, suffix: "亿" },
    { threshold: 10_000_000, suffix: "千万" },
    { threshold: 1_000_000, suffix: "百万" },
    { threshold: 10_000, suffix: "万" },
  ];
  const unit = units.find((item) => absValue >= item.threshold);
  if (!unit) {
    return new Intl.NumberFormat().format(value);
  }

  const formatted = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
  }).format(value / unit.threshold);
  return `${formatted}${unit.suffix}`;
}


/**
 * Formats cache token counts into "xxK" format (e.g. 10100 -> "10.1K", 10000 -> "10K", 500 -> "0.5K", 0 -> "0K").
 * Null, undefined, NaN, or non-finite values default to "0K".
 */
export function formatCacheTokensK(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value) || value <= 0) {
    return "0K";
  }
  const inK = value / 1000;
  const formatted = new Intl.NumberFormat("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 1,
  }).format(inK);
  return `${formatted}K`;
}


/**
 * A row is "unpriced" only when every request in scope lacks a price row.
 * A partially priced row still shows its priced amount.
 */
export function isUnpricedOnly(
  metrics: Pick<UsageMetrics, "request_count" | "unpriced_count">,
): boolean {
  return (
    metrics.request_count > 0 &&
    metrics.unpriced_count > 0 &&
    metrics.unpriced_count === metrics.request_count
  );
}


/** Amount cell for a usage-analysis row (`—` when fully unpriced). */
export function formatUsageRowAmount(metrics: UsageMetrics): string {
  if (isUnpricedOnly(metrics)) return "—";
  return formatUsageAmount(metrics.amount);
}


/** Display label for a grouped request-log row. */
export function formatUsageGroupLabel(
  _groupBy: UsageGroupBy,
  group: string,
): string {
  return group && group.trim() !== "" ? group : "—";
}


/** Clamp a 1-based page number into the valid range for the given page count. */
export function clampUsagePage(page: number, totalPages: number): number {
  if (!Number.isFinite(page)) return 1;
  if (!Number.isFinite(totalPages) || totalPages < 1) return 1;
  const floored = Math.floor(page);
  if (floored < 1) return 1;
  return Math.min(floored, Math.floor(totalPages));
}


/** Stable i18n key for a request-log status result. */
export function usageStatusTranslationKey(result: UsageLogResult): string {
  switch (result) {
    case "success":
      return "aiGatewayStatusSuccess";
    case "failure":
      return "aiGatewayStatusFailure";
  }
}
