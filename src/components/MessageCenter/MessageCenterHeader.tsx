import { useTranslation } from "react-i18next";
import {
  Bell,
  Columns2,
  MailCheck,
  PanelRight,
  RefreshCw,
  X,
} from "lucide-react";
import type { FilterTab } from "./types";

interface MessageCenterHeaderProps {
  unreadCount: number;
  alertsCount: number;
  totalCount: number;
  loading: boolean;
  isWideMode: boolean;
  activeTab: FilterTab;
  onTabChange: (tab: FilterTab) => void;
  onToggleWideMode: () => void;
  onReload: () => void;
  onMarkAllRead: () => void;
  onClose: () => void;
}

export function MessageCenterHeader({
  unreadCount,
  alertsCount,
  totalCount,
  loading,
  isWideMode,
  activeTab,
  onTabChange,
  onToggleWideMode,
  onReload,
  onMarkAllRead,
  onClose,
}: MessageCenterHeaderProps) {
  const { t } = useTranslation();

  return (
    <div className="shrink-0 border-b bg-card/40 backdrop-blur-md">
      {/* 顶栏控制行 */}
      <div className="flex h-14 items-center justify-between px-5">
        <div className="flex items-center gap-2.5">
          <div className="relative flex h-8 w-8 items-center justify-center rounded-lg bg-muted/60 text-foreground/80">
            <Bell className="h-4 w-4" />
            {unreadCount > 0 ? (
              <span className="absolute top-1.5 right-1.5 h-1.5 w-1.5 rounded-full bg-primary" />
            ) : null}
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-xs font-semibold tracking-tight text-foreground">
                {t("messageCenter", "消息中心")}
              </h2>
              {unreadCount > 0 ? (
                <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">
                  {t("messageCenterUnreadCount", "{{count}} unread", {
                    count: unreadCount,
                  })}
                </span>
              ) : (
                <span className="rounded bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground/70">
                  {t("messageCenterAllRead", "All caught up")}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* 右侧操作按钮 */}
        <div className="flex items-center gap-0.5">
          <button
            type="button"
            onClick={onToggleWideMode}
            className={`rounded-lg p-1.5 transition-colors ${
              isWideMode
                ? "bg-muted text-foreground font-medium"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
            title={
              isWideMode
                ? t("messageCenterStandardMode", "Compact drawer")
                : t("messageCenterWideMode", "Split view")
            }
          >
            {isWideMode ? (
              <PanelRight className="h-4 w-4" />
            ) : (
              <Columns2 className="h-4 w-4" />
            )}
          </button>
          <button
            type="button"
            onClick={onReload}
            className="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={t("refresh", "Refresh")}
          >
            <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
          </button>
          <button
            type="button"
            onClick={onMarkAllRead}
            disabled={unreadCount === 0}
            className="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30"
            title={t("markAllRead", "Mark all as read")}
          >
            <MailCheck className="h-4 w-4" />
          </button>
          <div className="mx-1 h-3.5 w-[1px] bg-border" />
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={t("close", "Close")}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* 现代极简跑道式分段器 Tabs */}
      <div className="flex items-center px-5 pb-2.5">
        <div className="inline-flex rounded-lg border border-border/40 bg-muted/30 p-0.5 text-xs text-muted-foreground">
          <button
            type="button"
            onClick={() => onTabChange("all")}
            className={`rounded-md px-2.5 py-1 transition-all ${
              activeTab === "all"
                ? "bg-background text-foreground shadow-xs font-medium"
                : "hover:text-foreground"
            }`}
          >
            {t("messageCenterTabAll", "All")} ({totalCount})
          </button>
          <button
            type="button"
            onClick={() => onTabChange("unread")}
            className={`relative rounded-md px-2.5 py-1 transition-all ${
              activeTab === "unread"
                ? "bg-background text-foreground shadow-xs font-medium"
                : "hover:text-foreground"
            }`}
          >
            <span>{t("messageCenterTabUnread", "Unread")}</span>
            {unreadCount > 0 ? (
              <span className="ml-1.5 rounded bg-muted px-1.5 py-0.2 font-mono text-[10px]">
                {unreadCount}
              </span>
            ) : null}
          </button>
          <button
            type="button"
            onClick={() => onTabChange("alerts")}
            className={`relative rounded-md px-2.5 py-1 transition-all ${
              activeTab === "alerts"
                ? "bg-background text-foreground shadow-xs font-medium"
                : "hover:text-foreground"
            }`}
          >
            <span>{t("messageCenterTabAlerts", "Alerts & Errors")}</span>
            {alertsCount > 0 ? (
              <span className="ml-1.5 rounded bg-muted px-1.5 py-0.2 font-mono text-[10px]">
                {alertsCount}
              </span>
            ) : null}
          </button>
        </div>
      </div>
    </div>
  );
}
