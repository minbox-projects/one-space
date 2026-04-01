import { useEffect, useMemo, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
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
} from 'lucide-react';
import {
  aiWorkspaceBootstrap,
  hideQuickAssistantWindow,
  type AssistantConversation,
  type AssistantPreset,
  type AssistantStreamEvent,
  type ModelRoleBinding,
  type QuickAssistantPreferences,
  workspaceConversationCreate,
  workspaceConversationGet,
  workspaceConversationSend,
  workspaceQuickAssistantSave,
} from '@/lib/aiWorkspace';

const QUICK_ASSISTANT_ROLES = [
  { role: 'quick_assistant', label: 'Quick Assistant' },
  { role: 'assistant', label: 'Assistant' },
  { role: 'chat', label: 'Chat' },
  { role: 'summary', label: 'Summary' },
  { role: 'translate', label: 'Translate' },
  { role: 'topic_naming', label: 'Topic Naming' },
] as const;

function formatTimestamp(ts: number) {
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function resolveRoleModelId(roleBindings: ModelRoleBinding[], role: string) {
  return roleBindings.find((binding) => binding.role === role)?.model_id || null;
}

function CompactMessage({ message }: { message: AssistantConversation['messages'][number] }) {
  const isAssistant = message.role === 'assistant';
  return (
    <div className={`rounded-2xl border px-4 py-3 ${isAssistant ? 'bg-card' : 'bg-muted/20'}`}>
      <div className="mb-2 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <div className={`rounded-full p-1.5 ${isAssistant ? 'bg-primary/10 text-primary' : 'bg-background'}`}>
            {isAssistant ? <Bot className="h-3.5 w-3.5" /> : <MessageSquare className="h-3.5 w-3.5" />}
          </div>
          <span>{isAssistant ? 'Assistant' : 'You'}</span>
        </div>
        <div className="text-[11px] text-muted-foreground">{formatTimestamp(message.created_at)}</div>
      </div>
      {isAssistant ? (
        <div className="prose prose-sm max-w-none dark:prose-invert">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content || ' '}</ReactMarkdown>
        </div>
      ) : (
        <div className="whitespace-pre-wrap text-sm leading-6">{message.content}</div>
      )}
      {message.sources.length > 0 ? (
        <div className="mt-3 space-y-2 rounded-xl border bg-muted/10 p-3">
          <div className="text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">Sources</div>
          {message.sources.slice(0, 3).map((source, index) => (
            <a
              key={`${source.url}-${index}`}
              href={source.url}
              target="_blank"
              rel="noreferrer"
              className="block rounded-lg border bg-background px-3 py-2 text-xs hover:bg-muted/30"
            >
              <div className="font-medium">{source.title || source.url}</div>
              <div className="mt-1 line-clamp-2 text-muted-foreground">{source.snippet}</div>
            </a>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function QuickAssistantWindow() {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [sending, setSending] = useState(false);
  const [preferences, setPreferences] = useState<QuickAssistantPreferences | null>(null);
  const [assistants, setAssistants] = useState<AssistantPreset[]>([]);
  const [roleBindings, setRoleBindings] = useState<ModelRoleBinding[]>([]);
  const [conversation, setConversation] = useState<AssistantConversation | null>(null);
  const [draft, setDraft] = useState('');
  const [runtimeError, setRuntimeError] = useState<string | null>(null);

  const bootstrap = async () => {
    setLoading(true);
    try {
      const data = await aiWorkspaceBootstrap();
      setPreferences(data.quick_assistant);
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
  }, []);

  useEffect(() => {
    if (!preferences?.read_clipboard_on_open) return;
    navigator.clipboard
      .readText()
      .then((text) => {
        if (text.trim()) {
          setDraft((current) => current || text.trim());
        }
      })
      .catch(() => {});
  }, [preferences?.read_clipboard_on_open]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<AssistantStreamEvent>('assistant-stream', (event) => {
      if (disposed || !event.payload) return;
      const payload = event.payload;
      setConversation((current) => {
        if (!current || current.id !== payload.conversation_id) {
          return current;
        }
        const messages = current.messages.map((message) => {
          if (message.id !== payload.message_id) return message;
          if (payload.kind === 'message.delta') {
            return { ...message, content: `${message.content}${payload.text || ''}`, status: 'streaming' };
          }
          if (payload.kind === 'reasoning.delta') {
            return { ...message, reasoning: `${message.reasoning || ''}${payload.text || ''}` };
          }
          if (payload.kind === 'sources') {
            return { ...message, sources: payload.sources || message.sources };
          }
          if (payload.kind === 'tool.started' && payload.tool) {
            return { ...message, tool_calls: [...message.tool_calls, payload.tool] };
          }
          if (payload.kind === 'tool.finished' && payload.tool) {
            return {
              ...message,
              tool_calls: [
                ...message.tool_calls.filter((item) => item.name !== payload.tool?.name),
                payload.tool,
              ],
            };
          }
          if (payload.kind === 'message.completed') {
            return {
              ...message,
              status: 'done',
              sources: payload.sources || message.sources,
            };
          }
          if (payload.kind === 'message.failed') {
            return { ...message, status: 'failed' };
          }
          return message;
        });
        return {
          ...current,
          updated_at: Math.floor(Date.now() / 1000),
          messages,
        };
      });

      if (payload.kind === 'message.completed' || payload.kind === 'message.failed') {
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
      assistants.find((assistant) => assistant.id === preferences?.preferred_assistant_id) ||
      assistants[0] ||
      null,
    [assistants, preferences?.preferred_assistant_id],
  );

  const activeRoleModelId = useMemo(
    () => resolveRoleModelId(roleBindings, preferences?.preferred_role || 'quick_assistant'),
    [preferences?.preferred_role, roleBindings],
  );

  const persistPreferences = async (next: QuickAssistantPreferences) => {
    setPreferences(next);
    setSaving(true);
    try {
      await workspaceQuickAssistantSave(next);
      setRuntimeError(null);
    } catch (error: any) {
      setRuntimeError(error?.toString?.() || String(error));
    } finally {
      setSaving(false);
    }
  };

  const ensureConversation = async () => {
    if (conversation) return conversation;
    const created = await workspaceConversationCreate({
      title: preferences?.prefer_assistant_mode && activeAssistant ? `${activeAssistant.name} Quick` : 'Quick Assistant',
      assistant_id: preferences?.prefer_assistant_mode ? activeAssistant?.id : undefined,
      model_override_id: preferences?.prefer_assistant_mode ? undefined : activeRoleModelId || undefined,
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
        assistant_id: preferences.prefer_assistant_mode ? activeAssistant?.id : undefined,
        model_override_id: preferences.prefer_assistant_mode ? undefined : activeRoleModelId || undefined,
        web_search_enabled: preferences.prefer_assistant_mode ? activeAssistant?.tool_policy.web_search : false,
      });
      setDraft('');
      const detail = await workspaceConversationGet(target.id);
      setConversation(detail);
    } catch (error: any) {
      setRuntimeError(error?.toString?.() || String(error));
      setSending(false);
    }
  };

  if (loading || !preferences) {
    return (
      <div className="flex h-screen items-center justify-center bg-background text-foreground">
        <div className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          Loading Quick Assistant...
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-screen flex-col overflow-hidden rounded-3xl border border-border bg-background text-foreground shadow-2xl">
      <div
        className="flex items-center justify-between gap-4 border-b bg-card/90 px-4 py-3"
        data-tauri-drag-region
        onMouseDown={() => {
          getCurrentWindow().startDragging().catch(() => {});
        }}
      >
        <div className="flex min-w-0 items-center gap-3">
          <div className="rounded-2xl bg-primary/10 p-2 text-primary">
            <Wand2 className="h-5 w-5" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold">Quick Assistant</div>
            <div className="truncate text-xs text-muted-foreground">
              {preferences.prefer_assistant_mode
                ? `助手模式 · ${activeAssistant?.name || '未选择助手'}`
                : `模型模式 · ${preferences.preferred_role}`}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {saving ? <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" /> : null}
          <button
            type="button"
            onClick={() => void hideQuickAssistantWindow()}
            className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
          >
            <Minimize2 className="h-4 w-4" />
            隐藏
          </button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 gap-0 lg:grid-cols-[280px,minmax(0,1fr)]">
        <aside className="border-r bg-card/60 p-4">
          <div className="space-y-4">
            <div className="rounded-2xl border bg-background p-4">
              <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">Launch Mode</div>
              <div className="mt-3 grid gap-2">
                <button
                  type="button"
                  onClick={() =>
                    void persistPreferences({
                      ...preferences,
                      prefer_assistant_mode: true,
                      preferred_assistant_id: activeAssistant?.id || assistants[0]?.id || null,
                    })
                  }
                  className={`rounded-xl border px-3 py-3 text-left ${preferences.prefer_assistant_mode ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'}`}
                >
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <Bot className="h-4 w-4" />
                    助手模式
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">继承助手的提示词、能力和默认模型</div>
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void persistPreferences({
                      ...preferences,
                      prefer_assistant_mode: false,
                    })
                  }
                  className={`rounded-xl border px-3 py-3 text-left ${!preferences.prefer_assistant_mode ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'}`}
                >
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <Sparkles className="h-4 w-4" />
                    模型模式
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">直接使用角色绑定的模型，不继承助手能力</div>
                </button>
              </div>
            </div>

            {preferences.prefer_assistant_mode ? (
              <div className="rounded-2xl border bg-background p-4">
                <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">Assistant</div>
                <select
                  value={activeAssistant?.id || ''}
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
                  {activeAssistant?.description || '当前助手还没有描述。'}
                </div>
              </div>
            ) : (
              <div className="rounded-2xl border bg-background p-4">
                <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">Model Role</div>
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
                  {QUICK_ASSISTANT_ROLES.map((item) => (
                    <option key={item.role} value={item.role}>
                      {item.label}
                    </option>
                  ))}
                </select>
                <div className="mt-3 rounded-xl border bg-muted/10 px-3 py-3 text-xs text-muted-foreground">
                  当前角色绑定模型：{activeRoleModelId || '未设置，请到 AI 连接中心绑定'}
                </div>
              </div>
            )}

            <label className="flex items-center justify-between rounded-2xl border bg-background px-4 py-3">
              <div>
                <div className="text-sm font-medium">打开时读取剪贴板</div>
                <div className="text-xs text-muted-foreground">适合快速处理选中的文本片段</div>
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
              <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">Hints</div>
              <div className="mt-3 space-y-2 text-xs text-muted-foreground">
                <div className="flex items-center gap-2">
                  <Clipboard className="h-3.5 w-3.5" />
                  适合快速改写、翻译、总结或补充说明。
                </div>
                <div className="flex items-center gap-2">
                  <Globe className="h-3.5 w-3.5" />
                  联网能力跟随助手策略或模型角色绑定。
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
                  <div className="text-base font-semibold">快速处理一个问题</div>
                  <div className="mt-2 text-sm text-muted-foreground">
                    这里会直接创建真实主题并保留消息流，你可以稍后回到 AI 工作台继续。
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
                placeholder="输入一句话，快速生成答复、改写、翻译或总结..."
                className="min-h-[110px] w-full resize-none bg-transparent text-sm leading-6 outline-none"
              />
              <div className="mt-3 flex items-center justify-between gap-3">
                <div className="text-xs text-muted-foreground">
                  {preferences.prefer_assistant_mode
                    ? `助手：${activeAssistant?.name || '未选择'}`
                    : `角色：${preferences.preferred_role}`}
                </div>
                <button
                  type="button"
                  onClick={() => void handleSend()}
                  disabled={!draft.trim() || sending}
                  className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {sending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
                  发送
                </button>
              </div>
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}
