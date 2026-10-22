import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  Copy,
  Check,
  ExternalLink,
  Layers,
  Terminal,
  MailCheck,
  MailOpen,
  X,
} from "lucide-react";
import type { MessageRecord, MessageSeverity, MessageTarget } from "@/lib/messages";
import { getSourceLabel } from "./types";

interface MessageDetailViewProps {
  message: MessageRecord;
  isWideMode: boolean;
  hasPrevious: boolean;
  hasNext: boolean;
  onBack: () => void;
  onSelectPrevious: () => void;
  onSelectNext: () => void;
  onToggleRead: (id: string) => void;
  onNavigate: (target: MessageTarget) => void;
  onClose: () => void;
}

function getSeverityBadgeClass(severity: MessageSeverity) {
  if (severity === "success")
    return "text-emerald-500 bg-muted/60 border-border/60";
  if (severity === "warning")
    return "text-amber-500 bg-muted/60 border-border/60";
  if (severity === "error")
    return "text-rose-500 bg-muted/60 border-border/60";
  return "text-sky-500 bg-muted/60 border-border/60";
}

function formatTimestamp(ts?: number | null) {
  if (!ts || !Number.isFinite(ts)) return "-";
  return new Date(ts * 1000).toLocaleString();
}

export function MessageDetailView({
  message,
  isWideMode,
  hasPrevious,
  hasNext,
  onBack,
  onSelectPrevious,
  onSelectNext,
  onToggleRead,
  onNavigate,
  onClose,
}: MessageDetailViewProps) {
  const { t } = useTranslation();
  const [logsCopied, setLogsCopied] = useState(false);
  const [jsonCopied, setJsonCopied] = useState(false);

  const isUnread = !message.read_at;

  const handleCopyLogs = () => {
    const text = message.detail || message.summary || message.title;
    navigator.clipboard.writeText(text).then(() => {
      setLogsCopied(true);
      setTimeout(() => setLogsCopied(false), 2000);
    });
  };

  const handleCopyJson = () => {
    let text = "";
    try {
      text = JSON.stringify(message.metadata, null, 2);
    } catch {
      text = String(message.metadata);
    }
    navigator.clipboard.writeText(text).then(() => {
      setJsonCopied(true);
      setTimeout(() => setJsonCopied(false), 2000);
    });
  };

  const formattedMetadata = (() => {
    if (!message.metadata) return null;
    try {
      const str = JSON.stringify(message.metadata, null, 2);
      return str === "null" ? null : str;
    } catch {
      return String(message.metadata);
    }
  })();

  return (
    <div className="flex h-full flex-col bg-background">
      {/* 顶部导航与控制条 */}
      <div className="flex h-14 shrink-0 items-center justify-between border-b px-5">
        <div className="flex items-center gap-2">
          {!isWideMode ? (
            <button
              type="button"
              onClick={onBack}
              className="inline-flex items-center gap-1.5 rounded-lg border bg-background px-2.5 py-1 text-xs font-medium text-foreground hover:bg-muted"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              <span>{t("messageCenterBackToList", "Back to list")}</span>
            </button>
          ) : (
            <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
              <Layers className="h-3.5 w-3.5 text-primary" />
              <span>{t("messageCenterTroubleshooting", "Troubleshooting & Stack Trace")}</span>
            </div>
          )}
        </div>

        {/* 上一条 / 下一条 / 状态切换 / 关闭 */}
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onSelectPrevious}
            disabled={!hasPrevious}
            className="rounded-lg p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-30"
            title={t("messageCenterPrev", "Previous")}
          >
            <ChevronLeft className="h-4 w-4" />
          </button>
          <button
            type="button"
            onClick={onSelectNext}
            disabled={!hasNext}
            className="rounded-lg p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-30"
            title={t("messageCenterNext", "Next")}
          >
            <ChevronRight className="h-4 w-4" />
          </button>
          <div className="mx-1 h-3.5 w-[1px] bg-border" />
          <button
            type="button"
            onClick={() => onToggleRead(message.id)}
            className="rounded-lg p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
            title={
              isUnread
                ? t("messageCenterMarkAsRead", "Mark as read")
                : t("messageCenterMarkAsUnread", "Mark as unread")
            }
          >
            {isUnread ? (
              <MailCheck className="h-4 w-4" />
            ) : (
              <MailOpen className="h-4 w-4" />
            )}
          </button>
          {!isWideMode ? (
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
              title={t("close", "Close")}
            >
              <X className="h-4 w-4" />
            </button>
          ) : null}
        </div>
      </div>

      {/* 滚动工作区内容 */}
      <div className="flex-1 overflow-y-auto p-6 space-y-6">
        {/* 来源与级别元数据 Banner */}
        <div className="rounded-xl border bg-card/60 p-4 shadow-xs">
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <span
              className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-semibold ${getSeverityBadgeClass(
                message.severity,
              )}`}
            >
              {t(`messageSeverity_${message.severity}`, message.severity)}
            </span>
            <span className="rounded-md border bg-muted/60 px-2 py-0.5 text-xs font-medium text-foreground">
              {t(
                `messageSource_${message.source}`,
                getSourceLabel(message.source),
              )}
            </span>
            {message.occurrences > 1 ? (
              <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs font-bold text-primary">
                {t("messageCenterOccurrences", "{{count}} occurrences", {
                  count: message.occurrences,
                })}
              </span>
            ) : null}
          </div>

          <h3 className="text-base font-bold leading-snug text-foreground">
            {message.title}
          </h3>

          <div className="mt-3 grid grid-cols-2 gap-3 border-t pt-3 text-xs text-muted-foreground">
            <div>
              <span className="font-medium text-foreground/70">
                {t("createdAt", "Created")}:
              </span>
              <div className="mt-0.5 font-mono">
                {formatTimestamp(message.created_at)}
              </div>
            </div>
            <div>
              <span className="font-medium text-foreground/70">
                {t("lastSeenAt", "Last seen")}:
              </span>
              <div className="mt-0.5 font-mono">
                {formatTimestamp(message.last_seen_at)}
              </div>
            </div>
          </div>
        </div>

        {/* 问题概述 */}
        {message.summary ? (
          <div className="space-y-1.5">
            <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              {t("messageCenterProblemSummary", "Problem Summary")}
            </h4>
            <div className="rounded-xl border bg-muted/30 p-3.5 text-xs leading-relaxed text-foreground">
              {message.summary}
            </div>
          </div>
        ) : null}

        {/* 详细排障日志与堆栈 */}
        {message.detail ? (
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                <Terminal className="h-3.5 w-3.5" />
                <span>{t("messageCenterTroubleshooting", "Troubleshooting Logs")}</span>
              </h4>
              <button
                type="button"
                onClick={handleCopyLogs}
                className="inline-flex items-center gap-1 rounded-md border bg-background px-2 py-1 text-xs font-medium text-foreground hover:bg-muted"
              >
                {logsCopied ? (
                  <>
                    <Check className="h-3 w-3 text-emerald-500" />
                    <span className="text-emerald-500">
                      {t("messageCenterLogsCopied", "Copied")}
                    </span>
                  </>
                ) : (
                  <>
                    <Copy className="h-3 w-3" />
                    <span>{t("messageCenterCopyLogs", "Copy logs")}</span>
                  </>
                )}
              </button>
            </div>
            <pre className="max-h-64 overflow-auto rounded-xl border bg-muted/60 p-4 font-mono text-xs leading-relaxed text-foreground/90 select-text whitespace-pre-wrap break-words">
              {message.detail}
            </pre>
          </div>
        ) : null}

        {/* 元数据 Metadata */}
        {formattedMetadata ? (
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("messageMetadata", "Metadata")}
              </h4>
              <button
                type="button"
                onClick={handleCopyJson}
                className="inline-flex items-center gap-1 rounded-md border bg-background px-2 py-1 text-xs font-medium text-foreground hover:bg-muted"
              >
                {jsonCopied ? (
                  <>
                    <Check className="h-3 w-3 text-emerald-500" />
                    <span className="text-emerald-500">
                      {t("messageCenterJsonCopied", "Copied")}
                    </span>
                  </>
                ) : (
                  <>
                    <Copy className="h-3 w-3" />
                    <span>{t("messageCenterCopyJson", "Copy JSON")}</span>
                  </>
                )}
              </button>
            </div>
            <pre className="max-h-48 overflow-auto rounded-xl border bg-muted/40 p-4 font-mono text-xs text-muted-foreground select-text whitespace-pre-wrap break-words">
              {formattedMetadata}
            </pre>
          </div>
        ) : null}
      </div>

      {/* 底部功能直达动作条 */}
      {message.target?.tab ? (
        <div className="shrink-0 border-t bg-card/40 p-4">
          <button
            type="button"
            onClick={() => onNavigate(message.target!)}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-xs font-semibold text-primary-foreground shadow-sm transition-all hover:bg-primary/90 active:scale-[0.99]"
          >
            <ExternalLink className="h-4 w-4" />
            <span>
              {t("openRelatedFeature", "Open related feature")} (
              {message.target.tab})
            </span>
          </button>
        </div>
      ) : null}
    </div>
  );
}
