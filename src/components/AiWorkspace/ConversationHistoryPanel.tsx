import { useState } from "react";
import {
  Archive,
  Clock3,
  MessageSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Pin,
  Plus,
  Search,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AssistantConversationListItem } from "@/lib/aiWorkspace";

interface ConversationHistoryPanelProps {
  conversations: AssistantConversationListItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreateNew: () => void;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
  loading?: boolean;
}

function formatTimestamp(ts?: number | null) {
  if (!ts) return "--";
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function ConversationHistoryPanel({
  conversations,
  selectedId,
  onSelect,
  onCreateNew,
  collapsed = false,
  onToggleCollapsed,
  loading = false,
}: ConversationHistoryPanelProps) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState("");
  const [expandedSections, setExpandedSections] = useState({
    pinned: true,
    recent: true,
    archived: false,
  });
  const normalizedQuery = searchQuery.trim().toLowerCase();

  // 分区: Pinned / Recent / Archived
  const pinnedConversations = conversations.filter(
    (c) => c.pinned && !c.archived,
  );
  const archivedConversations = conversations.filter((c) => c.archived);
  const recentConversations = conversations.filter(
    (c) => !c.pinned && !c.archived,
  );

  // 搜索过滤
  const filterBySearch = (list: AssistantConversationListItem[]) =>
    list.filter((c) => {
      if (!normalizedQuery) return true;
      const title = (c.title || "").toLowerCase();
      const preview = (c.preview || "").toLowerCase();
      return (
        title.includes(normalizedQuery) || preview.includes(normalizedQuery)
      );
    });

  const filteredPinned = filterBySearch(pinnedConversations);
  const filteredRecent = filterBySearch(recentConversations);
  const filteredArchived = filterBySearch(archivedConversations);

  const renderConversationItem = (
    conversation: AssistantConversationListItem,
  ) => (
    <button
      key={conversation.id}
      type="button"
      onClick={() => onSelect(conversation.id)}
      className={`w-full rounded-xl border px-3 py-2.5 text-left transition-colors ${
        selectedId === conversation.id
          ? "border-primary bg-primary/5"
          : "hover:bg-background"
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{conversation.title}</div>
        </div>
        {conversation.pinned ? (
          <Pin className="h-3.5 w-3.5 shrink-0 text-primary" />
        ) : null}
      </div>
      <div className="mt-1.5 flex items-center justify-between text-[11px] text-muted-foreground">
        <span>{formatTimestamp(conversation.updated_at)}</span>
        <span>
          {t("aiWorkspaceMessageCount", "{{count}} msgs", {
            count: conversation.message_count,
          })}
        </span>
      </div>
    </button>
  );

  const renderSection = (
    sectionId: "pinned" | "recent" | "archived",
    title: string,
    icon: React.ReactNode,
    items: AssistantConversationListItem[],
  ) => {
    const expanded = expandedSections[sectionId];
    if (items.length === 0 && normalizedQuery) return null;

    return (
      <div className="space-y-2">
        <button
          type="button"
          onClick={() =>
            setExpandedSections((current) => ({
              ...current,
              [sectionId]: !current[sectionId],
            }))
          }
          className="flex items-center gap-2 text-xs font-medium text-muted-foreground"
        >
          {icon}
          <span>{title}</span>
          <span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px]">
            {items.length}
          </span>
        </button>
        {expanded ? (
          <div className="space-y-2">{items.map(renderConversationItem)}</div>
        ) : null}
      </div>
    );
  };

  if (collapsed) {
    return (
      <div className="flex h-full flex-col items-center justify-between px-2 py-3">
        <button
          type="button"
          onClick={onToggleCollapsed}
          className="inline-flex h-10 w-10 items-center justify-center rounded-xl border hover:bg-muted"
          title={t("expandHistoryPanel", "Expand history")}
        >
          <PanelLeftOpen className="h-4 w-4" />
        </button>

        <div className="flex flex-col items-center gap-3">
          <div className="rounded-2xl border bg-muted/20 p-2 text-muted-foreground">
            <MessageSquare className="h-4 w-4" />
          </div>
          <button
            type="button"
            onClick={onCreateNew}
            disabled={loading}
            className="inline-flex h-10 w-10 items-center justify-center rounded-xl border hover:bg-muted disabled:opacity-50"
            title={t("newLabel", "New")}
          >
            <Plus className="h-4 w-4" />
          </button>
        </div>

        <div className="rounded-full bg-muted px-2 py-1 text-[10px] font-medium text-muted-foreground">
          {conversations.length}
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-col">
      {/* 标题栏 */}
      <div className="border-b px-4 py-3">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <MessageSquare className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm font-semibold">
              {t("historyLabel", "History")}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={onToggleCollapsed}
              className="inline-flex h-8 w-8 items-center justify-center rounded-lg border hover:bg-muted"
              title={t("collapseHistoryPanel", "Collapse history")}
            >
              <PanelLeftClose className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={onCreateNew}
              disabled={loading}
              className="inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("newLabel", "New")}
            </button>
          </div>
        </div>
      </div>

      {/* 搜索框 */}
      <div className="px-3 py-3">
        <div className="flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
          <Search className="h-4 w-4 text-muted-foreground" />
          <input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("aiWorkspaceSearchTopics", "Search conversations...")}
            className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </div>
      </div>

      {/* 会话列表 */}
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        <div className="space-y-4">
          {filteredPinned.length > 0
            ? renderSection(
                "pinned",
                t("pinnedLabel", "Pinned"),
                <Pin className="h-3.5 w-3.5" />,
                filteredPinned,
              )
            : null}

          {filteredRecent.length > 0
            ? renderSection(
                "recent",
                t("recentLabel", "Recent"),
                <Clock3 className="h-3.5 w-3.5" />,
                filteredRecent,
              )
            : null}

          {filteredArchived.length > 0
            ? renderSection(
                "archived",
                t("archivedLabel", "Archived"),
                <Archive className="h-3.5 w-3.5" />,
                filteredArchived,
              )
            : null}

          {filteredPinned.length === 0 &&
          filteredRecent.length === 0 &&
          filteredArchived.length === 0 ? (
            <div className="rounded-xl border bg-muted/10 p-4 text-center text-sm text-muted-foreground">
              {searchQuery
                ? t("aiWorkspaceNoSearchResults", "No results found")
                : t("aiWorkspaceNoConversations", "No conversations yet")}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
