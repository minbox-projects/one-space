import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { open } from "@tauri-apps/plugin-dialog";
import {
  sftpCancelTransfer,
  sftpDownloadFile,
  sftpGetDefaultDownloadDir,
  sftpStartLogTail,
  sftpUploadFile,
} from "@/lib/sftp";
import type {
  RemoteFileEntry,
  SftpTransferProgressPayload,
} from "@/lib/sftp";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import type { SftpTailSession } from "../types";
import type { PushToast } from "./workspaceActions";

type SftpTransferParams = {
  t: TFunction;
  pushToast: PushToast;
  currentPath: string;
  isConnected: boolean;
  selectedHost: string;
  customHostInput: string;
  connectedHost: string | null;
  setTransferTasks: Dispatch<
    SetStateAction<Record<string, SftpTransferProgressPayload>>
  >;
  setTransferPanelOpen: Dispatch<SetStateAction<boolean>>;
  setTailSession: Dispatch<SetStateAction<SftpTailSession | null>>;
  loadDirectory: (path: string, options?: { silent?: boolean }) => Promise<void>;
};

export function createSftpTransferActions({
  t,
  pushToast,
  currentPath,
  isConnected,
  selectedHost,
  customHostInput,
  connectedHost,
  setTransferTasks,
  setTransferPanelOpen,
  setTailSession,
  loadDirectory,
}: SftpTransferParams) {
  // 文件下载
  const handleDownloadFile = async (file: RemoteFileEntry) => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost || !isConnected) return;

    const taskId = `dl_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
    let localPath = "";
    try {
      let downloadDir = "";
      if (isToolboxInvokeAvailable()) {
        downloadDir = await sftpGetDefaultDownloadDir();
      }
      if (!downloadDir) {
        downloadDir = "~/Downloads";
      }

      // 目标完整文件路径
      localPath = downloadDir.endsWith("/")
        ? `${downloadDir}${file.name}`
        : `${downloadDir}/${file.name}`;

      // 初始化任务项
      setTransferTasks((prev) => ({
        ...prev,
        [taskId]: {
          task_id: taskId,
          transfer_type: "download",
          file_name: file.name,
          transferred_bytes: 0,
          total_bytes: file.size,
          percentage: 0,
          bytes_per_second: 0,
          eta_seconds: null,
          status: "transferring",
          error: null,
          retry_params: {
            host: activeHost,
            remote_path: file.path,
            local_path: localPath,
            file_size: file.size,
          },
        },
      }));
      setTransferPanelOpen(true);

      if (isToolboxInvokeAvailable()) {
        await sftpDownloadFile({
          taskId,
          host: activeHost,
          remotePath: file.path,
          localPath,
          fileSize: file.size,
        });
      } else {
        // Mock 环境体验
        setTimeout(() => {
          setTransferTasks((prev) => ({
            ...prev,
            [taskId]: {
              task_id: taskId,
              transfer_type: "download",
              file_name: file.name,
              transferred_bytes: file.size,
              total_bytes: file.size,
              percentage: 100,
              bytes_per_second: 1024 * 500,
              eta_seconds: 0,
              status: "completed",
              error: null,
            },
          }));
        }, 1200);
      }
    } catch (err) {
      setTransferTasks((prev) => ({
        ...prev,
        [taskId]: {
          task_id: taskId,
          transfer_type: "download",
          file_name: file.name,
          transferred_bytes: 0,
          total_bytes: file.size,
          percentage: 0,
          bytes_per_second: 0,
          eta_seconds: null,
          status: "error",
          error: err instanceof Error ? err.message : String(err),
          retry_params: {
            host: activeHost,
            remote_path: file.path,
            local_path: localPath,
            file_size: file.size,
          },
        },
      }));
    }
  };

  // 文件上传
  const handleUploadFiles = async () => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost || !isConnected) return;

    try {
      const selected = await open({
        multiple: true,
        directory: false,
      });

      if (!selected) return;

      const filePaths: string[] = Array.isArray(selected)
        ? (selected as string[])
        : [selected as string];

      if (filePaths.length === 0) return;

      setTransferPanelOpen(true);

      const normalizedRemoteDir = currentPath.endsWith("/")
        ? currentPath
        : `${currentPath}/`;

      for (const filePath of filePaths) {
        const fileName =
          filePath.substring(filePath.lastIndexOf("/") + 1) || filePath;
        const taskId = `up_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
        const retryParams = {
          host: activeHost,
          local_path: filePath,
          remote_path: normalizedRemoteDir,
          file_size: 0,
        };

        setTransferTasks((prev) => ({
          ...prev,
          [taskId]: {
            task_id: taskId,
            transfer_type: "upload",
            file_name: fileName,
            transferred_bytes: 0,
            total_bytes: 0,
            percentage: 0,
            bytes_per_second: 0,
            eta_seconds: null,
            status: "transferring",
            error: null,
            retry_params: retryParams,
          },
        }));

        if (isToolboxInvokeAvailable()) {
          void sftpUploadFile({
            taskId,
            host: activeHost,
            localPath: filePath,
            remotePath: normalizedRemoteDir,
          }).catch((err) => {
            setTransferTasks((prev) => ({
              ...prev,
              [taskId]: {
                task_id: taskId,
                transfer_type: "upload",
                file_name: fileName,
                transferred_bytes: 0,
                total_bytes: 0,
                percentage: 0,
                bytes_per_second: 0,
                eta_seconds: null,
                status: "error",
                error: err instanceof Error ? err.message : String(err),
                retry_params: retryParams,
              },
            }));
          });
        } else {
          // Mock 环境体验
          setTimeout(() => {
            setTransferTasks((prev) => ({
              ...prev,
              [taskId]: {
                task_id: taskId,
                transfer_type: "upload",
                file_name: fileName,
                transferred_bytes: 2048,
                total_bytes: 2048,
                percentage: 100,
                bytes_per_second: 512,
                eta_seconds: 0,
                status: "completed",
                error: null,
              },
            }));
            void loadDirectory(currentPath);
          }, 1200);
        }
      }
    } catch (err) {
      pushToast({
        title: t("sftpTransferFailed", "Transfer failed"),
        description: err instanceof Error ? err.message : String(err),
        kind: "error",
      });
    }
  };

  // 取消传输
  const handleCancelTransfer = async (taskId: string) => {
    try {
      if (isToolboxInvokeAvailable()) {
        await sftpCancelTransfer(taskId);
      }
      setTransferTasks((prev) => {
        const item = prev[taskId];
        if (!item) return prev;
        return {
          ...prev,
          [taskId]: {
            ...item,
            status: "cancelled",
          },
        };
      });
      pushToast({
        title: t("sftpTransferCancelled", "Transfer cancelled"),
        kind: "info",
      });
    } catch (err) {
      console.error(err);
    }
  };

  // 失败任务重试
  const handleRetryTransfer = async (task: SftpTransferProgressPayload) => {
    const params = task.retry_params;
    if (!params) return;

    setTransferTasks((prev) => ({
      ...prev,
      [task.task_id]: {
        ...task,
        status: "transferring",
        error: null,
        percentage: 0,
        bytes_per_second: 0,
        eta_seconds: null,
      },
    }));

    pushToast({
      title: t("sftpRetryToast", "正在重试传输..."),
      description: task.file_name,
      kind: "info",
    });

    if (task.transfer_type === "upload") {
      if (isToolboxInvokeAvailable()) {
        sftpUploadFile({
          taskId: task.task_id,
          host: params.host,
          localPath: params.local_path,
          remotePath: params.remote_path,
        }).catch((err) => {
          setTransferTasks((prev) => ({
            ...prev,
            [task.task_id]: {
              ...task,
              status: "error",
              error: err instanceof Error ? err.message : String(err),
            },
          }));
        });
      } else {
        setTimeout(() => {
          setTransferTasks((prev) => ({
            ...prev,
            [task.task_id]: {
              ...task,
              status: "completed",
              percentage: 100,
              eta_seconds: 0,
            },
          }));
        }, 800);
      }
    } else {
      if (isToolboxInvokeAvailable()) {
        sftpDownloadFile({
          taskId: task.task_id,
          host: params.host,
          remotePath: params.remote_path,
          localPath: params.local_path,
          fileSize: params.file_size,
        }).catch((err) => {
          setTransferTasks((prev) => ({
            ...prev,
            [task.task_id]: {
              ...task,
              status: "error",
              error: err instanceof Error ? err.message : String(err),
            },
          }));
        });
      } else {
        setTimeout(() => {
          setTransferTasks((prev) => ({
            ...prev,
            [task.task_id]: {
              ...task,
              status: "completed",
              percentage: 100,
              eta_seconds: 0,
            },
          }));
        }, 800);
      }
    }
  };

  // 清除已完成任务
  const handleClearCompletedTransfers = () => {
    setTransferTasks((prev) => {
      const next: Record<string, SftpTransferProgressPayload> = {};
      for (const [id, tsk] of Object.entries(prev)) {
        if (tsk.status === "transferring") {
          next[id] = tsk;
        }
      }
      return next;
    });
  };

  // 启动实时 Tail 日志
  const handleStartTail = async (entry: RemoteFileEntry) => {
    const activeHost = selectedHost || customHostInput.trim();
    if (!activeHost) return;

    const tailId = `tail_${Date.now()}`;
    setTailSession({
      id: tailId,
      fileName: entry.name,
      filePath: entry.path,
      lines: [`[Tail started for ${entry.path}]`],
      autoScroll: true,
      filter: "",
    });

    if (isToolboxInvokeAvailable()) {
      try {
        await sftpStartLogTail(activeHost, entry.path, tailId);
      } catch (err) {
        pushToast({
          title: t("sftpTailStartFailed", "Failed to start tail stream"),
          description: err instanceof Error ? err.message : String(err),
          kind: "error",
        });
      }
    }
  };

  return {
    handleDownloadFile,
    handleUploadFiles,
    handleCancelTransfer,
    handleRetryTransfer,
    handleClearCompletedTransfers,
    handleStartTail,
  };
}
