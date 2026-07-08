import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { skillModelOptions } from "./skillsModelOptions";
import { errorToMessage } from "@/lib/messages";

type AiModelId = "claude" | "gemini" | "codex" | "opencode";
type AiUsageWindowDays = 7 | 15 | 30;
type ToolLoadState = "loading" | "ready" | "error";

interface AiUsageSummary {
  total_tokens: number;
  calls: number;
  sessions: number;
  cache_hit_rate: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
}

interface AiUsageDaily extends AiUsageSummary {
  date: string;
}

interface AiUsageToolStats {
  tool: AiModelId;
  source_status: string;
  summary: AiUsageSummary;
  daily: AiUsageDaily[];
  peak_day?: {
    date: string;
    total_tokens: number;
    calls: number;
  } | null;
  scanned_sessions: number;
  scanned_calls: number;
  errors: string[];
}

interface AiUsageDayBreakdown {
  tool: AiModelId;
  total_tokens: number;
  calls: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
}

interface AiUsageDayStats {
  date: string;
  total_tokens: number;
  calls: number;
  sessions: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
  breakdown: AiUsageDayBreakdown[];
}

interface ToolState {
  status: ToolLoadState;
  data: AiUsageToolStats | null;
  error: string;
}

const AI_USAGE_WINDOWS: AiUsageWindowDays[] = [7, 15, 30];
const AI_USAGE_TOOLS: AiModelId[] = ["claude", "codex", "gemini", "opencode"];

function emptyAiUsageSummary(): AiUsageSummary {
  return {
    total_tokens: 0,
    calls: 0,
    sessions: 0,
    cache_hit_rate: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_tokens: 0,
  };
}

function initialToolStates(): Record<AiModelId, ToolState> {
  return AI_USAGE_TOOLS.reduce(
    (acc, tool) => ({
      ...acc,
      [tool]: { status: "loading", data: null, error: "" },
    }),
    {} as Record<AiModelId, ToolState>,
  );
}

function formatCompactNumber(value: number): string {
  return new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

function formatWholeNumber(value: number): string {
  return new Intl.NumberFormat(undefined).format(value);
}

function formatChineseUnitNumber(value: number): string {
  const absValue = Math.abs(value);
  const units = [
    { threshold: 100_000_000, suffix: "亿" },
    { threshold: 10_000_000, suffix: "千万" },
    { threshold: 1_000_000, suffix: "百万" },
    { threshold: 10_000, suffix: "万" },
  ];
  const unit = units.find((item) => absValue >= item.threshold);
  if (!unit) return formatWholeNumber(value);

  const formatted = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
  }).format(value / unit.threshold);
  return `${formatted}${unit.suffix}`;
}

function formatPercent(value: number): string {
  return `${Math.round(value)}%`;
}

function formatUsageDate(date: string): string {
  const parsed = new Date(`${date}T00:00:00`);
  if (Number.isNaN(parsed.getTime())) return date;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(parsed);
}

function usageSourceStatusLabel(status: string): string {
  switch (status) {
    case "available":
      return "Available";
    case "empty":
      return "No local sessions";
    case "error":
      return "Read error";
    default:
      return "Not found";
  }
}

function formatDailyTooltip(day: AiUsageDaily, t: ReturnType<typeof useTranslation>["t"]) {
  return [
    day.date,
    `${t("aiUsageTotalTokens", "Total Tokens")}: ${formatChineseUnitNumber(day.total_tokens)}`,
    `${t("aiUsageCalls", "Calls")}: ${formatChineseUnitNumber(day.calls)}`,
    `${t("aiUsageInput", "Input")}: ${formatChineseUnitNumber(day.input_tokens)}`,
    `${t("aiUsageOutput", "Output")}: ${formatChineseUnitNumber(day.output_tokens)}`,
    `${t("aiUsageCache", "Cache")}: ${formatChineseUnitNumber(day.cache_tokens)}`,
  ].join("\n");
}

export function AiUsageStats({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const [days, setDays] = useState<AiUsageWindowDays>(7);
  const [toolStates, setToolStates] =
    useState<Record<AiModelId, ToolState>>(initialToolStates);
  const requestSeqRef = useRef(0);

  const toolOptionMap = useMemo(
    () => new Map(skillModelOptions.map((option) => [option.id, option])),
    [],
  );
  const isRefreshing = Object.values(toolStates).some(
    (toolState) => toolState.status === "loading",
  );

  const todayStr = useMemo(() => {
    const now = new Date();
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
  }, []);
  const [selectedDate, setSelectedDate] = useState(todayStr);
  const [dayStats, setDayStats] = useState<AiUsageDayStats | null>(null);
  const [dayStatsLoading, setDayStatsLoading] = useState(false);
  const [dayStatsError, setDayStatsError] = useState("");

  const queryDayStats = (date: string) => {
    if (!date) return;
    setDayStatsError("");
    setDayStatsLoading(true);
    setDayStats(null);
    void invoke<AiUsageDayStats>("sessions_usage_day_stats", { date })
      .then((data) => {
        setDayStats(data);
        setDayStatsLoading(false);
      })
      .catch((error) => {
        setDayStatsError(errorToMessage(error));
        setDayStatsLoading(false);
      });
  };

  const loadUsage = (nextDays: AiUsageWindowDays) => {
    const requestSeq = requestSeqRef.current + 1;
    requestSeqRef.current = requestSeq;
    setToolStates((current) =>
      AI_USAGE_TOOLS.reduce(
        (acc, tool) => {
          acc[tool] = {
            status: "loading",
            data: current[tool]?.data || null,
            error: "",
          };
          return acc;
        },
        {} as Record<AiModelId, ToolState>,
      ),
    );

    AI_USAGE_TOOLS.forEach((tool) => {
      void invoke<AiUsageToolStats>("sessions_usage_tool_stats", {
        tool,
        days: nextDays,
      })
        .then((data) => {
          if (requestSeqRef.current !== requestSeq) return;
          setToolStates((current) => ({
            ...current,
            [tool]: { status: "ready", data, error: "" },
          }));
        })
        .catch((error) => {
          if (requestSeqRef.current !== requestSeq) return;
          setToolStates((current) => ({
            ...current,
            [tool]: {
              status: "error",
              data: current[tool]?.data || null,
              error: errorToMessage(error),
            },
          }));
        });
    });
  };

  const hasAutoQueried = useRef(false);

  useEffect(() => {
    if (!isVisible) return;
    loadUsage(days);
    if (!hasAutoQueried.current) {
      hasAutoQueried.current = true;
      queryDayStats(todayStr);
    }
  }, [isVisible, days, todayStr]);

  return (
    <div className="h-full overflow-y-auto bg-background p-6">
      <div className="mx-auto max-w-6xl space-y-6" data-testid="ai-usage-panel">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="space-y-1">
            <h1 className="text-2xl font-bold">
              {t("aiUsagePageTitle", "AI Usage Stats")}
            </h1>
            <p className="text-sm text-muted-foreground">
              {t(
                "aiUsagePageDesc",
                "Usage is calculated from local session history only.",
              )}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <div className="grid grid-cols-3 gap-1 rounded-xl bg-muted p-1">
              {AI_USAGE_WINDOWS.map((windowDays) => (
                <button
                  key={windowDays}
                  type="button"
                  onClick={() => setDays(windowDays)}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${
                    days === windowDays
                      ? "bg-background text-foreground shadow-sm"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t("aiUsageDaysLabel", "{{days}}d", { days: windowDays })}
                </button>
              ))}
            </div>
            <button
              type="button"
              onClick={() => loadUsage(days)}
              disabled={isRefreshing}
              className="inline-flex h-9 w-9 items-center justify-center rounded-xl border bg-background hover:bg-muted disabled:opacity-50"
              title={t("aiUsageRefresh", "Refresh")}
              aria-label={t("aiUsageRefresh", "Refresh")}
            >
              <RefreshCw
                className={`h-4 w-4 ${isRefreshing ? "animate-spin" : ""}`}
              />
            </button>
          </div>
        </div>

        {isRefreshing && (
          <div
            className="rounded-xl border bg-muted/30 px-4 py-3 text-sm text-muted-foreground"
            role="status"
          >
            {t("aiUsageLoadingData", "Loading usage data...")}
          </div>
        )}

        <div
          className="rounded-xl border bg-background p-4"
          data-testid="ai-usage-day-stats"
        >
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold">
              {t("aiUsageDayStatsTitle", "Daily Stats")}
            </h2>
            <div className="flex items-center gap-2">
              <input
                type="date"
                value={selectedDate}
                max={todayStr}
                onChange={(e) => {
                  setSelectedDate(e.target.value);
                  queryDayStats(e.target.value);
                }}
                className="rounded-lg border bg-background px-3 py-1.5 text-xs"
                aria-label={t("aiUsageDayStatsSelectDate", "Select Date")}
              />
            </div>
          </div>

          {dayStatsLoading && (
            <div className="mt-4 rounded-xl border border-dashed bg-muted/30 px-4 py-5 text-sm text-muted-foreground">
              {t("aiUsageLoadingData", "Loading usage data...")}
            </div>
          )}

          {dayStatsError && (
            <div className="mt-4 rounded-xl border border-destructive/20 bg-destructive/10 px-4 py-3 text-sm text-destructive">
              {dayStatsError}
            </div>
          )}

          {!dayStats && !dayStatsLoading && !dayStatsError && (
            <div className="mt-4 rounded-xl border border-dashed bg-muted/30 px-4 py-5 text-sm text-muted-foreground">
              {t("aiUsageDayStatsNoData", "Select a date to view total token usage.")}
            </div>
          )}

          {dayStats && (
            <>
              <div className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
                {[
                  {
                    label: t("aiUsageTotalTokens", "Total Tokens"),
                    value: formatCompactNumber(dayStats.total_tokens),
                  },
                  {
                    label: t("aiUsageCalls", "Calls"),
                    value: formatWholeNumber(dayStats.calls),
                  },
                  {
                    label: t("aiUsageSessions", "Sessions"),
                    value: formatWholeNumber(dayStats.sessions),
                  },
                  {
                    label: t("aiUsageInput", "Input"),
                    value: formatCompactNumber(dayStats.input_tokens),
                  },
                  {
                    label: t("aiUsageOutput", "Output"),
                    value: formatCompactNumber(dayStats.output_tokens),
                  },
                  {
                    label: t("aiUsageCache", "Cache"),
                    value: formatCompactNumber(dayStats.cache_tokens),
                  },
                ].map((item) => (
                  <div
                    key={`day-stat-${item.label}`}
                    className="rounded-lg border bg-card px-3 py-2"
                  >
                    <div className="text-[11px] font-medium uppercase text-muted-foreground">
                      {item.label}
                    </div>
                    <div className="mt-1 truncate text-base font-semibold">
                      {item.value}
                    </div>
                  </div>
                ))}
              </div>

              {dayStats.breakdown.some((b) => b.calls > 0) && (
                <div className="mt-4 overflow-hidden rounded-lg border">
                  <div className="bg-muted/50 px-3 py-2 text-[11px] font-medium uppercase text-muted-foreground">
                    {t("aiUsageDayStatsBreakdown", "Per-Tool Breakdown")}
                  </div>
                  <table className="w-full text-left text-xs">
                    <thead className="bg-muted/30 text-muted-foreground">
                      <tr>
                        <th className="px-3 py-2 font-medium">
                          {t("tool", "Tool")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("aiUsageTotalTokens", "Total Tokens")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("aiUsageCalls", "Calls")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("aiUsageInput", "Input")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("aiUsageOutput", "Output")}
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                          {t("aiUsageCache", "Cache")}
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {dayStats.breakdown.map((b) => {
                        const option = toolOptionMap.get(b.tool);
                        const Icon = option?.Icon;
                        return (
                          <tr
                            key={`day-breakdown-${b.tool}`}
                            className="border-t"
                          >
                            <td className="flex items-center gap-2 px-3 py-2">
                              {Icon && <Icon className="h-3.5 w-3.5 text-primary" />}
                              <span>{option?.label || b.tool}</span>
                            </td>
                            <td className="px-3 py-2 text-right font-medium">
                              {b.calls > 0 ? formatWholeNumber(b.total_tokens) : "-"}
                            </td>
                            <td className="px-3 py-2 text-right">
                              {b.calls > 0 ? formatWholeNumber(b.calls) : "-"}
                            </td>
                            <td className="px-3 py-2 text-right">
                              {b.calls > 0 ? formatWholeNumber(b.input_tokens) : "-"}
                            </td>
                            <td className="px-3 py-2 text-right">
                              {b.calls > 0 ? formatWholeNumber(b.output_tokens) : "-"}
                            </td>
                            <td className="px-3 py-2 text-right">
                              {b.calls > 0 ? formatWholeNumber(b.cache_tokens) : "-"}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              )}
            </>
          )}
        </div>

        <div className="space-y-4">
          {AI_USAGE_TOOLS.map((tool) => {
            const option = toolOptionMap.get(tool);
            const Icon = option?.Icon;
            const state = toolStates[tool];
            const toolStats = state.data;
            const summary = toolStats?.summary || emptyAiUsageSummary();
            const dailyStats = toolStats?.daily || [];
            const dailyStatsDesc = [...dailyStats].sort((first, second) =>
              second.date.localeCompare(first.date),
            );
            const maxDailyTokens = Math.max(
              1,
              ...dailyStats.map((day) => day.total_tokens),
            );
            const noUsage = summary.calls === 0;
            const status = toolStats?.source_status || "unavailable";
            return (
              <div
                key={`ai-usage-${tool}`}
                className="rounded-xl border bg-background p-4"
                data-testid={`ai-usage-tool-${tool}`}
              >
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div className="flex items-center gap-2">
                    {Icon && <Icon className="h-4 w-4 text-primary" />}
                    <div>
                      <h2 className="text-sm font-semibold">
                        {option?.label || tool}
                      </h2>
                      <p className="text-xs text-muted-foreground">
                        {state.status === "loading"
                          ? t("aiUsageLoading", "Loading...")
                          : state.status === "error"
                            ? t("aiUsageLoadFailed", "Load failed")
                            : t(
                                `aiUsageStatus_${status}`,
                                usageSourceStatusLabel(status),
                              )}
                      </p>
                    </div>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {t(
                      "aiUsageScanned",
                      "{{sessions}} sessions, {{calls}} calls",
                      {
                        sessions: formatWholeNumber(
                          toolStats?.scanned_sessions || 0,
                        ),
                        calls: formatWholeNumber(toolStats?.scanned_calls || 0),
                      },
                    )}
                  </div>
                </div>

                {state.status === "error" && (
                  <div className="mt-4 rounded-xl border border-destructive/20 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                    {t("aiUsageToolLoadError", "{{tool}} failed: {{error}}", {
                      tool: option?.label || tool,
                      error: state.error,
                    })}
                  </div>
                )}

                {state.status === "loading" && !toolStats ? (
                  <div className="mt-4 rounded-xl border border-dashed bg-muted/30 px-4 py-5 text-sm text-muted-foreground">
                    {t("aiUsageLoading", "Loading...")}
                  </div>
                ) : noUsage ? (
                  <div className="mt-4 rounded-xl border border-dashed bg-muted/30 px-4 py-5 text-sm text-muted-foreground">
                    {t(
                      "aiUsageEmpty",
                      "No token usage records found in this window.",
                    )}
                  </div>
                ) : (
                  <>
                    <div className="mt-4 grid grid-cols-2 gap-2 lg:grid-cols-4">
                      {[
                        {
                          label: t("aiUsageTotalTokens", "Total Tokens"),
                          value: formatCompactNumber(summary.total_tokens),
                        },
                        {
                          label: t("aiUsageCalls", "Calls"),
                          value: formatWholeNumber(summary.calls),
                        },
                        {
                          label: t("aiUsageSessions", "Sessions"),
                          value: formatWholeNumber(summary.sessions),
                        },
                        {
                          label: t("aiUsageAvgCacheHit", "Avg Cache Hit"),
                          value: formatPercent(summary.cache_hit_rate),
                        },
                        {
                          label: t("aiUsageInput", "Input"),
                          value: formatCompactNumber(summary.input_tokens),
                        },
                        {
                          label: t("aiUsageOutput", "Output"),
                          value: formatCompactNumber(summary.output_tokens),
                        },
                        {
                          label: t("aiUsageCache", "Cache"),
                          value: formatCompactNumber(summary.cache_tokens),
                        },
                        {
                          label: t("aiUsagePeakDay", "Peak Day"),
                          value: toolStats?.peak_day
                            ? `${formatUsageDate(toolStats.peak_day.date)} · ${formatCompactNumber(toolStats.peak_day.total_tokens)}`
                            : "-",
                        },
                      ].map((item) => (
                        <div
                          key={`${tool}-${item.label}`}
                          className="rounded-lg border bg-card px-3 py-2"
                        >
                          <div className="text-[11px] font-medium uppercase text-muted-foreground">
                            {item.label}
                          </div>
                          <div className="mt-1 truncate text-base font-semibold">
                            {item.value}
                          </div>
                        </div>
                      ))}
                    </div>

                    <div className="mt-4 rounded-lg border bg-card p-3">
                      <div className="mb-2 flex items-center justify-between text-xs text-muted-foreground">
                        <span>{t("aiUsageTrend", "Daily Trend")}</span>
                        {toolStats?.peak_day && (
                          <span>
                            {t(
                              "aiUsagePeakDayDetail",
                              "Peak: {{date}}, {{tokens}} tokens, {{calls}} calls",
                              {
                                date: formatUsageDate(toolStats.peak_day.date),
                                tokens: formatWholeNumber(
                                  toolStats.peak_day.total_tokens,
                                ),
                                calls: formatWholeNumber(
                                  toolStats.peak_day.calls,
                                ),
                              },
                            )}
                          </span>
                        )}
                      </div>
                      <div className="flex h-28 items-end gap-1">
                        {dailyStats.map((day) => {
                          const height = Math.max(
                            4,
                            Math.round(
                              (day.total_tokens / maxDailyTokens) * 100,
                            ),
                          );
                          return (
                            <div
                              key={`${tool}-bar-${day.date}`}
                              className="flex min-w-0 flex-1 flex-col items-center gap-1"
                            >
                              <div className="flex h-24 w-full items-end">
                                <div
                                  className="w-full rounded-t bg-primary/70"
                                  style={{ height: `${height}%` }}
                                  title={formatDailyTooltip(day, t)}
                                />
                              </div>
                              <span className="truncate text-[10px] text-muted-foreground">
                                {formatUsageDate(day.date)}
                              </span>
                            </div>
                          );
                        })}
                      </div>
                    </div>

                    <div className="mt-4 overflow-hidden rounded-lg border">
                      <table className="w-full text-left text-xs">
                        <thead className="bg-muted/50 text-muted-foreground">
                          <tr>
                            <th className="px-3 py-2 font-medium">
                              {t("date", "Date")}
                            </th>
                            <th className="px-3 py-2 text-right font-medium">
                              {t("aiUsageTotalTokens", "Total Tokens")}
                            </th>
                            <th className="px-3 py-2 text-right font-medium">
                              {t("aiUsageCalls", "Calls")}
                            </th>
                            <th className="px-3 py-2 text-right font-medium">
                              {t("aiUsageInput", "Input")}
                            </th>
                            <th className="px-3 py-2 text-right font-medium">
                              {t("aiUsageOutput", "Output")}
                            </th>
                            <th className="px-3 py-2 text-right font-medium">
                              {t("aiUsageCache", "Cache")}
                            </th>
                            <th className="px-3 py-2 text-right font-medium">
                              {t("aiUsageCacheHit", "Cache Hit")}
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {dailyStatsDesc.map((day) => (
                            <tr
                              key={`${tool}-row-${day.date}`}
                              className="border-t"
                            >
                              <td className="px-3 py-2">
                                {formatUsageDate(day.date)}
                              </td>
                              <td className="px-3 py-2 text-right font-medium">
                                {formatWholeNumber(day.total_tokens)}
                              </td>
                              <td className="px-3 py-2 text-right">
                                {formatWholeNumber(day.calls)}
                              </td>
                              <td className="px-3 py-2 text-right">
                                {formatWholeNumber(day.input_tokens)}
                              </td>
                              <td className="px-3 py-2 text-right">
                                {formatWholeNumber(day.output_tokens)}
                              </td>
                            <td className="px-3 py-2 text-right">
                              {formatWholeNumber(day.cache_tokens)}
                            </td>
                            <td className="px-3 py-2 text-right">
                              {day.calls > 0
                                ? formatPercent(day.cache_hit_rate)
                                : "-"}
                            </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </>
                )}

                {(toolStats?.errors || []).length > 0 && (
                  <div className="mt-3 rounded-lg border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-700">
                    {t("aiUsageToolErrors", "{{count}} source read errors", {
                      count: toolStats?.errors.length || 0,
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
