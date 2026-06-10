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

  useEffect(() => {
    if (!isVisible) return;
    loadUsage(days);
  }, [isVisible, days]);

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

        <div className="space-y-4">
          {AI_USAGE_TOOLS.map((tool) => {
            const option = toolOptionMap.get(tool);
            const Icon = option?.Icon;
            const state = toolStates[tool];
            const toolStats = state.data;
            const summary = toolStats?.summary || emptyAiUsageSummary();
            const maxDailyTokens = Math.max(
              1,
              ...(toolStats?.daily || []).map((day) => day.total_tokens),
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
                          label: t("aiUsageCacheHit", "Cache Hit"),
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
                        {(toolStats?.daily || []).map((day) => {
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
                                  title={`${day.date}: ${formatWholeNumber(day.total_tokens)}`}
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
                          </tr>
                        </thead>
                        <tbody>
                          {(toolStats?.daily || []).map((day) => (
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
