export type SshHost = {
  name: string;
  host_name: string;
  user: string;
  port: number;
};

export type SshTunnelSourceKind = "saved_host" | "custom";
export type SshTunnelAuthKind = "password" | "key";
export type SshTunnelForwardMode = "local" | "remote" | "dynamic";
export type SshTunnelStatus = "disconnected" | "connecting" | "connected" | "error";

export type SshTunnelForwardConfig = {
  mode: SshTunnelForwardMode;
  local_bind_host?: string | null;
  local_port?: number | null;
  remote_bind_host?: string | null;
  remote_port?: number | null;
  target_host?: string | null;
  target_port?: number | null;
  dynamic_probe_host?: string | null;
  dynamic_probe_port?: number | null;
};

export type SshTunnelCustomView = {
  host: string;
  port: number;
  user: string;
  auth_kind: SshTunnelAuthKind;
  key_path?: string | null;
  has_password: boolean;
};

export type SshTunnelGroupView = {
  id: string;
  name: string;
  created_at: number;
  updated_at: number;
  is_default: boolean;
};

export type SshTunnelView = {
  id: string;
  name: string;
  group_id: string;
  source_kind: SshTunnelSourceKind;
  saved_host_name?: string | null;
  custom?: SshTunnelCustomView | null;
  forward: SshTunnelForwardConfig;
  auto_connect: boolean;
  created_at: number;
  updated_at: number;
  last_connected_at?: number | null;
  last_error?: string | null;
};

export type SshTunnelRuntimeView = {
  id: string;
  status: SshTunnelStatus;
  active_client_count: number;
  mode: SshTunnelForwardMode;
  summary: string;
  resolved_server_host?: string | null;
  listening_addr?: string | null;
  last_error?: string | null;
};

export type SshTunnelProbeResult = {
  ok: boolean;
  mode: SshTunnelForwardMode;
  summary: string;
  message: string;
  last_error?: string | null;
};

export type SshTunnelsSnapshot = {
  groups: SshTunnelGroupView[];
  tunnels: SshTunnelView[];
  runtime: SshTunnelRuntimeView[];
};

export type TunnelFormState = {
  id?: string;
  name: string;
  group_id: string;
  source_kind: SshTunnelSourceKind;
  saved_host_name: string;
  custom_host: string;
  custom_port: string;
  custom_user: string;
  custom_auth_kind: SshTunnelAuthKind;
  custom_key_path: string;
  custom_password: string;
  preserve_password: boolean;
  forward_mode: SshTunnelForwardMode;
  local_port: string;
  remote_bind_host: string;
  remote_port: string;
  target_host: string;
  target_port: string;
  dynamic_probe_host: string;
  dynamic_probe_port: string;
  auto_connect: boolean;
};

export const DEFAULT_TUNNEL_GROUP_ID = "default";

export const DEFAULT_TUNNEL_FORM: TunnelFormState = {
  name: "",
  group_id: "",
  source_kind: "saved_host",
  saved_host_name: "",
  custom_host: "",
  custom_port: "22",
  custom_user: "root",
  custom_auth_kind: "password",
  custom_key_path: "",
  custom_password: "",
  preserve_password: false,
  forward_mode: "local",
  local_port: "5432",
  remote_bind_host: "127.0.0.1",
  remote_port: "15432",
  target_host: "127.0.0.1",
  target_port: "5432",
  dynamic_probe_host: "",
  dynamic_probe_port: "",
  auto_connect: false,
};
