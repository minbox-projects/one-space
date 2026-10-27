import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { FolderSync } from "lucide-react";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { useToast } from "./ToastProvider";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import { getToolboxTool } from "@/toolbox/registry";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import {
  listenSftpTransferProgress,
  sftpStopLogTail,
  type RemoteFileEntry,
  type SftpTransferProgressPayload,
  type SftpTrashItem,
  type SshHostOption,
} from "@/lib/sftp";
import {
  INITIAL_MOCK_ENTRIES,
  SFTP_RECENT_CONNECTIONS_KEY,
} from "./quickSftp/constants";
import type { SftpPreviewFile, SftpTailSession } from "./quickSftp/types";
import { createSftpWorkspaceActions } from "./quickSftp/actions/workspaceActions";
import { createSftpTransferActions } from "./quickSftp/actions/transferActions";
import { SftpConnectionBar } from "./quickSftp/components/SftpConnectionBar";
import { SftpFileTable } from "./quickSftp/components/SftpFileTable";
import { SftpTransferPanel } from "./quickSftp/components/SftpTransferPanel";
import { SftpLogConsole } from "./quickSftp/components/SftpLogConsole";
import { SftpPreviewDialog } from "./quickSftp/components/SftpPreviewDialog";
import { SftpTrashDialog } from "./quickSftp/components/SftpTrashDialog";

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

  // 最近连接记录
  const [recentConnections, setRecentConnections] = useState<string[]>(() => {
    try {
      const stored = localStorage.getItem(SFTP_RECENT_CONNECTIONS_KEY);
      if (!stored) return [];
      const parsed = JSON.parse(stored);
      return Array.isArray(parsed)
        ? parsed.filter(
            (item): item is string =>
              typeof item === "string" && Boolean(item.trim()),
          )
        : [];
    } catch {
      return [];
    }
  });

  // 目录浏览
  const [currentPath, setCurrentPathState] = useState("~");
  const currentPathRef = useRef("~");
  const setCurrentPath = (nextPath: string) => {
    currentPathRef.current = nextPath;
    setCurrentPathState(nextPath);
  };

  const [pathInput, setPathInput] = useState("~");
  const [files, setFiles] = useState<RemoteFileEntry[]>([]);
  const mockFilesRef = useRef<RemoteFileEntry[]>(INITIAL_MOCK_ENTRIES);
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
  const [previewFile, setPreviewFile] = useState<SftpPreviewFile | null>(null);

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
  const [tailSession, setTailSession] = useState<SftpTailSession | null>(null);

  const logConsoleEndRef = useRef<HTMLDivElement>(null);

  const { copied: copiedLog, copy: copyLog } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: t("sftpLogCopied", "Log output copied"),
        kind: "success",
      }),
  });

  const getCurrentPath = () => currentPathRef.current;
  const getMockFiles = () => mockFilesRef.current;
  const setMockFiles = (entries: RemoteFileEntry[]) => {
    mockFilesRef.current = entries;
  };
  const getMockTrash = () => mockTrashRef.current;
  const setMockTrash = (items: SftpTrashItem[]) => {
    mockTrashRef.current = items;
  };
  // eslint-disable-next-line react-hooks/refs -- refs are only read from async action callbacks, never during render
  const workspace = createSftpWorkspaceActions({
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
  });
  const transfers = createSftpTransferActions({
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
    loadDirectory: workspace.loadDirectory,
  });
  const { loadDirectory } = workspace;

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
        <SftpConnectionBar
          t={t}
          hosts={hosts}
          selectedHost={selectedHost}
          customHostInput={customHostInput}
          isConnected={isConnected}
          connecting={connecting}
          connectedHost={connectedHost}
          recentConnections={recentConnections}
          pathInput={pathInput}
          currentPath={currentPath}
          loading={loading}
          isRefreshing={isRefreshing}
          onHostChange={workspace.handleHostChange}
          onCustomHostChange={workspace.handleCustomHostChange}
          onConnect={(path) => void workspace.handleConnect(path)}
          onDisconnect={workspace.handleDisconnect}
          onQuickConnect={(host) => void workspace.handleQuickConnect(host)}
          onClearRecentConnections={workspace.handleClearRecentConnections}
          onSelectBookmark={workspace.handleSelectBookmark}
          onGoParent={workspace.handleGoParent}
          onSubmitPath={() => {
            if (isConnected) {
              workspace.navigateToDirectory(pathInput);
            } else {
              void workspace.handleConnect(pathInput);
            }
          }}
          onPathInputChange={setPathInput}
          onUploadFiles={() => void transfers.handleUploadFiles()}
          onOpenTrash={() => void workspace.handleOpenTrashModal()}
          onRefresh={() =>
            void loadDirectory(currentPath, { silent: files.length > 0 })
          }
        />

        {/* 文件列表区 */}
        <SftpFileTable
          t={t}
          isConnected={isConnected}
          connecting={connecting}
          currentPath={currentPath}
          pathInput={pathInput}
          filesLength={files.length}
          filteredFiles={filteredFiles}
          searchFilter={searchFilter}
          onSearchFilterChange={setSearchFilter}
          loading={loading}
          isRefreshing={isRefreshing}
          activeDownloadingFileNames={activeDownloadingFileNames}
          selectedHost={selectedHost}
          customHostInput={customHostInput}
          recentConnections={recentConnections}
          onConnect={(path) => void workspace.handleConnect(path)}
          onQuickConnect={(host) => void workspace.handleQuickConnect(host)}
          onLoadDirectory={(path) => void loadDirectory(path)}
          onPreviewFile={(entry) => void workspace.handlePreviewFile(entry)}
          onDownloadFile={(entry) => void transfers.handleDownloadFile(entry)}
          onStartTail={(entry) => void transfers.handleStartTail(entry)}
          onDeleteEntry={(entry) => void workspace.handleDeleteEntry(entry)}
        />

        {/* 实时传输任务抽屉卡片 */}
        {taskList.length > 0 ? (
          <SftpTransferPanel
            t={t}
            isZh={isZh}
            taskList={taskList}
            activeTaskCount={activeTaskCount}
            transferPanelOpen={transferPanelOpen}
            onTogglePanel={() => setTransferPanelOpen(!transferPanelOpen)}
            onClearCompleted={transfers.handleClearCompletedTransfers}
            onCancelTransfer={(taskId) => void transfers.handleCancelTransfer(taskId)}
            onRetryTransfer={(task) => void transfers.handleRetryTransfer(task)}
          />
        ) : null}

        {/* 实时 Log Tailer 抽屉控制台 */}
        {tailSession ? (
          <SftpLogConsole
            t={t}
            tailSession={tailSession}
            setTailSession={setTailSession}
            filteredLogLines={filteredLogLines}
            copiedLog={copiedLog}
            copyLog={copyLog}
            logConsoleEndRef={logConsoleEndRef}
          />
        ) : null}

        {/* 文件只读预览弹窗 */}
        {previewFile ? (
          <SftpPreviewDialog
            previewFile={previewFile}
            onClose={() => setPreviewFile(null)}
          />
        ) : null}

        {/* 远程回收站列表弹窗 */}
        {trashModalOpen ? (
          <SftpTrashDialog
            t={t}
            trashItems={trashItems}
            trashLoading={trashLoading}
            restoringTrashId={restoringTrashId}
            onClose={() => setTrashModalOpen(false)}
            onRefresh={() => void workspace.loadTrashItems()}
            onClearTrash={() => void workspace.handleClearTrash()}
            onRestoreTrashItem={(item) => void workspace.handleRestoreTrashItem(item)}
          />
        ) : null}
      </div>
    </ToolShell>
  );
}
