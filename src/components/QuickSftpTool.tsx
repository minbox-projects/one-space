import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  ArrowLeft,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Download,
  Eye,
  FileText,
  Folder,
  FolderOpen,
  FolderSync,
  Loader2,
  Plug,
  Radio,
  RefreshCw,
  RotateCcw,
  RotateCw,
  Search,
  Server,
  Terminal,
  Trash2,
  Unplug,
  Upload,
  X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { useToast } from "./ToastProvider";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import { getToolboxTool } from "@/toolbox/registry";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import {
  formatBytes,
  formatEta,
  formatSpeed,
  listenSftpTransferProgress,
  sftpCancelTransfer,
  sftpClearTrash,
  sftpDeleteEntry,
  sftpDownloadFile,
  sftpGetDefaultDownloadDir,
  sftpListDirectory,
  sftpListTrash,
  sftpReadFile,
  sftpRestoreEntry,
  sftpRestoreTrashItem,
  sftpStartLogTail,
  sftpStopLogTail,
  sftpUploadFile,
  type RemoteFileEntry,
  type SftpTransferProgressPayload,
  type SftpTrashItem,
  type SshHostOption,
} from "@/lib/sftp";

const DEFAULT_BOOKMARKS = ["~", "/var/log", "/etc", "/tmp", "~/.onespace_trash"];

const INITIAL_MOCK_ENTRIES: RemoteFileEntry[] = [
  {
    name: "nginx",
    is_dir: true,
    size: 4096,
    permissions: "drwxr-xr-x",
    modified: "2026-10-21 14:00",
    path: "~/nginx",
  },
  {
    name: "app.log",
    is_dir: false,
    size: 102400,
    permissions: "-rw-r--r--",
    modified: "2026-10-21 15:30",
    path: "~/app.log",
  },
  {
    name: "config.json",
    is_dir: false,
    size: 1240,
    permissions: "-rw-r--r--",
    modified: "2026-10-20 09:12",
    path: "~/config.json",
  },
];

export function QuickSftpTool() {
  const { t, i18n } = useTranslation();
  const isZh = i18n.language?.startsWith("zh");
  const { pushToast } = useToast();
  const confirmDialog = useConfirmDialog();
  const toolDescriptor = getToolboxTool("quick-sftp");
  const ToolIcon = toolDescriptor?.icon ?? FolderSync;
  const iconClassName =
    toolDescriptor?.iconClassName ?? "bg-purple-500/10 text-purple-600";

  // 服务器选择
  const [hosts, setHosts] = useState<SshHostOption[]>([]);
  const [selectedHost, setSelectedHost] = useState("");
  const [customHostInput, setCustomHostInput] = useState("");

  // 连接状态
  const [isConnected, setIsConnected] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [connectedHost, setConnectedHost] = useState<string | null>(null);

  // 目录浏览
  const [currentPath, setCurrentPath] = useState("~");
  const currentPathRef = useRef("~");
  currentPathRef.current = currentPath;

  const [pathInput, setPathInput] = useState("~");
  const [files, setFiles] = useState<RemoteFileEntry[]>([]);
  const mockFilesRef = useRef<RemoteFileEntry[]>(INITIAL_MOCK_ENTRIES);
  const getMockDirectoryEntries = (basePath: string): RemoteFileEntry[] => {
    return mockFilesRef.current.map((entry) => ({
      ...entry,
      path: `${basePath}/${entry.name}`.replace(/\/\//g, "/"),
    }));
  };
  const [loading, setLoading] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchFilter, setSearchFilter] = useState("");

  // 传输任务管理
  const [transferTasks, setTransferTasks] = useState<
    Record<string, SftpTransferProgressPayload>
  >({});
  const [transferPanelOpen, setTransferPanelOpen] = useState(true);

  // 文本预览 Modal
  const [previewFile, setPreviewFile] = useState<{
    name: string;
    content: string;
  } | null>(null);

  // 回收站 Modal 与列表状态
  const [trashModalOpen, setTrashModalOpen] = useState(false);
  const [trashItems, setTrashItems] = useState<SftpTrashItem[]>([]);
  const [trashLoading, setTrashLoading] = useState(false);
  const [restoringTrashId, setRestoringTrashId] = useState<string | null>(null);
  const mockTrashRef = useRef<SftpTrashItem[]>([
    {
      id: "1729584000_old_backup.tar.gz",
      name: "old_backup.tar.gz",
      original_path: "/root/old_backup.tar.gz",
      trash_path: "~/.onespace_trash/1729584000_old_backup.tar.gz",
      size: 1048576,
      is_dir: false,
      deleted_at: "2026-10-21 16:00",
    },
  ]);

  // 实时 Tail 日志控制台
  const [tailSession, setTailSession] = useState<{
    id: string;
    fileName: string;
    filePath: string;
    lines: string[];
    autoScroll: boolean;
    filter: string;
  } | null>(null);

  const logConsoleEndRef = useRef<HTMLDivElement>(null);

  const { copied: copiedLog, copy: copyLog } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: t("sftpLogCopied", "Log output copied"),
        kind: "success",
      }),
  });

  // 获取 SSH 主机列表
  useEffect(() => {
    if (!isToolboxInvokeAvailable()) return;
    invoke<SshHostOption[]>("get_ssh_hosts")
      .then((res) => {
        setHosts(res || []);
        if (res && res.length > 0) {
          setSelectedHost(res[0].name);
        }
      })
      .catch(() => {});
  }, []);

  // 监听实时传输进度事件
  useEffect(() => {
    if (!isToolboxInvokeAvailable()) return;
    let unlisten: (() => void) | undefined;
    listenSftpTransferProgress((payload) => {
      setTransferTasks((prev) => {
        const existing = prev[payload.task_id];
        return {
          ...prev,
          [payload.task_id]: {
            ...payload,
            retry_params: existing?.retry_params || payload.retry_params,
          },
        };
      });

      // 上传成功后自动静默刷新当前远程目录（不触发 loading 闪烁）
      if (payload.transfer_type === "upload" && payload.status === "completed") {
        void loadDirectory(currentPathRef.current, { silent: true });
        pushToast({
          title: t("sftpUploadSuccess", "Uploaded successfully"),
          description: payload.file_name,
          kind: "success",
        });
      }

      // 下载成功提示
      if (payload.transfer_type === "download" && payload.status === "completed") {
        pushToast({
          title: t("sftpDownloadSuccess", "Downloaded successfully"),
          description: payload.file_name,
          kind: "success",
        });
      }

      // 错误提示
      if (payload.status === "error") {
        pushToast({
          title: t("sftpTransferFailed", "Transfer failed"),
          description: payload.error || payload.file_name,
          kind: "error",
        });
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    return () => {
      if (unlisten) unlisten();
    };
  }, [t, pushToast]);

  // 建立连接
  const handleConnect = async (targetPath = pathInput || "~") => {
    const activeHost = selectedHost || customHostInput.trim();
    if (!activeHost) {
      pushToast({
        title: t("sftpSelectHostPrompt", "Please select or enter an SSH host"),
        kind: "warning",
      });
      return;
    }

    setConnecting(true);
    setError(null);
    try {
      if (isToolboxInvokeAvailable()) {
        const res = await sftpListDirectory(activeHost, targetPath);
        if (res && Array.isArray(res.entries)) {
          setFiles(res.entries);
          setCurrentPath(res.current_path || targetPath);
          setPathInput(res.current_path || targetPath);
        } else {
          setFiles(getMockDirectoryEntries(targetPath));
          setCurrentPath(targetPath);
          setPathInput(targetPath);
        }
      } else {
        setFiles(getMockDirectoryEntries(targetPath));
        setCurrentPath(targetPath);
        setPathInput(targetPath);
      }
      setIsConnected(true);
      setConnectedHost(activeHost);
      pushToast({
        title: t("sftpConnectedToast", "Connected to remote host"),
        description: activeHost,
        kind: "success",
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setIsConnected(false);
      setConnectedHost(null);
    } finally {
      setConnecting(false);
    }
  };

  // 断开连接
  const handleDisconnect = () => {
    if (tailSession && isToolboxInvokeAvailable()) {
      sftpStopLogTail(tailSession.id).catch(() => {});
    }
    setTailSession(null);
    setFiles([]);
    setIsConnected(false);
    setConnectedHost(null);
    setError(null);
  };

  // 切换 Host 处理
  const handleHostChange = (newHost: string) => {
    setSelectedHost(newHost);
    if (isConnected) {
      handleDisconnect();
    }
  };

  const handleCustomHostChange = (newVal: string) => {
    setCustomHostInput(newVal);
    if (isConnected) {
      handleDisconnect();
    }
  };

  // 加载目录（支持静默刷新 silent: true，避免破坏性卸载表格 DOM 产生视觉闪烁）
  const loadDirectory = async (path: string, options?: { silent?: boolean }) => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost) return;

    if (!isConnected) {
      await handleConnect(path);
      return;
    }

    const isSilent = options?.silent ?? false;
    if (!isSilent) {
      setLoading(true);
    } else {
      setIsRefreshing(true);
    }
    setError(null);
    try {
      if (isToolboxInvokeAvailable()) {
        const res = await sftpListDirectory(activeHost, path);
        if (res && Array.isArray(res.entries)) {
          setFiles(res.entries);
          setCurrentPath(res.current_path || path);
          setPathInput(res.current_path || path);
        } else {
          setFiles(getMockDirectoryEntries(path));
          setCurrentPath(path);
          setPathInput(path);
        }
      } else {
        setFiles(getMockDirectoryEntries(path));
        setCurrentPath(path);
        setPathInput(path);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (!isSilent) {
        setLoading(false);
      } else {
        setIsRefreshing(false);
      }
    }
  };

  // 删除文件或目录（二次确认 + 默认移入回收站 + 乐观更新 + 快捷撤销还原）
  const handleDeleteEntry = async (file: RemoteFileEntry) => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost || !isConnected) return;

    const isInTrash =
      currentPath.includes(".onespace_trash") ||
      file.path.includes(".onespace_trash");

    const confirmMessage = isInTrash
      ? t(
          "sftpPermanentDeleteConfirmMessage",
          '确定要彻底永久删除 "{{name}}" 吗？此操作不可恢复！',
          { name: file.name },
        )
      : t(
          "sftpDeleteConfirmMessage",
          '确定要将 "{{name}}" 移入回收站吗？可在回收站找回或立即撤销。',
          { name: file.name },
        );

    const confirmed = await confirmDialog(confirmMessage, {
      title: t("sftpDeleteConfirmTitle", "删除确认"),
      kind: "error",
      okLabel: t("sftpDelete", "删除"),
      cancelLabel: t("cancel", "取消"),
    });

    if (!confirmed) return;
    // 1. 乐观更新：立即在前端列表中过滤移除该条目，视觉零延迟响应
    mockFilesRef.current = mockFilesRef.current.filter((item) => item.name !== file.name);
    setFiles((prev) => prev.filter((item) => item.path !== file.path));
    if (!isInTrash) {
      const mockTrashItem: SftpTrashItem = {
        id: `${Date.now()}_${file.name}`,
        name: file.name,
        original_path: file.path,
        trash_path: `~/.onespace_trash/${Date.now()}_${file.name}`,
        size: file.size,
        is_dir: file.is_dir,
        deleted_at: new Date().toISOString().replace("T", " ").substring(0, 19),
      };
      mockTrashRef.current = [mockTrashItem, ...mockTrashRef.current];
    }

    try {
      if (isToolboxInvokeAvailable()) {
        const result = await sftpDeleteEntry(activeHost, file.path, isInTrash);

        // 如果移入回收站，支持在 Toast 中直接撤销 (Undo)
        if (!result.is_permanent && result.trash_path) {
          const trashPath = result.trash_path;
          pushToast({
            title: t("sftpDeletedToast", '已将 "{{name}}" 移入回收站', {
              name: file.name,
            }),
            description: file.path,
            kind: "success",
            durationMs: 6000,
            action: {
              label: t("sftpUndo", "撤销"),
              onClick: async () => {
                try {
                  await sftpRestoreEntry(activeHost, trashPath, file.path);
                  mockFilesRef.current = [file, ...mockFilesRef.current];
                  setFiles((prev) => [file, ...prev]);
                  pushToast({
                    title: t("sftpRestoredToast", '已恢复 "{{name}}"', {
                      name: file.name,
                    }),
                    kind: "success",
                  });
                  // 恢复后静默刷新
                  void loadDirectory(currentPathRef.current, { silent: true });
                } catch (restoreErr) {
                  pushToast({
                    title: t("sftpRestoreFailedToast", "恢复失败"),
                    description:
                      restoreErr instanceof Error
                        ? restoreErr.message
                        : String(restoreErr),
                    kind: "error",
                  });
                }
              },
            },
          });
        } else {
          pushToast({
            title: t("sftpPermanentDeletedToast", '已彻底删除 "{{name}}"', {
              name: file.name,
            }),
            kind: "success",
          });
        }

        // 静默刷新保持与远端数据一致性（不触发全局 loading 闪烁）
        void loadDirectory(currentPathRef.current, { silent: true });
      } else {
        // Mock 环境体验：同步维护 mock 数据引用，支持撤销与静默刷新
        pushToast({
          title: t("sftpDeletedToast", '已将 "{{name}}" 移入回收站', {
            name: file.name,
          }),
          kind: "success",
          durationMs: 6000,
          action: {
            label: t("sftpUndo", "撤销"),
            onClick: () => {
              mockFilesRef.current = [file, ...mockFilesRef.current];
              setFiles((prev) => [file, ...prev]);
              pushToast({
                title: t("sftpRestoredToast", '已恢复 "{{name}}"', {
                  name: file.name,
                }),
                kind: "success",
              });
            },
          },
        });
      }
    } catch (err) {
      // 若删除失败，静默刷新拉回真实状态并提示错误
      if (isToolboxInvokeAvailable()) {
        void loadDirectory(currentPathRef.current, { silent: true });
      }
      pushToast({
        title: t("sftpDeleteFailedToast", "删除失败"),
        description: err instanceof Error ? err.message : String(err),
        kind: "error",
      });
    }
  };

  // 回收站操作
  const handleOpenTrashModal = async () => {
    setTrashModalOpen(true);
    await loadTrashItems();
  };

  const loadTrashItems = async () => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost) return;

    setTrashLoading(true);
    try {
      if (isToolboxInvokeAvailable()) {
        const items = await sftpListTrash(activeHost);
        setTrashItems(items || []);
      } else {
        setTrashItems(mockTrashRef.current);
      }
    } catch (err) {
      pushToast({
        title: t("sftpRestoreFailedToast", "获取回收站列表失败"),
        description: err instanceof Error ? err.message : String(err),
        kind: "error",
      });
    } finally {
      setTrashLoading(false);
    }
  };

  const handleRestoreTrashItem = async (item: SftpTrashItem) => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost || !isConnected) return;

    setRestoringTrashId(item.id);
    try {
      if (isToolboxInvokeAvailable()) {
        const restoredPath = await sftpRestoreTrashItem(activeHost, item.id);
        setTrashItems((prev) => prev.filter((t) => t.id !== item.id));
        mockTrashRef.current = mockTrashRef.current.filter((t) => t.id !== item.id);
        pushToast({
          title: t("sftpRestoredToToast", '已将 "{{name}}" 恢复至 {{path}}', {
            name: item.name,
            path: restoredPath || item.original_path,
          }),
          kind: "success",
        });
        void loadDirectory(currentPathRef.current, { silent: true });
      } else {
        setTrashItems((prev) => prev.filter((t) => t.id !== item.id));
        mockTrashRef.current = mockTrashRef.current.filter((t) => t.id !== item.id);
        const restoredEntry: RemoteFileEntry = {
          name: item.name,
          is_dir: item.is_dir,
          size: item.size,
          permissions: "-rw-r--r--",
          modified: new Date().toISOString().replace("T", " ").substring(0, 16),
          path: item.original_path,
        };
        mockFilesRef.current = [restoredEntry, ...mockFilesRef.current];
        setFiles((prev) => [restoredEntry, ...prev]);
        pushToast({
          title: t("sftpRestoredToToast", '已将 "{{name}}" 恢复至 {{path}}', {
            name: item.name,
            path: item.original_path,
          }),
          kind: "success",
        });
        void loadDirectory(currentPathRef.current, { silent: true });
      }
    } catch (err) {
      pushToast({
        title: t("sftpRestoreFailedToast", "恢复失败"),
        description: err instanceof Error ? err.message : String(err),
        kind: "error",
      });
    } finally {
      setRestoringTrashId(null);
    }
  };

  const handleClearTrash = async () => {
    const activeHost = connectedHost || selectedHost || customHostInput.trim();
    if (!activeHost || !isConnected) return;

    const confirmed = await confirmDialog(
      t(
        "sftpClearTrashConfirmMessage",
        "确定要永久彻底清空回收站中的所有文件吗？此操作不可恢复！",
      ),
      {
        title: t("sftpClearTrashConfirmTitle", "清空回收站"),
        kind: "error",
        okLabel: t("sftpClearTrash", "清空回收站"),
        cancelLabel: t("cancel", "取消"),
      },
    );

    if (!confirmed) return;

    try {
      if (isToolboxInvokeAvailable()) {
        await sftpClearTrash(activeHost);
      }
      setTrashItems([]);
      mockTrashRef.current = [];
      pushToast({
        title: t("sftpTrashClearedToast", "已清空远程回收站"),
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: t("sftpClearTrashFailed", "清空回收站失败"),
        description: err instanceof Error ? err.message : String(err),
        kind: "error",
      });
    }
  };

  // 路径导航辅助
  const navigateToDirectory = (path: string) => {
    setCurrentPath(path);
    setPathInput(path);
    void loadDirectory(path);
  };

  // 选择书签快捷路径
  const handleSelectBookmark = (bm: string) => {
    setPathInput(bm);
    if (isConnected) {
      navigateToDirectory(bm);
    }
  };

  // 返回上一级
  const handleGoParent = () => {
    if (!isConnected || currentPath === "/" || currentPath === "~") return;
    const parent = currentPath.substring(0, currentPath.lastIndexOf("/")) || "/";
    navigateToDirectory(parent);
  };

  // 快速文本预览
  const handlePreviewFile = async (entry: RemoteFileEntry) => {
    const activeHost = selectedHost || customHostInput.trim();
    if (!activeHost) return;

    try {
      if (isToolboxInvokeAvailable()) {
        const content = await sftpReadFile(activeHost, entry.path, 128 * 1024);
        setPreviewFile({ name: entry.name, content });
      } else {
        setPreviewFile({
          name: entry.name,
          content: '{\n  "status": "success",\n  "port": 8080\n}',
        });
      }
    } catch (err) {
      pushToast({
        title: t("sftpPreviewError", "Failed to read file"),
        description: err instanceof Error ? err.message : String(err),
        kind: "error",
      });
    }
  };

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

  // 监听实时日志事件
  useEffect(() => {
    if (!tailSession) return;
    const tailId = tailSession.id;

    let unlisten: (() => void) | undefined;
    listen<string>(`sftp-log-line-${tailId}`, (event) => {
      setTailSession((prev) => {
        if (!prev || prev.id !== tailId) return prev;
        const newLines = [...prev.lines, event.payload];
        // 环形缓冲：最多保留 1000 行防卡死
        if (newLines.length > 1000) {
          newLines.splice(0, newLines.length - 1000);
        }
        return { ...prev, lines: newLines };
      });
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
      if (isToolboxInvokeAvailable()) {
        sftpStopLogTail(tailId).catch(() => {});
      }
    };
  }, [tailSession?.id]);

  // 自动滚屏吸底
  useEffect(() => {
    if (tailSession?.autoScroll) {
      logConsoleEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [tailSession?.lines]);

  // 过滤后的文件
  const filteredFiles = useMemo(() => {
    if (!searchFilter.trim()) return files;
    const lower = searchFilter.toLowerCase();
    return files.filter((f) => f.name.toLowerCase().includes(lower));
  }, [files, searchFilter]);

  // 过滤后的日志
  const filteredLogLines = useMemo(() => {
    if (!tailSession || !tailSession.filter.trim())
      return tailSession?.lines || [];
    const lower = tailSession.filter.toLowerCase();
    return tailSession.lines.filter((l) => l.toLowerCase().includes(lower));
  }, [tailSession?.lines, tailSession?.filter]);

  // 活跃中的下载任务文件名集合（用于列表行按钮禁用或高亮）
  const activeDownloadingFileNames = useMemo(() => {
    const set = new Set<string>();
    for (const task of Object.values(transferTasks)) {
      if (task.transfer_type === "download" && task.status === "transferring") {
        set.add(task.file_name);
      }
    }
    return set;
  }, [transferTasks]);

  const taskList = useMemo(() => Object.values(transferTasks), [transferTasks]);
  const activeTaskCount = useMemo(
    () => taskList.filter((t) => t.status === "transferring").length,
    [taskList],
  );

  return (
    <ToolShell
      titleId="quick-sftp-title"
      title={t("quickSftp", "Quick SFTP & Log Tailer")}
      description={t(
        "quickSftpDesc",
        "Browse remote server directories, download files, and stream live logs with zero lag.",
      )}
      icon={
        <div className={`rounded-lg p-2 ${iconClassName}`}>
          <ToolIcon className="h-5 w-5" />
        </div>
      }
      error={error}
    >
      <div className="space-y-4">
        {/* 服务器选择与书签快捷条 */}
        <div className="rounded-lg border bg-card p-3 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-2">
              <Server className="h-4 w-4 text-primary shrink-0" />
              <span className="text-xs font-semibold shrink-0">
                {t("sftpSelectHost", "SSH Host")}:
              </span>
              {hosts.length > 0 ? (
                <select
                  value={selectedHost}
                  onChange={(e) => handleHostChange(e.target.value)}
                  className="h-8 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                >
                  {hosts.map((h) => (
                    <option key={h.name} value={h.name}>
                      {h.name} ({h.user}@{h.host_name}:{h.port})
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  type="text"
                  value={customHostInput}
                  onChange={(e) => handleCustomHostChange(e.target.value)}
                  placeholder="user@host or config alias"
                  className="h-8 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                />
              )}

              {/* 连接 / 断开 控制按钮 */}
              {isConnected ? (
                <div className="flex items-center gap-2">
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
                    {t("sftpConnectedStatus", "已连接")}
                  </span>
                  <button
                    type="button"
                    onClick={handleDisconnect}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border border-rose-500/30 bg-rose-500/10 px-3 text-xs font-medium text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 transition-colors"
                  >
                    <Unplug className="h-3.5 w-3.5" />
                    {t("sftpDisconnect", "断开连接")}
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  onClick={() => void handleConnect(pathInput)}
                  disabled={
                    connecting || (!selectedHost && !customHostInput.trim())
                  }
                  className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors"
                >
                  {connecting ? (
                    <>
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      {t("sftpConnecting", "连接中...")}
                    </>
                  ) : (
                    <>
                      <Plug className="h-3.5 w-3.5" />
                      {t("sftpConnect", "连接 / 登录")}
                    </>
                  )}
                </button>
              )}
            </div>

            {/* 常用书签直达 */}
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-muted-foreground mr-1 text-[11px]">
                {t("sftpBookmarks", "Quick Paths")}:
              </span>
              {DEFAULT_BOOKMARKS.map((bm) => (
                <button
                  key={bm}
                  type="button"
                  onClick={() => handleSelectBookmark(bm)}
                  className="rounded-md border bg-background px-2 py-0.5 text-xs font-mono text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  {bm}
                </button>
              ))}
            </div>
          </div>

          {/* 路径与操作栏 */}
          <div className="flex items-center gap-2 border-t pt-2">
            <button
              type="button"
              onClick={handleGoParent}
              disabled={
                !isConnected || currentPath === "/" || currentPath === "~"
              }
              title={t("sftpParentDir", "Parent Directory")}
              className="inline-flex h-8 w-8 items-center justify-center rounded-md border bg-background hover:bg-muted text-muted-foreground disabled:opacity-40"
            >
              <ArrowLeft className="h-4 w-4" />
            </button>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (isConnected) {
                  navigateToDirectory(pathInput);
                } else {
                  void handleConnect(pathInput);
                }
              }}
              className="flex-1"
            >
              <input
                type="text"
                value={pathInput}
                onChange={(e) => setPathInput(e.target.value)}
                placeholder="~ or /var/log"
                className="h-8 w-full rounded-md border bg-background px-3 font-mono text-xs outline-none"
              />
            </form>

            {/* 上传文件按钮 */}
            <button
              type="button"
              onClick={() => void handleUploadFiles()}
              disabled={!isConnected}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-primary/30 bg-primary/10 px-3 text-xs font-medium text-primary hover:bg-primary/20 disabled:opacity-40 transition-colors"
              title={t("sftpUpload", "Upload")}
            >
              <Upload className="h-3.5 w-3.5" />
              <span>{t("sftpUpload", "Upload")}</span>
            </button>

            {/* 回收站按钮 */}
            <button
              type="button"
              onClick={() => void handleOpenTrashModal()}
              disabled={!isConnected}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-3 text-xs font-medium hover:bg-muted disabled:opacity-40 transition-colors"
              title={t("sftpTrash", "回收站")}
            >
              <Trash2 className="h-3.5 w-3.5 text-muted-foreground" />
              <span>{t("sftpTrash", "回收站")}</span>
            </button>

            {/* 刷新按钮 */}
            <button
              type="button"
              onClick={() => void loadDirectory(currentPath, { silent: files.length > 0 })}
              disabled={loading || isRefreshing || !isConnected}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-3 text-xs font-medium hover:bg-muted disabled:opacity-40"
            >
              <RefreshCw
                className={`h-3.5 w-3.5 ${loading || isRefreshing ? "animate-spin" : ""}`}
              />
              {t("sftpRefresh", "Refresh")}
            </button>
          </div>
        </div>

        {/* 文件列表区 */}
        <div className="rounded-lg border bg-card overflow-hidden shadow-sm">
          {/* 轻量静默刷新进度指示条 */}
          {isRefreshing ? (
            <div className="h-0.5 w-full bg-primary/20 overflow-hidden">
              <div className="h-full bg-primary animate-pulse w-full" />
            </div>
          ) : null}

          <div className="flex items-center justify-between border-b bg-muted/40 px-3 py-2 text-xs">
            <div className="flex items-center gap-2 font-medium">
              <FolderOpen className="h-4 w-4 text-primary" />
              <span>
                {isConnected
                  ? currentPath
                  : t("sftpNotConnectedLabel", "未连接")}{" "}
                ({isConnected ? filteredFiles.length : 0}{" "}
                {t("sftpItems", "items")})
              </span>
            </div>
            {isConnected ? (
              <div className="flex items-center gap-2">
                <Search className="h-3.5 w-3.5 text-muted-foreground" />
                <input
                  type="text"
                  value={searchFilter}
                  onChange={(e) => setSearchFilter(e.target.value)}
                  placeholder={t("sftpFilterPlaceholder", "Filter files...")}
                  className="h-7 w-40 rounded-md border bg-background px-2 text-xs outline-none"
                />
              </div>
            ) : null}
          </div>

          <div className="max-h-96 overflow-y-auto">
            {!isConnected ? (
              <div className="flex flex-col items-center justify-center py-16 text-center text-xs text-muted-foreground space-y-3">
                <div className="rounded-full bg-muted/60 p-4">
                  <Server className="h-8 w-8 text-muted-foreground/60" />
                </div>
                <div className="space-y-1">
                  <p className="font-medium text-foreground text-sm">
                    {t("sftpNotConnectedTitle", "尚未连接远程服务器")}
                  </p>
                  <p className="text-muted-foreground max-w-sm">
                    {t(
                      "sftpNotConnectedDesc",
                      "请在上方选择或输入 SSH 主机，然后点击「连接 / 登录」开始浏览远程目录和实时日志。",
                    )}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => void handleConnect(pathInput)}
                  disabled={
                    connecting || (!selectedHost && !customHostInput.trim())
                  }
                  className="inline-flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {connecting ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Plug className="h-3.5 w-3.5" />
                  )}
                  {connecting
                    ? t("sftpConnecting", "连接中...")
                    : t("sftpConnectNow", "立即连接")}
                </button>
              </div>
            ) : loading && files.length === 0 ? (
              <div className="flex items-center justify-center py-12 text-muted-foreground text-xs gap-2">
                <Loader2 className="h-4 w-4 animate-spin text-primary" />
                <span>{t("sftpLoadingDir", "Reading remote directory...")}</span>
              </div>
            ) : filteredFiles.length === 0 ? (
              <div className="py-12 text-center text-xs text-muted-foreground">
                {t("sftpEmptyDir", "Empty directory or no matching files.")}
              </div>
            ) : (
              <table className="w-full text-left text-xs">
                <thead className="border-b bg-muted/20 text-[11px] text-muted-foreground uppercase">
                  <tr>
                    <th className="py-2 px-3 font-medium">
                      {t("sftpColName", "Name")}
                    </th>
                    <th className="py-2 px-3 font-medium w-24">
                      {t("sftpColSize", "Size")}
                    </th>
                    <th className="py-2 px-3 font-medium w-36">
                      {t("sftpColModified", "Modified")}
                    </th>
                    <th className="py-2 px-3 font-medium w-28">
                      {t("sftpColPerms", "Permissions")}
                    </th>
                    <th className="py-2 px-3 font-medium w-36 text-right">
                      {t("sftpColActions", "Actions")}
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border/60">
                  {filteredFiles.map((file) => {
                    const isLogFile = file.name.endsWith(".log");
                    const isDownloading = activeDownloadingFileNames.has(
                      file.name,
                    );
                    return (
                      <tr
                        key={file.name}
                        onDoubleClick={() => {
                          if (file.is_dir) {
                            void loadDirectory(file.path);
                          } else {
                            void handlePreviewFile(file);
                          }
                        }}
                        className="hover:bg-muted/50 cursor-pointer transition-colors"
                      >
                        <td className="py-2 px-3 flex items-center gap-2 font-mono">
                          {file.is_dir ? (
                            <Folder className="h-4 w-4 text-sky-500 fill-sky-500/20" />
                          ) : isLogFile ? (
                            <Terminal className="h-4 w-4 text-amber-500" />
                          ) : (
                            <FileText className="h-4 w-4 text-muted-foreground" />
                          )}
                          <span
                            className={
                              file.is_dir ? "font-semibold text-foreground" : ""
                            }
                          >
                            {file.name}
                          </span>
                        </td>
                        <td className="py-2 px-3 font-mono text-muted-foreground">
                          {file.is_dir ? "-" : formatBytes(file.size)}
                        </td>
                        <td className="py-2 px-3 text-muted-foreground text-[11px]">
                          {file.modified}
                        </td>
                        <td className="py-2 px-3 font-mono text-[11px] text-muted-foreground">
                          {file.permissions}
                        </td>
                        <td className="py-2 px-3 text-right">
                          <div className="flex items-center justify-end gap-1.5">
                            {/* 下载按钮 */}
                            {!file.is_dir ? (
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  void handleDownloadFile(file);
                                }}
                                disabled={isDownloading}
                                className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
                                title={t("sftpDownload", "Download")}
                              >
                                {isDownloading ? (
                                  <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
                                ) : (
                                  <Download className="h-3.5 w-3.5" />
                                )}
                              </button>
                            ) : null}

                            {isLogFile ? (
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  void handleStartTail(file);
                                }}
                                className="inline-flex items-center gap-1 rounded bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-600 hover:bg-amber-500/20"
                              >
                                <Radio className="h-3 w-3 animate-pulse" />
                                {t("sftpLiveTail", "Tail")}
                              </button>
                            ) : null}
                            <button
                              type="button"
                              onClick={(e) => {
                                e.stopPropagation();
                                void handlePreviewFile(file);
                              }}
                              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                              title={t("sftpView", "View")}
                            >
                              <Eye className="h-3.5 w-3.5" />
                            </button>

                            {/* 删除按钮 */}
                            <button
                              type="button"
                              onClick={(e) => {
                                e.stopPropagation();
                                void handleDeleteEntry(file);
                              }}
                              className="rounded p-1 text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition-colors"
                              title={t("sftpDelete", "Delete")}
                              aria-label={`${t("sftpDelete", "Delete")} ${file.name}`}
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        </div>

        {/* 实时传输任务抽屉卡片 */}
        {taskList.length > 0 ? (
          <div className="rounded-lg border bg-card shadow-sm overflow-hidden text-xs">
            {/* 顶栏控制条 */}
            <div className="flex items-center justify-between border-b bg-muted/30 px-3 py-2">
              <div className="flex items-center gap-2">
                <span className="font-semibold text-foreground">
                  {t("sftpTransfersTitle", "Transfers")}
                </span>
                <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                  {activeTaskCount > 0
                    ? `${activeTaskCount} ${t("sftpTransferring", "Active")}`
                    : t("sftpTransferCompleted", "Completed")}
                </span>
              </div>

              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={handleClearCompletedTransfers}
                  className="text-muted-foreground hover:text-foreground transition-colors text-[11px]"
                >
                  {t("sftpClearCompleted", "Clear Completed")}
                </button>
                <button
                  type="button"
                  onClick={() => setTransferPanelOpen(!transferPanelOpen)}
                  className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  {transferPanelOpen ? (
                    <ChevronUp className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronDown className="h-3.5 w-3.5" />
                  )}
                </button>
              </div>
            </div>

            {/* 任务内容列表 */}
            {transferPanelOpen ? (
              <div className="divide-y divide-border/60 max-h-56 overflow-y-auto p-1">
                {taskList.map((task) => {
                  const isUpload = task.transfer_type === "upload";
                  return (
                    <div
                      key={task.task_id}
                      className="p-2 space-y-1.5 hover:bg-muted/30 rounded-md transition-colors"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2 min-w-0">
                          {isUpload ? (
                            <span className="inline-flex items-center gap-1 rounded bg-purple-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-purple-600 dark:text-purple-400">
                              <Upload className="h-3 w-3" />
                              {t("sftpUpload", "Upload")}
                            </span>
                          ) : (
                            <span className="inline-flex items-center gap-1 rounded bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-sky-600 dark:text-sky-400">
                              <Download className="h-3 w-3" />
                              {t("sftpDownload", "Download")}
                            </span>
                          )}
                          <span className="font-mono font-medium truncate max-w-xs text-foreground">
                            {task.file_name}
                          </span>
                        </div>

                        <div className="flex items-center gap-2 shrink-0">
                          {task.status === "transferring" ? (
                            <>
                              <span className="font-mono text-xs font-semibold text-primary">
                                {task.percentage.toFixed(1)}%
                              </span>
                              <button
                                type="button"
                                onClick={() =>
                                  void handleCancelTransfer(task.task_id)
                                }
                                className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-rose-500 transition-colors"
                                title={t("sftpCancel", "Cancel")}
                              >
                                <X className="h-3.5 w-3.5" />
                              </button>
                            </>
                          ) : task.status === "completed" ? (
                            <span className="inline-flex items-center gap-1 text-emerald-600 dark:text-emerald-400 text-[11px] font-medium">
                              <CheckCircle2 className="h-3.5 w-3.5" />
                              {t("sftpTransferCompleted", "Completed")}
                            </span>
                          ) : task.status === "cancelled" ? (
                            <span className="text-muted-foreground text-[11px]">
                              {t("sftpTransferCancelled", "Cancelled")}
                            </span>
                          ) : (
                            <div className="flex items-center gap-1.5">
                              <span
                                className="inline-flex items-center gap-1 text-rose-500 text-[11px] max-w-[140px] truncate"
                                title={task.error || t("sftpTransferFailed", "Failed")}
                              >
                                <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                                <span>{t("sftpTransferFailed", "Failed")}</span>
                              </span>
                              {task.retry_params ? (
                                <button
                                  type="button"
                                  onClick={() => void handleRetryTransfer(task)}
                                  className="inline-flex items-center gap-1 rounded bg-rose-500/10 px-2 py-0.5 text-[11px] font-medium text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 transition-colors"
                                  title={t("sftpRetry", "Retry")}
                                >
                                  <RotateCw className="h-3 w-3" />
                                  <span>{t("sftpRetry", "Retry")}</span>
                                </button>
                              ) : null}
                            </div>
                          )}
                        </div>
                      </div>

                      {/* 进度条 */}
                      <div className="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                        <div
                          className={`h-full transition-all duration-200 ${
                            task.status === "error"
                              ? "bg-rose-500"
                              : task.status === "completed"
                                ? "bg-emerald-500"
                                : task.status === "cancelled"
                                  ? "bg-zinc-400"
                                  : "bg-primary"
                          }`}
                          style={{
                            width: `${Math.min(100, Math.max(0, task.percentage))}%`,
                          }}
                        />
                      </div>

                      {/* 统计指标行：大小 / 网速 / 剩余时长 */}
                      <div className="flex items-center justify-between text-[11px] text-muted-foreground font-mono">
                        <div>
                          {formatBytes(task.transferred_bytes)} /{" "}
                          {formatBytes(task.total_bytes)}
                        </div>
                        {task.status === "transferring" ? (
                          <div className="flex items-center gap-3">
                            <span className="text-foreground">
                              {formatSpeed(task.bytes_per_second)}
                            </span>
                            <span className="text-muted-foreground">
                              {formatEta(task.eta_seconds, isZh)}
                            </span>
                          </div>
                        ) : task.status === "error" && task.error ? (
                          <span
                            className="text-rose-500 text-[10px] truncate max-w-xs"
                            title={task.error}
                          >
                            {task.error}
                          </span>
                        ) : null}
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : null}
          </div>
        ) : null}

        {/* 实时 Log Tailer 抽屉控制台 */}
        {tailSession ? (
          <div className="rounded-lg border bg-zinc-950 text-zinc-100 overflow-hidden shadow-xl space-y-2">
            {/* 控制台顶栏 */}
            <div className="flex items-center justify-between border-b border-zinc-800 bg-zinc-900/80 px-3 py-2 text-xs">
              <div className="flex items-center gap-2">
                <span className="flex h-2 w-2 rounded-full bg-emerald-500 animate-ping" />
                <span className="font-mono font-semibold">
                  {tailSession.fileName}
                </span>
                <span className="text-[11px] text-zinc-400">
                  ({tailSession.lines.length} lines)
                </span>
              </div>

              <div className="flex items-center gap-2">
                <input
                  type="text"
                  value={tailSession.filter}
                  onChange={(e) =>
                    setTailSession({ ...tailSession, filter: e.target.value })
                  }
                  placeholder={t("sftpLogFilter", "Filter keywords...")}
                  className="h-6 w-32 rounded bg-zinc-800 px-2 font-mono text-[11px] text-zinc-200 outline-none"
                />

                <button
                  type="button"
                  onClick={() =>
                    setTailSession({
                      ...tailSession,
                      autoScroll: !tailSession.autoScroll,
                    })
                  }
                  className={`rounded px-2 py-0.5 text-[11px] ${
                    tailSession.autoScroll
                      ? "bg-emerald-500/20 text-emerald-400"
                      : "bg-zinc-800 text-zinc-400"
                  }`}
                >
                  {t("sftpAutoScroll", "Auto-scroll")}
                </button>

                <button
                  type="button"
                  onClick={() => void copyLog(tailSession.lines.join("\n"))}
                  className="rounded px-2 py-0.5 text-[11px] bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
                >
                  {copiedLog ? (
                    t("sftpCopied", "Copied")
                  ) : (
                    t("sftpCopy", "Copy")
                  )}
                </button>

                <button
                  type="button"
                  onClick={() =>
                    setTailSession({ ...tailSession, lines: [] })
                  }
                  className="rounded px-2 py-0.5 text-[11px] bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
                >
                  {t("sftpClear", "Clear")}
                </button>

                <button
                  type="button"
                  onClick={() => setTailSession(null)}
                  className="rounded p-1 text-zinc-400 hover:text-rose-400"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>
            </div>

            {/* 控制台日志内容 */}
            <div className="max-h-80 overflow-y-auto p-3 font-mono text-xs leading-5">
              {filteredLogLines.map((line, idx) => (
                <div
                  key={idx}
                  className="hover:bg-zinc-900/60 px-1 rounded break-all whitespace-pre-wrap"
                >
                  {line}
                </div>
              ))}
              <div ref={logConsoleEndRef} />
            </div>
          </div>
        ) : null}

        {/* 文件只读预览弹窗 */}
        {previewFile ? (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
            <div className="w-full max-w-3xl rounded-lg border bg-card p-4 shadow-2xl space-y-3">
              <div className="flex items-center justify-between border-b pb-2">
                <span className="font-mono text-xs font-semibold">
                  {previewFile.name}
                </span>
                <button
                  type="button"
                  onClick={() => setPreviewFile(null)}
                  className="rounded p-1 text-muted-foreground hover:bg-muted"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>
              <pre className="max-h-96 overflow-auto rounded-md bg-muted/60 p-3 font-mono text-xs leading-5">
                {previewFile.content}
              </pre>
            </div>
          </div>
        ) : null}

        {/* 远程回收站列表弹窗 */}
        {trashModalOpen ? (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
            <div className="w-full max-w-3xl rounded-lg border bg-card p-4 shadow-2xl space-y-3">
              {/* 弹窗头部 */}
              <div className="flex items-center justify-between border-b pb-3">
                <div className="flex items-center gap-2">
                  <div className="rounded-md bg-destructive/10 p-1.5 text-destructive">
                    <Trash2 className="h-4 w-4" />
                  </div>
                  <div>
                    <h3 className="text-sm font-semibold text-foreground">
                      {t("sftpTrashModalTitle", "远程回收站")}
                    </h3>
                    <p className="text-[11px] text-muted-foreground">
                      {t("sftpTrashItemCount", "共 {{count}} 个已回收项目", {
                        count: trashItems.length,
                      })}
                    </p>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  {trashItems.length > 0 ? (
                    <button
                      type="button"
                      onClick={() => void handleClearTrash()}
                      className="inline-flex h-7 items-center gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 text-xs font-medium text-destructive hover:bg-destructive/20 transition-colors"
                    >
                      <Trash2 className="h-3 w-3" />
                      <span>{t("sftpClearTrash", "清空回收站")}</span>
                    </button>
                  ) : null}

                  <button
                    type="button"
                    onClick={() => void loadTrashItems()}
                    disabled={trashLoading}
                    className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
                    title={t("sftpRefresh", "刷新")}
                  >
                    <RefreshCw
                      className={`h-3.5 w-3.5 ${trashLoading ? "animate-spin" : ""}`}
                    />
                  </button>

                  <button
                    type="button"
                    onClick={() => setTrashModalOpen(false)}
                    className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                    title={t("close", "关闭")}
                  >
                    <X className="h-4 w-4" />
                  </button>
                </div>
              </div>

              {/* 回收站列表内容 */}
              <div className="max-h-96 overflow-y-auto">
                {trashLoading && trashItems.length === 0 ? (
                  <div className="flex items-center justify-center py-12 text-muted-foreground text-xs gap-2">
                    <Loader2 className="h-4 w-4 animate-spin text-primary" />
                    <span>{t("loading", "正在加载...")}</span>
                  </div>
                ) : trashItems.length === 0 ? (
                  <div className="flex flex-col items-center justify-center py-12 text-center text-xs text-muted-foreground space-y-2">
                    <div className="rounded-full bg-muted/60 p-3">
                      <Trash2 className="h-6 w-6 text-muted-foreground/50" />
                    </div>
                    <span>{t("sftpTrashEmpty", "回收站为空。")}</span>
                  </div>
                ) : (
                  <table className="w-full text-left text-xs">
                    <thead className="border-b bg-muted/20 text-[11px] text-muted-foreground uppercase">
                      <tr>
                        <th className="py-2 px-3 font-medium">
                          {t("sftpColName", "名称")}
                        </th>
                        <th className="py-2 px-3 font-medium">
                          {t("sftpColOriginalPath", "原路径")}
                        </th>
                        <th className="py-2 px-3 font-medium w-24">
                          {t("sftpColSize", "大小")}
                        </th>
                        <th className="py-2 px-3 font-medium w-36">
                          {t("sftpColDeletedAt", "删除时间")}
                        </th>
                        <th className="py-2 px-3 font-medium w-28 text-right">
                          {t("sftpColActions", "操作")}
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-border/60">
                      {trashItems.map((item) => (
                        <tr
                          key={item.id}
                          className="hover:bg-muted/50 transition-colors"
                        >
                          <td className="py-2 px-3 font-mono">
                            <div className="flex items-center gap-2">
                              {item.is_dir ? (
                                <Folder className="h-4 w-4 text-sky-500 fill-sky-500/20 shrink-0" />
                              ) : (
                                <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
                              )}
                              <span className="font-medium text-foreground truncate max-w-[180px]">
                                {item.name}
                              </span>
                            </div>
                          </td>
                          <td className="py-2 px-3 font-mono text-muted-foreground text-[11px] truncate max-w-xs">
                            <span title={item.original_path}>
                              {item.original_path}
                            </span>
                          </td>
                          <td className="py-2 px-3 font-mono text-muted-foreground">
                            {item.is_dir ? "-" : formatBytes(item.size)}
                          </td>
                          <td className="py-2 px-3 text-muted-foreground text-[11px]">
                            {item.deleted_at}
                          </td>
                          <td className="py-2 px-3 text-right">
                            <button
                              type="button"
                              onClick={() => void handleRestoreTrashItem(item)}
                              disabled={restoringTrashId === item.id}
                              className="inline-flex items-center gap-1.5 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary hover:bg-primary/20 disabled:opacity-50 transition-colors"
                              title={t("sftpRestoreToOriginal", "恢复到原路径")}
                            >
                              {restoringTrashId === item.id ? (
                                <Loader2 className="h-3 w-3 animate-spin" />
                              ) : (
                                <RotateCcw className="h-3 w-3" />
                              )}
                              <span>{t("sftpRestore", "恢复")}</span>
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </ToolShell>
  );
}
