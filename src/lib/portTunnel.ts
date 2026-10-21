import { invoke } from "@tauri-apps/api/core";

export type TunnelMode = "cloudflare" | "frp";
export type TunnelProtocol = "http" | "tcp";

export interface TunnelConfig {
  id: string;
  name: string;
  mode: TunnelMode;
  local_port: number;
  protocol: TunnelProtocol;
  remote_port?: number;
  frp_server?: string;
  frp_token?: string;
  custom_domain?: string;
  auto_start: boolean;
  created_at?: number;
  updated_at?: number;
}

export interface TunnelRuntimeStatus {
  id: string;
  state: "stopped" | "starting" | "running" | "error";
  public_url?: string;
  error?: string;
  started_at?: number;
  active_connections?: number;
}

export interface EnvironmentStatus {
  mode: TunnelMode;
  installed: boolean;
  binary_path?: string;
  version?: string;
  recommended_command: string;
}

export const TUNNEL_STATUS_CHANGED_EVENT = "tunnel-status-changed";

export async function tunnelList(): Promise<TunnelConfig[]> {
  return invoke<TunnelConfig[]>("tunnel_list");
}

export async function tunnelSave(config: TunnelConfig): Promise<TunnelConfig> {
  return invoke<TunnelConfig>("tunnel_save", { config });
}

export async function tunnelDelete(id: string): Promise<void> {
  return invoke<void>("tunnel_delete", { id });
}

export async function tunnelStart(id: string): Promise<TunnelRuntimeStatus> {
  return invoke<TunnelRuntimeStatus>("tunnel_start", { id });
}

export async function tunnelStop(id: string): Promise<TunnelRuntimeStatus> {
  return invoke<TunnelRuntimeStatus>("tunnel_stop", { id });
}

export async function tunnelStatus(id: string): Promise<TunnelRuntimeStatus> {
  return invoke<TunnelRuntimeStatus>("tunnel_status", { id });
}

export async function tunnelListStatus(): Promise<TunnelRuntimeStatus[]> {
  return invoke<TunnelRuntimeStatus[]>("tunnel_list_status");
}

export async function tunnelGetLogs(id: string): Promise<string[]> {
  return invoke<string[]>("tunnel_get_logs", { id });
}

export async function tunnelCheckEnvironment(mode: TunnelMode): Promise<EnvironmentStatus> {
  return invoke<EnvironmentStatus>("tunnel_check_environment", { mode });
}
