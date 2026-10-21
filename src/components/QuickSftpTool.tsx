import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  Eye,
  FileText,
  Folder,
  FolderOpen,
  FolderSync,
  Loader2,
  Radio,
  RefreshCw,
  Search,
  Server,
  Terminal,
  X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { useToast } from "./ToastProvider";
import { getToolboxTool } from "@/toolbox/registry";
import { isToolboxInvokeAvailable } from "@/toolbox/invoke";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";

interface RemoteFileEntry {
  name: string;
  is_dir: boolean;
  size: number;
  permissions: string;
  modified: string;
  path: string;
}

interface SshHostOption {
  name: string;
  host_name: string;
  user: string;
  port: number;
}

const DEFAULT_BOOKMARKS = ["~", "/var/log", "/etc", "/tmp"];

export function QuickSftpTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const toolDescriptor = getToolboxTool("quick-sftp");
  const ToolIcon = toolDescriptor?.icon ?? FolderSync;
  const iconClassName =
    toolDescriptor?.iconClassName ?? "bg-purple-500/10 text-purple-600";

  // 服务器选择
  const [hosts, setHosts] = useState<SshHostOption[]>([]);
  const [selectedHost, setSelectedHost] = useState("");
  const [customHostInput, setCustomHostInput] = useState("");

  // 目录浏览
  const [currentPath, setCurrentPath] = useState("~");
  const [pathInput, setPathInput] = useState("~");
  const [files, setFiles] = useState<RemoteFileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchFilter, setSearchFilter] = useState("");

  // 文本预览 Modal
  const [previewFile, setPreviewFile] = useState<{
    name: string;
    content: string;
  } | null>(null);

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

  // 加载目录
  const loadDirectory = async (path: string) => {
    const activeHost = selectedHost || customHostInput.trim();
    if (!activeHost) return;

    setLoading(true);
    setError(null);
    try {
      if (isToolboxInvokeAvailable()) {
        const entries = await invoke<RemoteFileEntry[]>("sftp_list_directory", {
          host: activeHost,
          remotePath: path,
        });
        setFiles(entries);
        setCurrentPath(path);
        setPathInput(path);
      } else {
        // Mock 测试数据
        setFiles([
          {
            name: "nginx",
            is_dir: true,
            size: 4096,
            permissions: "drwxr-xr-x",
            modified: "2026-10-21 14:00",
            path: `${path}/nginx`,
          },
          {
            name: "app.log",
            is_dir: false,
            size: 102400,
            permissions: "-rw-r--r--",
            modified: "2026-10-21 15:30",
            path: `${path}/app.log`,
          },
          {
            name: "config.json",
            is_dir: false,
            size: 1240,
            permissions: "-rw-r--r--",
            modified: "2026-10-20 09:12",
            path: `${path}/config.json`,
          },
        ]);
        setCurrentPath(path);
        setPathInput(path);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  // 路径导航辅助
  const navigateToDirectory = (path: string) => {
    setCurrentPath(path);
    setPathInput(path);
    void loadDirectory(path);
  };

  // 切换 Host 时刷新目录
  useEffect(() => {
    if (selectedHost || customHostInput) {
      void loadDirectory(currentPath);
    }
  }, [selectedHost]);

  // 返回上一级
  const handleGoParent = () => {
    if (currentPath === "/" || currentPath === "~") return;
    const parent = currentPath.substring(0, currentPath.lastIndexOf("/")) || "/";
    navigateToDirectory(parent);
  };

  // 快速文本预览
  const handlePreviewFile = async (entry: RemoteFileEntry) => {
    const activeHost = selectedHost || customHostInput.trim();
    if (!activeHost) return;

    try {
      if (isToolboxInvokeAvailable()) {
        const content = await invoke<string>("sftp_read_file", {
          host: activeHost,
          remotePath: entry.path,
          maxBytes: 128 * 1024,
        });
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
        await invoke("sftp_start_log_tail", {
          host: activeHost,
          remotePath: entry.path,
          tailId,
        });
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
        invoke("sftp_stop_log_tail", { tailId }).catch(() => {});
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
            <div className="flex items-center gap-2">
              <Server className="h-4 w-4 text-primary" />
              <span className="text-xs font-semibold">
                {t("sftpSelectHost", "SSH Host")}:
              </span>
              {hosts.length > 0 ? (
                <select
                  value={selectedHost}
                  onChange={(e) => setSelectedHost(e.target.value)}
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
                  onChange={(e) => setCustomHostInput(e.target.value)}
                  placeholder="user@host or config alias"
                  className="h-8 rounded-md border bg-background px-2.5 font-mono text-xs outline-none"
                />
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
                  onClick={() => navigateToDirectory(bm)}
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
              title={t("sftpParentDir", "Parent Directory")}
              className="inline-flex h-8 w-8 items-center justify-center rounded-md border bg-background hover:bg-muted text-muted-foreground"
            >
              <ArrowLeft className="h-4 w-4" />
            </button>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                navigateToDirectory(pathInput);
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

            <button
              type="button"
              onClick={() => void loadDirectory(currentPath)}
              disabled={loading}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-3 text-xs font-medium hover:bg-muted"
            >
              <RefreshCw
                className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`}
              />
              {t("sftpRefresh", "Refresh")}
            </button>
          </div>
        </div>

        {/* 文件列表区 */}
        <div className="rounded-lg border bg-card overflow-hidden shadow-sm">
          <div className="flex items-center justify-between border-b bg-muted/40 px-3 py-2 text-xs">
            <div className="flex items-center gap-2 font-medium">
              <FolderOpen className="h-4 w-4 text-primary" />
              <span>
                {currentPath} ({filteredFiles.length} {t("sftpItems", "items")})
              </span>
            </div>
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
          </div>

          <div className="max-h-96 overflow-y-auto">
            {loading ? (
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
                          {file.is_dir
                            ? "-"
                            : (file.size / 1024).toFixed(1) + " KB"}
                        </td>
                        <td className="py-2 px-3 text-muted-foreground text-[11px]">
                          {file.modified}
                        </td>
                        <td className="py-2 px-3 font-mono text-[11px] text-muted-foreground">
                          {file.permissions}
                        </td>
                        <td className="py-2 px-3 text-right">
                          <div className="flex items-center justify-end gap-1.5">
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
      </div>
    </ToolShell>
  );
}
