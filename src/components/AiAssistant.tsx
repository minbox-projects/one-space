import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  Archive,
  Bot,
  CheckCircle2,
  Globe,
  Loader2,
  MessageSquare,
  PanelRightClose,
  PanelRightOpen,
  Pin,
  PinOff,
  Plus,
  Search,
  Send,
  Sparkles,
  Trash2,
  XCircle,
} from "lucide-react";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import {
  assistantConversationCreate,
  assistantConversationDelete,
  assistantConversationGet,
  assistantConversationResetContext,
  assistantConversationsList,
  assistantScheduleResolveDraft,
  assistantConversationUpdate,
  assistantMessageSend,
  assistantSettingsGet,
  type AiAssistantModelProfile,
  type AiAssistantSettings,
  type AssistantConversation,
  type AssistantConversationListItem,
  type AssistantMessage,
  type AssistantScheduleDraft,
  type AssistantStreamEvent,
} from "@/lib/aiAssistant";

const PENDING_CONVERSATION_KEY = "onespace:pending-assistant-conversation";

function formatTimestamp(ts: number) {
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function getProfileLabel(
  profileId: string | null | undefined,
  profiles: AiAssistantModelProfile[],
) {
  if (!profileId) return "chat-default";
  return (
    profiles.find((profile) => profile.id === profileId)?.name || profileId
  );
}

function ScheduleDraftCard({
  draft,
  onResolve,
  loading,
}: {
  draft: AssistantScheduleDraft;
  onResolve: (approved: boolean) => void;
  loading: boolean;
}) {
  const schedule = draft.schedule;
  return (
    <div className="mt-4 rounded-xl border bg-muted/10 p-4">
      <div className="mb-3 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
        Schedule Draft
      </div>
      <div className="space-y-2 text-sm">
        <div className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2">
          <span className="text-muted-foreground">Action</span>
          <span className="font-medium">{draft.title}</span>
        </div>
        <div className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2">
          <span className="text-muted-foreground">Name</span>
          <span className="font-medium">
            {draft.target_schedule_name || schedule?.name || "--"}
          </span>
        </div>
        <div className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2">
          <span className="text-muted-foreground">Agent</span>
          <span className="font-medium">{draft.agent_name || "--"}</span>
        </div>
        <div className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2">
          <span className="text-muted-foreground">Trigger</span>
          <span className="font-medium">{draft.trigger_label || "--"}</span>
        </div>
        <div className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2">
          <span className="text-muted-foreground">Web Search</span>
          <span className="font-medium">
            {schedule?.web_search_enabled ? "ON" : "OFF"}
          </span>
        </div>
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => onResolve(true)}
          disabled={loading}
          className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        >
          {loading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <CheckCircle2 className="h-4 w-4" />
          )}
          Confirm
        </button>
        <button
          type="button"
          onClick={() => onResolve(false)}
          disabled={loading}
          className="inline-flex items-center gap-2 rounded-lg border px-4 py-2 text-sm hover:bg-muted disabled:opacity-50"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function MessageBlock({
  message,
  onResolveDraft,
  resolvingDraftId,
}: {
  message: AssistantMessage;
  onResolveDraft?: (message: AssistantMessage, approved: boolean) => void;
  resolvingDraftId?: string | null;
}) {
  if (message.role === "context_reset") {
    return (
      <div className="flex items-center gap-3 py-3">
        <div className="h-px flex-1 border-t border-dashed border-border" />
        <span className="rounded-full border bg-muted/40 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
          {message.content}
        </span>
        <div className="h-px flex-1 border-t border-dashed border-border" />
      </div>
    );
  }

  const isAssistant = message.role === "assistant";
  return (
    <div className="rounded-2xl border bg-card/80 px-4 py-4 shadow-sm">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <div
            className={`rounded-full p-2 ${isAssistant ? "bg-primary/10 text-primary" : "bg-muted text-foreground"}`}
          >
            {isAssistant ? (
              <Bot className="h-4 w-4" />
            ) : (
              <MessageSquare className="h-4 w-4" />
            )}
          </div>
          <div>
            <div className="text-sm font-medium">
              {isAssistant ? "Assistant" : "You"}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {formatTimestamp(message.created_at)}
            </div>
          </div>
        </div>
        {message.status === "streaming" && (
          <div className="inline-flex items-center gap-2 rounded-full border bg-muted/40 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Streaming
          </div>
        )}
        {message.status === "failed" && (
          <div className="inline-flex items-center gap-2 rounded-full border border-destructive/30 bg-destructive/5 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-destructive">
            <XCircle className="h-3.5 w-3.5" />
            Failed
          </div>
        )}
      </div>

      {message.reasoning && (
        <div className="mb-4 rounded-xl border bg-muted/20 p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Thinking
          </div>
          <pre className="whitespace-pre-wrap break-words text-xs leading-6 text-muted-foreground">
            {message.reasoning}
          </pre>
        </div>
      )}

      <div className="prose prose-sm max-w-none dark:prose-invert">
        {isAssistant ? (
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {message.content || " "}
          </ReactMarkdown>
        ) : (
          <div className="whitespace-pre-wrap break-words text-sm leading-6">
            {message.content}
          </div>
        )}
      </div>

      {message.sources.length > 0 && (
        <div className="mt-4 rounded-xl border bg-muted/10 p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Sources
          </div>
          <div className="space-y-2">
            {message.sources.map((source, index) => (
              <a
                key={`${source.url}-${index}`}
                href={source.url}
                target="_blank"
                rel="noreferrer"
                className="block rounded-lg border bg-background px-3 py-2 text-sm transition-colors hover:bg-muted/30"
              >
                <div className="font-medium">{source.title || source.url}</div>
                <div className="mt-1 text-xs text-muted-foreground line-clamp-2">
                  {source.snippet}
                </div>
              </a>
            ))}
          </div>
        </div>
      )}

      {message.tool_calls.length > 0 && (
        <div className="mt-4 rounded-xl border bg-muted/10 p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Tool Calls
          </div>
          <div className="space-y-2">
            {message.tool_calls.map((tool, index) => (
              <div
                key={`${tool.name}-${index}`}
                className="flex items-center justify-between rounded-lg border bg-background px-3 py-2 text-sm"
              >
                <div>
                  <div className="font-medium">{tool.name}</div>
                  {tool.summary ? (
                    <div className="mt-1 text-xs text-muted-foreground">
                      {tool.summary}
                    </div>
                  ) : null}
                </div>
                <span className="rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                  {tool.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {message.schedule_draft && onResolveDraft ? (
        <ScheduleDraftCard
          draft={message.schedule_draft}
          loading={resolvingDraftId === message.id}
          onResolve={(approved) => onResolveDraft(message, approved)}
        />
      ) : null}
    </div>
  );
}

export function AiAssistant({ isVisible = false }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [settings, setSettings] = useState<AiAssistantSettings | null>(null);
  const [conversations, setConversations] = useState<
    AssistantConversationListItem[]
  >([]);
  const [selectedConversationId, setSelectedConversationId] = useState<
    string | null
  >(null);
  const [selectedConversation, setSelectedConversation] =
    useState<AssistantConversation | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const deferredSearchQuery = useDeferredValue(searchQuery);
  const [draftMessage, setDraftMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [loading, setLoading] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [showRightPanel, setShowRightPanel] = useState(true);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [resolvingDraftId, setResolvingDraftId] = useState<string | null>(null);

  const profiles = settings?.profiles || [];
  const enabledProviders =
    settings?.providers.filter((provider) => provider.enabled) || [];
  const hasRuntimeConfig = enabledProviders.length > 0 && profiles.length > 0;

  const loadSettings = async () => {
    const next = await assistantSettingsGet();
    setSettings(next);
  };

  const loadConversations = async () => {
    const items = await assistantConversationsList();
    setConversations(items);
    setSelectedConversationId((current) => {
      if (current && items.some((item) => item.id === current)) {
        return current;
      }
      const pendingId = window.localStorage.getItem(PENDING_CONVERSATION_KEY);
      if (pendingId && items.some((item) => item.id === pendingId)) {
        window.localStorage.removeItem(PENDING_CONVERSATION_KEY);
        return pendingId;
      }
      return items[0]?.id || null;
    });
  };

  const loadConversationDetail = async (conversationId: string) => {
    setDetailLoading(true);
    try {
      const conversation = await assistantConversationGet(conversationId);
      setSelectedConversation(conversation);
      setRuntimeError(null);
    } catch (error) {
      console.error("Failed to load assistant conversation", error);
    } finally {
      setDetailLoading(false);
    }
  };

  useEffect(() => {
    if (!isVisible) return;
    setLoading(true);
    Promise.all([loadSettings(), loadConversations()])
      .catch((error) => {
        console.error("Failed to bootstrap AI assistant", error);
      })
      .finally(() => setLoading(false));
  }, [isVisible]);

  useEffect(() => {
    if (!isVisible || !selectedConversationId) return;
    void loadConversationDetail(selectedConversationId);
  }, [isVisible, selectedConversationId]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isVisible) return;
      const isModifier =
        (event.metaKey || event.ctrlKey) &&
        event.shiftKey &&
        event.key.toLowerCase() === "k";
      if (!isModifier || !selectedConversation) return;
      event.preventDefault();
      void (async () => {
        const confirmed = await confirmDialog(
          t(
            "assistantClearContextConfirm",
            "不删除已有历史消息，从下一条消息开始不再引用此前上下文。是否继续？",
          ),
          {
            title: t("assistantClearContext", "清空上下文"),
            okLabel: t("assistantClearContext", "清空上下文"),
          },
        );
        if (!confirmed) return;
        const updated = await assistantConversationResetContext(
          selectedConversation.id,
        );
        setSelectedConversation(updated);
        await loadConversations();
      })();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [confirmDialog, isVisible, selectedConversation, t]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<AssistantStreamEvent>("assistant-stream", (event) => {
      if (disposed) return;
      const payload = event.payload;
      if (!payload) return;

      setSelectedConversation((current) => {
        if (!current || current.id !== payload.conversation_id) {
          return current;
        }
        const nextMessages = current.messages.map((message) => {
          if (message.id !== payload.message_id) return message;
          if (payload.kind === "message.delta") {
            return {
              ...message,
              content: `${message.content}${payload.text || ""}`,
              status: "streaming",
            };
          }
          if (payload.kind === "reasoning.delta") {
            return {
              ...message,
              reasoning: `${message.reasoning || ""}${payload.text || ""}`,
            };
          }
          if (payload.kind === "sources") {
            return {
              ...message,
              sources: payload.sources || [],
              tool_calls: payload.tool
                ? [...message.tool_calls, payload.tool]
                : message.tool_calls,
            };
          }
          if (payload.kind === "tool.started" && payload.tool) {
            return {
              ...message,
              tool_calls: [...message.tool_calls, payload.tool],
            };
          }
          if (payload.kind === "tool.finished" && payload.tool) {
            return {
              ...message,
              tool_calls: [
                ...message.tool_calls.filter(
                  (tool) => tool.name !== payload.tool?.name,
                ),
                payload.tool,
              ],
            };
          }
          if (payload.kind === "message.completed") {
            return {
              ...message,
              status: "done",
              sources: payload.sources || message.sources,
            };
          }
          if (payload.kind === "message.failed") {
            return {
              ...message,
              status: "failed",
            };
          }
          return message;
        });
        return {
          ...current,
          updated_at: Math.floor(Date.now() / 1000),
          messages: nextMessages,
        };
      });

      if (
        payload.kind === "message.completed" ||
        payload.kind === "message.failed"
      ) {
        void loadConversations();
        setSending(false);
        setRuntimeError(payload.error || null);
      }
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, []);

  const filteredConversations = useMemo(() => {
    const normalized = deferredSearchQuery.trim().toLowerCase();
    if (!normalized) return conversations;
    return conversations.filter((conversation) => {
      const haystack =
        `${conversation.title} ${conversation.preview} ${conversation.search_text}`.toLowerCase();
      return haystack.includes(normalized);
    });
  }, [conversations, deferredSearchQuery]);

  const pinnedConversations = filteredConversations.filter(
    (item) => item.pinned && !item.archived,
  );
  const recentConversations = filteredConversations.filter(
    (item) => !item.pinned && !item.archived,
  );
  const archivedConversations = filteredConversations.filter(
    (item) => item.archived,
  );

  const handleCreateConversation = async () => {
    const created = await assistantConversationCreate();
    await loadConversations();
    setSelectedConversationId(created.id);
  };

  const handleSend = async () => {
    const content = draftMessage.trim();
    if (!content || sending) return;
    setSending(true);
    setRuntimeError(null);
    try {
      let targetId = selectedConversation?.id || selectedConversationId;
      if (!targetId) {
        const created = await assistantConversationCreate();
        targetId = created.id;
        setSelectedConversationId(created.id);
      }
      await assistantMessageSend({
        conversation_id: targetId,
        content,
        model_profile_id:
          selectedConversation?.model_profile_id ||
          settings?.default_chat_profile_id ||
          undefined,
        web_search_enabled: selectedConversation?.web_search_enabled ?? false,
      });
      setDraftMessage("");
      await loadConversationDetail(targetId);
      await loadConversations();
    } catch (error: any) {
      console.error("Failed to send assistant message", error);
      setRuntimeError(error?.toString?.() || String(error));
      setSending(false);
    }
  };

  const handleTogglePinned = async () => {
    if (!selectedConversation) return;
    const updated = await assistantConversationUpdate({
      conversation_id: selectedConversation.id,
      pinned: !selectedConversation.pinned,
    });
    setSelectedConversation(updated);
    await loadConversations();
  };

  const handleToggleArchived = async () => {
    if (!selectedConversation) return;
    const updated = await assistantConversationUpdate({
      conversation_id: selectedConversation.id,
      archived: !selectedConversation.archived,
    });
    setSelectedConversation(updated);
    await loadConversations();
  };

  const handleDeleteConversation = async () => {
    if (!selectedConversation) return;
    const confirmed = await confirmDialog(
      t(
        "assistantDeleteConversationConfirm",
        "删除当前对话后将无法恢复，是否继续？",
      ),
      {
        title: t("assistantDeleteConversation", "删除对话"),
        okLabel: t("delete", "Delete"),
      },
    );
    if (!confirmed) return;
    await assistantConversationDelete(selectedConversation.id);
    setSelectedConversation(null);
    setSelectedConversationId(null);
    await loadConversations();
  };

  const handleClearContext = async () => {
    if (!selectedConversation) return;
    const confirmed = await confirmDialog(
      t(
        "assistantClearContextConfirm",
        "不删除已有历史消息，从下一条消息开始不再引用此前上下文。是否继续？",
      ),
      {
        title: t("assistantClearContext", "清空上下文"),
        okLabel: t("assistantClearContext", "清空上下文"),
      },
    );
    if (!confirmed) return;
    const updated = await assistantConversationResetContext(
      selectedConversation.id,
    );
    setSelectedConversation(updated);
    await loadConversations();
  };

  const handleToggleWebSearch = async () => {
    const targetConversation =
      selectedConversation ||
      (await assistantConversationCreate(
        t("assistantDefaultConversationTitle", "新会话"),
      ));
    if (!selectedConversation) {
      setSelectedConversationId(targetConversation.id);
    }
    const updated = await assistantConversationUpdate({
      conversation_id: targetConversation.id,
      web_search_enabled: !targetConversation.web_search_enabled,
    });
    setSelectedConversation(updated);
    await loadConversations();
  };

  const handleResolveDraft = async (
    message: AssistantMessage,
    approved: boolean,
  ) => {
    if (!selectedConversation) return;
    setResolvingDraftId(message.id);
    try {
      const updated = await assistantScheduleResolveDraft({
        conversation_id: selectedConversation.id,
        message_id: message.id,
        approved,
      });
      setSelectedConversation(updated);
      await loadConversations();
    } catch (error: any) {
      setRuntimeError(error?.toString?.() || String(error));
    } finally {
      setResolvingDraftId(null);
    }
  };

  const openAssistantModelSettings = () => {
    const appWindow = window as Window & {
      setActiveTab?: (tab: string) => void;
    };
    appWindow.setActiveTab?.("ai-model-center");
  };

  const sections = [
    { title: t("pinned", "Pinned"), items: pinnedConversations },
    { title: t("recent", "Recent"), items: recentConversations },
    { title: t("archived", "Archived"), items: archivedConversations },
  ];

  return (
    <div className="h-full">
      <div className="grid h-full gap-6 xl:grid-cols-[320px,minmax(0,1fr)]">
        <div className="flex min-h-0 flex-col rounded-2xl border bg-card">
          <div className="border-b px-4 py-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-base font-semibold">
                  {t("aiAssistant", "AI 助手")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t(
                    "assistantHistoryDesc",
                    "Search, revisit, and continue lightweight assistant threads.",
                  )}
                </div>
              </div>
              <button
                type="button"
                onClick={() => void handleCreateConversation()}
                className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
              >
                <Plus className="h-4 w-4" />
                {t("newSession", "New")}
              </button>
            </div>
            <div className="mt-4 flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
              <Search className="h-4 w-4 text-muted-foreground" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("assistantSearchHistory", "搜索历史对话...")}
                className="w-full bg-transparent text-sm outline-none"
              />
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-4">
            {loading ? (
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                {t("loading", "Loading...")}
              </div>
            ) : (
              <div className="space-y-5">
                {sections.map((section) =>
                  section.items.length > 0 ? (
                    <div key={section.title}>
                      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        {section.title}
                      </div>
                      <div className="space-y-2">
                        {section.items.map((conversation) => (
                          <button
                            key={conversation.id}
                            type="button"
                            onClick={() =>
                              setSelectedConversationId(conversation.id)
                            }
                            className={`w-full rounded-xl border px-3 py-3 text-left transition-colors ${
                              selectedConversationId === conversation.id
                                ? "border-primary bg-primary/5"
                                : "hover:bg-muted/30"
                            }`}
                          >
                            <div className="flex items-start justify-between gap-2">
                              <div className="min-w-0">
                                <div className="truncate text-sm font-medium">
                                  {conversation.title}
                                </div>
                                <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                                  {conversation.preview ||
                                    t("assistantNoMessagesYet", "暂无消息")}
                                </div>
                              </div>
                              {conversation.pinned ? (
                                <Pin className="h-3.5 w-3.5 shrink-0 text-primary" />
                              ) : null}
                            </div>
                            <div className="mt-2 flex items-center justify-between text-[11px] text-muted-foreground">
                              <span>
                                {getProfileLabel(
                                  conversation.model_profile_id,
                                  profiles,
                                )}
                              </span>
                              <span>
                                {formatTimestamp(conversation.updated_at)}
                              </span>
                            </div>
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : null,
                )}
              </div>
            )}
          </div>
        </div>

        <div className="grid min-h-0 gap-6 xl:grid-cols-[minmax(0,1fr),320px]">
          <div className="flex min-h-0 flex-col rounded-2xl border bg-card">
            <div className="border-b px-6 py-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-base font-semibold">
                    {selectedConversation?.title ||
                      t("assistantEmptyTitle", "选择或创建一个会话")}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {selectedConversation
                      ? t(
                          "assistantShortcutHint",
                          "历史可搜索；使用 Cmd/Ctrl + Shift + K 可快速清空当前上下文。",
                        )
                      : t(
                          "assistantEmptyHint",
                          "内嵌 AI Runtime 使用设置中的 AI助手模型配置。",
                        )}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => setShowRightPanel((value) => !value)}
                    className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
                  >
                    {showRightPanel ? (
                      <PanelRightClose className="h-4 w-4" />
                    ) : (
                      <PanelRightOpen className="h-4 w-4" />
                    )}
                    {showRightPanel
                      ? t("collapse", "Collapse")
                      : t("expand", "Expand")}
                  </button>
                </div>
              </div>
              {!hasRuntimeConfig && (
                <div className="mt-4 rounded-xl border border-destructive/20 bg-destructive/5 px-4 py-3 text-sm text-destructive">
                  <div className="font-medium">
                    {t("assistantSetupRequired", "AI 助手模型尚未完成配置")}
                  </div>
                  <div className="mt-1 text-xs text-destructive/80">
                    {t(
                      "assistantSetupRequiredDesc",
                      "请先到 设置 -> AI助手模型 中配置可用的 Provider、模型 Profile 和联网搜索。",
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={openAssistantModelSettings}
                    className="mt-3 inline-flex items-center gap-2 rounded-md border border-destructive/30 px-3 py-2 text-sm hover:bg-destructive/10"
                  >
                    <Sparkles className="h-4 w-4" />
                    {t("openSettings", "Open Settings")}
                  </button>
                </div>
              )}
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
              {selectedConversation ? (
                detailLoading ? (
                  <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    {t("loading", "Loading...")}
                  </div>
                ) : (
                  <div className="space-y-4">
                    {selectedConversation.messages.length === 0 ? (
                      <div className="rounded-2xl border border-dashed bg-muted/10 px-6 py-10 text-center text-sm text-muted-foreground">
                        {t(
                          "assistantStartPrompt",
                          "从一个明确问题开始，或者开启联网搜索后再提问。",
                        )}
                      </div>
                    ) : (
                      selectedConversation.messages.map((message) => (
                        <MessageBlock
                          key={message.id}
                          message={message}
                          onResolveDraft={handleResolveDraft}
                          resolvingDraftId={resolvingDraftId}
                        />
                      ))
                    )}
                  </div>
                )
              ) : (
                <div className="flex h-full items-center justify-center">
                  <div className="max-w-md rounded-2xl border border-dashed bg-muted/10 px-6 py-10 text-center">
                    <div className="mx-auto mb-4 inline-flex rounded-full bg-primary/10 p-3 text-primary">
                      <MessageSquare className="h-6 w-6" />
                    </div>
                    <div className="text-base font-semibold">
                      {t("assistantWelcome", "准备好开始一个 AI 助手会话")}
                    </div>
                    <p className="mt-2 text-sm text-muted-foreground">
                      {t(
                        "assistantWelcomeDesc",
                        "左侧可以查看和搜索历史对话；右侧维持轻量上下文和模型配置。",
                      )}
                    </p>
                    <button
                      type="button"
                      onClick={() => void handleCreateConversation()}
                      className="mt-4 inline-flex items-center gap-2 rounded-lg border px-4 py-2 text-sm hover:bg-muted"
                    >
                      <Plus className="h-4 w-4" />
                      {t("newSession", "New")}
                    </button>
                  </div>
                </div>
              )}
            </div>

            <div className="border-t px-6 py-4">
              {runtimeError ? (
                <div className="mb-3 rounded-xl border border-destructive/20 bg-destructive/5 px-4 py-3 text-sm text-destructive">
                  {runtimeError}
                </div>
              ) : null}
              <div className="mb-3 flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  onClick={() => void handleToggleWebSearch()}
                  disabled={!selectedConversation}
                  className={`inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm transition-colors ${
                    selectedConversation?.web_search_enabled
                      ? "border-primary bg-primary/5 text-primary"
                      : "hover:bg-muted"
                  } disabled:opacity-50`}
                >
                  <Globe className="h-4 w-4" />
                  {t("assistantWebSearchToggle", "联网")}
                </button>
                <button
                  type="button"
                  onClick={() => void handleClearContext()}
                  disabled={!selectedConversation}
                  className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                >
                  <XCircle className="h-4 w-4" />
                  {t("assistantClearContext", "清空上下文")}
                </button>
                <button
                  type="button"
                  onClick={() => void handleTogglePinned()}
                  disabled={!selectedConversation}
                  className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                >
                  {selectedConversation?.pinned ? (
                    <PinOff className="h-4 w-4" />
                  ) : (
                    <Pin className="h-4 w-4" />
                  )}
                  {selectedConversation?.pinned
                    ? t("unpin", "Unpin")
                    : t("pin", "Pin")}
                </button>
                <button
                  type="button"
                  onClick={() => void handleToggleArchived()}
                  disabled={!selectedConversation}
                  className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                >
                  <Archive className="h-4 w-4" />
                  {selectedConversation?.archived
                    ? t("restore", "Restore")
                    : t("archive", "Archive")}
                </button>
                <button
                  type="button"
                  onClick={() => void handleDeleteConversation()}
                  disabled={!selectedConversation}
                  className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                >
                  <Trash2 className="h-4 w-4" />
                  {t("delete", "Delete")}
                </button>
              </div>

              <div className="rounded-2xl border bg-background p-3">
                <textarea
                  value={draftMessage}
                  onChange={(e) => setDraftMessage(e.target.value)}
                  placeholder={t(
                    "assistantInputPlaceholder",
                    "输入问题，或要求 AI 助手联网检索并总结...",
                  )}
                  className="min-h-[108px] w-full resize-none bg-transparent text-sm leading-6 outline-none"
                />
                <div className="mt-3 flex items-center justify-between gap-3">
                  <div className="text-xs text-muted-foreground">
                    {selectedConversation
                      ? `${t("assistantCurrentProfile", "当前模型")}: ${getProfileLabel(selectedConversation.model_profile_id, profiles)}`
                      : t("assistantCurrentProfile", "当前模型")}
                  </div>
                  <button
                    type="button"
                    onClick={() => void handleSend()}
                    disabled={
                      !draftMessage.trim() || sending || !hasRuntimeConfig
                    }
                    className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                  >
                    {sending ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Send className="h-4 w-4" />
                    )}
                    {t("send", "Send")}
                  </button>
                </div>
              </div>
            </div>
          </div>

          {showRightPanel ? (
            <div className="min-h-0 rounded-2xl border bg-card">
              <div className="border-b px-5 py-4">
                <div className="text-sm font-semibold">
                  {t("assistantRunPanel", "运行侧栏")}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {t(
                    "assistantRunPanelDesc",
                    "Keep model, web search, and thinking state visible without cluttering the chat area.",
                  )}
                </div>
              </div>
              <div className="space-y-5 p-5">
                <div>
                  <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t("assistantCurrentProfile", "当前模型")}
                  </div>
                  <select
                    value={
                      selectedConversation?.model_profile_id ||
                      settings?.default_chat_profile_id ||
                      ""
                    }
                    onChange={async (e) => {
                      if (!selectedConversation) return;
                      const updated = await assistantConversationUpdate({
                        conversation_id: selectedConversation.id,
                        model_profile_id: e.target.value,
                      });
                      setSelectedConversation(updated);
                      await loadConversations();
                    }}
                    className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                  >
                    {profiles.map((profile) => (
                      <option key={profile.id} value={profile.id}>
                        {profile.name} / {profile.model_id}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t("assistantWebSearch", "联网搜索")}
                  </div>
                  <div className="rounded-xl border bg-muted/10 px-4 py-3">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium">
                        {selectedConversation?.web_search_enabled
                          ? "Enabled"
                          : "Disabled"}
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleToggleWebSearch()}
                        disabled={!selectedConversation}
                        className={`rounded-full border px-3 py-1 text-xs ${
                          selectedConversation?.web_search_enabled
                            ? "border-primary text-primary"
                            : "text-muted-foreground"
                        }`}
                      >
                        {selectedConversation?.web_search_enabled
                          ? "ON"
                          : "OFF"}
                      </button>
                    </div>
                    <div className="mt-2 text-xs text-muted-foreground">
                      {settings?.active_search_provider_id
                        ? `${t("providerName", "Provider")}: ${settings.search_providers.find((provider) => provider.id === settings.active_search_provider_id)?.name || settings.active_search_provider_id}`
                        : t(
                            "assistantSearchProviderEmpty",
                            "尚未设置默认搜索提供商",
                          )}
                    </div>
                  </div>
                </div>

                <div>
                  <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t("reasoningSummary", "思考过程")}
                  </div>
                  <div className="rounded-xl border bg-muted/10 px-4 py-3 text-sm text-muted-foreground">
                    {selectedConversation?.messages
                      .filter((message) => message.role === "assistant")
                      .slice(-1)[0]?.reasoning ||
                      t(
                        "assistantReasoningFallback",
                        "如果厂商未提供原生 reasoning，这里会退化为工具轨迹和摘要。",
                      )}
                  </div>
                </div>

                <div>
                  <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t("assistantConversationStats", "会话信息")}
                  </div>
                  <div className="space-y-2 rounded-xl border bg-muted/10 p-4 text-sm">
                    <div className="flex items-center justify-between">
                      <span className="text-muted-foreground">
                        {t("messages", "Messages")}
                      </span>
                      <span>
                        {selectedConversation?.messages.filter(
                          (message) => message.role !== "context_reset",
                        ).length || 0}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-muted-foreground">
                        {t("assistantContextResets", "Context resets")}
                      </span>
                      <span>
                        {selectedConversation?.context_reset_count || 0}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-muted-foreground">
                        {t("updatedAt", "Updated")}
                      </span>
                      <span>
                        {selectedConversation
                          ? formatTimestamp(selectedConversation.updated_at)
                          : "--"}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
