import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  AlertTriangle,
  Bell,
  CheckCircle2,
  Clock,
  ExternalLink,
  Info,
  MailOpen,
  RefreshCw,
  X,
} from "lucide-react";
import {
  listMessages,
  markAllMessagesRead,
  markMessageRead,
  MESSAGES_UPDATED_EVENT,
  type MessageRecord,
  type MessageSeverity,
  type MessageTarget,
} from "@/lib/messages";

type MessageCenterProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onNavigate: (target: MessageTarget) => void;
};

const SOURCE_LABELS: Record<string, string> = {
  ai_news: "AI News",
  ssh_tunnels: "SSH Tunnels",
  sync: "Sync",
  system: "System",
  updater: "Updates",
  launcher: "Launcher",
  ai_environments: "AI Environments",
  skills: "Skills",
  subagents: "Subagents",
  mcp_servers: "MCP",
  workflows: "Workflows",
  settings: "Settings",
  mail: "Mail",
  backup: "Backup",
  workspaces: "Workspaces",
  content: "Content",
};

function severityIcon(severity: MessageSeverity) {
  if (severity === "success") return CheckCircle2;
  if (severity === "warning") return AlertTriangle;
  if (severity === "error") return AlertCircle;
  return Info;
}

function severityClass(severity: MessageSeverity) {
  if (severity === "success") return "text-emerald-600 bg-emerald-500/10";
  if (severity === "warning") return "text-amber-600 bg-amber-500/10";
  if (severity === "error") return "text-destructive bg-destructive/10";
  return "text-primary bg-primary/10";
}

function formatTimestamp(ts?: number | null) {
  if (!ts || !Number.isFinite(ts)) return "-";
  return new Date(ts * 1000).toLocaleString();
}

function formatRelativeTime(ts: number, language: string) {
  if (!Number.isFinite(ts) || ts <= 0) return "-";
  const diff = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  const zh = language.startsWith("zh");
  if (diff < 60) return zh ? "刚刚" : "just now";
  if (diff < 3600) {
    const value = Math.floor(diff / 60);
    return zh ? `${value} 分钟前` : `${value}m ago`;
  }
  if (diff < 86400) {
    const value = Math.floor(diff / 3600);
    return zh ? `${value} 小时前` : `${value}h ago`;
  }
  const value = Math.floor(diff / 86400);
  return zh ? `${value} 天前` : `${value}d ago`;
}

function sourceLabel(source: string) {
  return SOURCE_LABELS[source] || source || "System";
}

export function MessageCenter({
  open,
  onOpenChange,
  onNavigate,
}: MessageCenterProps) {
  const { t, i18n } = useTranslation();
  const [messages, setMessages] = useState<MessageRecord[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedMessage = useMemo(
    () => messages.find((message) => message.id === selectedId) || null,
    [messages, selectedId],
  );

  const unreadCount = useMemo(
    () => messages.filter((message) => !message.read_at).length,
    [messages],
  );

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
    if (!("__TAURI_INTERNALS__" in window)) return;
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

  const handleMarkAllRead = async () => {
    try {
      await markAllMessagesRead();
      const readAt = Math.floor(Date.now() / 1000);
      setMessages((prev) =>
        prev.map((message) => ({ ...message, read_at: message.read_at || readAt })),
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const renderMessageMetadata = (metadata: unknown) => {
    if (!metadata) return null;
    let text = "";
    try {
      text = JSON.stringify(metadata, null, 2);
    } catch {
      text = String(metadata);
    }
    if (!text || text === "null") return null;
    return (
      <div className="space-y-2">
        <h4 className="text-xs font-semibold uppercase tracking-[0.16em] text-muted-foreground">
          {t("messageMetadata", "Metadata")}
        </h4>
        <pre className="max-h-44 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted p-3 text-xs text-muted-foreground select-text">
          {text}
        </pre>
      </div>
    );
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[110]">
      <button
        type="button"
        className="absolute inset-0 cursor-default bg-black/20 backdrop-blur-[1px]"
        aria-label={t("close", "Close")}
        onClick={() => onOpenChange(false)}
      />
      <aside className="absolute right-0 top-0 flex h-full w-full max-w-[500px] flex-col border-l bg-background shadow-2xl animate-in slide-in-from-right duration-200">
        <div className="flex h-16 items-end justify-between border-b px-5 pb-2">
          <div className="flex items-center gap-3">
            <div className="flex h-9 w-9 items-center justify-center rounded-full bg-primary/10 text-primary">
              <Bell className="h-4 w-4" />
            </div>
            <div>
              <h2 className="text-sm font-semibold">
                {t("messageCenter", "消息中心")}
              </h2>
              <p className="text-xs text-muted-foreground">
                {unreadCount > 0
                  ? t("messageCenterUnreadCount", "{{count}} unread", {
                      count: unreadCount,
                    })
                  : t("messageCenterAllRead", "All caught up")}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => void reloadMessages()}
              className="rounded-md p-2 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              title={t("refresh", "Refresh")}
            >
              <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
            </button>
            <button
              type="button"
              onClick={() => void handleMarkAllRead()}
              disabled={unreadCount === 0}
              className="rounded-md p-2 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40"
              title={t("markAllRead", "Mark all as read")}
            >
              <MailOpen className="h-4 w-4" />
            </button>
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="rounded-md p-2 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              title={t("close", "Close")}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>

        {error ? (
          <div className="mx-5 mt-4 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        ) : null}

        <div className="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)_auto]">
          <div className="min-h-0 overflow-y-auto p-4">
            {loading && messages.length === 0 ? (
              <div className="text-sm text-muted-foreground">
                {t("loading", "Loading...")}
              </div>
            ) : null}

            {!loading && messages.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center text-center text-muted-foreground">
                <Bell className="mb-3 h-10 w-10 opacity-30" />
                <p className="text-sm font-medium">
                  {t("messageCenterEmpty", "No messages yet.")}
                </p>
                <p className="mt-1 max-w-xs text-xs">
                  {t(
                    "messageCenterEmptyHint",
                    "Important background results and failures will appear here.",
                  )}
                </p>
              </div>
            ) : null}

            <div className="space-y-2">
              {messages.map((message) => {
                const Icon = severityIcon(message.severity);
                const selected = selectedId === message.id;
                const unread = !message.read_at;
                return (
                  <button
                    key={message.id}
                    type="button"
                    onClick={() => setSelectedId(message.id)}
                    className={`w-full rounded-xl border p-3 text-left transition-colors ${
                      selected
                        ? "border-primary/40 bg-primary/5"
                        : "bg-card hover:border-primary/30 hover:bg-muted/30"
                    }`}
                  >
                    <div className="flex items-start gap-3">
                      <div className="pt-1">
                        <span
                          className={`block h-2 w-2 rounded-full ${
                            unread ? "bg-primary" : "bg-transparent"
                          }`}
                        />
                      </div>
                      <div
                        className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full ${severityClass(message.severity)}`}
                      >
                        <Icon className="h-4 w-4" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="mb-1 flex items-center gap-2">
                          <span className="rounded-full border bg-muted/40 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                            {t(
                              `messageSource_${message.source}`,
                              sourceLabel(message.source),
                            )}
                          </span>
                          <span className="flex items-center gap-1 text-[10px] text-muted-foreground">
                            <Clock className="h-3 w-3" />
                            {formatRelativeTime(message.last_seen_at, i18n.language)}
                          </span>
                          {message.occurrences > 1 ? (
                            <span className="rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                              x{message.occurrences}
                            </span>
                          ) : null}
                        </div>
                        <div
                          className={`truncate text-sm ${unread ? "font-semibold" : "font-medium"}`}
                        >
                          {message.title}
                        </div>
                        {message.summary ? (
                          <div className="mt-1 line-clamp-1 text-xs text-muted-foreground">
                            {message.summary}
                          </div>
                        ) : null}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {selectedMessage ? (
            <div className="max-h-[45vh] overflow-y-auto border-t bg-card/70 p-5">
              <div className="mb-4 flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="mb-2 flex flex-wrap items-center gap-2">
                    <span className="rounded-full border bg-background px-2 py-0.5 text-xs text-muted-foreground">
                      {t(
                        `messageSource_${selectedMessage.source}`,
                        sourceLabel(selectedMessage.source),
                      )}
                    </span>
                    <span
                      className={`rounded-full px-2 py-0.5 text-xs font-medium ${severityClass(selectedMessage.severity)}`}
                    >
                      {t(
                        `messageSeverity_${selectedMessage.severity}`,
                        selectedMessage.severity,
                      )}
                    </span>
                  </div>
                  <h3 className="text-base font-semibold leading-snug">
                    {selectedMessage.title}
                  </h3>
                </div>
                <button
                  type="button"
                  onClick={() => setSelectedId(null)}
                  className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  title={t("close", "Close")}
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className="space-y-4 text-sm">
                <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                  <div>
                    <span className="font-medium">
                      {t("createdAt", "Created")}
                    </span>
                    <div>{formatTimestamp(selectedMessage.created_at)}</div>
                  </div>
                  <div>
                    <span className="font-medium">
                      {t("lastSeenAt", "Last seen")}
                    </span>
                    <div>{formatTimestamp(selectedMessage.last_seen_at)}</div>
                  </div>
                </div>

                {selectedMessage.summary ? (
                  <p className="whitespace-pre-wrap break-words text-foreground">
                    {selectedMessage.summary}
                  </p>
                ) : null}

                {selectedMessage.detail ? (
                  <pre className="max-h-52 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted p-3 text-xs text-muted-foreground select-text">
                    {selectedMessage.detail}
                  </pre>
                ) : null}

                {renderMessageMetadata(selectedMessage.metadata)}

                {selectedMessage.target?.tab ? (
                  <button
                    type="button"
                    onClick={() => onNavigate(selectedMessage.target!)}
                    className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
                  >
                    <ExternalLink className="h-4 w-4" />
                    {t("openRelatedFeature", "打开相关功能")}
                  </button>
                ) : null}
              </div>
            </div>
          ) : null}
        </div>
      </aside>
    </div>
  );
}
