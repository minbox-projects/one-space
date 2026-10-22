import { useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  Clock,
  Loader2,
  Play,
  RotateCcw,
  ShieldCheck,
  Trash2,
  Wrench,
} from "lucide-react";
import { ToolStatusBadge } from "@/components/toolbox/ToolStatusBadge";
import { Switch } from "@/components/ui/switch";
import { HeartbeatBar } from "./HeartbeatBar";
import type { MonitorSummary } from "./types";

interface MonitorCardProps {
  summary: MonitorSummary;
  onToggle: (id: string, enabled: boolean) => void;
  onTestProbe: (summary: MonitorSummary) => void;
  onEdit: (summary: MonitorSummary) => void;
  onDelete: (id: string) => void;
  onClearHistory: (id: string) => void;
  isTesting?: boolean;
}

export function MonitorCard({
  summary,
  onToggle,
  onTestProbe,
  onEdit,
  onDelete,
  onClearHistory,
  isTesting = false,
}: MonitorCardProps) {
  const [expanded, setExpanded] = useState(false);
  const target = summary.target;

  const tone =
    summary.currentStatus === "up"
      ? "success"
      : summary.currentStatus === "down"
        ? "error"
        : summary.currentStatus === "degraded"
          ? "warning"
          : "neutral";

  const statusLabel =
    summary.currentStatus === "up"
      ? "正常"
      : summary.currentStatus === "down"
        ? "故障"
        : summary.currentStatus === "degraded"
          ? "性能下降"
          : summary.currentStatus === "paused"
            ? "已暂停"
            : "探测中";

  return (
    <div
      data-testid={`monitor-card-${target.id}`}
      className="rounded-xl border border-border/70 bg-card p-4 transition-all hover:border-border hover:shadow-sm"
    >
      <div className="flex flex-col gap-3">
        {/* 卡片头部 */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-center gap-2.5 flex-wrap">
            <ToolStatusBadge tone={tone} label={statusLabel} />
            <h3 className="font-semibold text-base text-foreground tracking-tight">
              {target.name}
            </h3>
            <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] uppercase text-muted-foreground">
              {target.probeType === "http"
                ? target.httpOptions?.method || "HTTP"
                : target.probeType}
            </span>
            {target.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-md border border-border/40 px-1.5 py-0.5 text-[11px] text-muted-foreground"
              >
                #{tag}
              </span>
            ))}
          </div>

          <div className="flex items-center gap-3">
            <Switch
              checked={target.isEnabled}
              onCheckedChange={(checked) => onToggle(target.id, checked)}
              title={target.isEnabled ? "暂停监控" : "开启监控"}
            />
          </div>
        </div>

        {/* 目标地址与参数 */}
        <div className="flex items-center gap-4 text-xs text-muted-foreground font-mono truncate">
          <span className="truncate" title={target.target}>
            {target.target}
          </span>
          <span className="shrink-0 flex items-center gap-1">
            <Clock className="w-3.5 h-3.5" />
            {target.intervalSec}s
          </span>
          {summary.sslDaysRemaining !== null && summary.sslDaysRemaining !== undefined && (
            <span className="shrink-0 flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
              <ShieldCheck className="w-3.5 h-3.5" />
              SSL: {summary.sslDaysRemaining}天
            </span>
          )}
        </div>

        {/* 心跳方块条 */}
        <div className="pt-1">
          <div className="flex items-center justify-between text-[11px] text-muted-foreground mb-1">
            <span>心跳历史 (最近 60 次)</span>
            <div className="flex items-center gap-3">
              <span>
                24h 可用率:{" "}
                <strong
                  className={`font-mono ${
                    summary.uptimeRatio24h >= 99.0
                      ? "text-emerald-600 dark:text-emerald-400"
                      : "text-amber-600 dark:text-amber-400"
                  }`}
                >
                  {summary.uptimeRatio24h.toFixed(2)}%
                </strong>
              </span>
              <span>
                最新响应:{" "}
                <strong className="font-mono text-foreground">
                  {summary.lastResponseTimeMs !== undefined && summary.lastResponseTimeMs !== null
                    ? `${summary.lastResponseTimeMs}ms`
                    : "--"}
                </strong>
              </span>
            </div>
          </div>
          <HeartbeatBar heartbeats={summary.recentHeartbeats} />
        </div>

        {/* 故障提示横幅（若存在活跃 Incident） */}
        {summary.activeIncident && (
          <div className="rounded-lg bg-rose-500/10 border border-rose-500/20 px-3 py-2 text-xs text-rose-600 dark:text-rose-400 flex items-start gap-2">
            <span className="font-semibold shrink-0">当前故障:</span>
            <span className="truncate">{summary.activeIncident.cause}</span>
          </div>
        )}

        {/* 底部操作工具栏 */}
        <div className="flex items-center justify-between pt-1 border-t border-border/40 text-xs">
          <button
            type="button"
            onClick={() => setExpanded(!expanded)}
            className="flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors py-1"
          >
            {expanded ? (
              <>
                <ChevronUp className="w-3.5 h-3.5" />
                收起详情
              </>
            ) : (
              <>
                <ChevronDown className="w-3.5 h-3.5" />
                详细分析
              </>
            )}
          </button>

          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={isTesting}
              onClick={() => onTestProbe(summary)}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors disabled:opacity-50"
              title="立即执行一次探测"
            >
              {isTesting ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Play className="w-3.5 h-3.5" />
              )}
              立即探测
            </button>
            <button
              type="button"
              onClick={() => onEdit(summary)}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            >
              <Wrench className="w-3.5 h-3.5" />
              编辑
            </button>
            <button
              type="button"
              onClick={() => onClearHistory(target.id)}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
              title="清空历史心跳"
            >
              <RotateCcw className="w-3.5 h-3.5" />
              重置
            </button>
            <button
              type="button"
              onClick={() => onDelete(target.id)}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-destructive hover:bg-destructive/10 transition-colors"
            >
              <Trash2 className="w-3.5 h-3.5" />
              删除
            </button>
          </div>
        </div>

        {/* 展开的分析区域 */}
        {expanded && (
          <div className="mt-2 rounded-lg bg-muted/30 p-3 text-xs space-y-2 border border-border/30 animate-in fade-in-0 duration-150">
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
              <div>
                <span className="text-muted-foreground block text-[11px]">24h 平均延迟</span>
                <span className="font-mono text-sm font-semibold">
                  {summary.avgResponseTime24hMs}ms
                </span>
              </div>
              <div>
                <span className="text-muted-foreground block text-[11px]">7 天可用率</span>
                <span className="font-mono text-sm font-semibold">
                  {summary.uptimeRatio7d.toFixed(2)}%
                </span>
              </div>
              <div>
                <span className="text-muted-foreground block text-[11px]">超时时间</span>
                <span className="font-mono text-sm font-semibold">
                  {target.timeoutMs}ms
                </span>
              </div>
              <div>
                <span className="text-muted-foreground block text-[11px]">防抖阈值</span>
                <span className="font-mono text-sm font-semibold">
                  失败 {target.retryCount} 次告警
                </span>
              </div>
            </div>

            {target.probeType === "http" && target.httpOptions && (
              <div className="pt-2 border-t border-border/30 text-[11px] text-muted-foreground space-y-1 font-mono">
                <div>
                  期望状态码: [{target.httpOptions.expectedStatus.join(", ")}]
                </div>
                {target.httpOptions.keyword && (
                  <div>包含关键字: "{target.httpOptions.keyword}"</div>
                )}
                {target.alertConfig.webhookUrl && (
                  <div className="truncate">
                    Webhook 报警: {target.alertConfig.webhookUrl}
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
