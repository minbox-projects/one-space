import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import {
  sftpClearTrash,
  sftpDeleteEntry,
  sftpListDirectory,
  sftpListTrash,
  sftpReadFile,
  sftpRestoreEntry,
  sftpRestoreTrashItem,
  sftpStopLogTail,
} from "@/lib/sftp";
import type {
  RemoteFileEntry,
  SftpTrashItem,
  SshHostOption,
} from "@/lib/sftp";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import {
  MAX_RECENT_CONNECTIONS,
  SFTP_RECENT_CONNECTIONS_KEY,
} from "../constants";
import type { SftpPreviewFile, SftpTailSession } from "../types";

export type PushToast = (toast: {
  title: string;
  description?: string;
  kind?: "info" | "success" | "warning" | "error" | "loading";
  durationMs?: number;
  action?: { label: string; onClick: () => void };
}) => string;

type ConfirmDialogFn = (
  message: string,
  options?: {
    title?: string;
    okLabel?: string;
    cancelLabel?: string;
    kind?: "info" | "warning" | "error";
  },
) => Promise<boolean>;

export type SftpWorkspaceParams = {
  t: TFunction;
  pushToast: PushToast;
  confirmDialog: ConfirmDialogFn;
  hosts: SshHostOption[];
  selectedHost: string;
  customHostInput: string;
  isConnected: boolean;
  connecting: boolean;
  connectedHost: string | null;
  currentPath: string;
  pathInput: string;
  tailSession: SftpTailSession | null;
  setSelectedHost: Dispatch<SetStateAction<string>>;
  setCustomHostInput: Dispatch<SetStateAction<string>>;
  setConnecting: Dispatch<SetStateAction<boolean>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setIsConnected: Dispatch<SetStateAction<boolean>>;
  setConnectedHost: Dispatch<SetStateAction<string | null>>;
  setRecentConnections: Dispatch<SetStateAction<string[]>>;
  setCurrentPath: (path: string) => void;
  setPathInput: Dispatch<SetStateAction<string>>;
  setFiles: Dispatch<SetStateAction<RemoteFileEntry[]>>;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setIsRefreshing: Dispatch<SetStateAction<boolean>>;
  setSearchFilter: Dispatch<SetStateAction<string>>;
  setPreviewFile: Dispatch<SetStateAction<SftpPreviewFile | null>>;
  setTrashModalOpen: Dispatch<SetStateAction<boolean>>;
  setTrashItems: Dispatch<SetStateAction<SftpTrashItem[]>>;
  setTrashLoading: Dispatch<SetStateAction<boolean>>;
  setRestoringTrashId: Dispatch<SetStateAction<string | null>>;
  setTailSession: Dispatch<SetStateAction<SftpTailSession | null>>;
  getCurrentPath: () => string;
  getMockFiles: () => RemoteFileEntry[];
  setMockFiles: (entries: RemoteFileEntry[]) => void;
  getMockTrash: () => SftpTrashItem[];
  setMockTrash: (items: SftpTrashItem[]) => void;
};

export function createSftpWorkspaceActions({
  t,
  pushToast,
  confirmDialog,
  hosts,
  selectedHost,
  customHostInput,
  isConnected,
  connecting,
  connectedHost,
  currentPath,
  pathInput,
  tailSession,
  setSelectedHost,
  setCustomHostInput,
  setConnecting,
  setError,
  setIsConnected,
  setConnectedHost,
  setRecentConnections,
  setCurrentPath,
  setPathInput,
  setFiles,
  setLoading,
  setIsRefreshing,
  setSearchFilter,
  setPreviewFile,
  setTrashModalOpen,
  setTrashItems,
  setTrashLoading,
  setRestoringTrashId,
  setTailSession,
  getCurrentPath,
  getMockFiles,
  setMockFiles,
  getMockTrash,
  setMockTrash,
}: SftpWorkspaceParams) {
  const addRecentConnection = (host: string) => {
    const trimmed = host.trim();
    if (!trimmed) return;
    setRecentConnections((prev) => {
      const next = [trimmed, ...prev.filter((h) => h !== trimmed)].slice(
        0,
        MAX_RECENT_CONNECTIONS,
      );
      try {
        localStorage.setItem(SFTP_RECENT_CONNECTIONS_KEY, JSON.stringify(next));
      } catch (err) {
        console.error("Failed to save sftp recent connection", err);
      }
      return next;
    });
  };

  const handleClearRecentConnections = () => {
    try {
      localStorage.removeItem(SFTP_RECENT_CONNECTIONS_KEY);
    } catch (err) {
      console.error("Failed to clear sftp recent connections", err);
    }
    setRecentConnections([]);
  };

  const getMockDirectoryEntries = (basePath: string): RemoteFileEntry[] => {
    return getMockFiles().map((entry) => ({
      ...entry,
      path: `${basePath}/${entry.name}`.replace(/\/\//g, "/"),
    }));
  };

  // 建立连接
  const handleConnect = async (
    targetPath = pathInput || "~",
    hostOverride?: string,
  ) => {
    const activeHost = (
      hostOverride ||
      selectedHost ||
      customHostInput
    ).trim();
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
      addRecentConnection(activeHost);
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
    setCurrentPath("~");
    setPathInput("~");
    setSearchFilter("");
  };

  // 快捷连接最近记录
  const handleQuickConnect = async (host: string) => {
    if (connecting) return;
    if (isConnected && connectedHost === host) return;

    const matchedHost = hosts.find((h) => h.name === host);
    if (matchedHost) {
      setSelectedHost(matchedHost.name);
    } else {
      setCustomHostInput(host);
    }

    if (isConnected) {
      handleDisconnect();
    }
    await handleConnect("~", host);
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
    setMockFiles(getMockFiles().filter((item) => item.name !== file.name));
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
      setMockTrash([mockTrashItem, ...getMockTrash()]);
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
                  setMockFiles([file, ...getMockFiles()]);
                  setFiles((prev) => [file, ...prev]);
                  pushToast({
                    title: t("sftpRestoredToast", '已恢复 "{{name}}"', {
                      name: file.name,
                    }),
                    kind: "success",
                  });
                  // 恢复后静默刷新
                  void loadDirectory(getCurrentPath(), { silent: true });
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
        void loadDirectory(getCurrentPath(), { silent: true });
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
              setMockFiles([file, ...getMockFiles()]);
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
        void loadDirectory(getCurrentPath(), { silent: true });
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
        setTrashItems(getMockTrash());
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
        setMockTrash(getMockTrash().filter((t) => t.id !== item.id));
        pushToast({
          title: t("sftpRestoredToToast", '已将 "{{name}}" 恢复至 {{path}}', {
            name: item.name,
            path: restoredPath || item.original_path,
          }),
          kind: "success",
        });
        void loadDirectory(getCurrentPath(), { silent: true });
      } else {
        setTrashItems((prev) => prev.filter((t) => t.id !== item.id));
        setMockTrash(getMockTrash().filter((t) => t.id !== item.id));
        const restoredEntry: RemoteFileEntry = {
          name: item.name,
          is_dir: item.is_dir,
          size: item.size,
          permissions: "-rw-r--r--",
          modified: new Date().toISOString().replace("T", " ").substring(0, 16),
          path: item.original_path,
        };
        setMockFiles([restoredEntry, ...getMockFiles()]);
        setFiles((prev) => [restoredEntry, ...prev]);
        pushToast({
          title: t("sftpRestoredToToast", '已将 "{{name}}" 恢复至 {{path}}', {
            name: item.name,
            path: item.original_path,
          }),
          kind: "success",
        });
        void loadDirectory(getCurrentPath(), { silent: true });
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
      setMockTrash([]);
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

  return {
    addRecentConnection,
    handleClearRecentConnections,
    handleConnect,
    handleDisconnect,
    handleQuickConnect,
    handleHostChange,
    handleCustomHostChange,
    loadDirectory,
    handleDeleteEntry,
    handleOpenTrashModal,
    loadTrashItems,
    handleRestoreTrashItem,
    handleClearTrash,
    navigateToDirectory,
    handleSelectBookmark,
    handleGoParent,
    handlePreviewFile,
  };
}
