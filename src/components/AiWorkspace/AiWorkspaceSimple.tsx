import {
  memo,
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Archive,
  Bot,
  Check,
  CircleEllipsis,
  ChevronDown,
  ChevronRight,
  Cloud,
  Copy,
  Loader2,
  MessageSquare,
  Pin,
  PinOff,
  RotateCcw,
  Send,
  Trash2,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "../ConfirmDialogProvider";
import { ConversationHistoryPanel } from "./ConversationHistoryPanel";
import { CapabilityBadges } from "./CapabilityBadges";
import { ToolCallsPanel } from "./ToolCallsPanel";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  aiWorkspaceBootstrap,
  type AiWorkspaceBootstrap,
  type AssistantCapabilitySnapshot,
  type AssistantConversation,
  type AssistantConversationListItem,
  type AssistantMessage,
  type AssistantPreset,
  type AssistantStreamEvent,
  type ManagedMcpCatalogResponse,
  type ModelCatalogItem,
  type ModelRoleBinding,
  workspaceAssistantMcpCatalog,
  workspaceConversationCreate,
  workspaceConversationDelete,
  workspaceConversationGet,
  workspaceConversationResetContext,
  workspaceConversationSend,
  workspaceConversationUpdate,
  workspaceConversationsList,
} from "@/lib/aiWorkspace";
import {
  mapMcpServerIdsToLabels,
  upsertToolCall,
} from "@/lib/assistantToolCalls";
import { buildMcpServerCardItems } from "@/lib/assistantMcpDisplay";
import { cn } from "@/lib/utils";

const HISTORY_PANEL_COLLAPSED_STORAGE_KEY =
  "onespace:ai-smart-assistant-history-collapsed";
const MIN_MODEL_SELECT_WIDTH = 148;
const MAX_MODEL_SELECT_WIDTH = 320;

type ComposerSelectOption = {
  value: string;
  label: string;
};

function formatRuntimeError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function copyTextToClipboard(text: string) {
  if (navigator?.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  const input = document.createElement("textarea");
  input.value = text;
  input.setAttribute("readonly", "true");
  input.style.position = "fixed";
  input.style.left = "-9999px";
  document.body.appendChild(input);
  input.select();
  const copied = document.execCommand("copy");
  document.body.removeChild(input);
  if (!copied) {
    throw new Error("copy_failed");
  }
}

function getAssistantPreferredModelId(assistant?: AssistantPreset | null) {
  return assistant?.primary_model_id || assistant?.light_model_id || null;
}

function getFirstEnabledModelId(
  enabledModels: ModelCatalogItem[],
  providerId?: string | null,
) {
  return (
    enabledModels.find(
      (item) => item.enabled && (!providerId || item.provider_id === providerId),
    )?.id || null
  );
}

function resolveEffectiveModelId(input: {
  explicitModelId?: string | null;
  assistant?: AssistantPreset | null;
  modelCatalog: ModelCatalogItem[];
  roleBindings: ModelRoleBinding[];
}) {
  const { explicitModelId, assistant, modelCatalog, roleBindings } = input;
  const enabledModels = modelCatalog.filter((item) => item.enabled);
  const enabledModelIds = new Set(enabledModels.map((item) => item.id));

  if (explicitModelId && enabledModelIds.has(explicitModelId)) {
    return explicitModelId;
  }

  const assistantModelId = getAssistantPreferredModelId(assistant);
  if (assistantModelId && enabledModelIds.has(assistantModelId)) {
    return assistantModelId;
  }

  const assistantProviderId = assistantModelId
    ? modelCatalog.find((item) => item.id === assistantModelId)?.provider_id || null
    : null;
  const providerFallbackModelId = getFirstEnabledModelId(
    enabledModels,
    assistantProviderId,
  );
  if (providerFallbackModelId) {
    return providerFallbackModelId;
  }

  const chatRoleModelId =
    roleBindings.find((binding) => binding.role === "chat")?.model_id || null;
  if (chatRoleModelId && enabledModelIds.has(chatRoleModelId)) {
    return chatRoleModelId;
  }

  return enabledModels[0]?.id || null;
}

function buildAssistantCapabilitySnapshot(
  assistant?: AssistantPreset | null,
): AssistantCapabilitySnapshot {
  return {
    web_search: assistant?.tool_policy.web_search ?? false,
    workspace_read: assistant?.tool_policy.workspace_read ?? false,
    notes_search: assistant?.tool_policy.notes_search ?? false,
    knowledge_base_ids: assistant?.knowledge_base_ids || [],
    mcp_server_ids: assistant?.mcp_server_ids || [],
    memory_enabled: assistant?.memory_enabled ?? false,
  };
}

function ComposerSelect({
  label,
  value,
  options,
  onChange,
  disabled = false,
  title,
  ariaLabel,
  className,
  icon: Icon,
  showLabel = true,
  style,
}: {
  label: string;
  value: string;
  options: ComposerSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  title: string;
  ariaLabel: string;
  className?: string;
  icon?: LucideIcon;
  showLabel?: boolean;
  style?: CSSProperties;
}) {
  return (
    <div
      className={cn(
        "inline-flex h-10 min-w-0 items-center gap-2 rounded-full border border-border/70 bg-card/80 px-3 text-sm shadow-sm transition-colors backdrop-blur-sm",
        "focus-within:border-primary/40 focus-within:bg-background",
        disabled && "opacity-60",
        className,
      )}
      style={style}
    >
      {Icon ? (
        <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      ) : null}
      {showLabel ? (
        <span className="shrink-0 text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          {label}
        </span>
      ) : null}
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        title={title}
        aria-label={ariaLabel}
        className="min-w-0 flex-1 appearance-none truncate bg-transparent font-medium text-foreground outline-none disabled:cursor-not-allowed"
      >
        {options.map((option) => (
          <option key={`${label}-${option.value || "__empty__"}`} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
    </div>
  );
}

function ConversationActionButton({
  active = false,
  children,
  className,
  disabled = false,
  onClick,
  title,
  tone = "default",
}: {
  active?: boolean;
  children: ReactNode;
  className?: string;
  disabled?: boolean;
  onClick: () => void;
  title: string;
  tone?: "default" | "danger";
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      aria-pressed={active}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "inline-flex h-9 w-9 items-center justify-center rounded-full border shadow-sm transition-colors",
        tone === "danger"
          ? "border-destructive/25 bg-destructive/5 text-destructive hover:bg-destructive/10"
          : active
            ? "border-primary/25 bg-primary/10 text-primary hover:bg-primary/15"
            : "border-border/70 bg-card/75 text-muted-foreground hover:bg-muted/60 hover:text-foreground",
        "disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
    >
      {children}
    </button>
  );
}

const MessageCard = memo(function MessageCard({ message }: { message: AssistantMessage }) {
  const { t } = useTranslation();
  const [showReasoning, setShowReasoning] = useState(false);
  const [copied, setCopied] = useState(false);

  const handleCopyMessage = async () => {
    if (!message.content) return;
    try {
      await copyTextToClipboard(message.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

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
        <div className="group relative max-w-[82%] rounded-[1.4rem] rounded-br-md border border-primary/15 bg-gradient-to-br from-primary/95 via-primary to-primary/85 px-4 py-3 pr-12 text-primary-foreground shadow-lg shadow-primary/15">
          <button
            type="button"
            onClick={() => void handleCopyMessage()}
            title={copied ? t("copied", "Copied!") : t("copy", "Copy")}
            aria-label={copied ? t("copied", "Copied!") : t("copy", "Copy")}
            className="absolute right-3 top-3 inline-flex h-7 w-7 items-center justify-center rounded-full border border-white/20 bg-white/10 text-primary-foreground opacity-0 shadow-sm transition-all hover:bg-white/20 group-hover:opacity-100 focus:opacity-100 focus:outline-none focus:ring-2 focus:ring-white/40"
          >
            {copied ? (
              <Check className="h-3.5 w-3.5" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
          </button>
          <div className="whitespace-pre-wrap break-words text-[14px] font-medium leading-6">
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
    <div className="rounded-[1.4rem] border border-border/70 bg-muted/[0.16] px-5 py-4 shadow-md shadow-black/5 will-change-transform dark:bg-muted/[0.2] dark:shadow-black/20">
      {/* 状态标签 */}
      {isStreaming ? (
        <div className="mb-3 inline-flex items-center gap-2 rounded-full border border-primary/20 bg-primary/10 px-3 py-1 text-[11px] font-medium text-primary">
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
            <div className="rounded-xl border border-dashed border-border/80 bg-muted/35 px-3 py-3">
              <div className="select-text whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
                {message.reasoning}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {/* 正式内容 */}
      <div className="prose prose-sm max-w-none select-text text-foreground dark:prose-invert prose-headings:text-foreground prose-p:my-3 prose-p:leading-7 prose-p:text-foreground prose-li:leading-7 prose-li:text-foreground prose-strong:text-foreground prose-a:text-primary prose-code:rounded prose-code:bg-muted prose-code:px-1 prose-code:py-0.5 prose-code:text-[0.9em] prose-pre:border prose-pre:border-border/70 prose-pre:bg-muted/40">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>
          {message.content || " "}
        </ReactMarkdown>
      </div>

      {message.sources.length > 0 ? (
        <div className="mt-4 rounded-xl border border-dashed bg-muted/[0.14] px-3 py-3 dark:bg-muted/[0.18]">
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

      <ToolCallsPanel toolCalls={message.tool_calls} />
    </div>
  );
});

export function AiWorkspaceSimple() {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();

  // Refs
  const messagesContainerRef = useRef<HTMLDivElement | null>(null);
  const modelWidthMeasureRef = useRef<HTMLSpanElement | null>(null);
  const actionMenuRef = useRef<HTMLDivElement | null>(null);

  // State
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [shouldAutoScroll, setShouldAutoScroll] = useState(true);
  const [historyPanelCollapsed, setHistoryPanelCollapsed] = useState(() => {
    if (typeof window === "undefined") return false;
    return (
      window.localStorage.getItem(HISTORY_PANEL_COLLAPSED_STORAGE_KEY) === "1"
    );
  });

  const [assistants, setAssistants] = useState<AssistantPreset[]>([]);
  const [conversations, setConversations] = useState<AssistantConversationListItem[]>([]);
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const [selectedConversation, setSelectedConversation] = useState<AssistantConversation | null>(null);
  const [workspaceSettings, setWorkspaceSettings] =
    useState<AiWorkspaceBootstrap["settings"] | null>(null);
  const [draftAssistantId, setDraftAssistantId] = useState<string | null>(null);
  const [draftModelOverrideId, setDraftModelOverrideId] = useState<string | null>(null);
  const [mcpCatalog, setMcpCatalog] = useState<ManagedMcpCatalogResponse | null>(null);
  const [draftMessage, setDraftMessage] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);
  const [modelSelectWidth, setModelSelectWidth] = useState(MIN_MODEL_SELECT_WIDTH);
  const [actionsExpanded, setActionsExpanded] = useState(false);
  const modelCatalog = workspaceSettings?.model_catalog || [];
  const roleBindings = workspaceSettings?.role_bindings || [];
  const enabledModels = useMemo(
    () => modelCatalog.filter((item) => item.enabled),
    [modelCatalog],
  );
  const enabledProviderIds = useMemo(
    () => new Set(enabledModels.map((item) => item.provider_id)),
    [enabledModels],
  );
  const availableProviders = useMemo(
    () =>
      (workspaceSettings?.providers || []).filter(
        (provider) => provider.enabled && enabledProviderIds.has(provider.id),
      ),
    [enabledProviderIds, workspaceSettings?.providers],
  );
  const selectedAssistantId = selectedConversation
    ? selectedConversation.assistant_id ?? null
    : draftAssistantId ?? null;
  const selectedAssistant =
    assistants.find((assistant) => assistant.id === selectedAssistantId) || null;
  const effectiveModelId = useMemo(
    () =>
      resolveEffectiveModelId({
        explicitModelId: selectedConversation
          ? selectedConversation.model_override_id
          : draftModelOverrideId,
        assistant: selectedAssistant,
        modelCatalog,
        roleBindings,
      }),
    [draftModelOverrideId, modelCatalog, roleBindings, selectedAssistant, selectedConversation],
  );
  const effectiveModel =
    modelCatalog.find((item) => item.id === effectiveModelId) || null;
  const selectedProviderId =
    effectiveModel?.provider_id || availableProviders[0]?.id || null;
  const availableModelsForSelectedProvider = useMemo(
    () =>
      enabledModels.filter((item) => item.provider_id === selectedProviderId),
    [enabledModels, selectedProviderId],
  );
  const hasAvailableModels = enabledModels.length > 0;
  const currentCapabilitySnapshot = useMemo(
    () =>
      selectedConversation?.capability_snapshot ||
      buildAssistantCapabilitySnapshot(selectedAssistant),
    [selectedAssistant, selectedConversation?.capability_snapshot],
  );
  const currentWebSearchEnabled =
    selectedConversation?.web_search_enabled ??
    currentCapabilitySnapshot.web_search ??
    false;
  const mcpServerNameById = useMemo(
    () => new Map((mcpCatalog?.items || []).map((item) => [item.server_id, item.name])),
    [mcpCatalog],
  );
  const assistantOptions = useMemo<ComposerSelectOption[]>(
    () => [
      { value: "", label: t("noPreset", "No preset") },
      ...assistants.map((assistant) => ({
        value: assistant.id,
        label: assistant.name,
      })),
    ],
    [assistants, t],
  );
  const providerOptions = useMemo<ComposerSelectOption[]>(
    () =>
      availableProviders.length > 0
        ? availableProviders.map((provider) => ({
            value: provider.id,
            label: provider.name,
          }))
        : [{ value: "", label: t("noProviderAvailable", "No provider available") }],
    [availableProviders, t],
  );
  const modelOptions = useMemo<ComposerSelectOption[]>(
    () =>
      availableModelsForSelectedProvider.length > 0
        ? availableModelsForSelectedProvider.map((item) => ({
            value: item.id,
            label: item.label,
          }))
        : [{ value: "", label: t("noModelAvailable", "No model available") }],
    [availableModelsForSelectedProvider, t],
  );
  const modelSelectDisplayLabel =
    modelOptions.find((item) => item.value === (effectiveModelId || ""))?.label ||
    effectiveModel?.label ||
    t("noModelAvailable", "No model available");
  const assistantSelectDisplayLabel =
    assistantOptions.find((item) => item.value === (selectedAssistantId || ""))?.label ||
    t("noPreset", "No preset");
  const providerSelectDisplayLabel =
    providerOptions.find((item) => item.value === (selectedProviderId || ""))?.label ||
    t("noProviderAvailable", "No provider available");
  const selectedConversationMcpLabels = useMemo(
    () =>
      mapMcpServerIdsToLabels(
        currentCapabilitySnapshot.mcp_server_ids || [],
        mcpServerNameById,
      ),
    [currentCapabilitySnapshot.mcp_server_ids, mcpServerNameById],
  );
  const selectedConversationMcpCards = useMemo(
    () =>
      buildMcpServerCardItems(
        mcpCatalog,
        currentCapabilitySnapshot.mcp_server_ids || [],
        t,
      ),
    [currentCapabilitySnapshot.mcp_server_ids, mcpCatalog, t],
  );
  const modelSelectStyle = useMemo<CSSProperties>(
    () => ({ width: `${modelSelectWidth}px` }),
    [modelSelectWidth],
  );

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
      const [data, catalog]: [AiWorkspaceBootstrap, ManagedMcpCatalogResponse] =
        await Promise.all([aiWorkspaceBootstrap(), workspaceAssistantMcpCatalog()]);
      setWorkspaceSettings(data.settings);
      setAssistants(data.assistants);
      setConversations(data.conversations);
      setDraftAssistantId(data.assistants[0]?.id || null);
      setDraftModelOverrideId(null);
      setSelectedConversationId(data.conversations[0]?.id || null);
      setMcpCatalog(catalog);
      setRuntimeError(null);
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

  useEffect(() => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem(
      HISTORY_PANEL_COLLAPSED_STORAGE_KEY,
      historyPanelCollapsed ? "1" : "0",
    );
  }, [historyPanelCollapsed]);

  useEffect(() => {
    const measureNode = modelWidthMeasureRef.current;
    if (!measureNode) return;
    const measuredWidth = Math.ceil(
      measureNode.getBoundingClientRect().width,
    );
    const nextWidth = Math.max(
      MIN_MODEL_SELECT_WIDTH,
      Math.min(MAX_MODEL_SELECT_WIDTH, measuredWidth + 92),
    );
    setModelSelectWidth((current) =>
      current === nextWidth ? current : nextWidth,
    );
  }, [modelSelectDisplayLabel]);

  useEffect(() => {
    if (!actionsExpanded) return;

    const handlePointerDown = (event: MouseEvent) => {
      if (
        actionMenuRef.current &&
        !actionMenuRef.current.contains(event.target as Node)
      ) {
        setActionsExpanded(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setActionsExpanded(false);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [actionsExpanded]);

  useEffect(() => {
    setActionsExpanded(false);
  }, [selectedConversationId]);

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
              tool_calls: upsertToolCall(message.tool_calls, payload.tool),
            };
          }
          if (payload.kind === "tool.finished" && payload.tool) {
            return {
              ...message,
              id: payload.message_id,
              tool_calls: upsertToolCall(message.tool_calls, payload.tool),
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
  const saveConversationUpdate = async (
    patch: Omit<
      Parameters<typeof workspaceConversationUpdate>[0],
      "conversation_id"
    >,
    optimisticUpdate?: (current: AssistantConversation) => AssistantConversation,
  ) => {
    if (!selectedConversation) return null;

    const previousConversation = selectedConversation;
    if (optimisticUpdate) {
      setSelectedConversation(optimisticUpdate(previousConversation));
    }

    try {
      const updated = await workspaceConversationUpdate({
        conversation_id: previousConversation.id,
        ...patch,
      });
      setSelectedConversation(updated);
      await refreshConversations();
      setRuntimeError(null);
      return updated;
    } catch (error: unknown) {
      if (optimisticUpdate) {
        setSelectedConversation(previousConversation);
      }
      setRuntimeError(formatRuntimeError(error));
      return null;
    }
  };

  const resolveAssistantSelectionModelId = (assistantId: string | null) => {
    if (!assistantId) {
      return effectiveModelId;
    }

    const nextAssistant =
      assistants.find((assistant) => assistant.id === assistantId) || null;
    return resolveEffectiveModelId({
      explicitModelId: getAssistantPreferredModelId(nextAssistant),
      assistant: nextAssistant,
      modelCatalog,
      roleBindings,
    });
  };

  const handleCreateConversation = async () => {
    try {
      const conversation = await workspaceConversationCreate({
        assistant_id: selectedAssistantId || undefined,
        model_override_id: effectiveModelId || undefined,
      });
      setSelectedConversation(conversation);
      setSelectedConversationId(conversation.id);
      setRuntimeError(null);
      await refreshConversations();
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    }
  };

  const handleSend = async () => {
    if (!draftMessage.trim() || sending || !hasAvailableModels) return;

    const userContent = draftMessage.trim();
    setDraftMessage("");
    setSending(true);
    setShouldAutoScroll(true);

    let targetConversationId = selectedConversationId;
    if (!targetConversationId) {
      try {
        const created = await workspaceConversationCreate({
          assistant_id: selectedAssistantId || undefined,
          model_override_id: effectiveModelId || undefined,
        });
        targetConversationId = created.id;
        setSelectedConversation(created);
        setSelectedConversationId(created.id);
        setRuntimeError(null);
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
        assistant_id: selectedAssistantId || undefined,
        model_override_id: effectiveModelId || undefined,
        web_search_enabled:
          selectedConversation?.web_search_enabled ??
          selectedAssistant?.tool_policy.web_search ??
          false,
      });
      const detail = await workspaceConversationGet(result.conversation_id);
      setSelectedConversation(detail);
      setRuntimeError(null);
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

  const handleAssistantChange = async (assistantId: string) => {
    const nextAssistantId = assistantId || null;
    const nextModelId = resolveAssistantSelectionModelId(nextAssistantId);
    const nextAssistant =
      assistants.find((assistant) => assistant.id === nextAssistantId) || null;
    const nextWebSearchEnabled =
      nextAssistant?.tool_policy.web_search ??
      selectedConversation?.web_search_enabled ??
      false;

    if (!selectedConversation) {
      setDraftAssistantId(nextAssistantId);
      setDraftModelOverrideId(nextModelId);
      setRuntimeError(null);
      return;
    }

    await saveConversationUpdate(
      {
        assistant_id: nextAssistantId || "",
        model_override_id: nextModelId || "",
        web_search_enabled: nextWebSearchEnabled,
      },
      (current) => ({
        ...current,
        assistant_id: nextAssistantId,
        model_override_id: nextModelId,
        web_search_enabled: nextWebSearchEnabled,
      }),
    );
  };

  const handleProviderChange = async (providerId: string) => {
    const nextModelId = getFirstEnabledModelId(enabledModels, providerId);
    if (!nextModelId) return;

    if (!selectedConversation) {
      setDraftModelOverrideId(nextModelId);
      setRuntimeError(null);
      return;
    }

    await saveConversationUpdate(
      { model_override_id: nextModelId },
      (current) => ({
        ...current,
        model_override_id: nextModelId,
      }),
    );
  };

  const handleModelChange = async (modelId: string) => {
    const nextModelId = modelId || null;

    if (!selectedConversation) {
      setDraftModelOverrideId(nextModelId);
      setRuntimeError(null);
      return;
    }

    await saveConversationUpdate(
      { model_override_id: nextModelId || "" },
      (current) => ({
        ...current,
        model_override_id: nextModelId,
      }),
    );
  };

  const handleToggleWebSearch = async () => {
    if (!selectedConversation) return;
    await saveConversationUpdate(
      { web_search_enabled: !selectedConversation.web_search_enabled },
      (current) => ({
        ...current,
        web_search_enabled: !current.web_search_enabled,
      }),
    );
  };

  const handleResetContext = async () => {
    if (!selectedConversation) return;
    try {
      await workspaceConversationResetContext(selectedConversation.id);
      await loadConversationDetail(selectedConversation.id);
      setRuntimeError(null);
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    }
  };

  const handleTogglePinned = async () => {
    if (!selectedConversation) return;
    await saveConversationUpdate(
      { pinned: !selectedConversation.pinned },
      (current) => ({
        ...current,
        pinned: !current.pinned,
      }),
    );
  };

  const handleToggleArchived = async () => {
    if (!selectedConversation) return;
    await saveConversationUpdate(
      { archived: !selectedConversation.archived },
      (current) => ({
        ...current,
        archived: !current.archived,
      }),
    );
  };

  const handleDeleteConversation = async () => {
    if (!selectedConversation) return;
    const confirmed = await confirmDialog(
      t("deleteConversationMessage", "Are you sure you want to delete this conversation?"),
      { title: t("deleteConversation", "Delete Conversation") },
    );
    if (!confirmed) return;
    try {
      await workspaceConversationDelete(selectedConversation.id);
      setDraftAssistantId(selectedAssistantId);
      setDraftModelOverrideId(effectiveModelId);
      setSelectedConversationId(null);
      setSelectedConversation(null);
      setRuntimeError(null);
      await refreshConversations();
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (
    runtimeError &&
    !workspaceSettings &&
    assistants.length === 0 &&
    conversations.length === 0
  ) {
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
    <div className="relative h-full">
      <div
        className={`grid h-full gap-6 ${
          historyPanelCollapsed
            ? "xl:grid-cols-[72px,minmax(0,1fr)]"
            : "xl:grid-cols-[280px,minmax(0,1fr)]"
        }`}
      >
        {/* 左侧历史会话面板 */}
        <aside className="flex min-h-0 flex-col overflow-hidden rounded-3xl border bg-card transition-[width] duration-200">
          <ConversationHistoryPanel
            conversations={conversations}
            selectedId={selectedConversationId}
            onSelect={(id) => setSelectedConversationId(id)}
            onCreateNew={handleCreateConversation}
            collapsed={historyPanelCollapsed}
            onToggleCollapsed={() =>
              setHistoryPanelCollapsed((current) => !current)
            }
            loading={loading}
          />
        </aside>

        {/* 右侧主内容 */}
        <main className="flex min-h-0 min-w-0 flex-col rounded-3xl border bg-card">
          {/* 消息列表 */}
          <div
            ref={messagesContainerRef}
            onScroll={handleScroll}
            className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden bg-muted/[0.18] px-4 py-5"
            style={{ scrollBehavior: "auto" }}
          >
            {detailLoading ? (
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                {t("loading", "Loading...")}
              </div>
            ) : selectedConversation ? (
              selectedConversation.messages?.length ? (
                <div className="space-y-5">
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
                    {t("startConversationDesc", "Select a conversation from the left or create a new conversation.")}
                  </p>
                </div>
              </div>
            )}
          </div>

          {/* 底部输入区 */}
          <div className="border-t px-4 py-3">
            {runtimeError ? (
              <div className="mb-3 rounded-xl border border-destructive/30 bg-destructive/5 px-4 py-3 text-sm text-destructive">
                {runtimeError}
              </div>
            ) : null}
            {!hasAvailableModels ? (
              <div className="mb-3 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
                {t(
                  "assistantNoAvailableModels",
                  "No available models. Open Model Center to enable at least one model.",
                )}
              </div>
            ) : null}
            <div className="rounded-[1.5rem] border border-border/55 bg-gradient-to-b from-background/96 to-muted/[0.1] p-3 shadow-sm">
              <div className="rounded-[1.1rem] bg-background/72 px-3 py-2 dark:bg-background/70">
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
                  disabled={!hasAvailableModels}
                  className="min-h-[84px] w-full resize-none bg-transparent text-[14px] leading-6 text-foreground outline-none placeholder:text-muted-foreground/75 disabled:cursor-not-allowed disabled:opacity-60"
                />
              </div>
              <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-[1.25rem] border border-border/70 bg-card/70 p-1.5 shadow-sm">
                    <ComposerSelect
                      label={t("assistantLabel", "Assistant")}
                      value={selectedAssistantId || ""}
                      onChange={(value) => void handleAssistantChange(value)}
                      options={assistantOptions}
                      title={t("assistantLabel", "Assistant")}
                      ariaLabel={t("assistantLabel", "Assistant")}
                      icon={Bot}
                      showLabel={false}
                      className="max-w-[12rem]"
                    />
                    <ComposerSelect
                      label={t("providerLabel", "Provider")}
                      value={selectedProviderId || ""}
                      onChange={(value) => void handleProviderChange(value)}
                      options={providerOptions}
                      disabled={!hasAvailableModels}
                      title={t("providerLabel", "Provider")}
                      ariaLabel={t("providerLabel", "Provider")}
                      icon={Cloud}
                      showLabel={false}
                      className="max-w-[11rem]"
                    />
                    <ComposerSelect
                      label={t("modelLabel", "Model")}
                      value={effectiveModelId || ""}
                      onChange={(value) => void handleModelChange(value)}
                      options={modelOptions}
                      disabled={
                        !hasAvailableModels ||
                        availableModelsForSelectedProvider.length === 0
                      }
                      title={t("modelLabel", "Model")}
                      ariaLabel={t("modelLabel", "Model")}
                      style={modelSelectStyle}
                    />
                  </div>
                  <div className="flex min-w-0 flex-wrap items-center gap-1.5 rounded-[1.25rem] border border-border/70 bg-muted/15 p-1.5 shadow-sm">
                    <CapabilityBadges
                      knowledgeBaseCount={
                        currentCapabilitySnapshot.knowledge_base_ids?.length || 0
                      }
                      knowledgeBaseIds={
                        currentCapabilitySnapshot.knowledge_base_ids || []
                      }
                      mcpServerCount={
                        currentCapabilitySnapshot.mcp_server_ids?.length || 0
                      }
                      mcpServerIds={currentCapabilitySnapshot.mcp_server_ids || []}
                      mcpServerLabels={selectedConversationMcpLabels}
                      mcpServerCards={selectedConversationMcpCards}
                      workspaceReadEnabled={
                        currentCapabilitySnapshot.workspace_read || false
                      }
                      notesSearchEnabled={
                        currentCapabilitySnapshot.notes_search || false
                      }
                      memoryEnabled={
                        currentCapabilitySnapshot.memory_enabled || false
                      }
                      webSearchEnabled={currentWebSearchEnabled}
                      onWebSearchToggle={
                        selectedConversation ? handleToggleWebSearch : undefined
                      }
                    />
                  </div>
                  <div ref={actionMenuRef} className="relative">
                    <ConversationActionButton
                      title={
                        actionsExpanded
                          ? t("collapseOptions", "Collapse Options")
                          : t("expandOptions", "Expand Options")
                      }
                      onClick={() =>
                        setActionsExpanded((current) => !current)
                      }
                      disabled={!selectedConversation}
                      active={actionsExpanded}
                    >
                      <CircleEllipsis className="h-4 w-4" />
                    </ConversationActionButton>
                    <div
                      className={cn(
                        "absolute bottom-full right-0 z-20 mb-2 origin-bottom-right rounded-[1.25rem] border border-border/80 bg-popover/95 p-1.5 shadow-xl backdrop-blur-md transition-all duration-200 ease-out",
                        actionsExpanded
                          ? "pointer-events-auto translate-y-0 scale-100 opacity-100"
                          : "pointer-events-none translate-y-2 scale-95 opacity-0",
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <ConversationActionButton
                          title={t("resetContext", "Reset Context")}
                          onClick={() => {
                            setActionsExpanded(false);
                            void handleResetContext();
                          }}
                          disabled={!selectedConversation}
                        >
                          <RotateCcw className="h-4 w-4" />
                        </ConversationActionButton>
                        <ConversationActionButton
                          title={
                            selectedConversation?.pinned
                              ? t("unpin", "Unpin")
                              : t("pin", "Pin")
                          }
                          onClick={() => {
                            setActionsExpanded(false);
                            void handleTogglePinned();
                          }}
                          disabled={!selectedConversation}
                          active={selectedConversation?.pinned || false}
                        >
                          {selectedConversation?.pinned ? (
                            <PinOff className="h-4 w-4" />
                          ) : (
                            <Pin className="h-4 w-4" />
                          )}
                        </ConversationActionButton>
                        <ConversationActionButton
                          title={
                            selectedConversation?.archived
                              ? t("restore", "Restore")
                              : t("archive", "Archive")
                          }
                          onClick={() => {
                            setActionsExpanded(false);
                            void handleToggleArchived();
                          }}
                          disabled={!selectedConversation}
                          active={selectedConversation?.archived || false}
                        >
                          <Archive className="h-4 w-4" />
                        </ConversationActionButton>
                        <ConversationActionButton
                          title={t("delete", "Delete")}
                          onClick={() => {
                            setActionsExpanded(false);
                            void handleDeleteConversation();
                          }}
                          disabled={!selectedConversation}
                          tone="danger"
                        >
                          <Trash2 className="h-4 w-4" />
                        </ConversationActionButton>
                      </div>
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-2">
                  <button
                    type="button"
                    onClick={() => void handleSend()}
                    disabled={!draftMessage.trim() || sending || !hasAvailableModels}
                    className="inline-flex h-10 shrink-0 items-center gap-2 rounded-full bg-primary px-5 text-sm font-medium text-primary-foreground shadow-lg shadow-primary/20 transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
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
        </main>
      </div>
      <span
        ref={modelWidthMeasureRef}
        aria-hidden="true"
        className="pointer-events-none absolute -left-[9999px] top-0 whitespace-pre text-sm font-medium"
      >
        {modelSelectDisplayLabel}
      </span>
      <span
        aria-hidden="true"
        className="pointer-events-none absolute -left-[9999px] top-0 whitespace-pre text-sm font-medium"
      >
        {assistantSelectDisplayLabel}
        {providerSelectDisplayLabel}
      </span>
    </div>
  );
}
