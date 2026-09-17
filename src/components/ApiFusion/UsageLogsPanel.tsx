import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw, SlidersHorizontal } from "lucide-react";
import {
  apiFusionRequestLogs,
  clampUsagePage,
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
} from "@/lib/apiFusion";
import { errorToMessage } from "@/lib/messages";

const RANGE_LABEL_KEYS: Record<UsageRangeKey, string> = {
  today: "apiFusionRangeToday",
  "7d": "apiFusionRange7d",
  "15d": "apiFusionRange15d",
  "30d": "apiFusionRange30d",
  all: "apiFusionRangeAll",
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
  { key: "none", labelKey: "apiFusionGroupNone", fallback: "No grouping" },
  { key: "model", labelKey: "apiFusionGroupModel", fallback: "Model" },
  { key: "day", labelKey: "apiFusionGroupDay", fallback: "Day (UTC+8)" },
];

const STATUS_OPTIONS: UsageLogResult[] = ["success", "failure", "cancelled"];

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

  const load = useCallback(
    async (options?: { refresh?: boolean }) => {
      const seq = requestSeqRef.current + 1;
      requestSeqRef.current = seq;
      setError("");
      if (options?.refresh) setRefreshing(true);
      else setLoading(true);
      try {
        const next = await apiFusionRequestLogs({
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

  const modelOptions = Array.from(
    new Set((pageData?.records ?? []).map((item) => item.local_model)),
  ).sort();

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

  return (
    <div className="rounded-2xl border bg-card p-5" data-testid="api-fusion-usage-logs">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <h2 className="text-base font-semibold text-foreground">
            {t("apiFusionLogsTab", "Request logs")}
          </h2>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex flex-wrap items-center gap-1 rounded-lg border bg-muted/40 p-1">
            {USAGE_RANGE_KEYS.map((key) => (
              <button
                key={key}
                type="button"
                onClick={() => {
                  setRange(key);
                  setPage(1);
                }}
                aria-pressed={range === key}
                className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                  range === key
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {t(RANGE_LABEL_KEYS[key], RANGE_LABEL_FALLBACKS[key])}
              </button>
            ))}
          </div>
          <button
            type="button"
            onClick={() => void load({ refresh: true })}
            disabled={refreshing}
            aria-label={t("apiFusionRefresh", "Refresh")}
            title={t("apiFusionRefresh", "Refresh")}
            className="inline-flex h-8 w-8 items-center justify-center rounded-lg border bg-background transition hover:bg-muted disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />
          </button>
          <button
            type="button"
            onClick={openFilter}
            aria-expanded={filterOpen}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium transition hover:bg-muted"
          >
            <SlidersHorizontal className="h-3.5 w-3.5" />
            {t("apiFusionFilter", "Filter")}
          </button>
        </div>
      </div>

      {filterOpen ? (
        <div
          data-testid="api-fusion-logs-filter-panel"
          className="mt-3 space-y-3 rounded-xl border bg-muted/20 p-4"
        >
          <div className="space-y-1.5">
            <div className="text-xs font-semibold text-muted-foreground">
              {t("apiFusionFilterStatus", "Status")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              <button
                type="button"
                onClick={() => setDraftStatus(null)}
                aria-pressed={draftStatus === null}
                className={`rounded-md border px-2.5 py-1 text-xs ${
                  draftStatus === null ? "bg-background shadow-sm" : "hover:bg-background"
                }`}
              >
                {t("apiFusionFilterAnyStatus", "Any status")}
              </button>
              {STATUS_OPTIONS.map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => setDraftStatus(option)}
                  aria-pressed={draftStatus === option}
                  className={`rounded-md border px-2.5 py-1 text-xs ${
                    draftStatus === option
                      ? "bg-background shadow-sm"
                      : "hover:bg-background"
                  }`}
                >
                  {t(usageStatusTranslationKey(option), STATUS_FALLBACKS[option])}
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-1.5">
            <div className="text-xs font-semibold text-muted-foreground">
              {t("apiFusionFilterModel", "Model")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              <button
                type="button"
                onClick={() => setDraftModel(null)}
                aria-pressed={draftModel === null}
                className={`rounded-md border px-2.5 py-1 text-xs ${
                  draftModel === null ? "bg-background shadow-sm" : "hover:bg-background"
                }`}
              >
                {t("apiFusionFilterAnyModel", "Any model")}
              </button>
              {modelOptions.map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => setDraftModel(option)}
                  aria-pressed={draftModel === option}
                  className={`rounded-md border px-2.5 py-1 text-xs ${
                    draftModel === option
                      ? "bg-background shadow-sm"
                      : "hover:bg-background"
                  }`}
                >
                  {option}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={clearFilters}
              className="rounded-md border bg-background px-3 py-1.5 text-xs font-medium transition hover:bg-muted"
            >
              {t("apiFusionFilterClear", "Clear")}
            </button>
            <button
              type="button"
              onClick={applyFilters}
              className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90"
            >
              {t("apiFusionFilterApply", "Apply")}
            </button>
          </div>
        </div>
      ) : null}

      <div className="mt-3 flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-1 rounded-lg border bg-muted/40 p-1">
          {GROUP_OPTIONS.map((option) => (
            <button
              key={option.key}
              type="button"
              onClick={() => {
                setGroupBy(option.key);
                setPage(1);
              }}
              aria-pressed={groupBy === option.key}
              className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                groupBy === option.key
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t(option.labelKey, option.fallback)}
            </button>
          ))}
        </div>
      </div>

      {refreshing ? (
        <div role="status" className="mt-3 text-xs text-muted-foreground">
          {t("apiFusionRefreshing", "Refreshing...")}
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
            {t("apiFusionLogsEmpty", "No matching requests.")}
          </p>
        ) : (
          <div className="mt-4 overflow-x-auto rounded-lg border">
            <table
              className="w-full text-left text-xs"
              data-testid="api-fusion-logs-grouped"
            >
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">
                    {t("apiFusionLogsGroupColumn", "Group")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiFusionLogsRequestsColumn", "Requests")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiFusionLogsErrorsColumn", "Errors")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiFusionLogsLastRequestColumn", "Last request")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {groups.map((group) => (
                  <tr
                    key={group.group}
                    className="border-t"
                    data-testid="api-fusion-logs-group-row"
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
          {t("apiFusionLogsEmpty", "No matching requests.")}
        </p>
      ) : (
        <div className="mt-4 space-y-2">
          <div className="overflow-x-auto rounded-lg border">
            <table
              className="w-full text-left text-xs"
              data-testid="api-fusion-logs-ungrouped"
            >
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">
                    {t("apiFusionLogsTimeColumn", "Time")}
                  </th>
                  <th className="px-3 py-2 font-medium">
                    {t("apiFusionLogsStatusColumn", "Status")}
                  </th>
                  <th className="px-3 py-2 font-medium">
                    {t("apiFusionLogsModelColumn", "Model")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiFusionLogsTokensColumn", "Tokens")}
                  </th>
                  <th className="px-3 py-2 text-right font-medium">
                    {t("apiFusionLogsCostColumn", "Cost ($)")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {records.map((item, index) => (
                  <tr
                    key={`${item.timestamp_ms}-${item.local_model}-${index}`}
                    className="border-t"
                    data-testid="api-fusion-logs-row"
                  >
                    <td className="px-3 py-2 font-mono">
                      {formatUtc8DateTime(item.timestamp_ms) ?? "—"}
                    </td>
                    <td className="px-3 py-2">
                      {t(
                        usageStatusTranslationKey(item.result),
                        STATUS_FALLBACKS[item.result],
                      )}
                    </td>
                    <td className="px-3 py-2">
                      <div className="font-medium">{item.local_model}</div>
                      {item.upstream_model !== item.local_model ? (
                        <div className="text-[10px] text-muted-foreground">
                          {item.upstream_model}
                        </div>
                      ) : null}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {new Intl.NumberFormat().format(item.total_tokens)}
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
              {t("apiFusionLogsPagePrev", "Previous")}
            </button>
            <span>
              {t(
                "apiFusionLogsPageSummary",
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
              {t("apiFusionLogsPageNext", "Next")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
