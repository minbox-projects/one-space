import {
  Rocket,
  Command,
  Globe,
  FolderOpen,
  Workflow,
} from "lucide-react";
import type { LauncherItem } from "./types";

export function sortLauncherItems(items: LauncherItem[]) {
  return [...items].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    if (a.pinned && b.pinned) return a.pin_order - b.pin_order;
    return (b.last_launched_at || 0) - (a.last_launched_at || 0);
  });
}

export function launcherIcon(type: LauncherItem["type"]) {
  if (type === "url") return Globe;
  if (type === "folder") return FolderOpen;
  if (type === "script") return Command;
  if (type === "internal") return Workflow;
  return Rocket;
}

export function formatInvokeError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const maybe = err as { message?: unknown; error?: unknown };
    if (typeof maybe.message === "string" && maybe.message.trim())
      return maybe.message;
    if (typeof maybe.error === "string" && maybe.error.trim())
      return maybe.error;
    try {
      return JSON.stringify(err);
    } catch (_e) {
      return String(err);
    }
  }
  return String(err);
}
