import type { LucideIcon } from "lucide-react";
import type { ToolStatusDotTone } from "../../toolbox/ToolStatusDot";
import type { ToolStatusTone } from "../../toolbox/ToolStatusBadge";
import { DEFAULT_TUNNEL_GROUP_ID } from "../types";
import type {
  SshTunnelForwardMode,
  SshTunnelGroupView,
  SshTunnelProbeResult,
  SshTunnelRuntimeView,
  SshTunnelStatus,
  SshTunnelView,
  SshTunnelsSnapshot,
} from "../types";

export const ALL_CONNECTED_TAB_ID = "__connected__";

export function parseOptionalPort(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number.parseInt(trimmed, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

export const SAVED_PROBE_TTL_MS = 5 * 60 * 1000;

export type SavedProbeEntry = {
  result: SshTunnelProbeResult;
  expiresAt: number;
};

export function isTunnelsSnapshot(payload: unknown): payload is SshTunnelsSnapshot {
  if (!payload || typeof payload !== "object") return false;
  const candidate = payload as SshTunnelsSnapshot;
  return (
    Array.isArray(candidate.groups) &&
    Array.isArray(candidate.tunnels) &&
    Array.isArray(candidate.runtime)
  );
}

export function statusTone(status: SshTunnelStatus): ToolStatusTone {
  switch (status) {
    case "connected":
      return "success";
    case "connecting":
    case "reconnecting":
      return "warning";
    case "error":
      return "error";
    default:
      return "neutral";
  }
}

export function tunnelErrorDisplay(
  runtime: SshTunnelRuntimeView | undefined,
  tunnel: SshTunnelView,
): { text: string; tone: "error" | "reconnecting" } | null {
  const status = runtime?.status;
  if (status !== "error" && status !== "reconnecting") {
    return null;
  }
  const text = runtime?.last_error || tunnel.last_error;
  if (!text) {
    return null;
  }
  return {
    text,
    tone: status === "reconnecting" ? "reconnecting" : "error",
  };
}

export function modeShort(mode: SshTunnelForwardMode) {
  if (mode === "local") return "L";
  if (mode === "remote") return "R";
  return "D";
}

export function normalizeTunnelGroupId(groupId?: string | null) {
  const trimmed = groupId?.trim();
  return trimmed ? trimmed : DEFAULT_TUNNEL_GROUP_ID;
}

export function normalizeTunnel(tunnel: SshTunnelView): SshTunnelView {
  return {
    ...tunnel,
    group_id: normalizeTunnelGroupId(tunnel.group_id),
    auto_reconnect: tunnel.auto_reconnect ?? true,
  };
}

export function ensureDefaultGroup(groups: SshTunnelGroupView[]) {
  if (groups.some((group) => group.is_default || group.id === DEFAULT_TUNNEL_GROUP_ID)) {
    return groups;
  }
  return [
    {
      id: DEFAULT_TUNNEL_GROUP_ID,
      name: "Default Group",
      created_at: 0,
      updated_at: 0,
      is_default: true,
    },
    ...groups,
  ];
}

export function sortTunnelGroups(groups: SshTunnelGroupView[]) {
  return [...groups].sort((a, b) => {
    if (a.is_default && !b.is_default) return -1;
    if (!a.is_default && b.is_default) return 1;
    if (a.created_at !== b.created_at) return a.created_at - b.created_at;
    return a.name.localeCompare(b.name);
  });
}

export function mapRuntimeById(runtime: SshTunnelRuntimeView[]) {
  return runtime.reduce<Record<string, SshTunnelRuntimeView>>((acc, item) => {
    acc[item.id] = item;
    return acc;
  }, {});
}

export type TunnelBusyAction = "probe" | "connect" | "disconnect" | "delete";

export type SshTunnelModeCard = {
  id: SshTunnelForwardMode;
  icon: LucideIcon;
  title: string;
  description: string;
  example: string;
};

export type SshTunnelGroupStatus = {
  tone: ToolStatusDotTone | null;
  connectedCount: number;
  errorCount: number;
  connectingCount: number;
  tooltip: string;
};

export type SshTunnelConnectedSection = {
  group: SshTunnelGroupView;
  tunnels: SshTunnelView[];
};
