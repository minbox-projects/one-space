import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  Database,
  Filter,
  Info,
  RefreshCw,
} from "lucide-react";
import { SelectDropdown } from "./SelectDropdown";
import {
  apiGatewayRequestLogs,
  clampUsagePage,
  formatGatewayTokens,
  formatUsageAmount,
  formatUsageGroupLabel,
  formatUtc8DateTime,
  USAGE_RANGE_KEYS,
  usageRangeToDays,
  usageStatusTranslationKey,
  type UsageGroupBy,
  type UsageLogResult,
  type UsageLogsPage,
  type UsageRangeKey,
} from "@/lib/apiGateway";
import { errorToMessage } from "@/lib/messages";

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

const STATUS_FALLBACKS: Record<UsageLogResult, string> = {
  success: "Success",
  failure: "Failure",
  cancelled: "Cancelled",
};

const GROUP_OPTIONS: Array<{
  key: UsageGroupBy;
  labelKey: string;
  fallback: string;
}> = [
  { key: "none", labelKey: "apiGatewayGroupNone", fallback: "No grouping" },
  { key: "model", labelKey: "apiGatewayGroupModel", fallback: "Model" },
  { key: "day", labelKey: "apiGatewayGroupDay", fallback: "Day (UTC+8)" },
];

const STATUS_OPTIONS: UsageLogResult[] = ["success", "failure", "cancelled"];

function statusBadgeStyle(result: UsageLogResult): {
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
    case "cancelled":
      return {
        badge: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
        dot: "bg-amber-500",
      };
    default:
      return {
        badge: "bg-muted text-muted-foreground border-border",
        dot: "bg-muted-foreground",
      };
  }
}

function getHttpStatusReason(
  status: number,
  t: TFunction,
): { title: string; label: string } {
  switch (status) {
    case 400:
      return {
        label: t("apiGatewayError400", "Bad request"),
        title: `HTTP 400: ${t("apiGatewayError400", "Bad request")}`,
      };
    case 401:
      return {
        label: t("apiGatewayError401", "Unauthorized / Invalid API key"),
        title: `HTTP 401: ${t("apiGatewayError401", "Unauthorized / Invalid API key")}`,
      };
    case 403:
      return {
        label: t("apiGatewayError403", "Forbidden / Access denied"),
        title: `HTTP 403: ${t("apiGatewayError403", "Forbidden / Access denied")}`,
      };
    case 404:
      return {
        label: t("apiGatewayError404", "Model or endpoint not found"),
        title: `HTTP 404: ${t("apiGatewayError404", "Model or endpoint not found")}`,
      };
    case 408:
      return {
        label: t("apiGatewayError408", "Request timeout"),
        title: `HTTP 408: ${t("apiGatewayError408", "Request timeout")}`,
      };
    case 413:
      return {
        label: t("apiGatewayError413", "Payload too large"),
        title: `HTTP 413: ${t("apiGatewayError413", "Payload too large")}`,
      };
    case 422:
      return {
        label: t("apiGatewayError422", "Unprocessable entity"),
        title: `HTTP 422: ${t("apiGatewayError422", "Unprocessable entity")}`,
      };
    case 429:
      return {
        label: t("apiGatewayError429", "Rate limit exceeded"),
        title: `HTTP 429: ${t("apiGatewayError429", "Rate limit exceeded")}`,
      };
    case 500:
      return {
        label: t("apiGatewayError500", "Internal server error"),
        title: `HTTP 500: ${t("apiGatewayError500", "Internal server error")}`,
      };
    case 502:
      return {
        label: t("apiGatewayError502", "Bad gateway / Upstream unavailable"),
        title: `HTTP 502: ${t("apiGatewayError502", "Bad gateway / Upstream unavailable")}`,
      };
    case 503:
      return {
        label: t("apiGatewayError503", "Service unavailable"),
        title: `HTTP 503: ${t("apiGatewayError503", "Service unavailable")}`,
      };
    case 504:
      return {
        label: t("apiGatewayError504", "Gateway timeout"),
        title: `HTTP 504: ${t("apiGatewayError504", "Gateway timeout")}`,
      };
    case 0:
      return {
        label: t("apiGatewayErrorNetwork", "Network error / Connection failed"),
        title: t("apiGatewayErrorNetwork", "Network error / Connection failed"),
      };
    default:
      if (status >= 400 && status < 500) {
        return {
          label: `HTTP ${status}`,
          title: `HTTP ${status}: ${t("apiGatewayErrorUnknown", "Client error")}`,
        };
      }
      if (status >= 500) {
        return {
          label: `HTTP ${status}`,
          title: `HTTP ${status}: ${t("apiGatewayErrorUnknown", "Server error")}`,
        };
      }
      return {
        label: t("apiGatewayErrorUnknown", "Request failed"),
        title: t("apiGatewayErrorUnknown", "Request failed"),
      };
  }
}

export function UsageLogsPanel({ isActive = true }: { isActive?: boolean }) {
  const { t } = useTranslation();
  const [range, setRange] = useState<UsageRangeKey>("today");
  const [groupBy, setGroupBy] = useState<UsageGroupBy>("none");
  const [status, setStatus] = useState<UsageLogResult | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageData, setPageData] = useState<UsageLogsPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [draftStatus, setDraftStatus] = useState<UsageLogResult | null>(null);
  const [draftModel, setDraftModel] = useState<string | null>(null);
  const requestSeqRef = useRef(0);
  const filterRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!filterOpen) return;
    function handleClickOutside(event: MouseEvent) {
      if (
        filterRef.current &&
        !filterRef.current.contains(event.target as Node)
      ) {
        setFilterOpen(false);
      }
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setFilterOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [filterOpen]);

  const load = useCallback(
    async (options?: { refresh?: boolean }) => {
      const seq = requestSeqRef.current + 1;
      requestSeqRef.current = seq;
      setError("");
      if (options?.refresh) setRefreshing(true);
      else setLoading(true);
      try {
        const next = await apiGatewayRequestLogs({
          days: usageRangeToDays(range),
          groupBy,
          status,
          model,
          page,
        });
        if (requestSeqRef.current !== seq) return;
        const clamped = clampUsagePage(next.page, next.total_pages);
        if (clamped !== next.page) {
          setPage(clamped);
          return;
        }
        setPageData(next);
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
    [groupBy, model, page, range, status],
  );

  useEffect(() => {
    if (!isActive) return;
    void load();
  }, [isActive, load]);

  const modelOptions = [
    ...(pageData?.models ??
      Array.from(
        new Set((pageData?.records ?? []).map((item) => item.local_model)),
      )),
  ].sort();

  const records = [...(pageData?.records ?? [])].sort(
    (first, second) => second.timestamp_ms - first.timestamp_ms,
  );
  const groups = pageData?.groups ?? [];
  const isGrouped = groupBy !== "none";
  const totalPages = pageData?.total_pages ?? 0;

  const openFilter = () => {
    setDraftStatus(status);
    setDraftModel(model);
    setFilterOpen((prev) => !prev);
  };

  const selectStatus = (newStatus: UsageLogResult | null) => {
    setDraftStatus(newStatus);
    setStatus(newStatus);
    setPage(1);
  };

  const selectModel = (newModel: string | null) => {
    setDraftModel(newModel);
    setModel(newModel);
    setPage(1);
  };

  const applyFilters = () => {
    setStatus(draftStatus);
    setModel(draftModel);
    setPage(1);
    setFilterOpen(false);
  };

  const clearFilters = () => {
    setDraftStatus(null);
    setDraftModel(null);
    setStatus(null);
    setModel(null);
    setPage(1);
    setFilterOpen(false);
  };

  const activeStatus = draftStatus ?? status;
  const activeModel = draftModel ?? model;
  const hasActiveFilter = activeStatus !== null || activeModel !== null;

  const filterLabel = (() => {
    const parts: string[] = [];
    if (activeStatus) {
      parts.push(t(usageStatusTranslationKey(activeStatus), STATUS_FALLBACKS[activeStatus]));
    }
    if (activeModel) {
      parts.push(activeModel);
    }
    return parts.length > 0 ? parts.join(" · ") : t("apiGatewayFilter", "Filter");
  })();

  return (
    <div className="rounded-2xl border bg-card p-5" data-testid="api-gateway-usage-logs">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <h2 className="text-base font-semibold text-foreground">
            {t("apiGatewayLogsTab", "Request logs")}
          </h2>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <SelectDropdown
            value={range}
            options={USAGE_RANGE_KEYS.map((key) => ({
              value: key,
              label: t(RANGE_LABEL_KEYS[key], RANGE_LABEL_FALLBACKS[key]),
            }))}
            onChange={(nextRange) => {
              setRange(nextRange);
              setPage(1);
            }}
            testId="api-gateway-logs-range"
            ariaLabel={t("apiGatewayRangeToday", "Time range")}
          />
          <SelectDropdown
            value={groupBy}
            options={GROUP_OPTIONS.map((option) => ({
              value: option.key,
              label: t(option.labelKey, option.fallback),
            }))}
            onChange={(nextGroup) => {
              setGroupBy(nextGroup);
              setPage(1);
            }}
            testId="api-gateway-logs-group"
            ariaLabel={t("apiGatewayGroupNone", "Grouping")}
          />
          <div className="relative inline-block text-left" ref={filterRef}>
            <button
              type="button"
              data-testid="api-gateway-logs-filter-trigger"
              onClick={openFilter}
              aria-expanded={filterOpen}
              className={`inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition hover:bg-muted ${
                hasActiveFilter
                  ? "border-primary/50 bg-primary/5 text-primary"
                  : "bg-background text-foreground"
              }`}
            >
              <Filter className="h-3.5 w-3.5" />
              <span>{filterLabel}</span>
              <ChevronDown
                className={`h-3.5 w-3.5 text-muted-foreground transition-transform duration-200 ${
                  filterOpen ? "rotate-180" : ""
                }`}
              />
            </button>

            {filterOpen ? (
              <div
                data-testid="api-gateway-logs-filter-panel"
                className="absolute right-0 top-full z-30 mt-1.5 w-80 space-y-3 rounded-xl border bg-card p-4 shadow-lg animate-in fade-in-0 zoom-in-95"
              >
                <div className="space-y-1.5">
                  <div className="text-xs font-semibold text-muted-foreground">
                    {t("apiGatewayFilterStatus", "Status")}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    <button
                      type="button"
                      onClick={() => selectStatus(null)}
                      aria-pressed={activeStatus === null}
                      className={`rounded-md border px-2.5 py-1 text-xs transition-colors ${
                        activeStatus === null
                          ? "border-primary bg-primary text-primary-foreground font-medium shadow-sm"
                          : "border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                      }`}
                    >
                      {t("apiGatewayFilterAnyStatus", "Any status")}
                    </button>
                    {STATUS_OPTIONS.map((option) => {
                      const style = statusBadgeStyle(option);
                      const isSelected = activeStatus === option;
                      return (
                        <button
                          key={option}
                          type="button"
                          onClick={() => selectStatus(option)}
                          aria-pressed={isSelected}
                          className={`inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs transition-colors ${
                            isSelected
                              ? "border-primary bg-primary text-primary-foreground font-medium shadow-sm"
                              : "border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                          }`}
                        >
                          <span
                            className={`h-1.5 w-1.5 rounded-full shrink-0 ${
                              isSelected ? "bg-primary-foreground" : style.dot
                            }`}
                          />
                          <span>
                            {t(
                              usageStatusTranslationKey(option),
                              STATUS_FALLBACKS[option],
                            )}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="space-y-1.5">
                  <div className="text-xs font-semibold text-muted-foreground">
                    {t("apiGatewayFilterModel", "Model")}
                  </div>
                  <div className="flex max-h-40 flex-wrap gap-1.5 overflow-y-auto">
                    <button
                      type="button"
                      onClick={() => selectModel(null)}
                      aria-pressed={activeModel === null}
                      className={`rounded-md border px-2.5 py-1 text-xs transition-colors ${
                        activeModel === null
                          ? "border-primary bg-primary text-primary-foreground font-medium shadow-sm"
                          : "border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                      }`}
                    >
                      {t("apiGatewayFilterAnyModel", "Any model")}
                    </button>
                    {modelOptions.map((option) => (
                      <button
                        key={option}
                        type="button"
                        onClick={() => selectModel(option)}
                        aria-pressed={activeModel === option}
                        className={`rounded-md border px-2.5 py-1 text-xs transition-colors ${
                          activeModel === option
                            ? "border-primary bg-primary text-primary-foreground font-medium shadow-sm"
                            : "border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                        }`}
                      >
                        {option}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="flex items-center justify-end gap-2 border-t pt-2.5">
                  {hasActiveFilter ? (
                    <button
                      type="button"
                      onClick={clearFilters}
                      className="rounded-md border bg-background px-3 py-1.5 text-xs font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground"
                    >
                      {t("apiGatewayFilterClear", "Clear")}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    onClick={applyFilters}
                    className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90"
                  >
                    {t("apiGatewayFilterApply", "Apply")}
                  </button>
                </div>
              </div>
            ) : null}
          </div>
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

      {!pageData ? (
        loading ? (
          <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
            {t("loading", "Loading...")}
          </p>
        ) : null
      ) : isGrouped ? (
        groups.length === 0 ? (
          <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
            {t("apiGatewayLogsEmpty", "No matching requests.")}
          </p>
        ) : (
          <div className="mt-4 overflow-x-auto rounded-lg border">
            <table
              className="w-full text-left text-xs"
              data-testid="api-gateway-logs-grouped"
            >
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">
                    {t("apiGatewayLogsGroupColumn", "Group")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiGatewayLogsRequestsColumn", "Requests")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiGatewayLogsErrorsColumn", "Errors")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiGatewayLogsLastRequestColumn", "Last request")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {groups.map((group) => (
                  <tr
                    key={group.group}
                    className="border-t"
                    data-testid="api-gateway-logs-group-row"
                  >
                    <td className="px-3 py-2 font-medium">
                      {formatUsageGroupLabel(groupBy, group.group)}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {new Intl.NumberFormat().format(group.request_count)}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {new Intl.NumberFormat().format(group.error_count)}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {formatUtc8DateTime(group.last_request_at_ms) ?? "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      ) : records.length === 0 ? (
        <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
          {t("apiGatewayLogsEmpty", "No matching requests.")}
        </p>
      ) : (
        <div className="mt-4 space-y-2">
          <div className="overflow-x-auto rounded-lg border min-h-[220px]">
            <table
              className="w-full text-left text-xs"
              data-testid="api-gateway-logs-ungrouped"
            >
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">
                    {t("apiGatewayLogsTimeColumn", "Time")}
                  </th>
                  <th className="px-3 py-2 font-medium">
                    {t("apiGatewayLogsStatusColumn", "Status")}
                  </th>
                  <th className="px-3 py-2 font-medium">
                    {t("apiGatewayLogsModelColumn", "Model")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiGatewayLogsTokensColumn", "Tokens")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiGatewayLogsCostColumn", "Cost ($)")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {records.map((item, index) => (
                  <tr
                    key={`${item.timestamp_ms}-${item.local_model}-${index}`}
                    className="border-t"
                    data-testid="api-gateway-logs-row"
                  >
                    <td className="px-3 py-2 font-mono">
                      {formatUtc8DateTime(item.timestamp_ms) ?? "—"}
                    </td>
                    <td className="px-3 py-2">
                      {(() => {
                        const style = statusBadgeStyle(item.result);
                        const isFailure = item.result === "failure";
                        const reason = isFailure ? getHttpStatusReason(item.status, t) : null;
                        return (
                          <div className="flex flex-col gap-0.5">
                            <span
                              className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium w-fit ${style.badge}`}
                              data-testid="api-gateway-logs-status-badge"
                              title={reason ? reason.title : undefined}
                            >
                              <span
                                className={`h-1.5 w-1.5 rounded-full shrink-0 ${style.dot}`}
                              />
                              <span>
                                {t(
                                  usageStatusTranslationKey(item.result),
                                  STATUS_FALLBACKS[item.result],
                                )}
                              </span>
                              {isFailure && item.status > 0 ? (
                                <span className="font-mono text-[10px] opacity-80">
                                  {item.status}
                                </span>
                              ) : null}
                            </span>
                            {reason ? (
                              <div
                                className="text-[10px] text-muted-foreground truncate max-w-[200px]"
                                title={reason.title}
                                data-testid="api-gateway-logs-status-reason"
                              >
                                {reason.label}
                              </div>
                            ) : null}
                          </div>
                        );
                      })()}
                    </td>
                    <td className="px-3 py-2">
                      <div className="font-medium whitespace-nowrap">{item.local_model}</div>
                      {(item.provider_name || item.upstream_model) ? (
                        <div className="text-[10px] text-muted-foreground flex items-center gap-1 whitespace-nowrap">
                          {item.provider_name ? (
                            <span
                              className="font-medium"
                              data-testid="api-gateway-logs-provider-name"
                            >
                              {item.provider_name}
                            </span>
                          ) : null}
                          {item.provider_name && item.upstream_model ? (
                            <span className="opacity-40">·</span>
                          ) : null}
                          {item.upstream_model ? (
                            <span
                              title={item.upstream_model}
                              data-testid="api-gateway-logs-upstream-model"
                            >
                              {item.upstream_model}
                            </span>
                          ) : null}
                        </div>
                      ) : null}
                    </td>
                    <td className="px-3 py-2 text-right" data-testid="api-gateway-logs-tokens-cell">
                      {(() => {
                        const cacheTokens = item.cache_read_tokens + item.cache_write_tokens;
                        const showAbove = index >= 4 && index >= records.length - 3;
                        const tooltipClass = showAbove ? "bottom-full mb-1.5" : "top-full mt-1.5";
                        return (
                          <div className="inline-flex items-center justify-end gap-1.5 font-mono text-xs">
                            <div
                              className="flex items-center gap-1.5 text-[11px] text-muted-foreground whitespace-nowrap"
                              data-testid="api-gateway-logs-tokens-breakdown"
                            >
                              <span
                                className="inline-flex items-center gap-0.5"
                                title={`${t("apiGatewayLogsTokensInput", "Input")}: ${new Intl.NumberFormat().format(item.input_tokens)}`}
                              >
                                <ArrowDown
                                  className="h-3 w-3 text-muted-foreground/80 shrink-0"
                                  aria-label={t("apiGatewayLogsTokensInput", "Input")}
                                  data-testid="api-gateway-logs-tokens-input-icon"
                                />
                                <span className="text-foreground font-medium">
                                  {formatGatewayTokens(item.input_tokens)}
                                </span>
                              </span>
                              <span className="text-muted-foreground/40 font-sans">·</span>
                              <span
                                className="inline-flex items-center gap-0.5"
                                title={`${t("apiGatewayLogsTokensOutput", "Output")}: ${new Intl.NumberFormat().format(item.output_tokens)}`}
                              >
                                <ArrowUp
                                  className="h-3 w-3 text-muted-foreground/80 shrink-0"
                                  aria-label={t("apiGatewayLogsTokensOutput", "Output")}
                                  data-testid="api-gateway-logs-tokens-output-icon"
                                />
                                <span className="text-foreground font-medium">
                                  {formatGatewayTokens(item.output_tokens)}
                                </span>
                              </span>
                              <span className="text-muted-foreground/40 font-sans">·</span>
                              <span
                                className="inline-flex items-center gap-0.5"
                                title={`${t("apiGatewayLogsTokensCache", "Cache")}: ${new Intl.NumberFormat().format(cacheTokens)}`}
                              >
                                <Database
                                  className="h-3 w-3 text-muted-foreground/80 shrink-0"
                                  aria-label={t("apiGatewayLogsTokensCache", "Cache")}
                                  data-testid="api-gateway-logs-tokens-cache-icon"
                                />
                                <span className="text-foreground font-medium">
                                  {formatGatewayTokens(cacheTokens)}
                                </span>
                              </span>
                            </div>

                            <div className="relative group inline-flex items-center shrink-0 group-hover:z-50 focus-within:z-50">
                              <button
                                type="button"
                                className="text-muted-foreground/60 hover:text-foreground transition-colors p-0.5 rounded focus:outline-none"
                                aria-label={t("apiGatewayLogsTokensDetail", "Tokens breakdown")}
                                data-testid="api-gateway-logs-tokens-info-btn"
                              >
                                <Info className="h-3.5 w-3.5" />
                              </button>
                              <div
                                role="tooltip"
                                className={`pointer-events-none absolute right-0 ${tooltipClass} hidden group-hover:flex group-focus-within:flex flex-col gap-1 rounded-md border bg-popover p-2 text-left text-xs text-popover-foreground shadow-lg z-50 min-w-[170px]`}
                                data-testid="api-gateway-logs-tokens-tooltip"
                              >
                                <div className="font-semibold text-[11px] border-b pb-1 text-muted-foreground">
                                  {t("apiGatewayLogsTokensDetail", "Tokens breakdown")}
                                </div>
                                <div className="space-y-0.5 pt-0.5 text-[11px]">
                                  <div className="flex items-center justify-between gap-3">
                                    <span className="inline-flex items-center gap-1 text-muted-foreground">
                                      <ArrowDown className="h-3 w-3 shrink-0" />
                                      {t("apiGatewayLogsTokensInput", "Input")}:
                                    </span>
                                    <span className="font-mono font-medium">
                                      {new Intl.NumberFormat().format(item.input_tokens)}
                                    </span>
                                  </div>
                                  <div className="flex items-center justify-between gap-3">
                                    <span className="inline-flex items-center gap-1 text-muted-foreground">
                                      <ArrowUp className="h-3 w-3 shrink-0" />
                                      {t("apiGatewayLogsTokensOutput", "Output")}:
                                    </span>
                                    <span className="font-mono font-medium">
                                      {new Intl.NumberFormat().format(item.output_tokens)}
                                    </span>
                                  </div>
                                  <div className="flex items-center justify-between gap-3">
                                    <span className="inline-flex items-center gap-1 text-muted-foreground">
                                      <Database className="h-3 w-3 shrink-0" />
                                      {t("apiGatewayLogsTokensCache", "Cache")}:
                                    </span>
                                    <span className="font-mono font-medium">
                                      {new Intl.NumberFormat().format(cacheTokens)}
                                    </span>
                                  </div>
                                  {(item.cache_read_tokens > 0 || item.cache_write_tokens > 0) && (
                                    <div className="text-[10px] text-muted-foreground/70 pl-2 space-y-0.5">
                                      <div className="flex items-center justify-between gap-3">
                                        <span>
                                          {t("apiGatewayLogsTokensCacheRead", "Cache read")}:
                                        </span>
                                        <span className="font-mono">
                                          {new Intl.NumberFormat().format(item.cache_read_tokens)}
                                        </span>
                                      </div>
                                      <div className="flex items-center justify-between gap-3">
                                        <span>
                                          {t("apiGatewayLogsTokensCacheWrite", "Cache write")}:
                                        </span>
                                        <span className="font-mono">
                                          {new Intl.NumberFormat().format(item.cache_write_tokens)}
                                        </span>
                                      </div>
                                    </div>
                                  )}
                                  <div className="border-t my-1 border-border"></div>
                                  <div className="flex items-center justify-between gap-3 font-semibold">
                                    <span>
                                      {t("apiGatewayLogsTokensTotal", "Total")}:
                                    </span>
                                    <span className="font-mono">
                                      {new Intl.NumberFormat().format(item.total_tokens)}
                                    </span>
                                  </div>
                                </div>
                              </div>
                            </div>
                          </div>
                        );
                      })()}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {item.amount === null ? "—" : formatUsageAmount(item.amount)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="flex items-center justify-end gap-2 text-xs text-muted-foreground">
            <button
              type="button"
              onClick={() => setPage((prev) => Math.max(1, prev - 1))}
              disabled={page <= 1}
              className="rounded-md border bg-background px-2.5 py-1 font-medium transition hover:bg-muted disabled:opacity-50"
            >
              {t("apiGatewayLogsPagePrev", "Previous")}
            </button>
            <span>
              {t(
                "apiGatewayLogsPageSummary",
                "Page {{page}} / {{total}}",
                { page: pageData.page, total: Math.max(1, totalPages) },
              )}
            </span>
            <button
              type="button"
              onClick={() => setPage((prev) => prev + 1)}
              disabled={totalPages < 1 || page >= totalPages}
              className="rounded-md border bg-background px-2.5 py-1 font-medium transition hover:bg-muted disabled:opacity-50"
            >
              {t("apiGatewayLogsPageNext", "Next")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
