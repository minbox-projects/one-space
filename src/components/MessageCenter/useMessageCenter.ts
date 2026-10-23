import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  listMessages,
  markAllMessagesRead,
  markMessageRead,
  MESSAGES_UPDATED_EVENT,
  type MessageRecord,
} from "@/lib/messages";
import {
  loadFocusedSubscriptionConfig,
  saveFocusedSubscriptionConfig,
  isMessageFocused as checkMessageFocused,
  type FocusedMessageSubscriptionConfig,
} from "@/lib/messageSubscriptions";
import type { MessageFilterState } from "./types";

const WIDE_MODE_STORAGE_KEY = "onespace_message_center_wide_mode";

export function useMessageCenter(open: boolean) {
  const [messages, setMessages] = useState<MessageRecord[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [isWideMode, setIsWideMode] = useState<boolean>(() => {
    try {
      return localStorage.getItem(WIDE_MODE_STORAGE_KEY) === "true";
    } catch {
      return false;
    }
  });

  const [filters, setFilters] = useState<MessageFilterState>({
    tab: "all",
    searchQuery: "",
    source: "all",
    severity: "all",
  });

  const [focusedConfig, setFocusedConfig] = useState<FocusedMessageSubscriptionConfig>(() =>
    loadFocusedSubscriptionConfig(),
  );

  // 当抽屉打开时，重新加载特别关注配置以防外部变更
  useEffect(() => {
    if (open) {
      setFocusedConfig(loadFocusedSubscriptionConfig());
    }
  }, [open]);

  const toggleWideMode = useCallback(() => {
    setIsWideMode((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(WIDE_MODE_STORAGE_KEY, String(next));
      } catch {
        // ignore storage errors
      }
      return next;
    });
  }, []);

  const reloadMessages = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await listMessages();
      setMessages(next);
      setSelectedId((current) =>
        current && next.some((message) => message.id === current)
          ? current
          : null,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void reloadMessages();
  }, [open, reloadMessages]);

  useEffect(() => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window))
      return;
    let unlisten: (() => void) | null = null;
    listen(MESSAGES_UPDATED_EVENT, () => {
      if (open) {
        void reloadMessages();
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.error("Failed to subscribe to messages", e));
    return () => {
      unlisten?.();
    };
  }, [open, reloadMessages]);

  // 计算未读总数
  const unreadCount = useMemo(
    () => messages.filter((m) => !m.read_at).length,
    [messages],
  );

  // 告警总数 (warning + error)
  const alertsCount = useMemo(
    () =>
      messages.filter(
        (m) => m.severity === "error" || m.severity === "warning",
      ).length,
    [messages],
  );

  // 特别关注总数
  const focusedCount = useMemo(
    () => messages.filter((m) => checkMessageFocused(m, focusedConfig)).length,
    [messages, focusedConfig],
  );

  // 特别关注未读数
  const focusedUnreadCount = useMemo(
    () =>
      messages.filter(
        (m) => !m.read_at && checkMessageFocused(m, focusedConfig),
      ).length,
    [messages, focusedConfig],
  );

  // 全部可用来源集合
  const availableSources = useMemo(() => {
    const set = new Set<string>();
    messages.forEach((m) => {
      if (m.source) set.add(m.source);
    });
    return Array.from(set).sort();
  }, [messages]);

  // 过滤后的消息列表
  const filteredMessages = useMemo(() => {
    let result = messages;

    // 1. Tab 过滤
    if (filters.tab === "unread") {
      result = result.filter((m) => !m.read_at);
    } else if (filters.tab === "alerts") {
      result = result.filter(
        (m) => m.severity === "error" || m.severity === "warning",
      );
    } else if (filters.tab === "focused") {
      result = result.filter((m) => checkMessageFocused(m, focusedConfig));
    }

    // 2. 来源过滤
    if (filters.source !== "all") {
      result = result.filter((m) => m.source === filters.source);
    }

    // 3. 严重级别过滤
    if (filters.severity !== "all") {
      result = result.filter((m) => m.severity === filters.severity);
    }

    // 4. 搜索关键词
    const query = filters.searchQuery.trim().toLowerCase();
    if (query) {
      result = result.filter((m) => {
        if (m.title.toLowerCase().includes(query)) return true;
        if (m.summary && m.summary.toLowerCase().includes(query)) return true;
        if (m.detail && m.detail.toLowerCase().includes(query)) return true;
        if (m.source.toLowerCase().includes(query)) return true;
        if (m.category && m.category.toLowerCase().includes(query)) return true;
        return false;
      });
    }

    return result;
  }, [messages, filters, focusedConfig]);

  // 当前选中的消息对象
  const selectedMessage = useMemo(
    () => messages.find((m) => m.id === selectedId) || null,
    [messages, selectedId],
  );

  // 选中一条消息后自动标为已读
  useEffect(() => {
    if (!selectedMessage || selectedMessage.read_at) return;
    const readAt = Math.floor(Date.now() / 1000);
    setMessages((prev) =>
      prev.map((message) =>
        message.id === selectedMessage.id
          ? { ...message, read_at: readAt }
          : message,
      ),
    );
    markMessageRead(selectedMessage.id).catch((e) => {
      console.error("Failed to mark message read", e);
    });
  }, [selectedMessage]);

  // 全部标为已读
  const handleMarkAllRead = useCallback(async () => {
    try {
      await markAllMessagesRead();
      const readAt = Math.floor(Date.now() / 1000);
      setMessages((prev) =>
        prev.map((message) => ({
          ...message,
          read_at: message.read_at || readAt,
        })),
      );
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // 切换单条已读/未读状态
  const toggleMessageRead = useCallback(
    async (id: string, e?: React.MouseEvent) => {
      e?.stopPropagation();
      const target = messages.find((m) => m.id === id);
      if (!target) return;

      const isRead = !!target.read_at;
      const nextReadAt = isRead ? null : Math.floor(Date.now() / 1000);

      setMessages((prev) =>
        prev.map((m) => (m.id === id ? { ...m, read_at: nextReadAt } : m)),
      );

      if (!isRead) {
        await markMessageRead(id).catch(console.error);
      }
    },
    [messages],
  );

  // 导航上一条 / 下一条
  const currentIndex = useMemo(() => {
    if (!selectedId) return -1;
    return filteredMessages.findIndex((m) => m.id === selectedId);
  }, [filteredMessages, selectedId]);

  const selectPrevious = useCallback(() => {
    if (currentIndex > 0) {
      setSelectedId(filteredMessages[currentIndex - 1].id);
    }
  }, [currentIndex, filteredMessages]);

  const selectNext = useCallback(() => {
    if (currentIndex >= 0 && currentIndex < filteredMessages.length - 1) {
      setSelectedId(filteredMessages[currentIndex + 1].id);
    }
  }, [currentIndex, filteredMessages]);

  const updateFilters = useCallback(
    (patch: Partial<MessageFilterState>) => {
      setFilters((prev) => ({ ...prev, ...patch }));
    },
    [],
  );

  const clearFilters = useCallback(() => {
    setFilters({
      tab: "all",
      searchQuery: "",
      source: "all",
      severity: "all",
    });
  }, []);

  const toggleMessageStar = useCallback((id: string, e?: React.MouseEvent) => {
    e?.stopPropagation();
    setFocusedConfig((prev) => {
      const currentManual = prev.manualStarredIds || [];
      const isCurrentlyStarred = currentManual.includes(id);
      const nextManual = isCurrentlyStarred
        ? currentManual.filter((item) => item !== id)
        : [...currentManual, id];
      const next = { ...prev, manualStarredIds: nextManual };
      saveFocusedSubscriptionConfig(next);
      return next;
    });
  }, []);

  const updateFocusedConfig = useCallback(
    (nextConfig: FocusedMessageSubscriptionConfig) => {
      setFocusedConfig(nextConfig);
      saveFocusedSubscriptionConfig(nextConfig);
    },
    [],
  );

  return {
    messages,
    filteredMessages,
    selectedId,
    setSelectedId,
    selectedMessage,
    unreadCount,
    alertsCount,
    focusedCount,
    focusedUnreadCount,
    focusedConfig,
    updateFocusedConfig,
    toggleMessageStar,
    isMessageFocused: useCallback(
      (m: Pick<MessageRecord, "id" | "source" | "category">) =>
        checkMessageFocused(m, focusedConfig),
      [focusedConfig],
    ),
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
    currentIndex,
    hasPrevious: currentIndex > 0,
    hasNext: currentIndex >= 0 && currentIndex < filteredMessages.length - 1,
    selectPrevious,
    selectNext,
  };
}
