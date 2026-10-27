export interface LauncherItem {
  id: string;
  name: string;
  type: "app" | "script" | "url" | "folder" | "internal";
  target: string;
  pinned: boolean;
  pin_order: number;
  launch_count: number;
  last_launched_at?: number;
  trusted: boolean;
  created_at: number;
  updated_at: number;
}

export interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

export interface LauncherItemInput {
  id?: string;
  name: string;
  type: LauncherItem["type"];
  target: string;
  pinned?: boolean;
  pin_order?: number;
  trusted?: boolean;
}

export type InternalTarget = {
  id: string;
  labelKey: string;
  fallback: string;
};

export type LauncherWindowBindings = typeof window & {
  setActiveTab?: (tab: string) => void;
  setSettingsTab?: (tab: string) => void;
};

import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

export type LauncherQuickToolEntry = {
  id: string;
  name: string;
  description: string;
  target: string;
  icon: LucideIcon;
  iconClassName: string;
  statusBadge: ReactNode;
  visible: boolean;
  aliasOnly?: boolean;
  searchText?: string;
};

export type LauncherDragState = {
  draggingId: string | null;
  handlePointerDown: (e: React.PointerEvent, id: string) => void;
  handlePointerUp: () => void;
  handleCardPointerOver: (targetId: string) => void;
};
