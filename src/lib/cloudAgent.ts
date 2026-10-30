import { invoke } from "@tauri-apps/api/core";

export const CLOUD_AGENT_SESSIONS_COUNT_EVENT =
  "onespace:cloud-agent-sessions-count";

export const CLOUD_AGENT_SETTINGS_UPDATED_EVENT =
  "onespace:cloud-agent-settings-updated";

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
  defaultHost: "",
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
  author?: string | null;
  trigger_type?: "first_run" | "follow_up" | string | null;
  attempts?: number | null;
  max_attempts?: number | null;
  diff_stat?: string | null;
  step?: string | null;
}

export interface GatewaySyncResult {
  success: boolean;
  providers_synced: number;
  model_pools_synced: number;
  message: string;
}

export const CLOUD_AGENT_DEPLOY_PROGRESS_EVENT =
  "cloud_agent_deploy_progress";

export type CloudAgentInstallationState =
  | "installed"
  | "incomplete"
  | "not_installed"
  | "probe_error";

export interface CloudAgentInstallationInfo {
  host: string;
  state: CloudAgentInstallationState;
  localLatestVersion: string | null;
  remoteVersion?: string | null;
  installedAt?: string | null;
  metadataReason?: string | null;
  configurationError?: string | null;
  serviceActive?: boolean | null;
  probeReason?: string | null;
}

export type CloudAgentDeployPhase =
  | "platform"
  | "dependencies"
  | "upload"
  | "config"
  | "publish"
  | "service"
  | "metadata"
  | "cleanup";

export type CloudAgentDeployPhaseStatus = "start" | "ok" | "error";

export interface CloudAgentDeploymentProgress {
  host: string;
  operationId: string;
  phase: CloudAgentDeployPhase;
  status: CloudAgentDeployPhaseStatus;
  reason?: string | null;
}

export interface CloudAgentDeploymentResult {
  host: string;
  operationId: string;
  success: boolean;
  phase: CloudAgentDeployPhase;
  reason?: string | null;
  remoteVersion?: string | null;
  installedAt?: string | null;
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
  if (typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent(CLOUD_AGENT_SETTINGS_UPDATED_EVENT, { detail: settings })
    );
  }
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

function mapInstallationInfo(raw: any): CloudAgentInstallationInfo {
  return {
    host: raw?.host ?? "",
    state: raw?.state ?? "probe_error",
    localLatestVersion: raw?.local_latest_version ?? null,
    remoteVersion: raw?.remote_version ?? null,
    installedAt: raw?.installed_at ?? null,
    metadataReason: raw?.metadata_reason ?? null,
    configurationError: raw?.configuration_error ?? null,
    serviceActive: raw?.service_active ?? null,
    probeReason: raw?.probe_reason ?? null,
  };
}

export async function cloudAgentGetInstallationInfo(
  host?: string
): Promise<CloudAgentInstallationInfo> {
  const raw = await invoke<any>("cloud_agent_get_installation_info", { host });
  return mapInstallationInfo(raw);
}

export async function cloudAgentDeploy(
  host: string,
  operationId: string
): Promise<CloudAgentDeploymentResult> {
  const raw = await invoke<any>("cloud_agent_deploy", { host, operationId });
  return {
    host: raw?.host ?? host,
    operationId: raw?.operation_id ?? operationId,
    success: Boolean(raw?.success),
    phase: (raw?.phase ?? "metadata") as CloudAgentDeployPhase,
    reason: raw?.reason ?? null,
    remoteVersion: raw?.remote_version ?? null,
    installedAt: raw?.installed_at ?? null,
  };
}

/** Normalize a snake_case `cloud_agent_deploy_progress` payload. */
export function mapDeploymentProgress(
  raw: any
): CloudAgentDeploymentProgress {
  return {
    host: raw?.host ?? "",
    operationId: raw?.operation_id ?? "",
    phase: raw?.phase as CloudAgentDeployPhase,
    status: raw?.status as CloudAgentDeployPhaseStatus,
    reason: raw?.reason ?? null,
  };
}

export interface ParsedSemver {
  major: number;
  minor: number;
  patch: number;
  prerelease: string[];
}

const SEMVER_PATTERN =
  /^[vV]?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?$/;

/** Parse a full semantic version; invalid or absent input yields `null`. */
export function parseSemver(value: unknown): ParsedSemver | null {
  if (typeof value !== "string") return null;
  const match = value.trim().match(SEMVER_PATTERN);
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] ? match[4].split(".") : [],
  };
}

function comparePrerelease(left: string[], right: string[]): number {
  if (left.length === 0 && right.length === 0) return 0;
  if (left.length === 0) return 1;
  if (right.length === 0) return -1;
  const max = Math.max(left.length, right.length);
  for (let i = 0; i < max; i += 1) {
    const l = left[i];
    const r = right[i];
    if (l === undefined) return -1;
    if (r === undefined) return 1;
    const lNumeric = /^\d+$/.test(l);
    const rNumeric = /^\d+$/.test(r);
    if (lNumeric && rNumeric) {
      const diff = Number(l) - Number(r);
      if (diff !== 0) return diff < 0 ? -1 : 1;
    } else if (lNumeric !== rNumeric) {
      return lNumeric ? -1 : 1;
    } else if (l !== r) {
      return l < r ? -1 : 1;
    }
  }
  return 0;
}

/**
 * Numeric semantic-version comparison. Build metadata is ignored and an
 * invalid or absent version returns `null`.
 */
export function compareSemver(left: unknown, right: unknown): number | null {
  const a = parseSemver(left);
  const b = parseSemver(right);
  if (!a || !b) return null;
  if (a.major !== b.major) return a.major < b.major ? -1 : 1;
  if (a.minor !== b.minor) return a.minor < b.minor ? -1 : 1;
  if (a.patch !== b.patch) return a.patch < b.patch ? -1 : 1;
  return comparePrerelease(a.prerelease, b.prerelease);
}

export const compareVersions = compareSemver;

/**
 * Compare the remote version against the locally bundled version.
 * `remote` is reported as older/newer/equal relative to `local`.
 */
export function compareSemanticVersions(
  remote: unknown,
  local: unknown
): "older" | "equal" | "newer" | "unknown" {
  const result = compareSemver(remote, local);
  if (result === null) return "unknown";
  if (result < 0) return "older";
  if (result > 0) return "newer";
  return "equal";
}

const notRecordedLabel = (language?: string) =>
  language?.toLowerCase().startsWith("zh") ? "未记录" : "Not recorded";

/** Format a UTC ISO-8601 installation timestamp in the user's locale. */
export function formatInstalledAt(
  value: string | number | null | undefined,
  language = "en"
): string {
  const fallback = notRecordedLabel(language);
  if (value === null || value === undefined) return fallback;

  let date: Date;
  if (typeof value === "number") {
    if (!Number.isFinite(value) || value === 0) return fallback;
    date = new Date(value);
  } else {
    const trimmed = value.trim();
    if (!trimmed || trimmed === "0") return fallback;
    date = new Date(trimmed);
  }
  if (Number.isNaN(date.getTime())) return fallback;

  const locale = language?.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
  try {
    return new Intl.DateTimeFormat(locale, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      timeZoneName: "short",
    }).format(date);
  } catch {
    return fallback;
  }
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

/**
 * 格式化任务耗时为人性化可读字符串（如 "45秒" / "2分15秒" / "1小时10分"）
 */
export function formatFriendlyDuration(
  seconds: number | null | undefined,
  language = "zh"
): string {
  if (seconds === null || seconds === undefined || seconds < 0) {
    return language === "zh" ? "0秒" : "0s";
  }
  const isZh = language === "zh";
  const s = Math.floor(seconds % 60);
  const m = Math.floor((seconds / 60) % 60);
  const h = Math.floor(seconds / 3600);

  if (h > 0) {
    return isZh
      ? `${h}小时${m > 0 ? `${m}分` : ""}`
      : `${h}h${m > 0 ? ` ${m}m` : ""}`;
  }
  if (m > 0) {
    return isZh
      ? `${m}分${s > 0 ? `${s}秒` : ""}`
      : `${m}m${s > 0 ? ` ${s}s` : ""}`;
  }
  return isZh ? `${s}秒` : `${s}s`;
}

/**
 * 从 PR 链接中提取 PR 编号（如从 https://github.com/org/repo/pull/105 提取 "#105"）
 */
export function extractPrNumber(prUrl: string | null | undefined): string | null {
  if (!prUrl) return null;
  const match = prUrl.match(/\/(?:pull|merge_requests)\/(\d+)/i);
  return match ? `#${match[1]}` : null;
}

/**
 * 生成规范的本地 Git 检出命令
 */
export function formatGitCheckoutCommand(branch: string): string {
  if (!branch) return "";
  return `git fetch origin ${branch} && git checkout ${branch}`;
}

/**
 * 格式化 Issue 标签显示：隐藏前部分前缀（如长 URL、组织名等），完整显示后部仓库名与 #Issue 编号
 */
export function formatIssueLabelTail(
  repo: string,
  issueNumber: number | string,
  maxRepoLen = 18
): string {
  if (!repo && !issueNumber) return "";
  const numStr = issueNumber ? `#${issueNumber}` : "";
  if (!repo) return numStr;

  const cleanRepo = repo.trim().replace(/\.git$/, "").replace(/\/+$/, "");

  // 1. 如果包含斜杠路径（包括 URL、SSH 路径或 owner/repo）
  const slashIdx = cleanRepo.lastIndexOf("/");
  if (slashIdx !== -1) {
    const tailRepo = cleanRepo.slice(slashIdx + 1);
    if (tailRepo.length > maxRepoLen) {
      return `…${tailRepo.slice(-maxRepoLen)}${numStr}`;
    }
    return `…/${tailRepo}${numStr}`;
  }

  // 2. 如果包含 SSH 冒号路径（如 git@github.com:repo）
  const colonIdx = cleanRepo.lastIndexOf(":");
  if (colonIdx !== -1) {
    const tailRepo = cleanRepo.slice(colonIdx + 1);
    return `…/${tailRepo}${numStr}`;
  }

  // 3. 单纯仓库名如果超长，保留尾部
  if (cleanRepo.length > maxRepoLen) {
    return `…${cleanRepo.slice(-maxRepoLen)}${numStr}`;
  }

  return `${cleanRepo}${numStr}`;
}
