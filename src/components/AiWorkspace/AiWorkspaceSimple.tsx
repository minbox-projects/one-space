import { memo, useEffect, useEffectEvent, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Archive,
  ChevronRight,
  Loader2,
  MessageSquare,
  Pin,
  PinOff,
  Send,
  Trash2,
  XCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "../ConfirmDialogProvider";
import { ConversationHistoryPanel } from "./ConversationHistoryPanel";
import { ChatTopBar } from "./ChatTopBar";
import { CapabilityBadges } from "./CapabilityBadges";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  aiWorkspaceBootstrap,
  showQuickAssistantWindow,
  type AiWorkspaceBootstrap,
  type AssistantConversation,
  type AssistantConversationListItem,
  type AssistantMessage,
  type AssistantPreset,
  type AssistantStreamEvent,
  workspaceConversationCreate,
  workspaceConversationDelete,
  workspaceConversationGet,
  workspaceConversationResetContext,
  workspaceConversationSend,
  workspaceConversationUpdate,
  workspaceConversationsList,
} from "@/lib/aiWorkspace";

function formatTimestamp(ts?: number | null) {
  if (!ts) return "--";
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatRuntimeError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const MessageCard = memo(function MessageCard({ message }: { message: AssistantMessage }) {
  const { t } = useTranslation();
  const [showReasoning, setShowReasoning] = useState(false);

  if (message.role === "context_reset") {
    return (
      <div className="flex items-center gap-3 py-2">
        <div className="h-px flex-1 border-t border-dashed border-border" />
        <span className="rounded-full border bg-muted/40 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
          {message.content}
        </span>
        <div className="h-px flex-1 border-t border-dashed border-border" />
      </div>
    );
  }

  const isAssistant = message.role === "assistant";

  // 用户消息：居右显示
  if (!isAssistant) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] rounded-2xl rounded-br-md bg-primary px-4 py-3 text-primary-foreground">
          <div className="whitespace-pre-wrap break-words text-sm leading-6">
            {message.content}
          </div>
        </div>
      </div>
    );
  }

  // 助手消息：全宽显示
  const hasReasoning = message.reasoning && message.reasoning.trim().length > 0;
  const isStreaming = message.status === "streaming";
  const isThinking = isStreaming && !message.content && hasReasoning;

  return (
    <div className="rounded-2xl border bg-card/90 px-4 py-4 shadow-sm will-change-transform">
      {/* 状态标签 */}
      {isStreaming ? (
        <div className="mb-3 inline-flex items-center gap-2 rounded-full border bg-primary/5 px-3 py-1 text-[11px] text-primary">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {isThinking
            ? t("thinkingStatusLabel", "Thinking...")
            : t("generatingLabel", "Generating...")}
        </div>
      ) : null}
      {message.status === "failed" ? (
        <div className="mb-3 inline-flex items-center gap-2 rounded-full border border-destructive/30 bg-destructive/5 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-destructive">
          {t("failedLabel", "Failed")}
        </div>
      ) : null}

      {/* 思考过程 */}
      {hasReasoning ? (
        <div className="mb-4">
          <button
            type="button"
            onClick={() => setShowReasoning(!showReasoning)}
            className="mb-2 inline-flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground hover:text-foreground"
          >
            <ChevronRight className={`h-3 w-3 transition-transform ${showReasoning ? "rotate-90" : ""}`} />
            {t("reasoningLabel", "Reasoning")}
          </button>
          {showReasoning ? (
            <div className="rounded-xl border border-dashed bg-muted/30 px-3 py-3">
              <div className="whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
                {message.reasoning}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {/* 正式内容 */}
      <div className="prose prose-sm max-w-none dark:prose-invert">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>
          {message.content || " "}
        </ReactMarkdown>
      </div>

      {message.sources.length > 0 ? (
        <div className="mt-4 rounded-xl border border-dashed bg-muted/20 px-3 py-3">
          <div className="mb-2 text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
            {t("sourcesLabel", "Sources")}
          </div>
          <div className="space-y-2">
            {message.sources.map((source, index) => (
              <a
                key={`${source.url}-${index}`}
                href={source.url}
                target="_blank"
                rel="noreferrer"
                className="block rounded-lg border bg-background px-3 py-2 text-sm transition-colors hover:bg-muted/40"
              >
                <div className="font-medium">{source.title || source.url}</div>
                {source.snippet ? (
                  <div className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
                    {source.snippet}
                  </div>
                ) : null}
              </a>
            ))}
          </div>
        </div>
      ) : null}

      {message.tool_calls.length > 0 ? (
        <div className="mt-4 rounded-xl border border-dashed bg-muted/20 px-3 py-3">
          <div className="mb-2 text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
            {t("toolCallsLabel", "Tool Calls")}
          </div>
          <div className="space-y-2">
            {message.tool_calls.map((tool, index) => (
              <div
                key={`${tool.name}-${index}`}
                className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2"
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{tool.name}</div>
                  {tool.summary ? (
                    <div className="mt-1 text-xs leading-5 text-muted-foreground">
                      {tool.summary}
                    </div>
                  ) : null}
                </div>
                <span className="shrink-0 rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                  {tool.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
});

export function AiWorkspaceSimple() {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();

  // Refs
  const messagesContainerRef = useRef<HTMLDivElement | null>(null);

  // State
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [shouldAutoScroll, setShouldAutoScroll] = useState(true);

  const [assistants, setAssistants] = useState<AssistantPreset[]>([]);
  const [conversations, setConversations] = useState<AssistantConversationListItem[]>([]);
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const [selectedConversation, setSelectedConversation] = useState<AssistantConversation | null>(null);
  const [conversationAssistantId, setConversationAssistantId] = useState<string | null>(null);
  const [draftMessage, setDraftMessage] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);
  const activeAssistant =
    assistants.find((assistant) => assistant.id === conversationAssistantId) || null;

  // 打开模型中心设置
  const openModelCenter = () => {
    const appWindow = window as Window & {
      setActiveTab?: (tab: string) => void;
    };
    appWindow.setActiveTab?.("ai-model-center");
  };

  // 打开助手库管理
  const openAssistantLibrary = () => {
    const appWindow = window as Window & {
      setActiveTab?: (tab: string) => void;
    };
    appWindow.setActiveTab?.("ai-assistants-library");
  };

  // 滚动到底部
  const scrollToBottom = () => {
    if (messagesContainerRef.current) {
      messagesContainerRef.current.scrollTop = messagesContainerRef.current.scrollHeight;
    }
  };

  // 监听滚动事件，检测用户是否手动滚动
  const handleScroll = () => {
    const container = messagesContainerRef.current;
    if (!container) return;

    const { scrollTop, scrollHeight, clientHeight } = container;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;

    // 如果用户滚动到底部附近，恢复自动滚动
    setShouldAutoScroll(isAtBottom);
  };

  // Load bootstrap
  const loadBootstrap = async () => {
    setLoading(true);
    try {
      const data: AiWorkspaceBootstrap = await aiWorkspaceBootstrap();
      setAssistants(data.assistants);
      setConversations(data.conversations);
      setConversationAssistantId(data.assistants[0]?.id || null);
      setSelectedConversationId(data.conversations[0]?.id || null);
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    } finally {
      setLoading(false);
    }
  };

  const loadConversationDetail = async (conversationId: string) => {
    setDetailLoading(true);
    try {
      const detail = await workspaceConversationGet(conversationId);
      setSelectedConversation(detail);
      setConversationAssistantId(detail.assistant_id || conversationAssistantId);
      setRuntimeError(null);
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    } finally {
      setDetailLoading(false);
    }
  };

  const refreshConversations = async () => {
    const items = await workspaceConversationsList();
    setConversations(items);
  };

  const loadConversationDetailEffect = useEffectEvent((conversationId: string) => {
    void loadConversationDetail(conversationId);
  });

  useEffect(() => {
    void loadBootstrap();
  }, []);

  useEffect(() => {
    if (selectedConversationId) {
      loadConversationDetailEffect(selectedConversationId);
    }
  }, [selectedConversationId]);

  // 会话加载完成后滚动到底部
  useEffect(() => {
    if (selectedConversation && !detailLoading && shouldAutoScroll) {
      scrollToBottom();
    }
  }, [selectedConversation, detailLoading, shouldAutoScroll]);

  // 流式输出事件监听
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<AssistantStreamEvent>("assistant-stream", (event) => {
      if (disposed || !event.payload) return;
      const payload = event.payload;

      setSelectedConversation((current) => {
        if (!current || current.id !== payload.conversation_id) {
          return current;
        }
        const messages = current.messages.map((message) => {
          // 匹配真实ID或正在streaming的临时助手消息
          const isTargetMessage =
            message.id === payload.message_id ||
            (message.role === "assistant" && message.status === "streaming" && message.id.startsWith("temp-assistant-"));

          if (!isTargetMessage) return message;

          if (payload.kind === "message.delta") {
            return {
              ...message,
              id: payload.message_id, // 更新为真实ID
              content: `${message.content}${payload.text || ""}`,
              status: "streaming" as const,
            };
          }
          if (payload.kind === "reasoning.delta") {
            return {
              ...message,
              id: payload.message_id,
              reasoning: `${message.reasoning || ""}${payload.text || ""}`,
            };
          }
          if (payload.kind === "sources") {
            return {
              ...message,
              id: payload.message_id,
              sources: payload.sources || message.sources
            };
          }
          if (payload.kind === "tool.started" && payload.tool) {
            return {
              ...message,
              id: payload.message_id,
              tool_calls: [...message.tool_calls, payload.tool],
            };
          }
          if (payload.kind === "tool.finished" && payload.tool) {
            return {
              ...message,
              id: payload.message_id,
              tool_calls: [
                ...message.tool_calls.filter(
                  (item) => item.name !== payload.tool?.name,
                ),
                payload.tool,
              ],
            };
          }
          if (payload.kind === "message.completed") {
            return {
              ...message,
              id: payload.message_id,
              status: "done" as const,
              sources: payload.sources || message.sources,
            };
          }
          if (payload.kind === "message.failed") {
            return {
              ...message,
              id: payload.message_id,
              status: "failed" as const
            };
          }
          return message;
        });
        return {
          ...current,
          updated_at: Math.floor(Date.now() / 1000),
          messages,
        };
      });

      // 流式输出时自动滚动到底部
      if (shouldAutoScroll) {
        scrollToBottom();
      }

      if (
        payload.kind === "message.completed" ||
        payload.kind === "message.failed"
      ) {
        setSending(false);
        void refreshConversations();
        if (payload.error) {
          setRuntimeError(payload.error);
        }
      }
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [shouldAutoScroll]);

  // Handlers
  const handleCreateConversation = async () => {
    if (!conversationAssistantId) return;
    try {
      const conversation = await workspaceConversationCreate({
        assistant_id: conversationAssistantId,
      });
      setSelectedConversationId(conversation.id);
      await refreshConversations();
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    }
  };

  const handleSend = async () => {
    if (!draftMessage.trim() || sending) return;

    const userContent = draftMessage.trim();
    setDraftMessage("");
    setSending(true);
    setShouldAutoScroll(true);

    let targetConversationId = selectedConversationId;
    if (!targetConversationId) {
      if (!conversationAssistantId) {
        setSending(false);
        return;
      }
      try {
        const created = await workspaceConversationCreate({
          assistant_id: conversationAssistantId,
        });
        targetConversationId = created.id;
        setSelectedConversationId(created.id);
        await refreshConversations();
      } catch (error: unknown) {
        setRuntimeError(formatRuntimeError(error));
        setSending(false);
        return;
      }
    }

    // 乐观更新：立即在本地添加用户消息和助手占位消息
    const now = Math.floor(Date.now() / 1000);
    const tempUserId = `temp-user-${Date.now()}`;
    const tempAssistantId = `temp-assistant-${Date.now()}`;

    setSelectedConversation((current) => {
      if (!current) return current;
      return {
        ...current,
        messages: [
          ...current.messages,
          {
            id: tempUserId,
            role: "user" as const,
            content: userContent,
            created_at: now,
            status: "done" as const,
            reasoning: null,
            sources: [],
            tool_calls: [],
          },
          {
            id: tempAssistantId,
            role: "assistant" as const,
            content: "",
            created_at: now,
            status: "streaming" as const,
            reasoning: null,
            sources: [],
            tool_calls: [],
          },
        ],
        updated_at: now,
      };
    });

    // 异步发送请求
    try {
      const result = await workspaceConversationSend({
        conversation_id: targetConversationId,
        content: userContent,
        assistant_id: conversationAssistantId || undefined,
        web_search_enabled:
          selectedConversation?.web_search_enabled ??
          activeAssistant?.tool_policy.web_search ??
          false,
      });
      const detail = await workspaceConversationGet(result.conversation_id);
      setSelectedConversation(detail);
    } catch (error: unknown) {
      // 发送失败，移除乐观添加的消息
      setSelectedConversation((current) => {
        if (!current) return current;
        return {
          ...current,
          messages: current.messages.filter(
            (msg) => msg.id !== tempUserId && msg.id !== tempAssistantId
          ),
        };
      });
      setRuntimeError(formatRuntimeError(error));
      setSending(false);
    }
    // 注意：sending 状态会在流式输出完成/失败后重置
  };

  const handleToggleWebSearch = async () => {
    if (!selectedConversation) return;
    const updated = await workspaceConversationUpdate({
      conversation_id: selectedConversation.id,
      web_search_enabled: !selectedConversation.web_search_enabled,
    });
    setSelectedConversation(updated);
  };

  const handleResetContext = async () => {
    if (!selectedConversation) return;
    await workspaceConversationResetContext(selectedConversation.id);
    await loadConversationDetail(selectedConversation.id);
  };

  const handleTogglePinned = async () => {
    if (!selectedConversation) return;
    const updated = await workspaceConversationUpdate({
      conversation_id: selectedConversation.id,
      pinned: !selectedConversation.pinned,
    });
    setSelectedConversation(updated);
    await refreshConversations();
  };

  const handleToggleArchived = async () => {
    if (!selectedConversation) return;
    const updated = await workspaceConversationUpdate({
      conversation_id: selectedConversation.id,
      archived: !selectedConversation.archived,
    });
    setSelectedConversation(updated);
    await refreshConversations();
  };

  const handleDeleteConversation = async () => {
    if (!selectedConversation) return;
    const confirmed = await confirmDialog(
      t("deleteConversationMessage", "Are you sure you want to delete this conversation?"),
      { title: t("deleteConversation", "Delete Conversation") },
    );
    if (!confirmed) return;
    await workspaceConversationDelete(selectedConversation.id);
    setSelectedConversationId(null);
    setSelectedConversation(null);
    await refreshConversations();
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (runtimeError) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-4 text-destructive">
          <p className="font-semibold">Error</p>
          <p className="text-sm">{runtimeError}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full">
      <div className="grid h-full gap-6 xl:grid-cols-[280px,minmax(0,1fr)]">
        {/* 左侧历史会话面板 */}
        <aside className="flex min-h-0 flex-col rounded-3xl border bg-card">
          <ConversationHistoryPanel
            conversations={conversations}
            selectedId={selectedConversationId}
            onSelect={(id) => setSelectedConversationId(id)}
            onCreateNew={handleCreateConversation}
            loading={loading}
          />
        </aside>

        {/* 右侧主内容 */}
        <main className="flex min-h-0 min-w-0 flex-col rounded-3xl border bg-card">
          {/* 顶部控制栏 */}
          <ChatTopBar
            currentAssistantId={conversationAssistantId}
            assistants={assistants}
            onAssistantChange={(id) => setConversationAssistantId(id)}
            onCreateTopic={handleCreateConversation}
            onOpenQuickAssistant={() => void showQuickAssistantWindow()}
            onOpenAssistantLibrary={openAssistantLibrary}
            onOpenSettings={openModelCenter}
          />

          {/* 聊天标题栏 */}
          <div className="border-b px-4 py-3">
            <div className="text-sm font-semibold">
              {selectedConversation?.title || t("selectOrCreateTopic", "Select or create a topic")}
            </div>
            {selectedConversation ? (
              <div className="mt-2 flex flex-wrap gap-2">
                <div className="rounded-full border bg-muted/20 px-3 py-1 text-xs text-muted-foreground">
                  {t("messagesLabel", "Messages")}:{" "}
                  <span className="font-medium text-foreground">
                    {selectedConversation.messages?.length || 0}
                  </span>
                </div>
                <div className="rounded-full border bg-muted/20 px-3 py-1 text-xs text-muted-foreground">
                  {t("updatedLabel", "Updated")}:{" "}
                  <span className="font-medium text-foreground">
                    {formatTimestamp(selectedConversation.updated_at)}
                  </span>
                </div>
              </div>
            ) : null}
          </div>

          {/* 消息列表 */}
          <div
            ref={messagesContainerRef}
            onScroll={handleScroll}
            className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-4 py-4"
            style={{ scrollBehavior: "auto" }}
          >
            {detailLoading ? (
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                {t("loading", "Loading...")}
              </div>
            ) : selectedConversation ? (
              selectedConversation.messages?.length ? (
                <div className="space-y-4">
                  {selectedConversation.messages.map((message) => (
                    <MessageCard key={message.id} message={message} />
                  ))}
                </div>
              ) : (
                <div className="rounded-xl border border-dashed bg-muted/10 px-6 py-10 text-center text-sm text-muted-foreground">
                  {t("emptyTopicHint", "Start a conversation by sending a message below.")}
                </div>
              )
            ) : (
              <div className="flex h-full items-center justify-center">
                <div className="max-w-md rounded-xl border border-dashed bg-muted/10 px-6 py-10 text-center">
                  <div className="mx-auto mb-4 inline-flex rounded-full bg-primary/10 p-3 text-primary">
                    <MessageSquare className="h-6 w-6" />
                  </div>
                  <div className="text-base font-semibold">
                    {t("startConversation", "Start a Conversation")}
                  </div>
                  <p className="mt-2 text-sm text-muted-foreground">
                    {t("startConversationDesc", "Select a conversation from the left or create a new topic.")}
                  </p>
                </div>
              </div>
            )}
          </div>

          {/* 底部输入区 */}
          <div className="border-t px-4 py-3">
            <div className="rounded-2xl border bg-background p-3">
              <textarea
                value={draftMessage}
                onChange={(e) => setDraftMessage(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    void handleSend();
                  }
                }}
                placeholder={t("composerPlaceholder", "Type a message...")}
                className="min-h-[80px] w-full resize-none bg-transparent text-sm leading-6 outline-none"
              />
              <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-2">
                  {/* 能力徽章 */}
                  <CapabilityBadges
                    knowledgeBaseCount={selectedConversation?.capability_snapshot?.knowledge_base_ids?.length || 0}
                    knowledgeBaseIds={selectedConversation?.capability_snapshot?.knowledge_base_ids || []}
                    mcpServerCount={selectedConversation?.capability_snapshot?.mcp_server_ids?.length || 0}
                    mcpServerIds={selectedConversation?.capability_snapshot?.mcp_server_ids || []}
                    workspaceReadEnabled={selectedConversation?.capability_snapshot?.workspace_read || false}
                    onWorkspaceReadToggle={() => {}}
                    notesSearchEnabled={selectedConversation?.capability_snapshot?.notes_search || false}
                    onNotesSearchToggle={() => {}}
                    memoryEnabled={selectedConversation?.capability_snapshot?.memory_enabled || false}
                    onMemoryToggle={() => {}}
                    webSearchEnabled={selectedConversation?.web_search_enabled || false}
                    onWebSearchToggle={handleToggleWebSearch}
                  />
                  <button
                    type="button"
                    title={t("resetContext", "Reset Context")}
                    onClick={() => void handleResetContext()}
                    disabled={!selectedConversation}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg border hover:bg-muted disabled:opacity-50"
                  >
                    <XCircle className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    title={selectedConversation?.pinned ? t("unpin", "Unpin") : t("pin", "Pin")}
                    onClick={() => void handleTogglePinned()}
                    disabled={!selectedConversation}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg border hover:bg-muted disabled:opacity-50"
                  >
                    {selectedConversation?.pinned ? (
                      <PinOff className="h-4 w-4" />
                    ) : (
                      <Pin className="h-4 w-4" />
                    )}
                  </button>
                  <button
                    type="button"
                    title={selectedConversation?.archived ? t("restore", "Restore") : t("archive", "Archive")}
                    onClick={() => void handleToggleArchived()}
                    disabled={!selectedConversation}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg border hover:bg-muted disabled:opacity-50"
                  >
                    <Archive className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    title={t("delete", "Delete")}
                    onClick={() => void handleDeleteConversation()}
                    disabled={!selectedConversation}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-destructive/30 text-destructive hover:bg-destructive/5 disabled:opacity-50"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
                <button
                  type="button"
                  onClick={() => void handleSend()}
                  disabled={!draftMessage.trim() || sending}
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
        </main>
      </div>
    </div>
  );
}
