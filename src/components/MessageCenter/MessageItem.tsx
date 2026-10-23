import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  AlertTriangle,
  Check,
  CheckCircle2,
  Copy,
  Info,
  MailCheck,
  MailOpen,
  Star,
} from "lucide-react";
import type { MessageRecord, MessageSeverity } from "@/lib/messages";
import { getSourceLabel } from "./types";

interface MessageItemProps {
  message: MessageRecord;
  isSelected: boolean;
  isFocused?: boolean;
  onSelect: (id: string) => void;
  onToggleRead: (id: string, e?: React.MouseEvent) => void;
  onToggleStar?: (id: string, e?: React.MouseEvent) => void;
}

function renderSeverityIcon(severity: MessageSeverity) {
  if (severity === "success") return <CheckCircle2 className="h-3.5 w-3.5" />;
  if (severity === "warning") return <AlertTriangle className="h-3.5 w-3.5" />;
  if (severity === "error") return <AlertCircle className="h-3.5 w-3.5" />;
  return <Info className="h-3.5 w-3.5" />;
}

function getSeverityIconColor(severity: MessageSeverity) {
  if (severity === "success") return "text-emerald-500";
  if (severity === "warning") return "text-amber-500";
  if (severity === "error") return "text-rose-500";
  return "text-sky-500";
}

function formatRelativeTime(ts: number, language: string) {
  if (!Number.isFinite(ts) || ts <= 0) return "-";
  const diff = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  const zh = language.startsWith("zh");
  if (diff < 60) return zh ? "刚刚" : "just now";
  if (diff < 3600) {
    const value = Math.floor(diff / 60);
    return zh ? `${value} 分钟前` : `${value}m ago`;
  }
  if (diff < 86400) {
    const value = Math.floor(diff / 3600);
    return zh ? `${value} 小时前` : `${value}h ago`;
  }
  const value = Math.floor(diff / 86400);
  return zh ? `${value} 天前` : `${value}d ago`;
}

export function MessageItem({
  message,
  isSelected,
  isFocused = false,
  onSelect,
  onToggleRead,
  onToggleStar,
}: MessageItemProps) {
  const { t, i18n } = useTranslation();
  const [copied, setCopied] = useState(false);

  const isUnread = !message.read_at;

  const handleCopySummary = (e: React.MouseEvent) => {
    e.stopPropagation();
    const text = `${message.title}\n${message.summary || ""}\n${message.detail || ""}`.trim();
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onSelect(message.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(message.id);
        }
      }}
      className={`group relative cursor-pointer rounded-xl border p-3 text-left transition-all duration-150 ${
        isSelected
          ? isFocused
            ? "border-amber-500/40 bg-muted/40 shadow-xs ring-1 ring-amber-500/20"
            : "border-foreground/30 bg-muted/40 shadow-xs ring-1 ring-foreground/10"
          : isUnread
            ? isFocused
              ? "border-amber-500/30 bg-card hover:border-amber-500/40 hover:bg-muted/20 shadow-xs"
              : "border-border/80 bg-card hover:border-foreground/20 hover:bg-muted/20 shadow-xs"
            : isFocused
              ? "border-amber-500/20 bg-card/60 opacity-85 hover:opacity-100 hover:border-amber-500/30 hover:bg-muted/20"
              : "border-border/40 bg-card/60 opacity-75 hover:opacity-100 hover:border-border hover:bg-muted/20"
      }`}
    >
      <div className="flex items-start gap-2.5">
        {/* 未读状态指示点 */}
        <div className="pt-2 shrink-0">
          <span
            className={`block h-1.5 w-1.5 rounded-full transition-transform ${
              isUnread ? "bg-primary scale-100" : "bg-transparent scale-50"
            }`}
          />
        </div>

        {/* 严重级别小图标（中性轻底色 + 柔和状态图标） */}
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-muted/60 transition-colors group-hover:bg-muted">
          <span className={getSeverityIconColor(message.severity)}>
            {renderSeverityIcon(message.severity)}
          </span>
        </div>

        {/* 核心内容区 */}
        <div className="min-w-0 flex-1">
          {/* 来源与时间栏 */}
          <div className="mb-1 flex items-center gap-1.5 flex-wrap">
            <span className="rounded bg-muted/70 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
              {t(`messageSource_${message.source}`, getSourceLabel(message.source))}
            </span>
            {isFocused ? (
              <span className="inline-flex items-center gap-0.5 rounded bg-amber-500/10 px-1.5 py-0.5 text-[9px] font-medium text-amber-600 dark:text-amber-400">
                <Star className="h-2.5 w-2.5 fill-amber-500 text-amber-500" />
                {t("messageCenterStarredBadge", "特别关注")}
              </span>
            ) : null}
            {message.occurrences > 1 ? (
              <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">
                x{message.occurrences}
              </span>
            ) : null}
            <span className="ml-auto text-[11px] text-muted-foreground/60">
              {formatRelativeTime(message.last_seen_at, i18n.language)}
            </span>
          </div>

          {/* 标题 */}
          <div
            className={`text-xs leading-snug break-words ${
              isUnread
                ? "font-semibold text-foreground"
                : "font-normal text-muted-foreground"
            }`}
          >
            {message.title}
          </div>

          {/* 摘要预览 */}
          {message.summary ? (
            <div className="mt-1 line-clamp-2 text-[11px] leading-relaxed text-muted-foreground/80">
              {message.summary}
            </div>
          ) : null}
        </div>
      </div>

      {/* 悬停快捷操作栏 (Hover Actions) */}
      <div className="absolute top-2.5 right-2.5 hidden items-center gap-1 rounded-lg border bg-background/90 p-0.5 shadow-xs backdrop-blur-xs group-hover:flex">
        {onToggleStar ? (
          <button
            type="button"
            onClick={(e) => onToggleStar(message.id, e)}
            className={`rounded-md p-1 transition-colors ${
              isFocused
                ? "text-amber-500 hover:bg-amber-500/10"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
            title={
              isFocused
                ? t("messageCenterUnstar", "取消关注")
                : t("messageCenterStar", "特别关注")
            }
          >
            <Star
              className={`h-3.5 w-3.5 ${
                isFocused ? "fill-amber-500 text-amber-500" : ""
              }`}
            />
          </button>
        ) : null}
        <button
          type="button"
          onClick={(e) => onToggleRead(message.id, e)}
          className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          title={
            isUnread
              ? t("messageCenterMarkAsRead", "Mark as read")
              : t("messageCenterMarkAsUnread", "Mark as unread")
          }
        >
          {isUnread ? <MailCheck className="h-3.5 w-3.5" /> : <MailOpen className="h-3.5 w-3.5" />}
        </button>
        <button
          type="button"
          onClick={handleCopySummary}
          className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          title={t("messageCenterCopyLogs", "Copy summary")}
        >
          {copied ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
        </button>
      </div>
    </div>
  );
}
