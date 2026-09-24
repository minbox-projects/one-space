import type { GatewayUpstreamProtocol, UsageRangeKey } from "@/lib/aiGateway";

/**
 * Format a quota/usage window reset timestamp for display.
 *
 * Numeric seconds/milliseconds, numeric strings and parseable date strings are
 * accepted. `null`/empty/zero values resolve through `defaultOffsetHours`
 * relative to `baseNow` when provided; otherwise `null` is returned.
 */
export function formatResetTime(
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
      const ms = resetAt < 10_000_000_000 ? resetAt * 1000 : resetAt;
      parsed = new Date(ms);
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
          const ms = num < 10_000_000_000 ? num * 1000 : num;
          parsed = new Date(ms);
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
    const nowMs =
      baseNow instanceof Date
        ? baseNow.getTime()
        : typeof baseNow === "number"
          ? baseNow
          : Date.now();
    parsed = new Date(nowMs + defaultOffsetHours * 3600 * 1000);
  }

  if (!parsed || Number.isNaN(parsed.getTime())) {
    return null;
  }

  return new Intl.DateTimeFormat(undefined, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(parsed);
}

/**
 * Tailwind classes for an upstream protocol badge. `bordered` adds the
 * outlined variant used by the template picker.
 */
export function protocolBadgeClass(
  protocol: GatewayUpstreamProtocol | null | undefined,
  options?: { bordered?: boolean },
): string {
  const isResponses = protocol === "responses";
  const tone = isResponses
    ? "bg-purple-500/10 text-purple-600 dark:text-purple-400"
    : "bg-blue-500/10 text-blue-600 dark:text-blue-400";
  if (!options?.bordered) return tone;
  return `${tone} border ${isResponses ? "border-purple-500/20" : "border-blue-500/20"}`;
}

/** i18n keys for the shared usage quick-range labels. */
export const RANGE_LABEL_KEYS: Record<UsageRangeKey, string> = {
  today: "aiGatewayRangeToday",
  yesterday: "aiGatewayRangeYesterday",
  "7d": "aiGatewayRange7d",
  "15d": "aiGatewayRange15d",
  "30d": "aiGatewayRange30d",
  all: "aiGatewayRangeAll",
};

/** Fallback labels for the shared usage quick-range keys. */
export const RANGE_LABEL_FALLBACKS: Record<UsageRangeKey, string> = {
  today: "Today",
  yesterday: "Yesterday",
  "7d": "7d",
  "15d": "15d",
  "30d": "30d",
  all: "All",
};
