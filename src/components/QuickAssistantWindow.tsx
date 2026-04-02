import { useEffect, useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  Bot,
  Clipboard,
  Globe,
  Loader2,
  MessageSquare,
  Minimize2,
  Send,
  Sparkles,
  Wand2,
} from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import {
  aiWorkspaceBootstrap,
  hideQuickAssistantWindow,
  hideSelectionAssistantWindow,
  type AssistantConversation,
  type AssistantPreset,
  type AssistantStreamEvent,
  type ModelRoleBinding,
  type QuickAssistantPreferences,
  type SelectionAssistantPreferences,
  workspaceConversationCreate,
  workspaceConversationGet,
  workspaceConversationSend,
  workspaceQuickAssistantSave,
  workspaceSelectionAssistantSave,
} from "@/lib/aiWorkspace";
import { upsertToolCall } from "@/lib/assistantToolCalls";

const QUICK_ASSISTANT_ROLES = [
  "quick_assistant",
  "selection_assistant",
  "assistant",
  "chat",
  "summary",
  "translate",
  "topic_naming",
] as const;

const SELECTION_ASSISTANT_ROLES = [
  "selection_assistant",
  "summary",
  "translate",
] as const;

function getAssistantWindowTitle(
  variant: AssistantWindowVariant,
  t: TFunction,
) {
  return variant === "selection"
    ? t("selectionAssistant", "Selection Assistant")
    : t("quickAssistant", "Quick Assistant");
}

function getAssistantRoleLabel(role: string, t: TFunction) {
  switch (role) {
    case "quick_assistant":
      return t("quickAssistant", "Quick Assistant");
    case "selection_assistant":
      return t("selectionAssistant", "Selection Assistant");
    case "assistant":
      return t("assistantLabel", "Assistant");
    case "chat":
      return t("chatLabel", "Chat");
    case "summary":
      return t("summaryLabel", "Summary");
    case "translate":
      return t("translateLabel", "Translate");
    case "topic_naming":
      return t("topicNamingLabel", "Conversation Naming");
    default:
      return role;
  }
}

function formatTimestamp(ts: number) {
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function resolveRoleModelId(roleBindings: ModelRoleBinding[], role: string) {
  return (
    roleBindings.find((binding) => binding.role === role)?.model_id || null
  );
}

function CompactMessage({
  message,
}: {
  message: AssistantConversation["messages"][number];
}) {
  const { t } = useTranslation();
  const isAssistant = message.role === "assistant";
  return (
    <div
      className={`rounded-2xl border px-4 py-3 ${isAssistant ? "bg-card" : "bg-muted/20"}`}
    >
      <div className="mb-2 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <div
            className={`rounded-full p-1.5 ${isAssistant ? "bg-primary/10 text-primary" : "bg-background"}`}
          >
            {isAssistant ? (
              <Bot className="h-3.5 w-3.5" />
            ) : (
              <MessageSquare className="h-3.5 w-3.5" />
            )}
          </div>
          <span>
            {isAssistant
              ? t("assistantLabel", "Assistant")
              : t("youLabel", "You")}
          </span>
        </div>
        <div className="text-[11px] text-muted-foreground">
          {formatTimestamp(message.created_at)}
        </div>
      </div>
      {isAssistant ? (
        <div className="prose prose-sm max-w-none dark:prose-invert">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {message.content || " "}
          </ReactMarkdown>
        </div>
      ) : (
        <div className="whitespace-pre-wrap text-sm leading-6">
          {message.content}
        </div>
      )}
      {message.sources.length > 0 ? (
        <div className="mt-3 space-y-2 rounded-xl border bg-muted/10 p-3">
          <div className="text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            {t("sourcesLabel", "Sources")}
          </div>
          {message.sources.slice(0, 3).map((source, index) => (
            <a
              key={`${source.url}-${index}`}
              href={source.url}
              target="_blank"
              rel="noreferrer"
              className="block rounded-lg border bg-background px-3 py-2 text-xs hover:bg-muted/30"
            >
              <div className="font-medium">{source.title || source.url}</div>
              <div className="mt-1 line-clamp-2 text-muted-foreground">
                {source.snippet}
              </div>
            </a>
          ))}
        </div>
      ) : null}
    </div>
  );
}

type AssistantWindowVariant = "quick" | "selection";
type AssistantWindowPreferences =
  | QuickAssistantPreferences
  | SelectionAssistantPreferences;

export function QuickAssistantWindow({
  variant = "quick",
}: {
  variant?: AssistantWindowVariant;
}) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [sending, setSending] = useState(false);
  const [preferences, setPreferences] =
    useState<AssistantWindowPreferences | null>(null);
  const [assistants, setAssistants] = useState<AssistantPreset[]>([]);
  const [roleBindings, setRoleBindings] = useState<ModelRoleBinding[]>([]);
  const [conversation, setConversation] =
    useState<AssistantConversation | null>(null);
  const [draft, setDraft] = useState("");
  const [runtimeError, setRuntimeError] = useState<string | null>(null);

  const bootstrap = async () => {
    setLoading(true);
    setRuntimeError(null);
    try {
      const data = await aiWorkspaceBootstrap();
      setPreferences(
        variant === "selection"
          ? data.selection_assistant
          : data.quick_assistant,
      );
      setAssistants(data.assistants);
      setRoleBindings(data.settings.role_bindings || []);
    } catch (error: any) {
      setRuntimeError(error?.toString?.() || String(error));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void bootstrap();
  }, [variant]);

  useEffect(() => {
    if (!preferences?.read_clipboard_on_open) return;
    if (!navigator.clipboard?.readText) {
      if (variant === "selection") {
        setRuntimeError(
          t(
            "selectionAssistantClipboardFallbackError",
            "Selection Assistant cannot read the system selection directly right now and has fallen back to a normal floating window.",
          ),
        );
      }
      return;
    }
    navigator.clipboard
      .readText()
      .then((text) => {
        if (text.trim()) {
          setDraft((current) => current || text.trim());
        }
      })
      .catch(() => {});
  }, [preferences?.read_clipboard_on_open, variant]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<AssistantStreamEvent>("assistant-stream", (event) => {
      if (disposed || !event.payload) return;
      const payload = event.payload;
      setConversation((current) => {
        if (!current || current.id !== payload.conversation_id) {
          return current;
        }
        const messages = current.messages.map((message) => {
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
            return { ...message, sources: payload.sources || message.sources };
          }
          if (payload.kind === "tool.started" && payload.tool) {
            return {
              ...message,
              tool_calls: upsertToolCall(message.tool_calls, payload.tool),
            };
          }
          if (payload.kind === "tool.finished" && payload.tool) {
            return {
              ...message,
              tool_calls: upsertToolCall(message.tool_calls, payload.tool),
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
            return { ...message, status: "failed" };
          }
          return message;
        });
        return {
          ...current,
          updated_at: Math.floor(Date.now() / 1000),
          messages,
        };
      });

      if (
        payload.kind === "message.completed" ||
        payload.kind === "message.failed"
      ) {
        setSending(false);
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
  }, []);

  const activeAssistant = useMemo(
    () =>
      assistants.find(
        (assistant) => assistant.id === preferences?.preferred_assistant_id,
      ) ||
      assistants[0] ||
      null,
    [assistants, preferences?.preferred_assistant_id],
  );

  const activeRoleModelId = useMemo(
    () =>
      resolveRoleModelId(
        roleBindings,
        preferences?.preferred_role ||
          (variant === "selection" ? "selection_assistant" : "quick_assistant"),
      ),
    [preferences?.preferred_role, roleBindings, variant],
  );
  const availableRoles =
    variant === "selection" ? SELECTION_ASSISTANT_ROLES : QUICK_ASSISTANT_ROLES;

  const persistPreferences = async (next: AssistantWindowPreferences) => {
    setPreferences(next);
    setSaving(true);
    try {
      if (variant === "selection") {
        await workspaceSelectionAssistantSave(
          next as SelectionAssistantPreferences,
        );
      } else {
        await workspaceQuickAssistantSave(next as QuickAssistantPreferences);
      }
      setRuntimeError(null);
    } catch (error: any) {
      setRuntimeError(error?.toString?.() || String(error));
    } finally {
      setSaving(false);
    }
  };

  const ensureConversation = async () => {
    if (conversation) return conversation;
    const windowTitle = getAssistantWindowTitle(variant, t);
    const created = await workspaceConversationCreate({
      title:
        preferences?.prefer_assistant_mode && activeAssistant
          ? `${activeAssistant.name} ${windowTitle}`
          : windowTitle,
      assistant_id: preferences?.prefer_assistant_mode
        ? activeAssistant?.id
        : undefined,
      model_override_id: preferences?.prefer_assistant_mode
        ? undefined
        : activeRoleModelId || undefined,
    });
    const detail = await workspaceConversationGet(created.id);
    setConversation(detail);
    return detail;
  };

  const handleSend = async () => {
    const content = draft.trim();
    if (!content || !preferences || sending) return;
    setSending(true);
    setRuntimeError(null);
    try {
      const target = await ensureConversation();
      await workspaceConversationSend({
        conversation_id: target.id,
        content,
        assistant_id: preferences.prefer_assistant_mode
          ? activeAssistant?.id
          : undefined,
        model_override_id: preferences.prefer_assistant_mode
          ? undefined
          : activeRoleModelId || undefined,
        web_search_enabled: preferences.prefer_assistant_mode
          ? activeAssistant?.tool_policy.web_search
          : false,
      });
      setDraft("");
      const detail = await workspaceConversationGet(target.id);
      setConversation(detail);
    } catch (error: any) {
      setRuntimeError(error?.toString?.() || String(error));
      setSending(false);
    }
  };

  if (loading || !preferences) {
    const loadingTitle = getAssistantWindowTitle(variant, t);
    return (
      <div className="flex h-screen items-center justify-center bg-background text-foreground">
        <div className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("loadingWithTitle", "Loading {{title}}...", {
            title: loadingTitle,
          })}
        </div>
      </div>
    );
  }

  const windowTitle = getAssistantWindowTitle(variant, t);
  const hideWindow =
    variant === "selection"
      ? hideSelectionAssistantWindow
      : hideQuickAssistantWindow;
  const modeDescription =
    variant === "selection"
      ? t(
          "selectionAssistantModeHint",
          "Process the current selected text and automatically fall back to clipboard mode when the system does not support direct selection capture.",
        )
      : t(
          "quickAssistantModeHint",
          "Best for quick Q&A, rewriting, translation, or summarization.",
        );
  const composerPlaceholder =
    variant === "selection"
      ? t(
          "selectionAssistantComposerPlaceholder",
          "Type or paste the selected text to quickly rewrite, translate, summarize, or explain it...",
        )
      : t(
          "quickAssistantComposerPlaceholder",
          "Type one sentence to quickly generate a reply, rewrite, translate, or summarize...",
        );

  return (
    <div className="flex h-screen flex-col overflow-hidden rounded-3xl border border-border bg-background text-foreground shadow-2xl">
      <div
        className="flex items-center justify-between gap-4 border-b bg-card/90 px-4 py-3"
        data-tauri-drag-region
        onMouseDown={() => {
          getCurrentWindow()
            .startDragging()
            .catch(() => {});
        }}
      >
        <div className="flex min-w-0 items-center gap-3">
          <div className="rounded-2xl bg-primary/10 p-2 text-primary">
            <Wand2 className="h-5 w-5" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold">{windowTitle}</div>
            <div className="truncate text-xs text-muted-foreground">
              {preferences.prefer_assistant_mode
                ? `${t("assistantMode", "Assistant Mode")} · ${
                    activeAssistant?.name ||
                    t(
                      "quickAssistantNoAssistantSelected",
                      "No assistant selected",
                    )
                  }`
                : `${t("modelMode", "Model Mode")} · ${getAssistantRoleLabel(preferences.preferred_role, t)}`}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {saving ? (
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          ) : null}
          <button
            type="button"
            onClick={() => void hideWindow()}
            className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
          >
            <Minimize2 className="h-4 w-4" />
            {t("hide", "Hide")}
          </button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 gap-0 lg:grid-cols-[280px,minmax(0,1fr)]">
        <aside className="border-r bg-card/60 p-4">
          <div className="space-y-4">
            <div className="rounded-2xl border bg-background p-4">
              <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {t("launchMode", "Launch Mode")}
              </div>
              <div className="mt-3 grid gap-2">
                <button
                  type="button"
                  onClick={() =>
                    void persistPreferences({
                      ...preferences,
                      prefer_assistant_mode: true,
                      preferred_assistant_id:
                        activeAssistant?.id || assistants[0]?.id || null,
                    })
                  }
                  className={`rounded-xl border px-3 py-3 text-left ${preferences.prefer_assistant_mode ? "border-primary bg-primary/5" : "hover:bg-muted/30"}`}
                >
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <Bot className="h-4 w-4" />
                    {t("assistantMode", "Assistant Mode")}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {t(
                      "quickAssistantAssistantModeDesc",
                      "Inherit the assistant preset prompt, capabilities, and default model.",
                    )}
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void persistPreferences({
                      ...preferences,
                      prefer_assistant_mode: false,
                    })
                  }
                  className={`rounded-xl border px-3 py-3 text-left ${!preferences.prefer_assistant_mode ? "border-primary bg-primary/5" : "hover:bg-muted/30"}`}
                >
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <Sparkles className="h-4 w-4" />
                    {t("modelMode", "Model Mode")}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {t(
                      "quickAssistantModelModeDesc",
                      "Use the role-bound model directly without inheriting assistant capabilities.",
                    )}
                  </div>
                </button>
              </div>
            </div>

            {preferences.prefer_assistant_mode ? (
              <div className="rounded-2xl border bg-background p-4">
                <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                  {t("assistantLabel", "Assistant")}
                </div>
                <select
                  value={activeAssistant?.id || ""}
                  onChange={(event) =>
                    void persistPreferences({
                      ...preferences,
                      preferred_assistant_id: event.target.value || null,
                    })
                  }
                  className="mt-3 w-full rounded-xl border bg-background px-3 py-2.5 text-sm"
                >
                  {assistants.map((assistant) => (
                    <option key={assistant.id} value={assistant.id}>
                      {assistant.name}
                    </option>
                  ))}
                </select>
                <div className="mt-3 rounded-xl border bg-muted/10 px-3 py-3 text-xs text-muted-foreground">
                  {activeAssistant?.description ||
                    t(
                      "quickAssistantNoAssistantDescription",
                      "This assistant does not have a description yet.",
                    )}
                </div>
              </div>
            ) : (
              <div className="rounded-2xl border bg-background p-4">
                <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                  {t("modelRoleLabel", "Model Role")}
                </div>
                <select
                  value={preferences.preferred_role}
                  onChange={(event) =>
                    void persistPreferences({
                      ...preferences,
                      preferred_role: event.target.value,
                    })
                  }
                  className="mt-3 w-full rounded-xl border bg-background px-3 py-2.5 text-sm"
                >
                  {availableRoles.map((role) => (
                    <option key={role} value={role}>
                      {getAssistantRoleLabel(role, t)}
                    </option>
                  ))}
                </select>
                <div className="mt-3 rounded-xl border bg-muted/10 px-3 py-3 text-xs text-muted-foreground">
                  {t(
                    "quickAssistantCurrentRoleModel",
                    "Current role-bound model",
                  )}
                  :{" "}
                  {activeRoleModelId ||
                    t(
                      "quickAssistantRoleBindHint",
                      "Not set. Bind it in the AI Connection Center.",
                    )}
                </div>
              </div>
            )}

            <label className="flex items-center justify-between rounded-2xl border bg-background px-4 py-3">
              <div>
                <div className="text-sm font-medium">
                  {t("readClipboardOnOpen", "Read Clipboard On Open")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t(
                    "readClipboardOnOpenDesc",
                    "Useful for quickly processing a selected text snippet.",
                  )}
                </div>
              </div>
              <input
                type="checkbox"
                checked={preferences.read_clipboard_on_open}
                onChange={(event) =>
                  void persistPreferences({
                    ...preferences,
                    read_clipboard_on_open: event.target.checked,
                  })
                }
                className="h-4 w-4"
              />
            </label>

            <div className="rounded-2xl border bg-background p-4">
              <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                {t("hintsLabel", "Hints")}
              </div>
              <div className="mt-3 space-y-2 text-xs text-muted-foreground">
                <div className="flex items-center gap-2">
                  <Clipboard className="h-3.5 w-3.5" />
                  {modeDescription}
                </div>
                <div className="flex items-center gap-2">
                  <Globe className="h-3.5 w-3.5" />
                  {t(
                    "quickAssistantInternetHint",
                    "Internet access follows the assistant policy or the model role binding.",
                  )}
                </div>
              </div>
            </div>
          </div>
        </aside>

        <main className="flex min-h-0 flex-col">
          <div className="flex-1 overflow-y-auto px-5 py-4">
            {conversation?.messages.length ? (
              <div className="space-y-3">
                {conversation.messages.map((message) => (
                  <CompactMessage key={message.id} message={message} />
                ))}
              </div>
            ) : (
              <div className="flex h-full items-center justify-center">
                <div className="max-w-md rounded-3xl border border-dashed bg-muted/10 px-8 py-10 text-center">
                  <div className="mx-auto mb-4 inline-flex rounded-2xl bg-primary/10 p-3 text-primary">
                    <Wand2 className="h-6 w-6" />
                  </div>
                  <div className="text-base font-semibold">
                    {variant === "selection"
                      ? t(
                          "selectionAssistantEmptyTitle",
                          "Quickly process a piece of text",
                        )
                      : t(
                          "quickAssistantEmptyTitle",
                          "Quickly process a question",
                        )}
                  </div>
                  <div className="mt-2 text-sm text-muted-foreground">
                    {t(
                      "quickAssistantEmptyDesc",
                      "A real conversation will be created here and the message stream will be preserved, so you can continue later in AI Workspace.",
                    )}
                  </div>
                </div>
              </div>
            )}
          </div>

          <div className="border-t bg-card/70 px-5 py-4">
            {runtimeError ? (
              <div className="mb-3 rounded-xl border border-destructive/30 bg-destructive/5 px-4 py-3 text-sm text-destructive">
                {runtimeError}
              </div>
            ) : null}
            <div className="rounded-3xl border bg-background p-3 shadow-sm">
              <textarea
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                placeholder={composerPlaceholder}
                className="min-h-[110px] w-full resize-none bg-transparent text-sm leading-6 outline-none"
              />
              <div className="mt-3 flex items-center justify-between gap-3">
                <div className="text-xs text-muted-foreground">
                  {preferences.prefer_assistant_mode
                    ? `${t("assistantLabel", "Assistant")}: ${
                        activeAssistant?.name ||
                        t(
                          "quickAssistantNoAssistantSelected",
                          "No assistant selected",
                        )
                      }`
                    : `${t("roleLabel", "Role")}: ${getAssistantRoleLabel(preferences.preferred_role, t)}`}
                </div>
                <button
                  type="button"
                  onClick={() => void handleSend()}
                  disabled={!draft.trim() || sending}
                  className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
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
