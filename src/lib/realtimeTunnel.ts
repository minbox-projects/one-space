import { invoke } from "@tauri-apps/api/core";

export interface TunnelRuntimeStatus {
  id: string;
  name: string;
  mode: "Cloudflare" | "Frp";
  status: "idle" | "starting" | "running" | "stopping" | "stopped" | "error";
  public_url?: string | null;
  error?: string | null;
  pid?: number | null;
  started_at?: number | null;
}

export const REALTIME_TUNNEL_EVENT = "realtime-tunnel-status";

export async function startRealtimeTunnel(localPort: number): Promise<TunnelRuntimeStatus> {
  return invoke<TunnelRuntimeStatus>("realtime_tunnel_start", { localPort });
}

export async function stopRealtimeTunnel(
  localPort: number,
): Promise<TunnelRuntimeStatus | null> {
  return invoke<TunnelRuntimeStatus | null>("realtime_tunnel_stop", { localPort });
}

export async function getRealtimeTunnelStatus(
  localPort: number,
): Promise<TunnelRuntimeStatus | null> {
  return invoke<TunnelRuntimeStatus | null>("realtime_tunnel_status", { localPort });
}
