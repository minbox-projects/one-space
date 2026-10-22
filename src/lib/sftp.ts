import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface RemoteFileEntry {
  name: string;
  is_dir: boolean;
  size: number;
  permissions: string;
  modified: string;
  path: string;
}

export interface SftpDirectoryResult {
  current_path: string;
  entries: RemoteFileEntry[];
}

export interface SftpDeleteResult {
  original_path: string;
  trash_path?: string | null;
  is_permanent: boolean;
}

export interface SftpTrashItem {
  id: string;
  name: string;
  original_path: string;
  trash_path: string;
  size: number;
  is_dir: boolean;
  deleted_at: string;
}

export interface SshHostOption {
  name: string;
  host_name: string;
  user: string;
  port: number;
}

export type SftpTransferType = "upload" | "download";
export type SftpTransferStatus =
  | "transferring"
  | "completed"
  | "error"
  | "cancelled";

export interface SftpRetryParams {
  host: string;
  remote_path: string;
  local_path: string;
  file_size?: number;
}

export interface SftpTransferProgressPayload {
  task_id: string;
  transfer_type: SftpTransferType;
  file_name: string;
  transferred_bytes: number;
  total_bytes: number;
  percentage: number;
  bytes_per_second: number;
  eta_seconds?: number | null;
  status: SftpTransferStatus;
  error?: string | null;
  retry_params?: SftpRetryParams;
}

export interface SftpDownloadInput {
  taskId: string;
  host: string;
  remotePath: string;
  localPath: string;
  fileSize?: number;
}

export interface SftpUploadInput {
  taskId: string;
  host: string;
  localPath: string;
  remotePath: string;
}

export class SftpError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SftpError";
  }
}

function normalizeError(error: unknown): SftpError {
  if (error instanceof SftpError) return error;
  if (error instanceof Error) return new SftpError(error.message);
  if (typeof error === "string") return new SftpError(error);
  try {
    return new SftpError(JSON.stringify(error) || "SFTP operation failed");
  } catch {
    return new SftpError("SFTP operation failed");
  }
}

/**
 * 格式化字节大小为易读字符串 (B / KB / MB / GB)
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const val = bytes / Math.pow(1024, i);
  return `${val >= 100 || i === 0 ? val.toFixed(0) : val.toFixed(1)} ${units[i]}`;
}

/**
 * 格式化传输网速 (如 3.2 MB/s)
 */
export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return "0 B/s";
  return `${formatBytes(bytesPerSecond)}/s`;
}

/**
 * 格式化剩余传输时长 (ETA)
 */
export function formatEta(
  seconds: number | null | undefined,
  isZh: boolean = false,
): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
    return isZh ? "计算中..." : "Calculating...";
  }

  if (seconds === 0) {
    return isZh ? "已完成" : "Done";
  }

  const s = Math.round(seconds);
  if (s < 60) {
    return isZh ? `剩余 ${s} 秒` : `${s}s left`;
  }

  const mins = Math.floor(s / 60);
  const remSecs = s % 60;
  if (mins < 60) {
    if (remSecs === 0) {
      return isZh ? `剩余 ${mins} 分钟` : `${mins}m left`;
    }
    return isZh ? `剩余 ${mins} 分 ${remSecs} 秒` : `${mins}m ${remSecs}s left`;
  }

  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  if (remMins === 0) {
    return isZh ? `剩余 ${hours} 小时` : `${hours}h left`;
  }
  return isZh ? `剩余 ${hours} 小时 ${remMins} 分` : `${hours}h ${remMins}m left`;
}

/**
 * 获取默认下载目录（含配置覆盖及操作系统默认回退）
 */
export async function sftpGetDefaultDownloadDir(): Promise<string> {
  try {
    return await invoke<string>("sftp_get_default_download_dir");
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 读取远程目录文件列表
 */
export async function sftpListDirectory(
  host: string,
  remotePath: string,
): Promise<SftpDirectoryResult> {
  try {
    return await invoke<SftpDirectoryResult>("sftp_list_directory", {
      host,
      remotePath,
    });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 快速读取文本文件前截断字节预览
 */
export async function sftpReadFile(
  host: string,
  remotePath: string,
  maxBytes?: number,
): Promise<string> {
  try {
    return await invoke<string>("sftp_read_file", {
      host,
      remotePath,
      maxBytes,
    });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 启动异步文件下载（带实时进度广播）
 */
export async function sftpDownloadFile(input: SftpDownloadInput): Promise<void> {
  try {
    await invoke("sftp_download_file", {
      taskId: input.taskId,
      host: input.host,
      remotePath: input.remotePath,
      localPath: input.localPath,
      fileSize: input.fileSize,
    });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 启动异步文件上传（带实时进度广播）
 */
export async function sftpUploadFile(input: SftpUploadInput): Promise<void> {
  try {
    await invoke("sftp_upload_file", {
      taskId: input.taskId,
      host: input.host,
      localPath: input.localPath,
      remotePath: input.remotePath,
    });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 中断/取消正在进行的传输
 */
export async function sftpCancelTransfer(taskId: string): Promise<void> {
  try {
    await invoke("sftp_cancel_transfer", { taskId });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 监听传输全局进度事件
 */
export async function listenSftpTransferProgress(
  callback: (payload: SftpTransferProgressPayload) => void,
): Promise<UnlistenFn> {
  return await listen<SftpTransferProgressPayload>(
    "sftp-transfer-progress",
    (event) => {
      callback(event.payload);
    },
  );
}

/**
 * 启动实时 Tail 日志流
 */
export async function sftpStartLogTail(
  host: string,
  remotePath: string,
  tailId: string,
): Promise<void> {
  try {
    await invoke("sftp_start_log_tail", { host, remotePath, tailId });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 停止实时 Tail 日志流
 */
export async function sftpStopLogTail(tailId: string): Promise<void> {
  try {
    await invoke("sftp_stop_log_tail", { tailId });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 删除远程文件或目录（默认移至 ~/.onespace_trash 回收站，若 permanent 为 true 则彻底删除）
 */
export async function sftpDeleteEntry(
  host: string,
  remotePath: string,
  permanent = false,
): Promise<SftpDeleteResult> {
  try {
    return await invoke<SftpDeleteResult>("sftp_delete_entry", {
      host,
      remotePath,
      permanent,
    });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 从回收站还原文件至原路径
 */
export async function sftpRestoreEntry(
  host: string,
  trashPath: string,
  originalPath: string,
): Promise<void> {
  try {
    await invoke("sftp_restore_entry", {
      host,
      trashPath,
      originalPath,
    });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 获取回收站文件列表
 */
export async function sftpListTrash(host: string): Promise<SftpTrashItem[]> {
  try {
    return await invoke<SftpTrashItem[]>("sftp_list_trash", { host });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 从回收站恢复指定条目至其原始路径
 */
export async function sftpRestoreTrashItem(
  host: string,
  trashName: string,
): Promise<string> {
  try {
    return await invoke<string>("sftp_restore_trash_item", { host, trashName });
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * 清空远程回收站
 */
export async function sftpClearTrash(host: string): Promise<void> {
  try {
    await invoke("sftp_clear_trash", { host });
  } catch (error) {
    throw normalizeError(error);
  }
}
