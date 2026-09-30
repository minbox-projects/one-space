import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertCircle,
  Globe,
  KeyRound,
  Link2,
  Loader2,
  MoreHorizontal,
  Network,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Save,
  Server,
  ShieldCheck,
  Trash2,
  Unplug,
  Waypoints,
} from "lucide-react";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import { useToast } from "./ToastProvider";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import { Switch } from "./ui/switch";
import { SshTunnelGroupManagerDialog } from "./sshTunnels/SshTunnelGroupManagerDialog";
import { errorToMessage, safeRecordMessage } from "@/lib/messages";
import {
  notifyActionResult,
  runUserAction,
} from "@/lib/userActions";
import {
  buildConnectTunnelActionDescriptor,
  buildDeleteTunnelActionDescriptor,
  buildDisconnectTunnelActionDescriptor,
} from "@/lib/actionDescriptors/sshTunnels";
import {
  DEFAULT_TUNNEL_FORM,
  DEFAULT_TUNNEL_GROUP_ID,
  type SshTunnelForwardMode,
  type SshHost,
  type SshTunnelGroupView,
  type SshTunnelProbeResult,
  type SshTunnelsSnapshot,
  type SshTunnelRuntimeView,
  type SshTunnelStatus,
  type SshTunnelView,
  type TunnelFormState,
  type SshTunnelBatchOperationResult,
} from "./sshTunnels/types";
import { localizeSshTunnelError } from "../lib/sshTunnelI18n";
import { getToolboxTool } from "@/toolbox/registry";
import { useTauriEvent } from "@/toolbox/useTauriEvent";
import { useVisibleInterval } from "@/toolbox/useVisibleInterval";
import { ToolStatusBadge, type ToolStatusTone } from "./toolbox/ToolStatusBadge";
import { ToolStatusDot, type ToolStatusDotTone } from "./toolbox/ToolStatusDot";
import { ToolEmptyState } from "./toolbox/ToolEmptyState";
import {
  sshHostsList,
  sshTunnelConnect,
  sshTunnelDelete,
  sshTunnelDisconnect,
  sshTunnelGroupConnect,
  sshTunnelGroupDelete,
  sshTunnelGroupDisconnect,
  sshTunnelGroupUpsert,
  sshTunnelProbeDraft,
  sshTunnelProbeSaved,
  sshTunnelsDisconnectAll,
  sshTunnelsRefreshStatus,
  sshTunnelsSnapshot,
  sshTunnelUpsert,
} from "@/lib/sshTunnels";

const ALL_CONNECTED_TAB_ID = "__connected__";

function parseOptionalPort(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number.parseInt(trimmed, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

const SAVED_PROBE_TTL_MS = 5 * 60 * 1000;

type SavedProbeEntry = {
  result: SshTunnelProbeResult;
  expiresAt: number;
};

function isTunnelsSnapshot(payload: unknown): payload is SshTunnelsSnapshot {
  if (!payload || typeof payload !== "object") return false;
  const candidate = payload as SshTunnelsSnapshot;
  return (
    Array.isArray(candidate.groups) &&
    Array.isArray(candidate.tunnels) &&
    Array.isArray(candidate.runtime)
  );
}

function statusTone(status: SshTunnelStatus): ToolStatusTone {
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

function tunnelErrorDisplay(
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

function modeShort(mode: SshTunnelForwardMode) {
  if (mode === "local") return "L";
  if (mode === "remote") return "R";
  return "D";
}

function normalizeTunnelGroupId(groupId?: string | null) {
  const trimmed = groupId?.trim();
  return trimmed ? trimmed : DEFAULT_TUNNEL_GROUP_ID;
}

function normalizeTunnel(tunnel: SshTunnelView): SshTunnelView {
  return {
    ...tunnel,
    group_id: normalizeTunnelGroupId(tunnel.group_id),
    auto_reconnect: tunnel.auto_reconnect ?? true,
  };
}

function ensureDefaultGroup(groups: SshTunnelGroupView[]) {
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

function sortTunnelGroups(groups: SshTunnelGroupView[]) {
  return [...groups].sort((a, b) => {
    if (a.is_default && !b.is_default) return -1;
    if (!a.is_default && b.is_default) return 1;
    if (a.created_at !== b.created_at) return a.created_at - b.created_at;
    return a.name.localeCompare(b.name);
  });
}

function mapRuntimeById(runtime: SshTunnelRuntimeView[]) {
  return runtime.reduce<Record<string, SshTunnelRuntimeView>>((acc, item) => {
    acc[item.id] = item;
    return acc;
  }, {});
}

type TunnelBusyAction = "probe" | "connect" | "disconnect" | "delete";

export function SshTunnels({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const { pushToast } = useToast();
  const { icon: ToolIcon, iconClassName } = getToolboxTool("ssh-tunnels")!;
  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [groups, setGroups] = useState<SshTunnelGroupView[]>([]);
  const [activeGroupId, setActiveGroupId] = useState(DEFAULT_TUNNEL_GROUP_ID);
  const [tunnels, setTunnels] = useState<SshTunnelView[]>([]);
  const [runtimeMap, setRuntimeMap] = useState<Record<string, SshTunnelRuntimeView>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<TunnelFormState>(DEFAULT_TUNNEL_FORM);
  const [editorOpen, setEditorOpen] = useState(false);
  const [groupManagerOpen, setGroupManagerOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [groupSubmitting, setGroupSubmitting] = useState(false);
  const [draftProbe, setDraftProbe] = useState<SshTunnelProbeResult | null>(null);
  const [savedProbeMap, setSavedProbeMap] = useState<Record<string, SavedProbeEntry>>({});
  const [busyAction, setBusyAction] = useState<{
    id: string;
    kind: TunnelBusyAction;
  } | null>(null);
  const [groupBusyAction, setGroupBusyAction] = useState<"connect" | "disconnect" | null>(null);
  const [groupMenuOpen, setGroupMenuOpen] = useState(false);
  const [openActionMenuId, setOpenActionMenuId] = useState<string | null>(null);
  const latestLoadRequestId = useRef(0);

  const isTauri = "__TAURI_INTERNALS__" in window;

  const modeCards = useMemo(
    () => [
      {
        id: "local" as const,
        icon: Link2,
        title: t("sshTunnelModeLocalTitle", "Local (-L)"),
        description: t(
          "sshTunnelModeLocalDesc",
          "Expose a remote service on a local port so local apps can connect to it.",
        ),
        example: t(
          "sshTunnelModeLocalExample",
          "Example: map remote PostgreSQL to 127.0.0.1:5432 on this device.",
        ),
      },
      {
        id: "remote" as const,
        icon: Waypoints,
        title: t("sshTunnelModeRemoteTitle", "Remote (-R)"),
        description: t(
          "sshTunnelModeRemoteDesc",
          "Expose a service running on this device to the SSH server through a remote port.",
        ),
        example: t(
          "sshTunnelModeRemoteExample",
          "Example: let the SSH server reach 127.0.0.1:7777 on this device through remote port 7777.",
        ),
      },
      {
        id: "dynamic" as const,
        icon: Globe,
        title: t("sshTunnelModeDynamicTitle", "Dynamic (-D)"),
        description: t(
          "sshTunnelModeDynamicDesc",
          "Create a local SOCKS5 proxy that can reach arbitrary destinations through SSH.",
        ),
        example: t(
          "sshTunnelModeDynamicExample",
          "Example: configure a browser or CLI to use 127.0.0.1:1080 as a SOCKS5 proxy.",
        ),
      },
    ],
    [t],
  );

  const groupsById = useMemo(
    () => new Map(groups.map((group) => [group.id, group])),
    [groups],
  );

  const isConnectedView = activeGroupId === ALL_CONNECTED_TAB_ID;

  const groupStatusMap = useMemo(() => {
    const result: Record<
      string,
      {
        tone: ToolStatusDotTone | null;
        connectedCount: number;
        errorCount: number;
        connectingCount: number;
        tooltip: string;
      }
    > = {};

    for (const group of groups) {
      const groupTunnels = tunnels.filter(
        (tunnel) => normalizeTunnelGroupId(tunnel.group_id) === group.id,
      );
      let connectedCount = 0;
      let errorCount = 0;
      let connectingCount = 0;

      for (const tunnel of groupTunnels) {
        const st = runtimeMap[tunnel.id]?.status;
        if (st === "connected") connectedCount++;
        else if (st === "error") errorCount++;
        else if (st === "connecting" || st === "reconnecting") connectingCount++;
      }

      let tone: ToolStatusDotTone | null = null;
      if (errorCount > 0) {
        tone = "error";
      } else if (connectingCount > 0) {
        tone = "warning";
      } else if (connectedCount > 0) {
        tone = "success";
      }

      let tooltip = "";
      if (connectedCount > 0 && errorCount > 0) {
        tooltip = t("sshTunnelGroupStatusTooltipMixed", {
          connected: connectedCount,
          errors: errorCount,
          defaultValue: `${connectedCount} connected, ${errorCount} failed`,
        });
      } else if (errorCount > 0) {
        tooltip = t("sshTunnelGroupStatusTooltipError", {
          errors: errorCount,
          defaultValue: `${errorCount} failed`,
        });
      } else if (connectingCount > 0) {
        tooltip = t("sshTunnelGroupStatusTooltipConnecting", "Connecting...");
      } else if (connectedCount > 0) {
        tooltip = t("sshTunnelGroupStatusTooltipConnected", {
          connected: connectedCount,
          defaultValue: `${connectedCount} connected`,
        });
      }

      result[group.id] = {
        tone,
        connectedCount,
        errorCount,
        connectingCount,
        tooltip,
      };
    }

    return result;
  }, [groups, tunnels, runtimeMap, t]);

  const connectedTunnels = useMemo(() => {
    return tunnels.filter((tunnel) => {
      const st = runtimeMap[tunnel.id]?.status;
      return (
        st === "connected" ||
        st === "connecting" ||
        st === "reconnecting" ||
        st === "error"
      );
    });
  }, [tunnels, runtimeMap]);

  const connectedViewTone: ToolStatusDotTone | null = useMemo(() => {
    let hasError = false;
    let hasConnecting = false;
    let hasConnected = false;
    for (const t of connectedTunnels) {
      const st = runtimeMap[t.id]?.status;
      if (st === "error") hasError = true;
      else if (st === "connecting" || st === "reconnecting") hasConnecting = true;
      else if (st === "connected") hasConnected = true;
    }
    if (hasError) return "error";
    if (hasConnecting) return "warning";
    if (hasConnected) return "success";
    return null;
  }, [connectedTunnels, runtimeMap]);

  const visibleTunnels = useMemo(() => {
    if (isConnectedView) {
      return connectedTunnels;
    }
    return tunnels.filter(
      (tunnel) => normalizeTunnelGroupId(tunnel.group_id) === activeGroupId,
    );
  }, [isConnectedView, connectedTunnels, tunnels, activeGroupId]);

  const connectedGroupSections = useMemo(() => {
    if (!isConnectedView) {
      return [];
    }

    const groupMap = new Map<string, SshTunnelView[]>();
    for (const tunnel of connectedTunnels) {
      const gid = normalizeTunnelGroupId(tunnel.group_id);
      const list = groupMap.get(gid) ?? [];
      list.push(tunnel);
      groupMap.set(gid, list);
    }

    const sections: {
      group: SshTunnelGroupView;
      tunnels: SshTunnelView[];
    }[] = [];

    for (const group of groups) {
      const groupTunnels = groupMap.get(group.id);
      if (groupTunnels && groupTunnels.length > 0) {
        sections.push({
          group,
          tunnels: groupTunnels,
        });
        groupMap.delete(group.id);
      }
    }

    if (groupMap.size > 0) {
      for (const [gid, fallbackTunnels] of groupMap.entries()) {
        sections.push({
          group: {
            id: gid,
            name: gid,
            created_at: 0,
            updated_at: 0,
            is_default: false,
          },
          tunnels: fallbackTunnels,
        });
      }
    }

    return sections;
  }, [isConnectedView, connectedTunnels, groups]);

  const getGroupLabel = (groupId?: string | null) => {
    if (!groupId || groupId === DEFAULT_TUNNEL_GROUP_ID) {
      return t("sshTunnelDefaultGroup");
    }
    const group = groupsById.get(groupId);
    return group?.is_default
      ? t("sshTunnelDefaultGroup")
      : group?.name || t("sshTunnelDefaultGroup");
  };

  const getStatusLabel = (status: SshTunnelStatus) => {
    switch (status) {
      case "connected":
        return t("sshTunnelStatusConnected", "Connected");
      case "connecting":
        return t("sshTunnelStatusConnecting", "Connecting");
      case "reconnecting":
        return t("sshTunnelStatusReconnecting", "Reconnecting");
      case "error":
        return t("sshTunnelStatusError", "Error");
      default:
        return t("sshTunnelStatusDisconnected", "Disconnected");
    }
  };

  const getBusyOverlayLabel = (kind: Exclude<TunnelBusyAction, "delete">) => {
    switch (kind) {
      case "connect":
        return t("sshTunnelConnectingOverlay", "Connecting tunnel...");
      case "disconnect":
        return t("sshTunnelDisconnectingOverlay", "Disconnecting tunnel...");
      default:
        return t("sshTunnelCheckingOverlay", "Checking tunnel...");
    }
  };

  const hydrateForm = (tunnel?: SshTunnelView | null) => {
    if (!tunnel) {
      setForm(DEFAULT_TUNNEL_FORM);
      setDraftProbe(null);
      return;
    }
    setForm({
      id: tunnel.id,
      name: tunnel.name,
      group_id:
        normalizeTunnelGroupId(tunnel.group_id) === DEFAULT_TUNNEL_GROUP_ID
          ? ""
          : normalizeTunnelGroupId(tunnel.group_id),
      source_kind: tunnel.source_kind,
      saved_host_name: tunnel.saved_host_name || "",
      custom_host: tunnel.custom?.host || "",
      custom_port: String(tunnel.custom?.port ?? 22),
      custom_user: tunnel.custom?.user || "root",
      custom_auth_kind: tunnel.custom?.auth_kind || "password",
      custom_key_path: tunnel.custom?.key_path || "",
      custom_password: "",
      preserve_password: tunnel.custom?.has_password || false,
      forward_mode: tunnel.forward.mode,
      local_port: String(tunnel.forward.local_port ?? ""),
      remote_bind_host: tunnel.forward.remote_bind_host || "127.0.0.1",
      remote_port: String(tunnel.forward.remote_port ?? ""),
      target_host: tunnel.forward.target_host || "127.0.0.1",
      target_port: String(tunnel.forward.target_port ?? ""),
      dynamic_probe_host: tunnel.forward.dynamic_probe_host || "",
      dynamic_probe_port: String(tunnel.forward.dynamic_probe_port ?? ""),
      auto_connect: tunnel.auto_connect,
      auto_reconnect: tunnel.auto_reconnect ?? true,
    });
    setDraftProbe(null);
  };

  const resetEditor = () => {
    setForm(DEFAULT_TUNNEL_FORM);
    setDraftProbe(null);
    setEditorOpen(false);
  };

  const buildPayload = () => ({
    id: form.id,
    name: form.name.trim(),
    group_id: form.group_id.trim() || undefined,
    source_kind: form.source_kind,
    saved_host_name:
      form.source_kind === "saved_host" ? form.saved_host_name.trim() : undefined,
    custom:
      form.source_kind === "custom"
        ? {
            host: form.custom_host.trim(),
            port: parseOptionalPort(form.custom_port) ?? 0,
            user: form.custom_user.trim(),
            auth_kind: form.custom_auth_kind,
            key_path:
              form.custom_auth_kind === "key"
                ? form.custom_key_path.trim() || undefined
                : undefined,
            password:
              form.custom_auth_kind === "password" ? form.custom_password : undefined,
            preserve_password:
              form.custom_auth_kind === "password" ? form.preserve_password : false,
          }
        : undefined,
    forward: {
      mode: form.forward_mode,
      local_port: parseOptionalPort(form.local_port),
      remote_bind_host:
        form.forward_mode === "remote" ? form.remote_bind_host.trim() || undefined : undefined,
      remote_port: parseOptionalPort(form.remote_port),
      target_host:
        form.forward_mode === "dynamic" ? undefined : form.target_host.trim() || undefined,
      target_port:
        form.forward_mode === "dynamic"
          ? undefined
          : parseOptionalPort(form.target_port),
      dynamic_probe_host:
        form.forward_mode === "dynamic"
          ? form.dynamic_probe_host.trim() || undefined
          : undefined,
      dynamic_probe_port:
        form.forward_mode === "dynamic"
          ? parseOptionalPort(form.dynamic_probe_port)
          : undefined,
    },
    auto_connect: form.auto_connect,
    auto_reconnect: form.auto_reconnect,
  });

  const refreshStatuses = async () => {
    if (!isTauri) return;
    try {
      const runtime = await sshTunnelsRefreshStatus<SshTunnelRuntimeView[]>();
      setRuntimeMap(mapRuntimeById(runtime));
    } catch (refreshError) {
      setError(formatTunnelError(refreshError));
    }
  };

  const getSavedProbe = (id: string): SshTunnelProbeResult | null => {
    const entry = savedProbeMap[id];
    if (!entry || entry.expiresAt <= Date.now()) {
      return null;
    }
    return entry.result;
  };

  const applySnapshot = (snapshot: SshTunnelsSnapshot) => {
    setGroups(sortTunnelGroups(ensureDefaultGroup(snapshot.groups)));
    setTunnels(snapshot.tunnels.map(normalizeTunnel));
    setRuntimeMap(mapRuntimeById(snapshot.runtime));
  };

  const loadData = async () => {
    if (!isTauri) {
      setError(t("notInTauri", "This feature is only available inside the desktop app."));
      setLoading(false);
      return;
    }
    const requestId = ++latestLoadRequestId.current;
    try {
      setLoading(true);
      setError(null);
      const [loadedHosts, loadedSnapshot] = await Promise.allSettled([
        sshHostsList<SshHost[]>(),
        sshTunnelsSnapshot<SshTunnelsSnapshot>(),
      ]);

      const errors: string[] = [];

      if (loadedHosts.status === "fulfilled") {
        if (requestId === latestLoadRequestId.current) {
          setHosts(loadedHosts.value);
        }
      } else {
        errors.push(formatTunnelError(loadedHosts.reason));
      }

      if (loadedSnapshot.status === "fulfilled") {
        if (requestId === latestLoadRequestId.current) {
          applySnapshot(loadedSnapshot.value);
        }
      } else {
        errors.push(formatTunnelError(loadedSnapshot.reason));
      }

      if (requestId === latestLoadRequestId.current && errors.length > 0) {
        setError(errors.join("\n"));
      }
    } catch (err) {
      if (requestId === latestLoadRequestId.current) {
        setError(formatTunnelError(err));
      }
    } finally {
      if (requestId === latestLoadRequestId.current) {
        setLoading(false);
      }
    }
  };

  useEffect(() => {
    void loadData();
  }, []);

  useTauriEvent("ssh-tunnels-updated", (payload) => {
    if (isTunnelsSnapshot(payload)) {
      applySnapshot(payload);
      setLoading(false);
      return;
    }
    void loadData();
  });

  useVisibleInterval(
    () => {
      void refreshStatuses();
    },
    5000,
    isVisible && isTauri,
  );

  useEffect(() => {
    if (!openActionMenuId) return;

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("[data-ssh-tunnel-menu-root]")) return;
      setOpenActionMenuId(null);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenActionMenuId(null);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [openActionMenuId]);

  useEffect(() => {
    if (!groupMenuOpen) return;

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("[data-group-menu-root]")) return;
      setGroupMenuOpen(false);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setGroupMenuOpen(false);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [groupMenuOpen]);

  useEffect(() => {
    if (!openActionMenuId) return;
    if (!visibleTunnels.some((tunnel) => tunnel.id === openActionMenuId)) {
      setOpenActionMenuId(null);
    }
  }, [openActionMenuId, visibleTunnels]);

  useEffect(() => {
    if (groups.length === 0) return;
    setActiveGroupId((prev) => {
      if (prev === ALL_CONNECTED_TAB_ID) return prev;
      return groups.some((group) => group.id === prev) ? prev : DEFAULT_TUNNEL_GROUP_ID;
    });
  }, [groups]);

  const notify = (
    description?: string,
    kind: "error" | "info" | "success" = "error",
    title = t("sshTunnels", "SSH Tunnels"),
  ) => {
    pushToast({
      title,
      description,
      kind,
    });
  };

  const formatTunnelError = (error: unknown) => localizeSshTunnelError(t, error);
  const recordTunnelMessage = (
    category: string,
    title: string,
    summary: string,
    detail: unknown,
    id?: string,
  ) => {
    void safeRecordMessage({
      source: "ssh_tunnels",
      category,
      severity: "error",
      title,
      summary,
      detail: errorToMessage(detail),
      dedupe_key: `ssh-tunnels:${category}:${id || "draft"}`,
      target: { tab: "ssh-tunnels", entity_id: id },
    });
  };
  const formatProbeMessage = (probe?: SshTunnelProbeResult | null) => {
    if (!probe) return "";
    return probe.ok ? probe.message : formatTunnelError(probe.message);
  };

  const openCreateEditor = () => {
    setOpenActionMenuId(null);
    hydrateForm(null);
    setEditorOpen(true);
  };

  const openEditEditor = (tunnel: SshTunnelView) => {
    setOpenActionMenuId(null);
    hydrateForm(tunnel);
    setEditorOpen(true);
  };

  const handleCreateGroup = async (name: string) => {
    if (!isTauri) return;
    try {
      setGroupSubmitting(true);
      setError(null);
      const created = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: "create-group",
          target: { tab: "ssh-tunnels" },
          dedupeKey: `ssh-tunnels:create-group:${name.trim()}`,
          success: {
            title: t("sshTunnelGroupCreated", "Group created"),
            summary: t("sshTunnelGroupCreatedSummary", "Tunnel group created successfully."),
          },
          error: {
            title: t("sshTunnelGroupCreateFailed", "Failed to create tunnel group"),
          },
        },
        () => sshTunnelGroupUpsert<SshTunnelGroupView>({ name: name.trim() }),
      );
      if (!created) return;
      setGroups((prev) => sortTunnelGroups(ensureDefaultGroup([...prev, created])));
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
      throw err;
    } finally {
      setGroupSubmitting(false);
    }
  };

  const handleRenameGroup = async (group: SshTunnelGroupView, name: string) => {
    if (!isTauri) return;
    try {
      setGroupSubmitting(true);
      setError(null);
      const updated = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: "rename-group",
          target: { tab: "ssh-tunnels", entity_id: group.id },
          dedupeKey: `ssh-tunnels:rename-group:${group.id}`,
          success: {
            title: t("sshTunnelGroupRenamed", "Group updated"),
            summary: t("sshTunnelGroupRenamedSummary", "Tunnel group updated successfully."),
          },
          error: {
            title: t("sshTunnelGroupRenameFailed", "Failed to update tunnel group"),
          },
        },
        () => sshTunnelGroupUpsert<SshTunnelGroupView>({ id: group.id, name: name.trim() }),
      );
      if (!updated) return;
      setGroups((prev) =>
        sortTunnelGroups(
          ensureDefaultGroup(prev.map((item) => (item.id === updated.id ? updated : item))),
        ),
      );
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
      throw err;
    } finally {
      setGroupSubmitting(false);
    }
  };

  const handleDeleteGroup = async (group: SshTunnelGroupView) => {
    const confirmed = await confirmDialog(
      t(
        "sshTunnelDeleteGroupConfirm",
        'Delete environment group "{{name}}"? Tunnels in this group will move to the default group.',
        { name: group.name },
      ),
      {
        okLabel: t("delete", "Delete"),
        cancelLabel: t("cancel", "Cancel"),
      },
    );
    if (!confirmed || !isTauri) return;
    try {
      setGroupSubmitting(true);
      setError(null);
      await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "delete",
          action: "delete-group",
          target: { tab: "ssh-tunnels", entity_id: group.id },
          dedupeKey: `ssh-tunnels:delete-group:${group.id}`,
          success: {
            title: t("sshTunnelGroupDeleted", "Group deleted"),
            summary: t("sshTunnelGroupDeletedSummary", "Tunnel group deleted successfully."),
          },
          error: {
            title: t("sshTunnelGroupDeleteFailed", "Failed to delete tunnel group"),
          },
        },
        () => sshTunnelGroupDelete(group.id),
      );
      setGroups((prev) => prev.filter((item) => item.id !== group.id));
      setTunnels((prev) =>
        prev.map((item) =>
          item.group_id === group.id ? { ...item, group_id: DEFAULT_TUNNEL_GROUP_ID } : item,
        ),
      );
      setActiveGroupId((prev) =>
        prev === group.id ? DEFAULT_TUNNEL_GROUP_ID : prev,
      );
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setGroupSubmitting(false);
    }
  };

  const handlePickKey = async () => {
    try {
      const selected = await open({ multiple: false });
      if (selected && typeof selected === "string") {
        setForm((prev) => ({ ...prev, custom_key_path: selected }));
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSave = async () => {
    if (!isTauri) return;
    try {
      setSaving(true);
      setError(null);
      const saved = await runUserAction(
        actionContext,
        {
          source: "ssh_tunnels",
          category: "save",
          action: form.id ? "update-tunnel" : "create-tunnel",
          target: { tab: "ssh-tunnels", entity_id: form.id || null },
          dedupeKey: `ssh-tunnels:save:${form.id || form.name.trim()}`,
          success: {
            title: form.id
              ? t("sshTunnelUpdated", "Tunnel updated")
              : t("sshTunnelCreated", "Tunnel created"),
            summary: form.id
              ? t("sshTunnelUpdatedSummary", "Tunnel updated successfully.")
              : t("sshTunnelCreatedSummary", "Tunnel created successfully."),
          },
          error: {
            title: t("sshTunnelSaveFailed", "Failed to save tunnel"),
          },
        },
        () => sshTunnelUpsert<SshTunnelView>(buildPayload() as Record<string, unknown>),
      );
      if (!saved) return;
      const normalizedSaved = normalizeTunnel(saved);
      setSavedProbeMap((prev) => {
        const next = { ...prev };
        delete next[normalizedSaved.id];
        return next;
      });
      setRuntimeMap((prev) => {
        const next = { ...prev };
        delete next[normalizedSaved.id];
        return next;
      });
      setTunnels((prev) => {
        const next = prev.filter((item) => item.id !== normalizedSaved.id);
        next.unshift(normalizedSaved);
        next.sort((a, b) => b.updated_at - a.updated_at);
        return next;
      });
      setActiveGroupId(normalizedSaved.group_id);
      resetEditor();
      void loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setSaving(false);
    }
  };

  const handleDraftProbe = async () => {
    if (!isTauri) return;
    try {
      setSaving(true);
      const result = await sshTunnelProbeDraft<SshTunnelProbeResult>(
        buildPayload() as Record<string, unknown>,
      );
      setDraftProbe(result);
      if (!result.ok) {
        recordTunnelMessage(
          "probe",
          t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
          formatProbeMessage(result),
          result.last_error || result.message,
        );
        await notify(formatProbeMessage(result));
      }
    } catch (err) {
      const text = formatTunnelError(err);
      setDraftProbe({
        ok: false,
        summary: "",
        message: text,
        last_error: text,
      });
      recordTunnelMessage(
        "probe",
        t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
        text,
        err,
      );
      await notify(text);
    } finally {
      setSaving(false);
    }
  };

  const handleSavedProbe = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id, kind: "probe" });
      const result = await sshTunnelProbeSaved<SshTunnelProbeResult>(id);
      setSavedProbeMap((prev) => ({
        ...prev,
        [id]: { result, expiresAt: Date.now() + SAVED_PROBE_TTL_MS },
      }));
      const message = formatProbeMessage(result);
      if (!result.ok) {
        recordTunnelMessage(
          "probe",
          t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
          message,
          result.last_error || result.message,
          id,
        );
      }
      notify(
        message,
        result.ok ? "success" : "error",
        result.ok
          ? t("sshTunnelProbeSuccess", "Tunnel check succeeded.")
          : t("sshTunnelProbeFailed", "Tunnel check failed."),
      );
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      recordTunnelMessage(
        "probe",
        t("sshTunnelProbeFailedMessageTitle", "SSH tunnel probe failed"),
        text,
        err,
        id,
      );
      notify(text, "error", t("sshTunnelProbeFailed", "Tunnel check failed."));
    } finally {
      setBusyAction(null);
    }
  };

  const handleConnect = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id, kind: "connect" });
      const runtime = await runUserAction(
        actionContext,
        buildConnectTunnelActionDescriptor(t, id),
        () => sshTunnelConnect<SshTunnelRuntimeView>(id),
      );
      if (!runtime) return;
      await loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      const tunnel = tunnels.find((item) => item.id === id);
      setError(text);
      recordTunnelMessage(
        "manual-connect",
        t(
          "sshTunnelConnectFailedMessageTitle",
          "SSH tunnel connection failed",
        ),
        `${tunnel?.name || id}: ${text}`,
        err,
        id,
      );
      notify(text, "error", t("sshTunnelConnectFailed", "Failed to connect tunnel."));
      await loadData();
    } finally {
      setBusyAction(null);
    }
  };

  const handleDisconnect = async (id: string) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id, kind: "disconnect" });
      const runtime = await runUserAction(
        actionContext,
        buildDisconnectTunnelActionDescriptor(t, id),
        () => sshTunnelDisconnect<SshTunnelRuntimeView>(id),
      );
      if (!runtime) return;
      await loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      notify(
        text,
        "error",
        t("sshTunnelDisconnectFailed", "Failed to disconnect tunnel."),
      );
      await loadData();
    } finally {
      setBusyAction(null);
    }
  };

  const handleDelete = async (tunnel: SshTunnelView) => {
    if (!isTauri) return;
    try {
      setBusyAction({ id: tunnel.id, kind: "delete" });
      await runUserAction(
        actionContext,
        buildDeleteTunnelActionDescriptor(t, {
          id: tunnel.id,
          name: tunnel.name,
        }),
        () => sshTunnelDelete(tunnel.id),
      );
      setSavedProbeMap((prev) => {
        const next = { ...prev };
        delete next[tunnel.id];
        return next;
      });
      await loadData();
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      await notify(text);
    } finally {
      setBusyAction(null);
    }
  };

  const handleGroupBatch = async (
    operation: "connect" | "disconnect",
    groupId: string,
  ) => {
    if (!isTauri) return;

    const isConnect = operation === "connect";
    const candidates = visibleTunnels.filter((tunnel) => {
      const status = runtimeMap[tunnel.id]?.status;
      return isConnect
        ? status !== "connected" && status !== "connecting"
        : status === "connected" || status === "connecting";
    });

    if (candidates.length === 0) {
      pushToast({
        title: t(
          isConnect
            ? "sshTunnelGroupNoConnectable"
            : "sshTunnelGroupNoDisconnectable",
        ),
        description: t(
          isConnect ? "sshTunnelGroupConnectInfo" : "sshTunnelGroupDisconnectInfo",
        ),
        kind: "info",
      });
      return;
    }

    const keyPrefix = isConnect
      ? "sshTunnelGroupConnect"
      : "sshTunnelGroupDisconnect";
    const action = `group-${operation}`;

    try {
      setGroupBusyAction(operation);
      const result = isConnect
        ? await sshTunnelGroupConnect<SshTunnelBatchOperationResult>(groupId)
        : await sshTunnelGroupDisconnect<SshTunnelBatchOperationResult>(groupId);

      await loadData();

      if (result.failed_count === 0) {
        const skipped =
          result.skipped_count > 0
            ? t(
                isConnect
                  ? "sshTunnelGroupSkippedConnected"
                  : "sshTunnelGroupSkippedDisconnected",
                { count: result.skipped_count },
              )
            : "";
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: operation,
            action,
            target: { tab: "ssh-tunnels", entity_id: groupId },
            dedupeKey: `ssh-tunnels:${action}:${groupId}`,
          },
          "success",
          {
            title: t(`${keyPrefix}SuccessTitle`),
            summary: t(`${keyPrefix}SuccessDesc`, {
              count: result.success_count,
              group: result.group_name,
              skipped,
            }),
          },
        );
      } else {
        const failureNames = result.failures.map((f) => f.tunnel_name).join(", ");
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: operation,
            action,
            target: { tab: "ssh-tunnels", entity_id: groupId },
            dedupeKey: `ssh-tunnels:${action}:${groupId}:partial`,
          },
          "error",
          {
            title: t(`${keyPrefix}PartialTitle`),
            summary: t(`${keyPrefix}PartialDesc`, {
              success: result.success_count,
              failed: result.failed_count,
              names: failureNames,
            }),
          },
        );
      }
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      pushToast({
        title: text,
        description: t(
          isConnect ? "sshTunnelGroupConnectFailed" : "sshTunnelGroupDisconnectFailed",
        ),
        kind: "error",
      });
    } finally {
      setGroupBusyAction(null);
    }
  };

  const handleAllDisconnect = async () => {
    if (!isTauri) return;

    const candidates = tunnels.filter((tunnel) => {
      const status = runtimeMap[tunnel.id]?.status;
      return status === "connected" || status === "connecting";
    });

    if (candidates.length === 0) {
      pushToast({
        title: t("sshTunnelGroupNoDisconnectable"),
        description: t("sshTunnelGroupDisconnectInfo"),
        kind: "info",
      });
      return;
    }

    try {
      setGroupBusyAction("disconnect");
      const result = await sshTunnelsDisconnectAll<SshTunnelBatchOperationResult>();
      await loadData();

      if (result.failed_count === 0) {
        const skipped =
          result.skipped_count > 0
            ? t("sshTunnelGroupSkippedDisconnected", { count: result.skipped_count })
            : "";
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: "disconnect",
            action: "tunnels-disconnect-all",
            target: { tab: "ssh-tunnels", entity_id: "all" },
            dedupeKey: "ssh-tunnels:disconnect:all",
          },
          "success",
          {
            title: t("sshTunnelAllDisconnectSuccessTitle"),
            summary: t("sshTunnelAllDisconnectSuccessDesc", {
              count: result.success_count,
              skipped,
            }),
          },
        );
      } else {
        const failureNames = result.failures.map((f) => f.tunnel_name).join(", ");
        await notifyActionResult(
          { pushToast, recordMessage: safeRecordMessage },
          {
            source: "ssh_tunnels",
            category: "disconnect",
            action: "tunnels-disconnect-all",
            target: { tab: "ssh-tunnels", entity_id: "all" },
            dedupeKey: "ssh-tunnels:disconnect:all:partial",
          },
          "error",
          {
            title: t("sshTunnelGroupDisconnectPartialTitle"),
            summary: t("sshTunnelGroupDisconnectPartialDesc", {
              success: result.success_count,
              failed: result.failed_count,
              names: failureNames,
            }),
          },
        );
      }
    } catch (err) {
      const text = formatTunnelError(err);
      setError(text);
      pushToast({
        title: text,
        description: t("sshTunnelGroupDisconnectFailed"),
        kind: "error",
      });
    } finally {
      setGroupBusyAction(null);
    }
  };

  const renderTunnelCard = (tunnel: SshTunnelView) => {
    const runtime = runtimeMap[tunnel.id];
    const probe = getSavedProbe(tunnel.id);
    const errorDisplay = tunnelErrorDisplay(runtime, tunnel);
    const currentBusyAction =
      busyAction?.id === tunnel.id ? busyAction.kind : null;
    const busy = currentBusyAction !== null || groupBusyAction !== null;
    const showBusyOverlay =
      (currentBusyAction !== null && currentBusyAction !== "delete") ||
      groupBusyAction !== null;
    const status = runtime?.status || "disconnected";
    const probeDisabledBecauseConnected = status === "connected";
    const probeDisabled = busy || probeDisabledBecauseConnected;
    const probeDisabledTitle = probeDisabledBecauseConnected
      ? t(
          "sshTunnelProbeDisabledConnected",
          "Tunnel is already connected; no need to check it again.",
        )
      : undefined;

    return (
      <div
        key={tunnel.id}
        className="relative rounded-xl border bg-card p-5 shadow-sm transition-all hover:border-primary/30"
      >
        {showBusyOverlay ? (
          <div className="absolute inset-0 z-10 flex items-center justify-center rounded-xl bg-background/75 px-6 text-center backdrop-blur-[1px]">
            <div className="flex flex-col items-center gap-3">
              <Loader2 className="h-6 w-6 animate-spin text-primary" />
              <div className="text-sm font-medium text-foreground">
                {groupBusyAction
                  ? groupBusyAction === "connect"
                    ? t("sshTunnelGroupConnecting")
                    : t("sshTunnelGroupDisconnecting")
                  : getBusyOverlayLabel(
                      currentBusyAction as Exclude<TunnelBusyAction, "delete">,
                    )}
              </div>
            </div>
          </div>
        ) : null}
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div className="flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-base font-semibold">{tunnel.name}</span>
              <ToolStatusBadge
                tone={statusTone(status)}
                label={getStatusLabel(status)}
              />
              <span className="rounded-full border bg-muted px-2 py-0.5 text-[11px] font-medium">
                {modeShort(tunnel.forward.mode)}
              </span>
            </div>
            <div className="mt-2 text-sm text-muted-foreground">
              {runtime?.summary ||
                t(
                  "sshTunnelWaitingForStatusRefresh",
                  "Waiting for status refresh...",
                )}
            </div>
            <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
              <span>
                {t("sshTunnelSource", "Source")}:{" "}
                {tunnel.source_kind === "saved_host"
                  ? `${t("sshServers", "SSH Servers")} / ${tunnel.saved_host_name || "-"}`
                  : `${tunnel.custom?.user || "-"}@${tunnel.custom?.host || "-"}:${tunnel.custom?.port || 22}`}
              </span>
              <span>
                {t("sshTunnelEnvironmentGroup")}:{" "}
                {getGroupLabel(tunnel.group_id)}
              </span>
              <span>
                {t("sshTunnelAuthMethod", "Authentication Method")}:{" "}
                {tunnel.source_kind === "saved_host"
                  ? t("sshTunnelAuthInherited", "Inherited from SSH config")
                  : tunnel.custom?.auth_kind === "password"
                    ? t("password", "Password")
                    : t("sshKey", "SSH Key")}
              </span>
              <span>
                {t("sshTunnelLaunchAtLogin", "Launch at login")}:{" "}
                {tunnel.auto_connect ? t("yes", "Yes") : t("no", "No")}
              </span>
              <span>
                {t("sshTunnelAutoReconnectLabel", "Auto reconnect")}:{" "}
                {tunnel.auto_reconnect ? t("yes", "Yes") : t("no", "No")}
              </span>
              {runtime?.resolved_server_host ? (
                <span>
                  {t("sshTunnelResolvedServer", "Resolved SSH Server")}:{" "}
                  {runtime.resolved_server_host}
                </span>
              ) : null}
              {runtime?.listening_addr ? (
                <span>
                  {t("sshTunnelListening", "Listening")}: {runtime.listening_addr}
                </span>
              ) : null}
              {runtime?.active_client_count ? (
                <span>
                  {t("sshTunnelClients", "Clients")}:{" "}
                  {runtime.active_client_count}
                </span>
              ) : null}
            </div>
          </div>

          <div
            className="relative flex w-full shrink-0 flex-col gap-2 lg:w-auto lg:min-w-[190px]"
            data-ssh-tunnel-menu-root
          >
            {status === "connected" || status === "connecting" ? (
              <button
                type="button"
                onClick={() => {
                  setOpenActionMenuId(null);
                  void handleDisconnect(tunnel.id);
                }}
                disabled={busy}
                className="inline-flex w-full items-center justify-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm font-medium text-destructive transition-colors hover:bg-destructive/15 disabled:opacity-60"
              >
                <Unplug className="h-4 w-4" />
                {t("disconnect", "Disconnect")}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => {
                  setOpenActionMenuId(null);
                  void handleConnect(tunnel.id);
                }}
                disabled={busy}
                className="inline-flex w-full items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
              >
                <Play className="h-4 w-4" />
                {t("connect", "Connect")}
              </button>
            )}

            <div className="grid grid-cols-[minmax(0,1fr)_44px] gap-2">
              <div className="min-w-0" title={probeDisabledTitle}>
                <button
                  type="button"
                  onClick={() => {
                    setOpenActionMenuId(null);
                    void handleSavedProbe(tunnel.id);
                  }}
                  disabled={probeDisabled}
                  className="inline-flex w-full items-center justify-center gap-2 rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
                >
                  {busy ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Activity className="h-4 w-4" />
                  )}
                  {t("sshTunnelProbe", "Detect Connection")}
                </button>
              </div>
              <button
                type="button"
                onClick={() =>
                  setOpenActionMenuId((current) =>
                    current === tunnel.id ? null : tunnel.id,
                  )
                }
                disabled={busy}
                aria-haspopup="menu"
                aria-expanded={openActionMenuId === tunnel.id}
                aria-label={t("sshTunnelMoreActions", "More actions")}
                title={t("sshTunnelMoreActions", "More actions")}
                className="inline-flex items-center justify-center rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
              >
                <MoreHorizontal className="h-4 w-4" />
              </button>
            </div>

            {openActionMenuId === tunnel.id ? (
              <div
                role="menu"
                className="absolute right-0 top-full z-20 mt-2 w-44 rounded-lg border bg-popover p-1 shadow-lg"
              >
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setOpenActionMenuId(null);
                    openEditEditor(tunnel);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors hover:bg-muted"
                >
                  <Pencil className="h-4 w-4" />
                  {t("sshTunnelEditAction", "Edit tunnel")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setOpenActionMenuId(null);
                    void handleDelete(tunnel);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm font-medium text-destructive transition-colors hover:bg-destructive/10"
                >
                  <Trash2 className="h-4 w-4" />
                  {t("sshTunnelDeleteAction", "Delete tunnel")}
                </button>
              </div>
            ) : null}
          </div>
        </div>

        {probe ? (
          <div
            className={`mt-4 rounded-lg border px-3 py-2 text-sm ${
              probe.ok
                ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700"
                : "border-destructive/20 bg-destructive/10 text-destructive"
            }`}
          >
            {formatProbeMessage(probe)}
          </div>
        ) : errorDisplay ? (
          <div
            className={`mt-4 rounded-lg border px-3 py-2 text-sm ${
              errorDisplay.tone === "reconnecting"
                ? "border-amber-500/20 bg-amber-500/10 text-amber-700"
                : "border-destructive/20 bg-destructive/10 text-destructive"
            }`}
          >
            {errorDisplay.tone === "reconnecting"
              ? `${t("sshTunnelLastReconnectError", "Last attempt failed")}: `
              : ""}
            {formatTunnelError(errorDisplay.text)}
          </div>
        ) : null}
      </div>
    );
  };

  return (
    <div className="flex h-full flex-col space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div>
            <h2 className="text-xl font-bold tracking-tight">
              {t("sshTunnels", "SSH Tunnels")}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t(
                "sshTunnelsDesc",
                "Create, detect, connect, and disconnect local, remote, or dynamic SSH forwarding profiles.",
              )}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void loadData()}
            className="inline-flex items-center gap-2 rounded-md bg-secondary px-3 py-2 text-sm font-medium text-secondary-foreground transition-colors hover:bg-secondary/80"
          >
            <RefreshCw className="h-4 w-4" />
            {t("refresh", "Refresh")}
          </button>
          <button
            type="button"
            onClick={openCreateEditor}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
            {t("newSshTunnel", "New Tunnel")}
          </button>
        </div>
      </div>

      {error ? (
        <div className="rounded-xl border border-destructive/20 bg-destructive/10 p-4 text-sm text-destructive">
          <div className="flex items-start gap-3">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
            <div>{error}</div>
          </div>
        </div>
      ) : null}

      <div className="grid gap-3 md:grid-cols-3">
        {modeCards.map((card) => {
          const Icon = card.icon;
          return (
            <div key={card.id} className="rounded-xl border bg-card p-4 shadow-sm">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <Icon className="h-4 w-4 text-primary" />
                {card.title}
              </div>
              <p className="mt-2 text-sm text-muted-foreground">{card.description}</p>
              <p className="mt-3 text-xs leading-5 text-muted-foreground/90">{card.example}</p>
            </div>
          );
        })}
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="inline-flex w-fit items-center rounded-lg border border-black bg-white p-1">
          <button
            type="button"
            onClick={() => setActiveGroupId(ALL_CONNECTED_TAB_ID)}
            title={
              connectedViewTone
                ? t("sshTunnelConnectedViewAria", {
                    count: connectedTunnels.length,
                    defaultValue: `View all active tunnels (${connectedTunnels.length})`,
                  })
                : undefined
            }
            className={`group relative inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm transition-colors ${
              isConnectedView
                ? "bg-black text-white"
                : "bg-white text-black hover:bg-neutral-50"
            }`}
            data-testid="ssh-tunnel-connected-view-tab"
          >
            <Activity
              className={`h-3.5 w-3.5 transition-colors ${
                isConnectedView
                  ? "text-white"
                  : "text-muted-foreground group-hover:text-foreground"
              }`}
            />
            <span className="font-medium">{t("sshTunnelConnectedView", "Connected")}</span>
            <span
              className={`rounded-full px-1.5 py-0.2 text-[11px] font-semibold transition-colors ${
                isConnectedView
                  ? "bg-white/20 text-white"
                  : "bg-muted text-muted-foreground group-hover:bg-muted/80 group-hover:text-foreground"
              }`}
            >
              {connectedTunnels.length}
            </span>
            {connectedViewTone && (
              <ToolStatusDot
                tone={connectedViewTone}
                className="right-0.5 top-0.5"
                testId="connected-view-status-dot"
              />
            )}
          </button>
          <div
            className="mx-1 h-4 w-px bg-neutral-300 dark:bg-neutral-600"
            aria-hidden="true"
          />
          {groups.map((group) => {
            const label = group.is_default
              ? t("sshTunnelDefaultGroup")
              : group.name;
            const statusInfo = groupStatusMap[group.id];
            const isActive = !isConnectedView && activeGroupId === group.id;
            return (
              <button
                key={group.id}
                type="button"
                onClick={() => setActiveGroupId(group.id)}
                title={statusInfo?.tooltip || undefined}
                className={`relative px-3 py-1.5 rounded-md text-sm transition-colors ${
                  isActive ? "bg-black text-white" : "bg-white text-black"
                }`}
                data-testid={`ssh-tunnel-group-tab-${group.id}`}
              >
                <span>{label}</span>
                {statusInfo?.tone && (
                  <ToolStatusDot
                    tone={statusInfo.tone}
                    className="right-0.5 top-0.5"
                    testId={`group-tab-dot-${group.id}`}
                  />
                )}
              </button>
            );
          })}
        </div>

        <div className="relative" data-group-menu-root>
          <button
            type="button"
            onClick={() => setGroupMenuOpen((prev) => !prev)}
            disabled={groupBusyAction !== null}
            aria-haspopup="menu"
            aria-expanded={groupMenuOpen}
            className="inline-flex items-center gap-1.5 rounded-md border px-2 py-1.5 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-50"
          >
            {groupBusyAction ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <MoreHorizontal className="h-3.5 w-3.5" />
            )}
            {t("sshTunnelGroupActions")}
          </button>
          {groupMenuOpen ? (
            <div
              role="menu"
              className="absolute left-0 top-full z-20 mt-1 w-44 rounded-lg border bg-popover p-1 shadow-lg"
            >
              {isConnectedView ? (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setGroupMenuOpen(false);
                    void handleAllDisconnect();
                  }}
                  disabled={visibleTunnels.filter(
                    (tunnel) =>
                      runtimeMap[tunnel.id]?.status === "connected" ||
                      runtimeMap[tunnel.id]?.status === "connecting",
                  ).length === 0}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                >
                  <Unplug className="h-3.5 w-3.5" />
                  {t("sshTunnelConnectedDisconnectAll", "Disconnect all active tunnels")}
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setGroupMenuOpen(false);
                      void handleGroupBatch("connect", activeGroupId);
                    }}
                    disabled={visibleTunnels.filter(
                      (tunnel) =>
                        runtimeMap[tunnel.id]?.status !== "connected" &&
                        runtimeMap[tunnel.id]?.status !== "connecting",
                    ).length === 0}
                    className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium transition-colors hover:bg-muted disabled:opacity-50"
                  >
                    <Play className="h-3.5 w-3.5" />
                    {t("sshTunnelGroupConnectAll")}
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setGroupMenuOpen(false);
                      void handleGroupBatch("disconnect", activeGroupId);
                    }}
                    disabled={visibleTunnels.filter(
                      (tunnel) =>
                        runtimeMap[tunnel.id]?.status === "connected" ||
                        runtimeMap[tunnel.id]?.status === "connecting",
                    ).length === 0}
                    className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                  >
                    <Unplug className="h-3.5 w-3.5" />
                    {t("sshTunnelGroupDisconnectAll")}
                  </button>
                </>
              )}
              <div className="my-1 border-t" />
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setGroupMenuOpen(false);
                  setGroupManagerOpen(true);
                }}
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-medium transition-colors hover:bg-muted"
              >
                <Pencil className="h-3.5 w-3.5" />
                {t("sshTunnelManageGroups")}
              </button>
            </div>
          ) : null}
        </div>
      </div>

      {loading ? (
        <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
          <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          {t("loading", "Loading...")}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          {visibleTunnels.length === 0 ? (
            <div className="space-y-4">
              <ToolEmptyState
                title={
                  isConnectedView
                    ? t("sshTunnelConnectedEmpty", "No active or connected SSH tunnels currently.")
                    : tunnels.length === 0
                      ? t("sshTunnelEmpty")
                      : t("sshTunnelEmptyForGroup")
                }
              />
              {!isConnectedView && (
                <div className="flex justify-center">
                  <button
                    type="button"
                    onClick={openCreateEditor}
                    className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
                  >
                    <Plus className="h-4 w-4" />
                    {t("newSshTunnel")}
                  </button>
                </div>
              )}
            </div>
          ) : isConnectedView ? (
            <div className="space-y-6">
              {connectedGroupSections.map(({ group, tunnels: groupTunnels }) => {
                const groupLabel = group.is_default
                  ? t("sshTunnelDefaultGroup")
                  : group.name;
                return (
                  <section
                    key={group.id}
                    data-testid={`ssh-tunnel-connected-group-${group.id}`}
                    className="space-y-3"
                  >
                    <div className="flex items-center gap-2 px-1">
                      <h3 className="text-sm font-semibold text-foreground">
                        {groupLabel}
                      </h3>
                      <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                        {groupTunnels.length}
                      </span>
                    </div>
                    <div className="space-y-4">
                      {groupTunnels.map(renderTunnelCard)}
                    </div>
                  </section>
                );
              })}
            </div>
          ) : (
            <div className="space-y-4">
              {visibleTunnels.map(renderTunnelCard)}
            </div>
          )}
        </div>
      )}

      <SshTunnelGroupManagerDialog
        open={groupManagerOpen}
        onOpenChange={setGroupManagerOpen}
        groups={groups}
        submitting={groupSubmitting}
        onCreate={handleCreateGroup}
        onRename={handleRenameGroup}
        onDelete={handleDeleteGroup}
      />

      <Dialog
        open={editorOpen}
        onOpenChange={(open) => {
          if (!open) {
            resetEditor();
            return;
          }
          setEditorOpen(true);
        }}
      >
        {editorOpen && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] overflow-hidden p-0">
            <DialogHeader className="border-b px-6 pt-6 pb-4">
              <DialogTitle>
                {form.id
                  ? t("editSshTunnel", "Edit SSH Tunnel")
                  : t("newSshTunnel", "New Tunnel")}
              </DialogTitle>
              <DialogDescription>
                {t(
                  "sshTunnelEditorDesc",
                  "Choose a forwarding mode, define the SSH source, then test the tunnel before saving.",
                )}
              </DialogDescription>
            </DialogHeader>

            <div className="overflow-y-auto px-6 py-5">
              <div className="space-y-6">
                <div className="space-y-2">
                  <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("name", "Name")}
                  </label>
                  <input
                    type="text"
                    value={form.name}
                    onChange={(event) =>
                      setForm((prev) => ({ ...prev, name: event.target.value }))
                    }
                    placeholder={t("sshTunnelNamePlaceholder", "e.g. Redis via Bastion")}
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                </div>

                <div className="space-y-2">
                  <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("sshTunnelEnvironmentGroupOptional")}
                  </label>
                  <select
                    value={form.group_id}
                    onChange={(event) =>
                      setForm((prev) => ({ ...prev, group_id: event.target.value }))
                    }
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  >
                    <option value="">
                      {t("sshTunnelEnvironmentGroupDefaultOption")}
                    </option>
                    {groups
                      .filter((group) => !group.is_default)
                      .map((group) => (
                        <option key={group.id} value={group.id}>
                          {group.name}
                        </option>
                      ))}
                  </select>
                </div>

                <div className="space-y-3">
                  <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("sshTunnelForwardMode", "Forwarding Mode")}
                  </div>
                  <div className="grid gap-3 md:grid-cols-3">
                    {modeCards.map((card) => {
                      const Icon = card.icon;
                      const active = form.forward_mode === card.id;
                      return (
                        <button
                          key={card.id}
                          type="button"
                          onClick={() =>
                            setForm((prev) => ({
                              ...prev,
                              forward_mode: card.id,
                              local_port:
                                card.id === "dynamic"
                                  ? prev.local_port || "1080"
                                  : prev.local_port,
                            }))
                          }
                          className={`rounded-xl border p-4 text-left transition-all ${
                            active
                              ? "border-primary bg-primary/5 shadow-sm"
                              : "hover:border-primary/30"
                          }`}
                        >
                          <div className="flex items-center gap-2 text-sm font-semibold">
                            <Icon className="h-4 w-4 text-primary" />
                            {card.title}
                          </div>
                          <div className="mt-2 text-sm text-muted-foreground">
                            {card.description}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="space-y-3">
                  <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("sshTunnelSource", "SSH Source")}
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <button
                      type="button"
                      onClick={() =>
                        setForm((prev) => ({ ...prev, source_kind: "saved_host" }))
                      }
                      className={`rounded-xl border p-4 text-left transition-all ${
                        form.source_kind === "saved_host"
                          ? "border-primary bg-primary/5 shadow-sm"
                          : "hover:border-primary/30"
                      }`}
                    >
                      <div className="flex items-center gap-2 text-sm font-semibold">
                        <Server className="h-4 w-4 text-primary" />
                        {t("sshTunnelUseSavedServer", "Use SSH Server")}
                      </div>
                      <p className="mt-2 text-sm text-muted-foreground">
                        {t(
                          "sshTunnelUseSavedServerDesc",
                          "Reuse an alias from SSH Servers and inherit host/user/key settings from your SSH config.",
                        )}
                      </p>
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setForm((prev) => ({ ...prev, source_kind: "custom" }))
                      }
                      className={`rounded-xl border p-4 text-left transition-all ${
                        form.source_kind === "custom"
                          ? "border-primary bg-primary/5 shadow-sm"
                          : "hover:border-primary/30"
                      }`}
                    >
                      <div className="flex items-center gap-2 text-sm font-semibold">
                        <Network className="h-4 w-4 text-primary" />
                        {t("sshTunnelUseCustomServer", "Custom SSH")}
                      </div>
                      <p className="mt-2 text-sm text-muted-foreground">
                        {t(
                          "sshTunnelUseCustomServerDesc",
                          "Enter host, port, and credentials directly for a fully managed tunnel.",
                        )}
                      </p>
                    </button>
                  </div>
                </div>

                {form.source_kind === "saved_host" ? (
                  <div className="space-y-2">
                    <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("sshTunnelSelectServer", "SSH Server")}
                    </label>
                    <select
                      value={form.saved_host_name}
                      onChange={(event) =>
                        setForm((prev) => ({
                          ...prev,
                          saved_host_name: event.target.value,
                        }))
                      }
                      className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                    >
                      <option value="">
                        {t("sshTunnelSelectServerPlaceholder", "Choose an SSH server...")}
                      </option>
                      {hosts.map((host) => (
                        <option key={host.name} value={host.name}>
                          {host.name} · {host.user}@{host.host_name}:{host.port}
                        </option>
                      ))}
                    </select>
                  </div>
                ) : (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("hostOrIp", "Host or IP")}
                      </label>
                      <input
                        type="text"
                        value={form.custom_host}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, custom_host: event.target.value }))
                        }
                        placeholder={t("hostOrIpPlaceholder", "e.g. 113.128.187.178")}
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("username", "Username")}
                      </label>
                      <input
                        type="text"
                        value={form.custom_user}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, custom_user: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("port", "Port")}
                      </label>
                      <input
                        type="number"
                        value={form.custom_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, custom_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("authMethod", "Authentication Method")}
                      </label>
                      <div className="grid gap-3 md:grid-cols-2">
                        <button
                          type="button"
                          onClick={() =>
                            setForm((prev) => ({ ...prev, custom_auth_kind: "password" }))
                          }
                          className={`rounded-xl border p-4 text-left transition-all ${
                            form.custom_auth_kind === "password"
                              ? "border-primary bg-primary/5 shadow-sm"
                              : "hover:border-primary/30"
                          }`}
                        >
                          <div className="flex items-center gap-2 text-sm font-semibold">
                            <ShieldCheck className="h-4 w-4 text-primary" />
                            {t("password", "Password")}
                          </div>
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            setForm((prev) => ({ ...prev, custom_auth_kind: "key" }))
                          }
                          className={`rounded-xl border p-4 text-left transition-all ${
                            form.custom_auth_kind === "key"
                              ? "border-primary bg-primary/5 shadow-sm"
                              : "hover:border-primary/30"
                          }`}
                        >
                          <div className="flex items-center gap-2 text-sm font-semibold">
                            <KeyRound className="h-4 w-4 text-primary" />
                            {t("sshKey", "SSH Key")}
                          </div>
                        </button>
                      </div>
                    </div>
                    {form.custom_auth_kind === "password" ? (
                      <div className="space-y-2 md:col-span-2">
                        <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                          {t("password", "Password")}
                        </label>
                        <input
                          type="password"
                          value={form.custom_password}
                          onChange={(event) =>
                            setForm((prev) => ({
                              ...prev,
                              custom_password: event.target.value,
                              preserve_password: false,
                            }))
                          }
                          placeholder={
                            form.preserve_password
                              ? t(
                                  "sshTunnelPasswordSaved",
                                  "Saved password will be kept unless you type a new one.",
                                )
                              : t("sshTunnelPasswordPlaceholder", "Enter SSH password")
                          }
                          className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                        />
                        {form.id && form.preserve_password ? (
                          <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
                            <input
                              type="checkbox"
                              checked={form.preserve_password}
                              onChange={(event) =>
                                setForm((prev) => ({
                                  ...prev,
                                  preserve_password: event.target.checked,
                                }))
                              }
                              className="h-4 w-4"
                            />
                            {t(
                              "sshTunnelKeepPassword",
                              "Keep the saved password until I enter a new one.",
                            )}
                          </label>
                        ) : null}
                      </div>
                    ) : (
                      <div className="space-y-2 md:col-span-2">
                        <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                          {t("sshKeyPath", "Private Key Path")}
                        </label>
                        <div className="flex gap-2">
                          <input
                            type="text"
                            value={form.custom_key_path}
                            onChange={(event) =>
                              setForm((prev) => ({
                                ...prev,
                                custom_key_path: event.target.value,
                              }))
                            }
                            placeholder={t(
                              "sshKeyPathPlaceholder",
                              "e.g. /Users/name/.ssh/id_rsa",
                            )}
                            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                          />
                          <button
                            type="button"
                            onClick={() => void handlePickKey()}
                            className="rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted"
                          >
                            {t("browse", "Browse")}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                )}

                {form.forward_mode === "local" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelLocalPort", "Local Port")}
                      </label>
                      <input
                        type="number"
                        value={form.local_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, local_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("targetHost", "Target Host")}
                      </label>
                      <input
                        type="text"
                        value={form.target_host}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_host: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("targetPort", "Target Port")}
                      </label>
                      <input
                        type="number"
                        value={form.target_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="rounded-xl border bg-muted/40 p-4 text-sm text-muted-foreground">
                      {t(
                        "sshTunnelLocalBindHint",
                        "OneSpace always binds local and dynamic ports to 127.0.0.1 for safety.",
                      )}
                    </div>
                  </div>
                ) : null}

                {form.forward_mode === "remote" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelRemoteBindHost", "Remote Listen Host")}
                      </label>
                      <input
                        type="text"
                        value={form.remote_bind_host}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            remote_bind_host: event.target.value,
                          }))
                        }
                        placeholder={t(
                          "sshTunnelRemoteBindHostPlaceholder",
                          "127.0.0.1, 10.1.3.2, or 0.0.0.0",
                        )}
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelRemotePort", "Remote Port")}
                      </label>
                      <input
                        type="number"
                        value={form.remote_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, remote_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="rounded-xl border bg-muted/40 p-4 text-sm text-muted-foreground">
                      {t(
                        "sshTunnelRemoteBindHint",
                        "Remote forwarding listens on the host and port you set on the SSH server, then forwards traffic to the service on this device.",
                      )}
                    </div>
                    <div className="rounded-xl border bg-amber-500/10 p-4 text-sm text-amber-700 dark:text-amber-300 md:col-span-2">
                      {t(
                        "sshTunnelRemoteBindScopeHint",
                        "Use 127.0.0.1 to keep the remote port private to the SSH server itself. Use the server IP or 0.0.0.0 only if you want other machines to reach it, and make sure sshd allows GatewayPorts yes or GatewayPorts clientspecified.",
                      )}
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelLocalTargetHost", "This Device Service Host")}
                      </label>
                      <input
                        type="text"
                        value={form.target_host}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_host: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelLocalTargetPort", "This Device Service Port")}
                      </label>
                      <input
                        type="number"
                        value={form.target_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, target_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                  </div>
                ) : null}

                {form.forward_mode === "dynamic" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelSocksPort", "SOCKS5 Port")}
                      </label>
                      <input
                        type="number"
                        value={form.local_port}
                        onChange={(event) =>
                          setForm((prev) => ({ ...prev, local_port: event.target.value }))
                        }
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="rounded-xl border bg-muted/40 p-4 text-sm text-muted-foreground">
                      {t(
                        "sshTunnelDynamicHint",
                        "Use the optional probe target below if you want Detect Connection to confirm that a real destination is reachable through the SOCKS5 proxy.",
                      )}
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelProbeHost", "Probe Host (Optional)")}
                      </label>
                      <input
                        type="text"
                        value={form.dynamic_probe_host}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            dynamic_probe_host: event.target.value,
                          }))
                        }
                        placeholder="example.com"
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        {t("sshTunnelProbePort", "Probe Port (Optional)")}
                      </label>
                      <input
                        type="number"
                        value={form.dynamic_probe_port}
                        onChange={(event) =>
                          setForm((prev) => ({
                            ...prev,
                            dynamic_probe_port: event.target.value,
                          }))
                        }
                        placeholder="443"
                        className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      />
                    </div>
                  </div>
                ) : null}

                <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
                  <div className="space-y-1 text-sm text-muted-foreground">
                    <div className="font-medium text-foreground">
                      {t("sshTunnelAutoConnectLabel", "Launch at startup")}
                    </div>
                    <div>
                      {t(
                        "sshTunnelAutoConnect",
                        "Automatically connect this tunnel when OneSpace starts.",
                      )}
                    </div>
                  </div>
                  <Switch
                    checked={form.auto_connect}
                    onCheckedChange={(checked) =>
                      setForm((prev) => ({
                        ...prev,
                        auto_connect: checked,
                      }))
                    }
                    aria-label={t("sshTunnelAutoConnectLabel", "Launch at startup")}
                  />
                </div>

                <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
                  <div className="space-y-1 text-sm text-muted-foreground">
                    <div className="font-medium text-foreground">
                      {t("sshTunnelAutoReconnectLabel", "Auto reconnect")}
                    </div>
                    <div>
                      {t(
                        "sshTunnelAutoReconnectDesc",
                        "When the SSH connection drops, the network recovers, or the system wakes from sleep, OneSpace will try to restore this tunnel with delay, backoff, and debounce to avoid frequent retries.",
                      )}
                    </div>
                  </div>
                  <Switch
                    checked={form.auto_reconnect}
                    onCheckedChange={(checked) =>
                      setForm((prev) => ({
                        ...prev,
                        auto_reconnect: checked,
                      }))
                    }
                    aria-label={t("sshTunnelAutoReconnectLabel", "Auto reconnect")}
                  />
                </div>

                {draftProbe ? (
                  <div
                    className={`rounded-xl border px-4 py-3 text-sm ${
                      draftProbe.ok
                        ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700"
                        : "border-destructive/20 bg-destructive/10 text-destructive"
                    }`}
                  >
                    <div className="font-medium">{draftProbe.summary}</div>
                    <div className="mt-1">{draftProbe.message}</div>
                  </div>
                ) : null}
              </div>
            </div>

            <DialogFooter className="border-t px-6 py-4">
              <button
                type="button"
                onClick={resetEditor}
                className="rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted"
              >
                {t("cancel", "Cancel")}
              </button>
              <button
                type="button"
                onClick={() => void handleDraftProbe()}
                disabled={saving}
                className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
              >
                {saving ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Activity className="h-4 w-4" />
                )}
                {t("sshTunnelProbe", "Detect Connection")}
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={saving}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
              >
                {saving ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Save className="h-4 w-4" />
                )}
                {t("save", "Save")}
              </button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
