import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import {
  Activity,
  Loader2,
  MoreHorizontal,
  Pencil,
  Play,
  Trash2,
  Unplug,
} from "lucide-react";
import { ToolStatusBadge } from "../../toolbox/ToolStatusBadge";
import type {
  SshTunnelProbeResult,
  SshTunnelRuntimeView,
  SshTunnelStatus,
  SshTunnelView,
} from "../types";
import {
  modeShort,
  statusTone,
  tunnelErrorDisplay,
} from "../helpers/sshTunnelHelpers";
import type { TunnelBusyAction } from "../helpers/sshTunnelHelpers";

type TunnelCardProps = {
  t: TFunction;
  tunnel: SshTunnelView;
  runtime?: SshTunnelRuntimeView;
  probe: SshTunnelProbeResult | null;
  busyAction: { id: string; kind: TunnelBusyAction } | null;
  groupBusyAction: "connect" | "disconnect" | null;
  openActionMenuId: string | null;
  setOpenActionMenuId: Dispatch<SetStateAction<string | null>>;
  getBusyOverlayLabel: (kind: Exclude<TunnelBusyAction, "delete">) => string;
  getGroupLabel: (groupId?: string | null) => string;
  getStatusLabel: (status: SshTunnelStatus) => string;
  formatProbeMessage: (probe?: SshTunnelProbeResult | null) => string;
  formatTunnelError: (error: unknown) => string;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onProbe: (id: string) => void;
  onEdit: (tunnel: SshTunnelView) => void;
  onDelete: (tunnel: SshTunnelView) => void;
};

export function TunnelCard({
  t,
  tunnel,
  runtime,
  probe,
  busyAction,
  groupBusyAction,
  openActionMenuId,
  setOpenActionMenuId,
  getBusyOverlayLabel,
  getGroupLabel,
  getStatusLabel,
  formatProbeMessage,
  formatTunnelError,
  onConnect,
  onDisconnect,
  onProbe,
  onEdit,
  onDelete,
}: TunnelCardProps) {
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
                void onDisconnect(tunnel.id);
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
                void onConnect(tunnel.id);
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
                  void onProbe(tunnel.id);
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
                  onEdit(tunnel);
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
                  void onDelete(tunnel);
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
}
