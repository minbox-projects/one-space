import { invoke } from "@tauri-apps/api/core";

export const MESSAGES_UPDATED_EVENT = "messages-updated";

export type MessageSeverity = "info" | "success" | "warning" | "error";

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

function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export async function listMessages() {
  if (!isTauriRuntime()) return [];
  return invoke<MessageRecord[]>("messages_list");
}

export async function getUnreadMessageCount() {
  if (!isTauriRuntime()) return 0;
  return invoke<number>("messages_unread_count");
}

export async function createMessage(input: MessageCreateInput) {
  if (!isTauriRuntime()) return null;
  return invoke<MessageRecord>("messages_create", { input });
}

export async function recordMessage(input: MessageCreateInput) {
  try {
    await createMessage(input);
  } catch (error) {
    console.error("Failed to record message", error);
  }
}

export async function safeRecordMessage(input: MessageCreateInput) {
  await recordMessage(input);
}

export async function markMessageRead(id: string) {
  if (!isTauriRuntime()) return;
  await invoke("messages_mark_read", { id });
}

export async function markAllMessagesRead() {
  if (!isTauriRuntime()) return;
  await invoke("messages_mark_all_read");
}

export function errorToMessage(error: unknown) {
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
