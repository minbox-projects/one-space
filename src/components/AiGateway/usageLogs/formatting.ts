import type { TFunction } from "i18next";
import i18n from "@/i18n";
import type {
  ModelPoolNativeReasoning,
  UsageGroupBy,
  UsageLogResult,
} from "@/lib/aiGateway";

const countFormatters = new Map<string, Intl.NumberFormat>();

export function formatCount(value: number): string {
  const locale = i18n.resolvedLanguage ?? i18n.language ?? "en";
  let formatter = countFormatters.get(locale);
  if (!formatter) {
    formatter = new Intl.NumberFormat(locale);
    countFormatters.set(locale, formatter);
  }
  return formatter.format(value);
}

export const STATUS_FALLBACKS: Record<UsageLogResult, string> = {
  success: "Success",
  failure: "Failure",
};

export const GROUP_OPTIONS: Array<{
  key: UsageGroupBy;
  labelKey: string;
  fallback: string;
}> = [
  { key: "none", labelKey: "aiGatewayGroupNone", fallback: "No grouping" },
  { key: "model", labelKey: "aiGatewayGroupModel", fallback: "Model" },
  { key: "provider", labelKey: "aiGatewayGroupProvider", fallback: "Provider" },
  { key: "day", labelKey: "aiGatewayGroupDay", fallback: "Day (UTC+8)" },
];

export const STATUS_OPTIONS: UsageLogResult[] = ["success", "failure"];

export function statusBadgeStyle(result: UsageLogResult): {
  badge: string;
  dot: string;
} {
  switch (result) {
    case "success":
      return {
        badge: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        dot: "bg-emerald-500",
      };
    case "failure":
      return {
        badge: "bg-destructive/10 text-destructive border-destructive/20",
        dot: "bg-destructive",
      };
    default:
      return {
        badge: "bg-muted text-muted-foreground border-border",
        dot: "bg-muted-foreground",
      };
  }
}

export function getHttpStatusReason(
  status: number,
  t: TFunction,
): { title: string; label: string } {
  switch (status) {
    case 400:
      return {
        label: t("aiGatewayError400", "Bad request"),
        title: `HTTP 400: ${t("aiGatewayError400", "Bad request")}`,
      };
    case 401:
      return {
        label: t("aiGatewayError401", "Unauthorized / Invalid API key"),
        title: `HTTP 401: ${t("aiGatewayError401", "Unauthorized / Invalid API key")}`,
      };
    case 403:
      return {
        label: t("aiGatewayError403", "Forbidden / Access denied"),
        title: `HTTP 403: ${t("aiGatewayError403", "Forbidden / Access denied")}`,
      };
    case 404:
      return {
        label: t("aiGatewayError404", "Model or endpoint not found"),
        title: `HTTP 404: ${t("aiGatewayError404", "Model or endpoint not found")}`,
      };
    case 408:
      return {
        label: t("aiGatewayError408", "Request timeout"),
        title: `HTTP 408: ${t("aiGatewayError408", "Request timeout")}`,
      };
    case 413:
      return {
        label: t("aiGatewayError413", "Payload too large"),
        title: `HTTP 413: ${t("aiGatewayError413", "Payload too large")}`,
      };
    case 422:
      return {
        label: t("aiGatewayError422", "Unprocessable entity"),
        title: `HTTP 422: ${t("aiGatewayError422", "Unprocessable entity")}`,
      };
    case 429:
      return {
        label: t("aiGatewayError429", "Rate limit exceeded"),
        title: `HTTP 429: ${t("aiGatewayError429", "Rate limit exceeded")}`,
      };
    case 500:
      return {
        label: t("aiGatewayError500", "Internal server error"),
        title: `HTTP 500: ${t("aiGatewayError500", "Internal server error")}`,
      };
    case 502:
      return {
        label: t("aiGatewayError502", "Bad gateway / Upstream unavailable"),
        title: `HTTP 502: ${t("aiGatewayError502", "Bad gateway / Upstream unavailable")}`,
      };
    case 503:
      return {
        label: t("aiGatewayError503", "Service unavailable"),
        title: `HTTP 503: ${t("aiGatewayError503", "Service unavailable")}`,
      };
    case 504:
      return {
        label: t("aiGatewayError504", "Gateway timeout"),
        title: `HTTP 504: ${t("aiGatewayError504", "Gateway timeout")}`,
      };
    case 0:
      return {
        label: t("aiGatewayErrorNetwork", "Network error / Connection failed"),
        title: t("aiGatewayErrorNetwork", "Network error / Connection failed"),
      };
    default:
      if (status >= 400 && status < 500) {
        return {
          label: `HTTP ${status}`,
          title: `HTTP ${status}: ${t("aiGatewayErrorUnknown", "Client error")}`,
        };
      }
      if (status >= 500) {
        return {
          label: `HTTP ${status}`,
          title: `HTTP ${status}: ${t("aiGatewayErrorUnknown", "Server error")}`,
        };
      }
      return {
        label: t("aiGatewayErrorUnknown", "Request failed"),
        title: t("aiGatewayErrorUnknown", "Request failed"),
      };
  }
}

export function resolveLogActionableHint(
  status: number,
  errorMessage: string,
  t: (key: string, fallback: string) => string,
): string | null {
  const lower = errorMessage.toLowerCase();
  if (status === 429) {
    const isQuota =
      lower.includes("quota") ||
      lower.includes("limit") ||
      lower.includes("usage") ||
      lower.includes("balance") ||
      lower.includes("billing") ||
      lower.includes("exceeded") ||
      lower.includes("额度");
    if (isQuota) {
      return t(
        "aiGatewayLogsHintQuotaExceeded",
        "排查建议：上游服务商账户额度或周期配额已耗尽，请充值或切换备用服务商。",
      );
    }
    return t(
      "aiGatewayLogsHintRateLimited",
      "排查建议：触发上游速率或并发限制，建议降低并发或稍后重试。",
    );
  }
  if (status === 0 || lower.includes("network error")) {
    if (
      lower.includes("refused") ||
      lower.includes("unreachable") ||
      lower.includes("unable to connect") ||
      lower.includes("failed to connect")
    ) {
      return t(
        "aiGatewayLogsHintNetworkRefused",
        "排查建议：无法连接上游地址，请检查服务商 Base URL 是否正确或网络代理配置。",
      );
    }
    if (lower.includes("timed out") || lower.includes("timeout")) {
      return t(
        "aiGatewayLogsHintTimeout",
        "排查建议：连接上游超时，请检查网络稳定性或代理延迟。",
      );
    }
    if (lower.includes("dns") || lower.includes("resolve")) {
      return t(
        "aiGatewayLogsHintDns",
        "排查建议：无法解析上游域名，请检查 Base URL 拼写及 DNS 解析配置。",
      );
    }
    return t(
      "aiGatewayLogsHintNetwork",
      "排查建议：网络连接失败，请检查上游端点连通性及本地网络设置。",
    );
  }
  return null;
}

export function formatCostRate(rate: number | null | undefined): string {
  if (rate === null || rate === undefined || !Number.isFinite(rate)) return "—";
  return `$${String(rate)}`;
}

const NATIVE_ENUM_PARAMETER_PATHS: ReadonlyArray<readonly string[]> = [
  ["reasoning_effort"],
  ["reasoning", "effort"],
  ["output_config", "effort"],
  ["thinking", "type"],
];

function readNativeParameter(
  parameters: Record<string, unknown> | null | undefined,
  path: readonly string[],
): unknown {
  let current: unknown = parameters;
  for (const segment of path) {
    if (current === null || typeof current !== "object" || Array.isArray(current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function readEnumNativeValue(
  parameters: Record<string, unknown> | null | undefined,
): string | null {
  for (const path of NATIVE_ENUM_PARAMETER_PATHS) {
    const value = readNativeParameter(parameters, path);
    if (typeof value === "string" && value.trim() !== "") return value.trim();
  }
  return null;
}

function readBudgetNativeTokens(
  parameters: Record<string, unknown> | null | undefined,
): string | null {
  const value = readNativeParameter(parameters, ["thinking", "budget_tokens"]);
  if (typeof value === "number") {
    return Number.isFinite(value) ? String(value) : null;
  }
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed !== "" && Number.isFinite(Number(trimmed)) ? trimmed : null;
  }
  return null;
}

export interface NativeReasoningDisplay {
  summary: string;
  modeLabel: string;
  value: string | null;
  valueLabelKey: string;
  valueLabelFallback: string;
}

export function describeNativeReasoning(
  native: ModelPoolNativeReasoning,
  t: TFunction,
): NativeReasoningDisplay {
  const fixedMode = (labelKey: string, fallback: string): NativeReasoningDisplay => {
    const modeLabel = t(labelKey, fallback);
    return {
      summary: modeLabel,
      modeLabel,
      value: null,
      valueLabelKey: "aiGatewayLogsReasoningNativeValue",
      valueLabelFallback: "Native value",
    };
  };

  switch (native.mode) {
    case "enum": {
      const modeLabel = t("aiGatewayLogsReasoningModeEnum", "Enum");
      const value = readEnumNativeValue(native.parameters);
      return {
        summary: value ? `${modeLabel} · ${value}` : modeLabel,
        modeLabel,
        value,
        valueLabelKey: "aiGatewayLogsReasoningNativeValue",
        valueLabelFallback: "Native value",
      };
    }
    case "budget": {
      const modeLabel = t("aiGatewayLogsReasoningModeBudget", "Budget");
      const value = readBudgetNativeTokens(native.parameters);
      return {
        summary: value ? `${modeLabel} · ${value}` : modeLabel,
        modeLabel,
        value,
        valueLabelKey: "aiGatewayLogsReasoningBudgetTokens",
        valueLabelFallback: "Budget tokens",
      };
    }
    case "disable":
      return fixedMode("aiGatewayLogsReasoningModeDisable", "Disabled");
    case "provider_default":
      return fixedMode("aiGatewayLogsReasoningModeProviderDefault", "Provider default");
    default:
      return fixedMode("aiGatewayLogsReasoningModeUnknown", "Unknown");
  }
}

export function isDayString(value: string | null | undefined): boolean {
  return Boolean(value && /^\d{4}-\d{2}-\d{2}$/.test(value));
}
