import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, ChevronDown, Filter, RefreshCw, X } from "lucide-react";
import { SelectDropdown } from "./SelectDropdown";
import {
  aiGatewayRequestLogs,
  clampUsagePage,
  formatUsageGroupLabel,
  formatUtc8DateTime,
  USAGE_RANGE_KEYS,
  usageStatusTranslationKey,
  type UsageGroupBy,
  type UsageLogResult,
  type UsageLogsPage,
  type UsageLogsQuery,
  type UsageRangeKey,
} from "@/lib/aiGateway";
import { errorToMessage } from "@/lib/messages";
import { RANGE_LABEL_FALLBACKS, RANGE_LABEL_KEYS } from "./gatewayShared";
import { LogRecordRow } from "./usageLogs/LogRecordRow";
import {
  formatCount,
  GROUP_OPTIONS,
  isDayString,
  statusBadgeStyle,
  STATUS_FALLBACKS,
  STATUS_OPTIONS,
} from "./usageLogs/formatting";

function UsageLogsPanelComponent({
  isActive = true,
  focusModel = null,
  focusToken = 0,
  focusProviderId = null,
  focusProviderName = null,
  focusDay = null,
  onReturnToProviderHistory,
  onClearProviderDayFocus,
}: {
  isActive?: boolean;
  /** Alias filter requested by an external shortcut (e.g. a pool card log button). */
  focusModel?: string | null;
  /** Bumped by the external shortcut; each new value re-applies `focusModel`. */
  focusToken?: number;
  /**
   * One-shot provider and day focus handed down by a Provider history daily row.
   * When set, directly sets provider and range to reuse existing log filters.
   */
  focusProviderId?: string | null;
  focusProviderName?: string | null;
  focusDay?: string | null;
  /** Restores the originating Provider history view (same selection). */
  onReturnToProviderHistory?: () => void;
  /**
   * Exits the exact provider/day focus while staying in the request-log view so
   * ordinary quick-range browsing resumes on the next request.
   */
  onClearProviderDayFocus?: () => void;
}) {
  const { t } = useTranslation();
  const [range, setRange] = useState<string>("today");
  const [groupBy, setGroupBy] = useState<UsageGroupBy>("none");
  const [status, setStatus] = useState<UsageLogResult | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const [provider, setProvider] = useState<string | null>(null);
  const [appliedProviderId, setAppliedProviderId] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageData, setPageData] = useState<UsageLogsPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [draftStatus, setDraftStatus] = useState<UsageLogResult | null>(null);
  const [draftModel, setDraftModel] = useState<string | null>(null);
  const [draftProvider, setDraftProvider] = useState<string | null>(null);
  const [focusDismissed, setFocusDismissed] = useState(false);
  const requestSeqRef = useRef(0);
  const filterRef = useRef<HTMLDivElement>(null);
  // Highest `focusToken` already reflected in the local filters: the external
  // focus guard is one-shot and releases once this value catches up.
  const appliedFocusTokenRef = useRef(0);

  const activeFocusProviderId = focusDismissed ? null : focusProviderId;
  const activeFocusDay = focusDismissed ? null : focusDay;

  // Identity of the exact provider/day query context currently painted.
  const focusBoundaryKey = `${activeFocusProviderId ?? ""}\u0000${focusProviderName ?? ""}\u0000${activeFocusDay ?? ""}`;
  const paintedFocusBoundaryRef = useRef(focusBoundaryKey);

  useEffect(() => {
    if (paintedFocusBoundaryRef.current === focusBoundaryKey) return;
    paintedFocusBoundaryRef.current = focusBoundaryKey;
    requestSeqRef.current += 1;
    setPageData(null);
    setError("");
  }, [focusBoundaryKey]);

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
        const hasExactDay = isDayString(range);
        const query: UsageLogsQuery = {
          range: hasExactDay ? "today" : (range as UsageRangeKey),
          groupBy,
          status,
          model,
          provider,
          page,
        };
        if (hasExactDay) {
          query.day = range;
        }
        if (appliedProviderId) {
          query.providerId = appliedProviderId;
        }
        const next = await aiGatewayRequestLogs(query, {
          fresh: Boolean(options?.refresh),
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
    [
      groupBy,
      model,
      page,
      provider,
      range,
      status,
      appliedProviderId,
    ],
  );

  const isFromProviderHistoryRef = useRef(false);

  // External shortcut: model-pool card log button or provider history view day button.
  // Reuses the existing request-log filter state without inventing a new format.
  useEffect(() => {
    if (focusToken === 0) return;
    setFocusDismissed(false);
    if (
      focusProviderId !== null ||
      focusProviderName !== null ||
      focusDay !== null
    ) {
      isFromProviderHistoryRef.current = true;
      if (focusDay) {
        setRange(focusDay);
      }
      const activeProviderLabel = focusProviderName ?? null;
      setProvider(activeProviderLabel);
      setDraftProvider(activeProviderLabel);
      setAppliedProviderId(focusProviderId);
      setModel(null);
      setDraftModel(null);
      setStatus(null);
      setDraftStatus(null);
      setGroupBy("none");
      setPage(1);
      return;
    }
    if (focusModel) {
      isFromProviderHistoryRef.current = false;
      setModel(focusModel);
      setDraftModel(focusModel);
      setPage(1);
      return;
    }
    // Reset to ordinary quick-range browsing when all focus props are cleared
    isFromProviderHistoryRef.current = false;
    setProvider(null);
    setDraftProvider(null);
    setAppliedProviderId(null);
    setModel(null);
    setDraftModel(null);
    setStatus(null);
    setDraftStatus(null);
    setGroupBy("none");
    setRange("today");
    setPage(1);
  }, [focusToken, focusModel, focusProviderId, focusProviderName, focusDay]);

  useEffect(() => {
    if (!isActive) return;
    // Avoid a transient stale-filter request while an external focus is still
    // being applied, then load exactly once the applied state catches up.
    const providerDayPending =
      (focusProviderId !== null ||
        focusDay !== null ||
        focusProviderName !== null) &&
      (groupBy !== "none" ||
        status !== null ||
        model !== null ||
        (focusDay !== null && range !== focusDay) ||
        page !== 1);
    const modelPending =
      focusProviderId === null &&
      focusDay === null &&
      focusModel !== null &&
      model !== focusModel;
    if (
      focusToken > appliedFocusTokenRef.current &&
      (providerDayPending || modelPending)
    ) {
      return;
    }
    if (focusToken > appliedFocusTokenRef.current) {
      appliedFocusTokenRef.current = focusToken;
    }
    void load();
    // Leaving the view, or a superseding query, invalidates the in-flight
    // request so a hidden or replaced answer can never commit its snapshot.
    return () => {
      requestSeqRef.current += 1;
    };
  }, [
    focusModel,
    focusProviderId,
    focusProviderName,
    focusDay,
    focusToken,
    isActive,
    load,
    model,
    groupBy,
    status,
    provider,
    range,
    page,
  ]);

  const rangeOptions = useMemo(() => {
    const base = USAGE_RANGE_KEYS.map((key) => ({
      value: key as string,
      label: t(RANGE_LABEL_KEYS[key], RANGE_LABEL_FALLBACKS[key]),
    }));
    if (isDayString(range)) {
      return [{ value: range, label: range }, ...base];
    }
    return base;
  }, [range, t]);

  const visibleRecords = pageData?.records ?? [];

  const modelOptions = [
    ...(pageData?.models ??
      Array.from(new Set(visibleRecords.map((item) => item.local_model)))),
  ].sort();

  const providerOptions = useMemo(() => {
    const set = new Set(pageData?.providers ?? []);
    if (provider) set.add(provider);
    return Array.from(set).sort();
  }, [pageData?.providers, provider]);

  const records = [...visibleRecords].sort(
    (first, second) => second.timestamp_ms - first.timestamp_ms,
  );
  const groups = pageData?.groups ?? [];
  const isGrouped = groupBy !== "none";
  const totalPages = pageData?.total_pages ?? 0;

  const openFilter = () => {
    setDraftStatus(status);
    setDraftModel(model);
    setDraftProvider(provider);
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

  const selectProvider = (newProvider: string | null) => {
    setDraftProvider(newProvider);
    setProvider(newProvider);
    if (newProvider !== (focusProviderName || focusProviderId)) {
      setAppliedProviderId(null);
    }
    setPage(1);
  };

  const applyFilters = () => {
    setStatus(draftStatus);
    setModel(draftModel);
    setProvider(draftProvider);
    if (draftProvider !== (focusProviderName || focusProviderId)) {
      setAppliedProviderId(null);
    }
    setPage(1);
    setFilterOpen(false);
  };

  const clearFilters = () => {
    setDraftStatus(null);
    setDraftModel(null);
    setDraftProvider(null);
    setStatus(null);
    setModel(null);
    setProvider(null);
    setAppliedProviderId(null);
    setPage(1);
    setFilterOpen(false);
  };

  const resetAllFilters = useCallback(() => {
    isFromProviderHistoryRef.current = false;
    setFocusDismissed(true);
    setRange("today");
    setGroupBy("none");
    setStatus(null);
    setDraftStatus(null);
    setModel(null);
    setDraftModel(null);
    setProvider(null);
    setDraftProvider(null);
    setAppliedProviderId(null);
    setPage(1);
    setFilterOpen(false);
    onClearProviderDayFocus?.();
  }, [onClearProviderDayFocus]);

  const prevActiveRef = useRef(isActive);
  useEffect(() => {
    if (prevActiveRef.current && !isActive) {
      if (
        isFromProviderHistoryRef.current ||
        activeFocusProviderId !== null ||
        activeFocusDay !== null
      ) {
        resetAllFilters();
      }
    }
    prevActiveRef.current = isActive;
  }, [isActive, activeFocusProviderId, activeFocusDay, resetAllFilters]);

  const activeStatus = draftStatus ?? status;
  const activeModel = draftModel ?? model;
  const activeProvider = draftProvider ?? provider;
  const hasActiveFilter =
    activeStatus !== null || activeModel !== null || activeProvider !== null;

  const hasActiveQuery =
    range !== "today" ||
    groupBy !== "none" ||
    status !== null ||
    model !== null ||
    provider !== null ||
    appliedProviderId !== null ||
    activeFocusProviderId !== null ||
    activeFocusDay !== null;

  const filterLabel = (() => {
    const parts: string[] = [];
    if (activeStatus) {
      parts.push(t(usageStatusTranslationKey(activeStatus), STATUS_FALLBACKS[activeStatus]));
    }
    if (activeModel) {
      parts.push(activeModel);
    }
    if (activeProvider) {
      parts.push(activeProvider);
    }
    return parts.length > 0 ? parts.join(" · ") : t("aiGatewayFilter", "Filter");
  })();

  return (
    <div className="rounded-2xl border bg-card p-5" data-testid="ai-gateway-usage-logs">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2">
          {activeFocusProviderId || activeFocusDay ? (
            <button
              type="button"
              data-testid="ai-gateway-logs-return-history"
              onClick={() => onReturnToProviderHistory?.()}
              aria-label={t("aiGatewayLogsReturnHistory", "Back to provider history")}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium transition hover:bg-muted"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              <span>{t("aiGatewayLogsReturnHistory", "Back to provider history")}</span>
            </button>
          ) : null}
          <div className="space-y-0.5">
            <h2 className="text-base font-semibold text-foreground">
              {t("aiGatewayLogsTab", "Request logs")}
            </h2>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <SelectDropdown
            value={range}
            options={rangeOptions}
            onChange={(nextRange) => {
              setRange(nextRange);
              setPage(1);
            }}
            testId="ai-gateway-logs-range"
            ariaLabel={t("aiGatewayRangeToday", "Time range")}
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
            testId="ai-gateway-logs-group"
            ariaLabel={t("aiGatewayGroupNone", "Grouping")}
          />
          <div className="relative inline-block text-left" ref={filterRef}>
            <button
              type="button"
              data-testid="ai-gateway-logs-filter-trigger"
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
                data-testid="ai-gateway-logs-filter-panel"
                className="absolute right-0 top-full z-30 mt-1.5 w-80 space-y-3 rounded-xl border bg-card p-4 shadow-lg animate-in fade-in-0 zoom-in-95"
              >
                <div className="space-y-1.5">
                  <div className="text-xs font-semibold text-muted-foreground">
                    {t("aiGatewayFilterStatus", "Status")}
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
                      {t("aiGatewayFilterAnyStatus", "Any status")}
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
                    {t("aiGatewayFilterModel", "Model")}
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
                      {t("aiGatewayFilterAnyModel", "Any model")}
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

                <div className="space-y-1.5">
                  <div className="text-xs font-semibold text-muted-foreground">
                    {t("aiGatewayFilterProvider", "Provider")}
                  </div>
                  <div className="flex max-h-40 flex-wrap gap-1.5 overflow-y-auto">
                    <button
                      type="button"
                      onClick={() => selectProvider(null)}
                      aria-pressed={activeProvider === null}
                      className={`rounded-md border px-2.5 py-1 text-xs transition-colors ${
                        activeProvider === null
                          ? "border-primary bg-primary text-primary-foreground font-medium shadow-sm"
                          : "border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                      }`}
                    >
                      {t("aiGatewayFilterAnyProvider", "Any provider")}
                    </button>
                    {providerOptions.map((option) => (
                      <button
                        key={option}
                        type="button"
                        onClick={() => selectProvider(option)}
                        aria-pressed={activeProvider === option}
                        className={`rounded-md border px-2.5 py-1 text-xs transition-colors ${
                          activeProvider === option
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
                      {t("aiGatewayFilterClear", "Clear")}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    onClick={applyFilters}
                    className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90"
                  >
                    {t("aiGatewayFilterApply", "Apply")}
                  </button>
                </div>
              </div>
            ) : null}
          </div>
          {hasActiveQuery ? (
            <button
              type="button"
              data-testid="ai-gateway-logs-reset-filters"
              onClick={resetAllFilters}
              aria-label={t("aiGatewayLogsResetFilters", "Clear filters")}
              title={t("aiGatewayLogsResetFilters", "Clear filters")}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-dashed px-3 text-xs whitespace-nowrap text-muted-foreground transition hover:bg-muted hover:text-foreground shadow-xs"
            >
              <X className="h-3.5 w-3.5" />
              <span>{t("aiGatewayLogsResetFilters", "Clear filters")}</span>
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => void load({ refresh: true })}
            disabled={refreshing}
            aria-label={t("aiGatewayRefresh", "Refresh")}
            title={t("aiGatewayRefresh", "Refresh")}
            className="inline-flex h-8 w-8 items-center justify-center rounded-lg border bg-background transition hover:bg-muted disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />
          </button>
        </div>
      </div>


      {refreshing ? (
        <div role="status" className="mt-3 text-xs text-muted-foreground">
          {t("aiGatewayRefreshing", "Refreshing...")}
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
            {t("aiGatewayLogsEmpty", "No matching requests.")}
          </p>
        ) : (
          <div className="mt-4 overflow-x-hidden rounded-lg border">
            <table
              className="w-full text-left text-xs"
              data-testid="ai-gateway-logs-grouped"
            >
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsGroupColumn", "Group")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsRequestsColumn", "Requests")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsErrorsColumn", "Errors")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsLastRequestColumn", "Last request")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {groups.map((group) => (
                  <tr
                    key={group.group}
                    className="border-t"
                    data-testid="ai-gateway-logs-group-row"
                  >
                    <td className="px-3 py-2 font-medium whitespace-nowrap">
                      {formatUsageGroupLabel(groupBy, group.group)}
                    </td>
                    <td className="px-3 py-2 text-left whitespace-nowrap">
                      {formatCount(group.request_count)}
                    </td>
                    <td className="px-3 py-2 text-left whitespace-nowrap">
                      {formatCount(group.error_count)}
                    </td>
                    <td className="px-3 py-2 text-left whitespace-nowrap">
                      {formatUtc8DateTime(group.last_request_at_ms) ?? "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      ) : records.length === 0 ? (
        (isDayString(range) || activeFocusDay !== null) &&
        (provider !== null || activeFocusProviderId !== null) &&
        !status &&
        !model ? (
          <p
            data-testid="ai-gateway-logs-day-empty"
            className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground"
          >
            {t(
              "aiGatewayLogsDayEmpty",
              "No request details are retained for this provider and day.",
            )}
          </p>
        ) : (
          <p className="mt-4 rounded-xl border border-dashed bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
            {t("aiGatewayLogsEmpty", "No matching requests.")}
          </p>
        )
      ) : (
        <div className="mt-4 space-y-2">
          <div className="overflow-x-hidden rounded-lg border min-h-[220px]">
            <table
              className="w-full text-left text-xs"
              data-testid="ai-gateway-logs-ungrouped"
            >
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsTimeColumn", "Time")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsStatusColumn", "Status")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsModelColumn", "Model")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsDurationColumn", "Latency")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsTokensColumn", "Tokens")}
                  </th>
                  <th className="px-3 py-2 text-left font-medium whitespace-nowrap">
                    {t("aiGatewayLogsCostColumn", "Cost ($)")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {records.map((item, index) => (
                  <LogRecordRow
                    key={`${item.timestamp_ms}-${item.local_model}-${index}`}
                    item={item}
                    index={index}
                    recordCount={records.length}
                    t={t}
                  />
                ))}
              </tbody>
            </table>
          </div>

          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span data-testid="ai-gateway-logs-total-count">
              {t(
                "aiGatewayLogsTotalCount",
                "Total {{total}} requests",
                { total: formatCount(pageData.total) },
              )}
            </span>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => setPage((prev) => Math.max(1, prev - 1))}
                disabled={page <= 1}
                className="rounded-md border bg-background px-2.5 py-1 font-medium transition hover:bg-muted disabled:opacity-50"
              >
                {t("aiGatewayLogsPagePrev", "Previous")}
              </button>
              <span>
                {t(
                  "aiGatewayLogsPageSummary",
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
                {t("aiGatewayLogsPageNext", "Next")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export const UsageLogsPanel = memo(UsageLogsPanelComponent);
