import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Layers } from "lucide-react";
import { useMessageCenter } from "./useMessageCenter";
import { MessageCenterHeader } from "./MessageCenterHeader";
import { MessageCenterToolbar } from "./MessageCenterToolbar";
import { MessageList } from "./MessageList";
import { MessageDetailView } from "./MessageDetailView";
import type { MessageCenterProps } from "./types";

export function MessageCenter({
  open,
  onOpenChange,
  onNavigate,
}: MessageCenterProps) {
  const { t } = useTranslation();
  const {
    messages,
    filteredMessages,
    selectedId,
    setSelectedId,
    selectedMessage,
    unreadCount,
    alertsCount,
    availableSources,
    loading,
    error,
    isWideMode,
    toggleWideMode,
    filters,
    updateFilters,
    clearFilters,
    reloadMessages,
    handleMarkAllRead,
    toggleMessageRead,
    hasPrevious,
    hasNext,
    selectPrevious,
    selectNext,
  } = useMessageCenter(open);

  // Esc 键盘快捷键监听
  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (selectedId && !isWideMode) {
          setSelectedId(null);
        } else {
          onOpenChange(false);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, selectedId, isWideMode, onOpenChange, setSelectedId]);

  if (!open) return null;

  const hasActiveFilters =
    filters.tab !== "all" ||
    filters.source !== "all" ||
    filters.severity !== "all" ||
    filters.searchQuery.trim().length > 0;

  return (
    <div className="fixed inset-0 z-[110]">
      {/* 背景遮罩 */}
      <button
        type="button"
        className="absolute inset-0 cursor-default bg-black/30 backdrop-blur-[2px] transition-opacity animate-in fade-in duration-200"
        aria-label={t("close", "Close")}
        onClick={() => onOpenChange(false)}
      />

      {/* 滑出抽屉主体 */}
      <aside
        className={`absolute top-0 right-0 flex h-full w-full flex-col border-l bg-background shadow-2xl transition-all duration-300 ease-out animate-in slide-in-from-right ${
          isWideMode ? "max-w-[880px]" : "max-w-[580px]"
        }`}
      >
        {/* 全局错误警告 */}
        {error ? (
          <div className="mx-5 mt-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        ) : null}

        {/* 顶部标题与 Tab 栏 */}
        <MessageCenterHeader
          unreadCount={unreadCount}
          alertsCount={alertsCount}
          totalCount={messages.length}
          loading={loading}
          isWideMode={isWideMode}
          activeTab={filters.tab}
          onTabChange={(tab) => updateFilters({ tab })}
          onToggleWideMode={toggleWideMode}
          onReload={() => void reloadMessages()}
          onMarkAllRead={() => void handleMarkAllRead()}
          onClose={() => onOpenChange(false)}
        />

        {/* 双栏工作台模式 */}
        {isWideMode ? (
          <div className="grid min-h-0 flex-1 grid-cols-[360px_1fr] divide-x">
            {/* 左侧消息流面板 */}
            <div className="flex min-h-0 flex-col">
              <MessageCenterToolbar
                searchQuery={filters.searchQuery}
                selectedSource={filters.source}
                availableSources={availableSources}
                hasActiveFilters={hasActiveFilters}
                onSearchChange={(searchQuery) =>
                  updateFilters({ searchQuery })
                }
                onSourceChange={(source) => updateFilters({ source })}
                onClearFilters={clearFilters}
              />
              <MessageList
                messages={filteredMessages}
                selectedId={selectedId}
                loading={loading}
                hasActiveFilters={hasActiveFilters}
                onSelect={(id) => setSelectedId(id)}
                onToggleRead={toggleMessageRead}
                onClearFilters={clearFilters}
              />
            </div>

            {/* 右侧常驻详情检视区 */}
            <div className="flex min-h-0 flex-col bg-muted/10">
              {selectedMessage ? (
                <MessageDetailView
                  message={selectedMessage}
                  isWideMode={true}
                  hasPrevious={hasPrevious}
                  hasNext={hasNext}
                  onBack={() => setSelectedId(null)}
                  onSelectPrevious={selectPrevious}
                  onSelectNext={selectNext}
                  onToggleRead={toggleMessageRead}
                  onNavigate={(target) => {
                    onOpenChange(false);
                    onNavigate(target);
                  }}
                  onClose={() => onOpenChange(false)}
                />
              ) : (
                <div className="flex h-full flex-col items-center justify-center p-8 text-center text-muted-foreground">
                  <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-muted/40">
                    <Layers className="h-6 w-6 opacity-40" />
                  </div>
                  <p className="text-xs font-semibold text-foreground">
                    {t(
                      "messageCenterSelectPrompt",
                      "Select a message to view details",
                    )}
                  </p>
                  <p className="mt-1 max-w-xs text-[11px] text-muted-foreground">
                    {t(
                      "messageCenterSelectPromptHint",
                      "View logs, stack traces and quick troubleshooting actions.",
                    )}
                  </p>
                </div>
              )}
            </div>
          </div>
        ) : (
          /* 单栏抽屉平滑滑入模式 */
          <div className="relative min-h-0 flex-1 overflow-hidden">
            {/* 列表视图 */}
            <div
              className={`flex h-full flex-col transition-transform duration-200 ${
                selectedMessage
                  ? "-translate-x-full pointer-events-none opacity-0"
                  : "translate-x-0 opacity-100"
              }`}
            >
              <MessageCenterToolbar
                searchQuery={filters.searchQuery}
                selectedSource={filters.source}
                availableSources={availableSources}
                hasActiveFilters={hasActiveFilters}
                onSearchChange={(searchQuery) =>
                  updateFilters({ searchQuery })
                }
                onSourceChange={(source) => updateFilters({ source })}
                onClearFilters={clearFilters}
              />
              <MessageList
                messages={filteredMessages}
                selectedId={selectedId}
                loading={loading}
                hasActiveFilters={hasActiveFilters}
                onSelect={(id) => setSelectedId(id)}
                onToggleRead={toggleMessageRead}
                onClearFilters={clearFilters}
              />
            </div>

            {/* 详情视图（从右侧滑入覆盖） */}
            {selectedMessage ? (
              <div className="absolute inset-0 z-10 flex flex-col bg-background animate-in slide-in-from-right-full duration-200">
                <MessageDetailView
                  message={selectedMessage}
                  isWideMode={false}
                  hasPrevious={hasPrevious}
                  hasNext={hasNext}
                  onBack={() => setSelectedId(null)}
                  onSelectPrevious={selectPrevious}
                  onSelectNext={selectNext}
                  onToggleRead={toggleMessageRead}
                  onNavigate={(target) => {
                    onOpenChange(false);
                    onNavigate(target);
                  }}
                  onClose={() => onOpenChange(false)}
                />
              </div>
            ) : null}
          </div>
        )}

        {/* 底部小提示条 */}
        <div className="flex h-8 shrink-0 items-center justify-between border-t bg-muted/20 px-5 text-[11px] text-muted-foreground">
          <span>{t("messageCenterEscHint", "Press Esc to close")}</span>
          <span>
            {t("messageCenterFilteredCount", "{{count}} of {{total}}", {
              count: filteredMessages.length,
              total: messages.length,
            })}
          </span>
        </div>
      </aside>
    </div>
  );
}
