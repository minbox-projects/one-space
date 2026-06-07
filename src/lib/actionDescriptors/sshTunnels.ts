import type { TFunction } from "i18next";
import type { ActionDescriptor } from "@/lib/userActions";

export function buildConnectTunnelActionDescriptor(
  t: TFunction,
  id: string,
): ActionDescriptor {
  return {
    source: "ssh_tunnels",
    category: "connect",
    action: "connect-tunnel",
    target: { tab: "ssh-tunnels", entity_id: id },
    dedupeKey: `ssh-tunnels:connect:${id}`,
    success: {
      title: t("sshTunnelConnectSuccess", "Tunnel connected successfully."),
    },
    error: {
      title: t("sshTunnelConnectFailed", "Failed to connect tunnel."),
    },
  };
}

export function buildDisconnectTunnelActionDescriptor(
  t: TFunction,
  id: string,
): ActionDescriptor {
  return {
    source: "ssh_tunnels",
    category: "disconnect",
    action: "disconnect-tunnel",
    target: { tab: "ssh-tunnels", entity_id: id },
    dedupeKey: `ssh-tunnels:disconnect:${id}`,
    success: {
      title: t(
        "sshTunnelDisconnectSuccess",
        "Tunnel disconnected successfully.",
      ),
    },
    error: {
      title: t("sshTunnelDisconnectFailed", "Failed to disconnect tunnel."),
    },
  };
}

export function buildDeleteTunnelActionDescriptor(
  t: TFunction,
  input: { id: string; name: string },
): ActionDescriptor {
  return {
    source: "ssh_tunnels",
    category: "delete",
    action: "delete-tunnel",
    target: { tab: "ssh-tunnels", entity_id: input.id },
    dedupeKey: `ssh-tunnels:delete:${input.id}`,
    confirm: {
      message: t("confirmDelete", { name: input.name }),
      okLabel: t("delete", "Delete"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "error",
    },
    success: {
      title: t("sshTunnelDeleted", "Tunnel deleted"),
      summary: t(
        "sshTunnelDeletedSummary",
        "Tunnel deleted successfully.",
      ),
    },
    error: {
      title: t("sshTunnelDeleteFailed", "Failed to delete tunnel"),
    },
  };
}
