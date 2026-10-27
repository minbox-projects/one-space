import { useEffect, useMemo, useRef, useState } from "react";
import type { TFunction } from "i18next";
import { Globe, Link2, Waypoints } from "lucide-react";
import { notifyMessage } from "@/lib/messages";
import { localizeSshTunnelError } from "@/lib/sshTunnelI18n";
import {
  sshHostsList,
  sshTunnelsRefreshStatus,
  sshTunnelsSnapshot,
} from "@/lib/sshTunnels";
import { useTauriEvent } from "@/toolbox/useTauriEvent";
import { useVisibleInterval } from "@/toolbox/useVisibleInterval";
import {
  DEFAULT_TUNNEL_FORM,
  DEFAULT_TUNNEL_GROUP_ID,
} from "../types";
import type {
  SshCommonPortView,
  SshHost,
  SshTunnelGroupView,
  SshTunnelProbeResult,
  SshTunnelRuntimeView,
  SshTunnelStatus,
  SshTunnelView,
  SshTunnelsSnapshot,
  TunnelFormState,
} from "../types";
import {
  ALL_CONNECTED_TAB_ID,
  ensureDefaultGroup,
  isTunnelsSnapshot,
  mapRuntimeById,
  normalizeTunnel,
  normalizeTunnelGroupId,
  parseOptionalPort,
  sortTunnelGroups,
} from "../helpers/sshTunnelHelpers";
import type {
  SavedProbeEntry,
  SshTunnelConnectedSection,
  SshTunnelGroupStatus,
  TunnelBusyAction,
} from "../helpers/sshTunnelHelpers";

type PushToast = (toast: {
  title: string;
  description?: string;
  kind?: "info" | "success" | "warning" | "error" | "loading";
}) => string;

type UseSshTunnelsDataParams = {
  isVisible: boolean;
  initialTab?: string;
  navigationNonce?: number;
  t: TFunction;
  pushToast: PushToast;
};

export function useSshTunnelsData({
  isVisible,
  initialTab,
  navigationNonce,
  t,
  pushToast,
}: UseSshTunnelsDataParams) {
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [groups, setGroups] = useState<SshTunnelGroupView[]>([]);
  const [activeGroupId, setActiveGroupId] = useState(DEFAULT_TUNNEL_GROUP_ID);

  useEffect(() => {
    if (initialTab) {
      setActiveGroupId(initialTab);
    }
  }, [initialTab, navigationNonce]);
  const [tunnels, setTunnels] = useState<SshTunnelView[]>([]);
  const [runtimeMap, setRuntimeMap] = useState<Record<string, SshTunnelRuntimeView>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<TunnelFormState>(DEFAULT_TUNNEL_FORM);
  const [editorOpen, setEditorOpen] = useState(false);
  const [groupManagerOpen, setGroupManagerOpen] = useState(false);
  const [commonPorts, setCommonPorts] = useState<SshCommonPortView[]>([]);
  const [commonPortManagerOpen, setCommonPortManagerOpen] = useState(false);
  const [commonPortSubmitting, setCommonPortSubmitting] = useState(false);
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
    const result: Record<string, SshTunnelGroupStatus> = {};

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

      let tone: SshTunnelGroupStatus["tone"] = null;
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

  const connectedViewTone: SshTunnelGroupStatus["tone"] = useMemo(() => {
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

    const sections: SshTunnelConnectedSection[] = [];

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
    if (Array.isArray(snapshot.common_ports)) {
      setCommonPorts(snapshot.common_ports);
    }
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

  useTauriEvent(
    "ssh-tunnels-updated",
    (payload) => {
      if (isTunnelsSnapshot(payload)) {
        applySnapshot(payload);
        setLoading(false);
        return;
      }
      void loadData();
    },
    isTauri,
    { respectVisibility: true },
  );

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
    void notifyMessage.error({
      source: "ssh_tunnels",
      category,
      title,
      summary,
      error: detail,
      dedupeKey: `ssh-tunnels:${category}:${id || "draft"}`,
      target: { tab: "ssh-tunnels", entity_id: id },
    });
  };
  const formatProbeMessage = (probe?: SshTunnelProbeResult | null) => {
    if (!probe) return "";
    return probe.ok ? probe.message : formatTunnelError(probe.message);
  };

  return {
    hosts,
    setHosts,
    groups,
    setGroups,
    activeGroupId,
    setActiveGroupId,
    tunnels,
    setTunnels,
    runtimeMap,
    setRuntimeMap,
    loading,
    error,
    setError,
    form,
    setForm,
    editorOpen,
    setEditorOpen,
    groupManagerOpen,
    setGroupManagerOpen,
    commonPorts,
    setCommonPorts,
    commonPortManagerOpen,
    setCommonPortManagerOpen,
    commonPortSubmitting,
    setCommonPortSubmitting,
    saving,
    setSaving,
    groupSubmitting,
    setGroupSubmitting,
    draftProbe,
    setDraftProbe,
    savedProbeMap,
    setSavedProbeMap,
    busyAction,
    setBusyAction,
    groupBusyAction,
    setGroupBusyAction,
    groupMenuOpen,
    setGroupMenuOpen,
    openActionMenuId,
    setOpenActionMenuId,
    isTauri,
    modeCards,
    groupsById,
    isConnectedView,
    groupStatusMap,
    connectedTunnels,
    connectedViewTone,
    visibleTunnels,
    connectedGroupSections,
    getGroupLabel,
    getStatusLabel,
    getBusyOverlayLabel,
    hydrateForm,
    resetEditor,
    buildPayload,
    getSavedProbe,
    loadData,
    notify,
    formatTunnelError,
    recordTunnelMessage,
    formatProbeMessage,
  };
}

export type SshTunnelsData = ReturnType<typeof useSshTunnelsData>;
