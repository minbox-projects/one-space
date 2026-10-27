import type { TFunction } from "i18next";
import {
  Download,
  Eye,
  FileText,
  Folder,
  FolderOpen,
  Loader2,
  Plug,
  Radio,
  Search,
  Server,
  Terminal,
  Trash2,
} from "lucide-react";
import { formatBytes } from "@/lib/sftp";
import type { RemoteFileEntry } from "@/lib/sftp";

type SftpFileTableProps = {
  t: TFunction;
  isConnected: boolean;
  connecting: boolean;
  currentPath: string;
  pathInput: string;
  filesLength: number;
  filteredFiles: RemoteFileEntry[];
  searchFilter: string;
  onSearchFilterChange: (value: string) => void;
  loading: boolean;
  isRefreshing: boolean;
  activeDownloadingFileNames: Set<string>;
  selectedHost: string;
  customHostInput: string;
  recentConnections: string[];
  onConnect: (path: string) => void;
  onQuickConnect: (host: string) => void;
  onLoadDirectory: (path: string) => void;
  onPreviewFile: (entry: RemoteFileEntry) => void;
  onDownloadFile: (entry: RemoteFileEntry) => void;
  onStartTail: (entry: RemoteFileEntry) => void;
  onDeleteEntry: (entry: RemoteFileEntry) => void;
};

export function SftpFileTable({
  t,
  isConnected,
  connecting,
  currentPath,
  pathInput,
  filesLength,
  filteredFiles,
  searchFilter,
  onSearchFilterChange,
  loading,
  isRefreshing,
  activeDownloadingFileNames,
  selectedHost,
  customHostInput,
  recentConnections,
  onConnect,
  onQuickConnect,
  onLoadDirectory,
  onPreviewFile,
  onDownloadFile,
  onStartTail,
  onDeleteEntry,
}: SftpFileTableProps) {
  return (
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
              onChange={(e) => onSearchFilterChange(e.target.value)}
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
              onClick={() => onConnect(pathInput)}
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

            {recentConnections.length > 0 ? (
              <div className="pt-2 flex flex-col items-center gap-1.5">
                <span className="text-[11px] text-muted-foreground">
                  {t("sftpRecentConnections", "最近连接")}:
                </span>
                <div className="flex flex-wrap justify-center gap-1.5 max-w-md">
                  {recentConnections.map((recHost) => (
                    <button
                      key={recHost}
                      type="button"
                      data-testid={`sftp-empty-recent-${recHost}`}
                      onClick={() => onQuickConnect(recHost)}
                      disabled={connecting}
                      className="inline-flex items-center gap-1 rounded-md border bg-background px-2.5 py-1 font-mono text-xs hover:border-primary/50 hover:bg-primary/5 hover:text-primary transition-colors"
                    >
                      <Server className="h-3 w-3 opacity-70" />
                      <span>{recHost}</span>
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        ) : loading && filesLength === 0 ? (
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
                        onLoadDirectory(file.path);
                      } else {
                        onPreviewFile(file);
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
                              onDownloadFile(file);
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
                              onStartTail(file);
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
                            onPreviewFile(file);
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
                            onDeleteEntry(file);
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
  );
}
