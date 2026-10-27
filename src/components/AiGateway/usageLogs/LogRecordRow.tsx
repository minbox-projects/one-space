import { AlertCircle, ArrowDown, ArrowUp, Info, Zap } from "lucide-react";
import type { TFunction } from "i18next";
import {
  formatCacheTokensK,
  formatGatewayDuration,
  formatGatewayTps,
  formatUtc8DateTime,
  getGatewayDurationColorClass,
  getGatewayFirstTokenColorClass,
  getGatewayTpsColorClass,
  usageStatusTranslationKey,
  type UsageLogRecord,
} from "@/lib/aiGateway";
import { CostDetail } from "./CostDetail";
import { LatencyMetricRow } from "./LatencyMetricRow";
import {
  describeNativeReasoning,
  formatCount,
  getHttpStatusReason,
  resolveLogActionableHint,
  statusBadgeStyle,
  STATUS_FALLBACKS,
} from "./formatting";

type LogRecordRowProps = {
  item: UsageLogRecord;
  index: number;
  recordCount: number;
  t: TFunction;
};

export function LogRecordRow({
  item,
  index,
  recordCount,
  t,
}: LogRecordRowProps) {
  return (
    <tr
      className="border-t"
      data-testid="ai-gateway-logs-row"
    >
      <td className="px-3 py-2 font-mono whitespace-nowrap">
        {formatUtc8DateTime(item.timestamp_ms) ?? "—"}
      </td>
      <td className="px-3 py-2 whitespace-nowrap">
        {(() => {
          const style = statusBadgeStyle(item.result);
          const isFailure = item.result === "failure";
          // Absent `terminal` mirrors the backend serde default: terminal.
          const isTerminal = item.terminal !== false;
          const reason = isFailure ? getHttpStatusReason(item.status, t) : null;
          const storedErrorMessage = isFailure
            ? (item.error_message?.trim() ?? "")
            : "";
          const hasErrorInfo = isFailure && (storedErrorMessage !== "" || !!reason);
          const showAbove = index >= 4 && index >= recordCount - 3;
          const tooltipClass = showAbove ? "bottom-full mb-1.5" : "top-full mt-1.5";
          return (
            <div className="flex flex-col gap-0.5">
              <div className="inline-flex items-center gap-1.5">
                <span
                  className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium w-fit ${style.badge}`}
                  data-testid="ai-gateway-logs-status-badge"
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
                {hasErrorInfo ? (
                  <div className="relative group inline-flex items-center group-hover:z-50 focus-within:z-50">
                    <button
                      type="button"
                      className="inline-flex items-center justify-center rounded p-0.5 text-destructive hover:bg-destructive/10 focus:outline-none focus:ring-1 focus:ring-destructive/30 transition-colors cursor-pointer"
                      aria-label={
                        storedErrorMessage !== ""
                          ? `${t("aiGatewayLogsUpstreamError", "Upstream error")}: ${storedErrorMessage}`
                          : (reason?.title ?? t("aiGatewayLogsErrorDetail", "Error details"))
                      }
                      data-testid="ai-gateway-logs-status-reason"
                    >
                      <AlertCircle className="h-3.5 w-3.5 text-destructive shrink-0" data-testid="ai-gateway-logs-error-icon" />
                    </button>
                    <div
                      role="tooltip"
                      className={`pointer-events-none absolute left-0 ${tooltipClass} hidden group-hover:flex group-focus-within:flex flex-col gap-1 rounded-md border bg-popover p-2.5 text-left text-xs text-popover-foreground shadow-lg z-50 min-w-[220px] max-w-[360px]`}
                      data-testid="ai-gateway-logs-error-tooltip"
                    >
                      <div className="font-semibold text-[11px] border-b pb-1 text-destructive flex items-center gap-1">
                        <AlertCircle className="h-3 w-3 shrink-0" />
                        <span>{t("aiGatewayLogsErrorDetail", "Error details")}</span>
                      </div>
                      {storedErrorMessage !== "" ? (
                        <div className="whitespace-pre-wrap break-words text-[11px] text-foreground">
                          {storedErrorMessage}
                        </div>
                      ) : null}
                      {reason ? (
                        <div
                          className={`text-[10px] text-muted-foreground ${
                            storedErrorMessage !== "" ? "pt-0.5 border-t border-border/50" : ""
                          }`}
                        >
                          {reason.title}
                        </div>
                      ) : null}
                      {(() => {
                        const hint = resolveLogActionableHint(
                          item.status,
                          storedErrorMessage,
                          t,
                        );
                        if (!hint) return null;
                        return (
                          <div
                            className="pt-1 mt-0.5 border-t border-border/50 text-[10px] text-amber-600 dark:text-amber-400 font-medium"
                            data-testid="ai-gateway-logs-actionable-hint"
                          >
                            {hint}
                          </div>
                        );
                      })()}
                    </div>
                  </div>
                ) : null}
              </div>
              {!isTerminal ? (
                <span
                  className="text-[10px] text-muted-foreground w-fit"
                  data-testid="ai-gateway-logs-attempt-label"
                >
                  {t("aiGatewayLogsAttemptLabel", "Attempt")}
                </span>
              ) : null}
            </div>
          );
        })()}
      </td>
      <td className="px-3 py-2 min-w-0">
        <div className="flex items-center gap-1.5 whitespace-nowrap min-w-0">
          <span className="font-medium truncate" title={item.local_model}>{item.local_model}</span>
          {item.reasoning_effort ? (
            <span
              className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-normal leading-none bg-purple-500/10 text-purple-600 dark:text-purple-400 border border-purple-500/20 shrink-0"
              data-testid="ai-gateway-logs-reasoning-effort"
              title={`${
                (item.resolved_reasoning_effort?.trim() ?? "") !== "" ||
                item.native_reasoning != null
                  ? t("aiGatewayLogsReasoningRequested", "Requested")
                  : t("reasoningEffort", "Reasoning Effort")
              }: ${item.reasoning_effort}`}
            >
              {item.reasoning_effort}
            </span>
          ) : null}
        </div>
        {(() => {
          const resolved = item.resolved_reasoning_effort?.trim() ?? "";
          const native = item.native_reasoning ?? null;
          if (!resolved && !native) return null;
          const nativeDetailId = `ai-gateway-logs-native-reasoning-detail-${item.timestamp_ms}-${index}`;
          const parameterEntries = native?.parameters
            ? Object.entries(native.parameters)
            : [];
          const nativeDisplay = native
            ? describeNativeReasoning(native, t)
            : null;
          const nativeLabel = nativeDisplay
            ? nativeDisplay.summary
            : t(
                "aiGatewayLogsReasoningNativeUnavailable",
                "Unavailable",
              );
          return (
            <div className="mt-1 flex flex-wrap items-center gap-1.5 text-[10px]">
              {resolved ? (
                <span
                  className="inline-flex items-center rounded border border-indigo-500/20 bg-indigo-500/10 px-1.5 py-0.5 leading-none text-indigo-600 dark:text-indigo-400"
                  data-testid="ai-gateway-logs-resolved-reasoning-effort"
                  title={`${t("aiGatewayLogsReasoningResolved", "Resolved")}: ${resolved}`}
                >
                  {resolved}
                </span>
              ) : null}
              <div className="relative group inline-flex items-center group-hover:z-50 focus-within:z-50">
                <button
                  type="button"
                  data-testid="ai-gateway-logs-native-reasoning"
                  aria-label={`${t("aiGatewayLogsReasoningNative", "Native control")}: ${nativeLabel}`}
                  aria-describedby={nativeDetailId}
                  className="inline-flex items-center rounded border border-border/70 bg-muted/50 px-1.5 py-0.5 font-mono leading-none text-foreground/85 transition hover:bg-muted focus:outline-none focus:ring-1 focus:ring-primary/40"
                >
                  {nativeLabel}
                </button>
                <div
                  id={nativeDetailId}
                  role="tooltip"
                  data-testid="ai-gateway-logs-native-reasoning-detail"
                  className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-max min-w-[180px] max-w-[320px] flex-col gap-1 rounded-md border bg-popover p-2.5 text-left text-xs text-popover-foreground shadow-lg group-hover:flex group-focus-within:flex"
                >
                  <div className="border-b pb-1 text-[11px] font-semibold text-muted-foreground">
                    {t("aiGatewayLogsReasoningNative", "Native control")}
                  </div>
                  {native ? (
                    <>
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-muted-foreground">
                          {t("aiGatewayLogsReasoningProtocol", "Protocol")}
                        </span>
                        <span className="font-mono">{native.protocol}</span>
                      </div>
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-muted-foreground">
                          {t("aiGatewayLogsReasoningMode", "Mode")}
                        </span>
                        <span className="font-mono">
                          {nativeDisplay?.modeLabel}
                        </span>
                      </div>
                      {nativeDisplay?.value ? (
                        <div className="flex items-center justify-between gap-3">
                          <span className="text-muted-foreground">
                            {t(
                              nativeDisplay.valueLabelKey,
                              nativeDisplay.valueLabelFallback,
                            )}
                          </span>
                          <span className="font-mono">
                            {nativeDisplay.value}
                          </span>
                        </div>
                      ) : null}
                      {parameterEntries.length > 0 ? (
                        <div
                          data-testid="ai-gateway-logs-native-reasoning-parameters"
                          className="space-y-0.5 border-t border-border/50 pt-1 font-mono text-[10px]"
                        >
                          {parameterEntries.map(([key, value]) => (
                            <div key={key} className="break-all">
                              {key}:{" "}
                              {typeof value === "string"
                                ? value
                                : JSON.stringify(value)}
                            </div>
                          ))}
                        </div>
                      ) : null}
                    </>
                  ) : (
                    <div className="text-[11px] text-muted-foreground">
                      {t(
                        "aiGatewayLogsReasoningNativeUnavailable",
                        "Unavailable",
                      )}
                    </div>
                  )}
                </div>
              </div>
            </div>
          );
        })()}
        {(item.provider_name || item.upstream_model) ? (
          <div className="text-[10px] text-muted-foreground flex items-center gap-1 whitespace-nowrap min-w-0">
            {item.provider_name ? (
              <span
                className="font-medium shrink-0"
                data-testid="ai-gateway-logs-provider-name"
              >
                {item.provider_name}
              </span>
            ) : null}
            {item.provider_name && item.upstream_model ? (
              <span className="opacity-40 shrink-0">·</span>
            ) : null}
            {item.upstream_model ? (
              <span
                className="truncate"
                title={item.upstream_model}
                data-testid="ai-gateway-logs-upstream-model"
              >
                {item.upstream_model}
              </span>
            ) : null}
          </div>
        ) : null}
      </td>
      <td
        className="px-3 py-2 text-left text-[11px] whitespace-nowrap"
        data-testid="ai-gateway-logs-duration-cell"
      >
        <div className="flex flex-col gap-0.5">
          <LatencyMetricRow
            labelTestId="ai-gateway-logs-latency-total-label"
            valueTestId="ai-gateway-logs-latency-total-value"
            unitTestId="ai-gateway-logs-latency-total-unit"
            label={t("aiGatewayLogsLatencyTotal", "Total")}
            value={formatGatewayDuration(item.duration_ms)}
            unit="s"
            colorClass={getGatewayDurationColorClass(item.duration_ms)}
          />
          <LatencyMetricRow
            labelTestId="ai-gateway-logs-latency-first-token-label"
            valueTestId="ai-gateway-logs-latency-first-token-value"
            unitTestId="ai-gateway-logs-latency-first-token-unit"
            label={t("aiGatewayLogsLatencyFirstToken", "First token")}
            value={formatGatewayDuration(item.first_token_ms)}
            unit="s"
            colorClass={getGatewayFirstTokenColorClass(item.first_token_ms)}
          />
          <LatencyMetricRow
            labelTestId="ai-gateway-logs-latency-avg-tps-label"
            valueTestId="ai-gateway-logs-latency-avg-tps-value"
            unitTestId="ai-gateway-logs-latency-avg-tps-unit"
            label={t("aiGatewayLogsLatencyAverageTps", "Average TPS")}
            value={formatGatewayTps(item.avg_tps)}
            unit="t/s"
            colorClass={getGatewayTpsColorClass(item.avg_tps)}
          />
        </div>
      </td>
      <td className="px-3 py-2 text-left font-mono" data-testid="ai-gateway-logs-tokens-cell">
        {(() => {
          const cacheTokens = item.cache_read_tokens + item.cache_write_tokens;
          const showAbove = index >= 4 && index >= recordCount - 3;
          const tooltipClass = showAbove ? "bottom-full mb-1.5" : "top-full mt-1.5";
          const inputFormatted = formatCount(item.input_tokens);
          const outputFormatted = formatCount(item.output_tokens);
          const cacheFormatted = formatCacheTokensK(cacheTokens);
          return (
            <div className="flex flex-col gap-0.5">
              {/* 第一行：下行与上行在同一行，显示真实原值 */}
              <div
                className="flex items-center gap-1.5 text-xs whitespace-nowrap"
                data-testid="ai-gateway-logs-tokens-breakdown"
              >
                <span
                  className="inline-flex items-center gap-0.5"
                  title={`${t("aiGatewayLogsTokensInput", "Input")}: ${inputFormatted}`}
                >
                  <ArrowDown
                    className="h-3 w-3 text-emerald-500 shrink-0"
                    aria-label={t("aiGatewayLogsTokensInput", "Input")}
                    data-testid="ai-gateway-logs-tokens-input-icon"
                  />
                  <span className="text-foreground font-medium">
                    {inputFormatted}
                  </span>
                </span>
                <span className="text-muted-foreground/40 font-sans">·</span>
                <span
                  className="inline-flex items-center gap-0.5"
                  title={`${t("aiGatewayLogsTokensOutput", "Output")}: ${outputFormatted}`}
                >
                  <ArrowUp
                    className="h-3 w-3 text-sky-500 shrink-0"
                    aria-label={t("aiGatewayLogsTokensOutput", "Output")}
                    data-testid="ai-gateway-logs-tokens-output-icon"
                  />
                  <span className="text-foreground font-medium">
                    {outputFormatted}
                  </span>
                </span>
              </div>

              {/* 第二行：缓存在一行，显示 xxK 格式，带有 Zap 图标和明细提示 */}
              <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground whitespace-nowrap">
                <span
                  className="inline-flex items-center gap-0.5"
                  title={`${t("aiGatewayLogsTokensCache", "Cache")}: ${formatCount(cacheTokens)}`}
                >
                  <Zap
                    className="h-3 w-3 text-amber-500/80 shrink-0"
                    aria-label={t("aiGatewayLogsTokensCache", "Cache")}
                    data-testid="ai-gateway-logs-tokens-cache-icon"
                  />
                  <span data-testid="ai-gateway-logs-tokens-cache-value">{cacheFormatted}</span>
                </span>

                <div className="relative group inline-flex items-center shrink-0 group-hover:z-50 focus-within:z-50">
                  <button
                    type="button"
                    className="text-muted-foreground/60 hover:text-foreground transition-colors p-0.5 rounded focus:outline-none cursor-pointer"
                    aria-label={t("aiGatewayLogsTokensDetail", "Tokens breakdown")}
                    data-testid="ai-gateway-logs-tokens-info-btn"
                  >
                    <Info className="h-3 w-3" />
                  </button>
                  <div
                    role="tooltip"
                    className={`pointer-events-none absolute left-0 ${tooltipClass} hidden group-hover:flex group-focus-within:flex flex-col gap-1 rounded-md border bg-popover p-2.5 text-left text-xs text-popover-foreground shadow-lg z-50 min-w-[180px]`}
                    data-testid="ai-gateway-logs-tokens-tooltip"
                  >
                    <div className="font-semibold text-[11px] border-b pb-1 text-muted-foreground">
                      {t("aiGatewayLogsTokensDetail", "Tokens breakdown")}
                    </div>
                    <div className="space-y-0.5 pt-0.5 text-[11px]">
                      <div className="flex items-center justify-between gap-3">
                        <span className="inline-flex items-center gap-1 text-muted-foreground">
                          <ArrowDown className="h-3 w-3 text-emerald-500 shrink-0" />
                          {t("aiGatewayLogsTokensInput", "Input")}:
                        </span>
                        <span className="font-mono font-medium">
                          {inputFormatted}
                        </span>
                      </div>
                      <div className="flex items-center justify-between gap-3">
                        <span className="inline-flex items-center gap-1 text-muted-foreground">
                          <ArrowUp className="h-3 w-3 text-sky-500 shrink-0" />
                          {t("aiGatewayLogsTokensOutput", "Output")}:
                        </span>
                        <span className="font-mono font-medium">
                          {outputFormatted}
                        </span>
                      </div>
                      <div className="flex items-center justify-between gap-3">
                        <span className="inline-flex items-center gap-1 text-muted-foreground">
                          <Zap className="h-3 w-3 text-amber-500/80 shrink-0" />
                          {t("aiGatewayLogsTokensCache", "Cache")}:
                        </span>
                        <span className="font-mono font-medium">
                          {formatCount(cacheTokens)}
                        </span>
                      </div>
                      {(item.cache_read_tokens > 0 || item.cache_write_tokens > 0) && (
                        <div className="text-[10px] text-muted-foreground/70 pl-2 space-y-0.5">
                          <div className="flex items-center justify-between gap-3">
                            <span>
                              {t("aiGatewayLogsTokensCacheRead", "Cache read")}:
                            </span>
                            <span className="font-mono">
                              {formatCount(item.cache_read_tokens)}
                            </span>
                          </div>
                          <div className="flex items-center justify-between gap-3">
                            <span>
                              {t("aiGatewayLogsTokensCacheWrite", "Cache write")}:
                            </span>
                            <span className="font-mono">
                              {formatCount(item.cache_write_tokens)}
                            </span>
                          </div>
                        </div>
                      )}
                      <div className="border-t my-1 border-border"></div>
                      <div className="flex items-center justify-between gap-3 font-semibold">
                        <span>
                          {t("aiGatewayLogsTokensTotal", "Total")}:
                        </span>
                        <span className="font-mono">
                          {formatCount(item.total_tokens)}
                        </span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          );
        })()}
      </td>
      <td className="px-3 py-2 text-left whitespace-nowrap">
        <CostDetail item={item} t={t} />
      </td>
    </tr>
  );
}
