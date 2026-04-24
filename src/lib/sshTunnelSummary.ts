import type { SshTunnelsSnapshot, SshTunnelRuntimeView } from "../components/sshTunnels/types";

export type SshTunnelHeaderSummary = {
  hasTunnels: boolean;
  connectedCount: number;
  disconnectedCount: number;
  connectingCount: number;
  reconnectingCount: number;
  totalCount: number;
  hasErrors: boolean;
  hasConnecting: boolean;
  errorTunnelNames: string[];
};

export function deriveSshTunnelHeaderSummary(
  snapshot: SshTunnelsSnapshot,
): SshTunnelHeaderSummary {
  const connectedCount = snapshot.runtime.filter(
    (r) => r.status === "connected",
  ).length;

  const errorRuntimes = snapshot.runtime.filter((r) => r.status === "error");
  const disconnectedCount = errorRuntimes.length;

  const connectingCount = snapshot.runtime.filter(
    (r) => r.status === "connecting",
  ).length;

  const reconnectingCount = snapshot.runtime.filter(
    (r) => r.status === "reconnecting",
  ).length;

  const hasTunnels = snapshot.tunnels.length > 0;

  const errorTunnelNames = errorRuntimes
    .map((r) => {
      const tunnel = snapshot.tunnels.find((t) => t.id === r.id);
      return tunnel?.name ?? r.summary;
    })
    .filter(Boolean);

  return {
    hasTunnels,
    connectedCount,
    disconnectedCount,
    connectingCount,
    reconnectingCount,
    totalCount: snapshot.runtime.length,
    hasErrors: disconnectedCount > 0,
    hasConnecting: connectingCount > 0 || reconnectingCount > 0,
    errorTunnelNames,
  };
}

export type LauncherSshTunnelSummary = {
  state: "connected" | "connecting" | "failed";
  connectedCount: number;
  autoConnectingCount: number;
  autoConnectFailedCount: number;
};

export function deriveSshTunnelLauncherSummary(
  snapshot: SshTunnelsSnapshot,
): LauncherSshTunnelSummary {
  const runtimeById = new Map<string, SshTunnelRuntimeView>(
    snapshot.runtime.map((runtime) => [runtime.id, runtime]),
  );

  const connectedCount = snapshot.runtime.filter(
    (runtime) => runtime.status === "connected",
  ).length;

  const autoConnectingCount = snapshot.tunnels.filter((tunnel) => {
    if (!tunnel.auto_connect) return false;
    return runtimeById.get(tunnel.id)?.status === "connecting";
  }).length;

  const autoConnectFailedCount = snapshot.tunnels.filter((tunnel) => {
    if (!tunnel.auto_connect) return false;
    const runtime = runtimeById.get(tunnel.id);
    if (runtime?.status === "error") return true;
    if (runtime?.status === "connected") return false;
    return Boolean(
      runtime?.last_error?.trim() || tunnel.last_error?.trim(),
    );
  }).length;

  const state =
    autoConnectFailedCount > 0
      ? "failed"
      : autoConnectingCount > 0
        ? "connecting"
        : "connected";

  return {
    state,
    connectedCount,
    autoConnectingCount,
    autoConnectFailedCount,
  };
}
