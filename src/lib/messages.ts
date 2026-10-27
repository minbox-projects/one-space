import { invoke } from "@tauri-apps/api/core";

export const MESSAGES_UPDATED_EVENT = "messages-updated";

export type MessageSeverity = "info" | "success" | "warning" | "error";

export type MessageSource =
  | "ai_news"
  | "ssh_tunnels"
  | "sync"
  | "system"
  | "updater"
  | "launcher"
  | "ai_environments"
  | "skills"
  | "subagents"
  | "mcp_servers"
  | "settings"
  | "mail"
  | "backup"
  | "workspaces"
  | "content"
  | "ai_gateway"
  | "uptime_monitor"
  | "server_monitor"
  | "ssl_certificate"
  | "snippets"
  | "notes"
  | "bookmarks"
  | "file_sharing"
  | (string & {});

export interface MessageTarget {
  tab: string;
  section?: string | null;
  entity_id?: string | null;
}

export interface MessageRecord {
  id: string;
  source: string;
  category: string;
  severity: MessageSeverity;
  title: string;
  summary?: string | null;
  detail?: string | null;
  created_at: number;
  read_at?: number | null;
  dedupe_key?: string | null;
  occurrences: number;
  last_seen_at: number;
  target?: MessageTarget | null;
  metadata?: unknown;
}

export interface MessageCreateInput {
  source: string;
  category: string;
  severity: MessageSeverity;
  title: string;
  summary?: string | null;
  detail?: string | null;
  dedupe_key?: string | null;
  target?: MessageTarget | null;
  metadata?: unknown;
}

export interface AppMessageInput {
  source: MessageSource;
  category: string;
  severity: MessageSeverity;
  title: string;
  summary?: string | null;
  detail?: string | null;
  error?: unknown;
  dedupeKey?: string | null;
  dedupe_key?: string | null;
  target?: MessageTarget | null;
  metadata?: unknown;
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function errorToMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.stack || error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  try {
    return JSON.stringify(error, null, 2) || String(error);
  } catch {
    return String(error);
  }
}

function extractDetail(detail?: string | null, error?: unknown): string | null {
  if (detail && detail.trim()) return detail;
  if (error !== undefined && error !== null) {
    return errorToMessage(error);
  }
  return null;
}

function extractSummary(
  summary?: string | null,
  detail?: string | null,
  error?: unknown,
): string | null {
  if (summary && summary.trim()) return summary.trim();
  if (error !== undefined && error !== null) {
    const raw = error instanceof Error && error.message ? error.message : errorToMessage(error);
    const firstLine = raw
      .split("\n")
      .map((s) => s.trim())
      .find(Boolean);
    if (firstLine) return firstLine;
  }
  if (detail) {
    const firstLine = detail
      .split("\n")
      .map((s) => s.trim())
      .find(Boolean);
    if (firstLine) return firstLine;
  }
  return null;
}

function deriveDedupeKey(input: AppMessageInput): string | null {
  if (input.dedupeKey) return input.dedupeKey;
  if (input.dedupe_key) return input.dedupe_key;
  if (input.source && input.category) {
    return `${input.source}:${input.category}:${input.severity}`;
  }
  return null;
}

export async function listMessages(): Promise<MessageRecord[]> {
  if (!isTauriRuntime()) return [];
  return invoke<MessageRecord[]>("messages_list");
}

export async function getUnreadMessageCount(): Promise<number> {
  if (!isTauriRuntime()) return 0;
  return invoke<number>("messages_unread_count");
}

export async function createMessage(
  input: MessageCreateInput,
): Promise<MessageRecord | null> {
  if (!isTauriRuntime()) return null;
  return invoke<MessageRecord>("messages_create", { input });
}

export async function recordAppMessage(
  input: AppMessageInput,
): Promise<MessageRecord | null> {
  try {
    const detail = extractDetail(input.detail, input.error);
    const summary = extractSummary(input.summary, detail, input.error);
    const dedupe_key = deriveDedupeKey(input);

    const payload: MessageCreateInput = {
      source: input.source,
      category: input.category,
      severity: input.severity,
      title: input.title,
      summary,
      detail,
      dedupe_key,
      target: input.target ?? null,
      metadata: input.metadata,
    };
    return await createMessage(payload);
  } catch (error) {
    console.error("Failed to record app message", error);
    return null;
  }
}

export const notifyMessage = {
  error: (input: Omit<AppMessageInput, "severity">) =>
    recordAppMessage({ ...input, severity: "error" }),
  warn: (input: Omit<AppMessageInput, "severity">) =>
    recordAppMessage({ ...input, severity: "warning" }),
  info: (input: Omit<AppMessageInput, "severity">) =>
    recordAppMessage({ ...input, severity: "info" }),
  success: (input: Omit<AppMessageInput, "severity">) =>
    recordAppMessage({ ...input, severity: "success" }),
};

export async function recordMessage(input: MessageCreateInput) {
  await recordAppMessage(input);
}

export async function safeRecordMessage(input: MessageCreateInput) {
  await recordAppMessage(input);
}

export async function markMessageRead(id: string): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("messages_mark_read", { id });
}

export async function markAllMessagesRead(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("messages_mark_all_read");
}
