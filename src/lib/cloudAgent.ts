import { invoke } from "@tauri-apps/api/core";

export interface CloudAgentSettings {
  defaultHost: string;
  remoteWorkspaceRoot: string;
  remoteAgentDir: string;
  opencodeYoloMode: boolean;
  opencodeExtraArgs: string;
  autoRefreshIntervalSeconds: number;
  diskAlertThresholdPercent: number;
  terminalApp: string;
}

export const DEFAULT_CLOUD_AGENT_SETTINGS: CloudAgentSettings = {
  defaultHost: "dmit-server",
  remoteWorkspaceRoot: "/root/cloud-agent-workspaces",
  remoteAgentDir: "/root/onespace-cloud-agent",
  opencodeYoloMode: true,
  opencodeExtraArgs: "",
  autoRefreshIntervalSeconds: 10,
  diskAlertThresholdPercent: 80,
  terminalApp: "auto",
};

export interface CloudSession {
  session_name: string;
  repo_name: string;
  branch: string;
  working_dir: string;
  created_at: number;
  windows_count: number;
  pane_pid: number;
  is_active: boolean;
  worktree_name?: string;
  opencode_session_id?: string | null;
}

export interface LastCommit {
  hash: string;
  author: string;
  timestamp: number;
  message: string;
}

export interface CloudWorktree {
  repo_name: string;
  branch: string;
  worktree_path: string;
  has_active_session: boolean;
  session_name: string;
  last_commit?: LastCommit | null;
  last_modified: number;
}

export interface DiskUsage {
  total_gb: number;
  used_gb: number;
  free_gb: number;
  use_percent: number;
}

export interface CloudGatewayModel {
  id: string;
  upstream_model: string;
  display_name?: string | null;
  context_window?: number | null;
  reasoning_efforts?: string[];
}

export interface CloudGatewayProvider {
  id: string;
  name: string;
  base_url: string;
  is_active: boolean;
  models: CloudGatewayModel[];
}

export interface GatewayHealth {
  status: string;
  port?: number;
  providers_count?: number;
  model_pools?: string[];
  providers?: CloudGatewayProvider[];
  error?: string;
}

export interface CloudAgentStatus {
  service_active: boolean;
  disk: DiskUsage;
  gateway: GatewayHealth;
  timestamp: number;
}

export interface CloudAgentTask {
  id: string;
  repo: string;
  issue_number: number;
  title: string;
  status: "running" | "completed" | "failed" | "no_changes" | string;
  started_at: number;
  finished_at?: number | null;
  duration_seconds?: number | null;
  branch?: string | null;
  pr_url?: string | null;
  error_message?: string | null;
}

export interface GatewaySyncResult {
  success: boolean;
  providers_synced: number;
  model_pools_synced: number;
  message: string;
}

// ==================== Tauri Command Invocation Wrappers ====================

export async function cloudAgentGetSettings(): Promise<CloudAgentSettings> {
  const res = await invoke<{
    default_host: string;
    remote_workspace_root: string;
    remote_agent_dir: string;
    opencode_yolo_mode: boolean;
    opencode_extra_args: string;
    auto_refresh_interval_seconds: number;
    disk_alert_threshold_percent: number;
    terminal_app?: string;
  }>("cloud_agent_get_settings");

  return {
    defaultHost: res.default_host,
    remoteWorkspaceRoot: res.remote_workspace_root,
    remoteAgentDir: res.remote_agent_dir,
    opencodeYoloMode: res.opencode_yolo_mode,
    opencodeExtraArgs: res.opencode_extra_args,
    autoRefreshIntervalSeconds: res.auto_refresh_interval_seconds,
    diskAlertThresholdPercent: res.disk_alert_threshold_percent,
    terminalApp: res.terminal_app || "auto",
  };
}

export async function cloudAgentSaveSettings(
  settings: CloudAgentSettings
): Promise<void> {
  await invoke("cloud_agent_save_settings", {
    settings: {
      default_host: settings.defaultHost,
      remote_workspace_root: settings.remoteWorkspaceRoot,
      remote_agent_dir: settings.remoteAgentDir,
      opencode_yolo_mode: settings.opencodeYoloMode,
      opencode_extra_args: settings.opencodeExtraArgs,
      auto_refresh_interval_seconds: settings.autoRefreshIntervalSeconds,
      disk_alert_threshold_percent: settings.diskAlertThresholdPercent,
      terminal_app: settings.terminalApp,
    },
  });
}

export async function cloudAgentGetStatus(
  host?: string
): Promise<CloudAgentStatus> {
  return await invoke<CloudAgentStatus>("cloud_agent_get_status", { host });
}

export async function cloudAgentListSessions(
  host?: string
): Promise<CloudSession[]> {
  return await invoke<CloudSession[]>("cloud_agent_list_sessions", { host });
}

export async function cloudAgentListWorktrees(
  host?: string
): Promise<CloudWorktree[]> {
  return await invoke<CloudWorktree[]>("cloud_agent_list_worktrees", { host });
}

export async function cloudAgentListTasks(
  host?: string
): Promise<CloudAgentTask[]> {
  return await invoke<CloudAgentTask[]>("cloud_agent_list_tasks", { host });
}

export interface CreateSessionParams {
  host?: string;
  repo: string;
  branch?: string;
  useYolo?: boolean;
  extraArgs?: string;
}

export async function cloudAgentCreateSession(
  params: CreateSessionParams
): Promise<any> {
  return await invoke("cloud_agent_create_session", {
    host: params.host,
    repo: params.repo,
    branch: params.branch,
    useYolo: params.useYolo,
    extraArgs: params.extraArgs,
  });
}

export async function cloudAgentKillSession(
  sessionName: string,
  host?: string
): Promise<boolean> {
  return await invoke<boolean>("cloud_agent_kill_session", {
    host,
    sessionName,
  });
}

export interface LaunchTerminalParams {
  host?: string;
  target: string;
  branch?: string;
  useYolo?: boolean;
  extraArgs?: string;
  terminalApp?: string;
}

export async function cloudAgentLaunchTerminal(
  params: LaunchTerminalParams
): Promise<void> {
  await invoke("cloud_agent_launch_terminal", {
    host: params.host,
    target: params.target,
    branch: params.branch,
    useYolo: params.useYolo,
    extraArgs: params.extraArgs,
    terminalApp: params.terminalApp,
  });
}

export async function cloudAgentSyncGateway(
  host?: string
): Promise<GatewaySyncResult> {
  return await invoke<GatewaySyncResult>("cloud_agent_sync_gateway", { host });
}

/**
 * 格式化工作目录路径：
 * - 隐藏前部分超长前缀，完整显示后部目录与特性文件夹；
 * - 当路径短于或等于 maxLen 时完整展示；
 * - 超过 maxLen 时，在前端追加省略号“…”并保留尾部有效路径。
 */
export function formatWorkingDirTail(
  path: string,
  maxLen = 38
): { display: string; isTruncated: boolean; fullPath: string } {
  if (!path) {
    return { display: "—", isTruncated: false, fullPath: "" };
  }
  if (path.length <= maxLen) {
    return { display: path, isTruncated: false, fullPath: path };
  }

  const tail = path.slice(-maxLen);
  const slashIdx = tail.indexOf("/");
  let displayTail = tail;
  if (slashIdx > 0 && slashIdx < tail.length - 8) {
    displayTail = tail.slice(slashIdx);
  }
  const prefix = displayTail.startsWith("/") ? "…" : "…/";
  return {
    display: `${prefix}${displayTail}`,
    isTruncated: true,
    fullPath: path,
  };
}
