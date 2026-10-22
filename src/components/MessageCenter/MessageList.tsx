import { useTranslation } from "react-i18next";
import { Bell, SearchX } from "lucide-react";
import type { MessageRecord } from "@/lib/messages";
import { MessageItem } from "./MessageItem";

interface MessageListProps {
  messages: MessageRecord[];
  selectedId: string | null;
  loading: boolean;
  hasActiveFilters: boolean;
  onSelect: (id: string) => void;
  onToggleRead: (id: string, e?: React.MouseEvent) => void;
  onClearFilters: () => void;
}

export function MessageList({
  messages,
  selectedId,
  loading,
  hasActiveFilters,
  onSelect,
  onToggleRead,
  onClearFilters,
}: MessageListProps) {
  const { t } = useTranslation();

  if (loading && messages.length === 0) {
    return (
      <div className="flex flex-1 flex-col gap-3 p-4">
        {[1, 2, 3, 4].map((i) => (
          <div
            key={i}
            className="h-20 w-full animate-pulse rounded-xl border bg-muted/40"
          />
        ))}
      </div>
    );
  }

  // 搜索无匹配结果
  if (messages.length === 0 && hasActiveFilters) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center p-8 text-center text-muted-foreground">
        <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-muted/60">
          <SearchX className="h-6 w-6 opacity-60" />
        </div>
        <p className="text-sm font-semibold text-foreground">
          {t("messageCenterNoSearchResults", "No matching messages found")}
        </p>
        <p className="mt-1 max-w-xs text-xs text-muted-foreground">
          {t(
            "messageCenterNoSearchResultsHint",
            "Try adjusting your search query or filters",
          )}
        </p>
        <button
          type="button"
          onClick={onClearFilters}
          className="mt-4 inline-flex items-center rounded-lg border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-muted"
        >
          {t("messageCenterClearFilters", "Clear filters")}
        </button>
      </div>
    );
  }

  // 全局无消息
  if (messages.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center p-8 text-center text-muted-foreground">
        <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-muted/60">
          <Bell className="h-6 w-6 opacity-40" />
        </div>
        <p className="text-sm font-semibold text-foreground">
          {t("messageCenterEmpty", "No messages yet.")}
        </p>
        <p className="mt-1 max-w-xs text-xs text-muted-foreground">
          {t(
            "messageCenterEmptyHint",
            "Important background results and failures will appear here.",
          )}
        </p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-2.5">
      {messages.map((message) => (
        <MessageItem
          key={message.id}
          message={message}
          isSelected={selectedId === message.id}
          onSelect={onSelect}
          onToggleRead={onToggleRead}
        />
      ))}
    </div>
  );
}
