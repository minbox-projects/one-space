import type { TunnelRuntimeStatus } from "./portTunnel";

export interface PortTunnelHeaderSummary {
  connectedCount: number;
  startingCount: number;
  errorCount: number;
  hasErrors: boolean;
  hasStarting: boolean;
  activeCount: number;
}

export function derivePortTunnelHeaderSummary(
  statuses: TunnelRuntimeStatus[],
): PortTunnelHeaderSummary {
  const list = Array.isArray(statuses) ? statuses : [];
  const connectedCount = list.filter((s) => s.state === "running").length;
  const startingCount = list.filter((s) => s.state === "starting").length;
  const errorCount = list.filter((s) => s.state === "error").length;

  return {
    connectedCount,
    startingCount,
    errorCount,
    hasErrors: errorCount > 0,
    hasStarting: startingCount > 0,
    activeCount: connectedCount + startingCount,
  };
}
