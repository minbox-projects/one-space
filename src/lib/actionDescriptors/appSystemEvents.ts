import type { TFunction } from "i18next";
import type { SystemEventDescriptor } from "@/lib/userActions";
import { localizeSshTunnelError } from "@/lib/sshTunnelI18n";

export interface UpdaterEventInput {
  version?: string | null;
  currentVersion?: string | null;
  body?: string | null;
  status: string;
  error?: string | null;
  showUpdateIndicator: boolean;
  source?: string | null;
}

export function buildUpdaterSystemEvent(
  t: TFunction,
  input: UpdaterEventInput,
): { key: string; descriptor: SystemEventDescriptor } | null {
  const version = input.version || "";

  if (version && input.showUpdateIndicator && input.status === "available") {
    const key = `updater:available:${version}`;
    return {
      key,
      descriptor: {
        source: "updater",
        category: "update",
        severity: "info",
        target: { tab: "settings", section: "updates" },
        metadata: {
          version,
          currentVersion: input.currentVersion,
          source: input.source,
        },
        message: {
          title: t(
            "updateAvailableMessageTitle",
            "Installable update available",
          ),
          summary: t(
            "updateAvailableMessageSummary",
            "OneSpace {{version}} is available",
            { version },
          ),
          detail: input.body || undefined,
        },
        dedupeKey: key,
        toast: false,
      },
    };
  }

  if (version && input.status === "downloaded") {
    const key = `updater:downloaded:${version}`;
    return {
      key,
      descriptor: {
        source: "updater",
        category: "update",
        severity: "success",
        target: { tab: "settings", section: "updates" },
        metadata: {
          version,
          currentVersion: input.currentVersion,
        },
        message: {
          title: t("updateDownloadedMessageTitle", "Update downloaded"),
          summary: t(
            "updateDownloadedMessageSummary",
            "OneSpace {{version}} has been downloaded and is ready to install",
            { version },
          ),
          detail: input.body || undefined,
        },
        dedupeKey: key,
        toast: false,
      },
    };
  }

  if (input.status === "error" && input.error) {
    return {
      key: `updater:error:${input.error}`,
      descriptor: {
        source: "updater",
        category: "update",
        severity: "error",
        target: { tab: "settings", section: "updates" },
        message: {
          title: t(
            "updateFailedMessageTitle",
            "Update check or install failed",
          ),
          summary: input.error,
          detail: input.error,
        },
        dedupeKey: "updater:error",
        toast: true,
        toastKind: "error",
      },
    };
  }

  return null;
}

export function buildSshAutoConnectFailedEvent(
  t: TFunction,
  payload: { name?: string; error?: string },
): SystemEventDescriptor {
  const tunnelName = payload.name || t("sshTunnelUnnamed", "Unnamed tunnel");
  const text = payload.error
    ? `${tunnelName}: ${localizeSshTunnelError(t, payload.error)}`
    : t(
        "sshTunnelAutoConnectFailed",
        "A tunnel failed to connect automatically.",
      );

  return {
    source: "ssh_tunnels",
    category: "connect",
    action: "auto-connect-failed",
    severity: "error",
    dedupeKey: `ssh-tunnels:auto-connect-failed:${tunnelName}`,
    target: { tab: "ssh-tunnels" },
    message: {
      title: t("sshTunnels", "SSH Tunnels"),
      summary: text,
      detail: text,
    },
    toast: true,
    toastKind: "error",
  };
}

export function buildSshUnexpectedDisconnectEvent(
  t: TFunction,
  name: string,
): SystemEventDescriptor {
  const text = t(
    "sshTunnelDisconnectedToast",
    "SSH tunnel {{name}} disconnected",
    { name },
  );

  return {
    source: "ssh_tunnels",
    category: "disconnect",
    action: "unexpected-disconnect",
    severity: "error",
    dedupeKey: `ssh-tunnels:unexpected-disconnect:${name}`,
    target: { tab: "ssh-tunnels" },
    message: {
      title: t("sshTunnelStatusIndicatorTitle", "SSH Tunnels"),
      summary: text,
      detail: text,
    },
    toast: true,
    toastKind: "error",
  };
}

export function buildSshWindowReconnectDoneEvent(
  t: TFunction,
  payload: { total?: number; succeeded?: number; failed?: number },
): SystemEventDescriptor | null {
  const total = payload.total ?? 0;
  const failed = payload.failed ?? 0;
  const succeeded = payload.succeeded ?? 0;

  if (total === 0) {
    return null;
  }

  if (failed === 0) {
    return {
      source: "ssh_tunnels",
      category: "connect",
      action: "window-reconnect-done",
      severity: "success",
      dedupeKey: `ssh-tunnels:window-reconnect:success:${total}`,
      target: { tab: "ssh-tunnels" },
      message: {
        title: t("sshTunnelReconnectDone", "SSH Tunnels Reconnected"),
        summary:
          total > 1
            ? t(
                "sshTunnelReconnectAllSuccess",
                "All {{count}} tunnels reconnected successfully",
                { count: total },
              )
            : t(
                "sshTunnelReconnectSuccess",
                "1 tunnel reconnected successfully",
              ),
      },
      toast: true,
      toastKind: "success",
    };
  }

  if (succeeded === 0) {
    return {
      source: "ssh_tunnels",
      category: "connect",
      action: "window-reconnect-failed",
      severity: "error",
      dedupeKey: `ssh-tunnels:window-reconnect:failed:${total}`,
      target: { tab: "ssh-tunnels" },
      message: {
        title: t("sshTunnelReconnectFailed", "SSH Tunnel Reconnection Failed"),
        summary:
          total > 1
            ? t(
                "sshTunnelReconnectAllFailed",
                "All {{count}} tunnels failed to reconnect",
                { count: total },
              )
            : t(
                "sshTunnelReconnectSingleFailed",
                "1 tunnel failed to reconnect",
              ),
      },
      toast: true,
      toastKind: "error",
    };
  }

  return {
    source: "ssh_tunnels",
    category: "connect",
    action: "window-reconnect-partial",
    severity: "error",
    dedupeKey: `ssh-tunnels:window-reconnect:partial:${total}:${failed}`,
    target: { tab: "ssh-tunnels" },
    message: {
      title: t(
        "sshTunnelReconnectPartial",
        "SSH Tunnels Partially Reconnected",
      ),
      summary: t(
        "sshTunnelReconnectPartialDetail",
        "{{succeeded}} reconnected, {{failed}} failed",
        {
          succeeded,
          failed,
        },
      ),
    },
    toast: true,
    toastKind: "error",
  };
}
