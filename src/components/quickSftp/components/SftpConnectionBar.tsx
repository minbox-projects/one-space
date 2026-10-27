import type { TFunction } from "i18next";
import {
  ArrowLeft,
  History,
  Loader2,
  Plug,
  RefreshCw,
  Server,
  Trash2,
  Unplug,
  Upload,
} from "lucide-react";
import type { SshHostOption } from "@/lib/sftp";
import { DEFAULT_BOOKMARKS } from "../constants";

type SftpConnectionBarProps = {
  t: TFunction;
  hosts: SshHostOption[];
  selectedHost: string;
  customHostInput: string;
  isConnected: boolean;
  connecting: boolean;
  connectedHost: string | null;
  recentConnections: string[];
  pathInput: string;
  currentPath: string;
  loading: boolean;
  isRefreshing: boolean;
  onHostChange: (host: string) => void;
  onCustomHostChange: (value: string) => void;
  onConnect: (path: string) => void;
  onDisconnect: () => void;
  onQuickConnect: (host: string) => void;
  onClearRecentConnections: () => void;
  onSelectBookmark: (path: string) => void;
  onGoParent: () => void;
  onSubmitPath: () => void;
  onPathInputChange: (value: string) => void;
  onUploadFiles: () => void;
  onOpenTrash: () => void;
  onRefresh: () => void;
};

export function SftpConnectionBar({
  t,
  hosts,
  selectedHost,
  customHostInput,
  isConnected,
  connecting,
  connectedHost,
  recentConnections,
  pathInput,
  currentPath,
  loading,
  isRefreshing,
  onHostChange,
  onCustomHostChange,
  onConnect,
  onDisconnect,
  onQuickConnect,
  onClearRecentConnections,
  onSelectBookmark,
  onGoParent,
  onSubmitPath,
  onPathInputChange,
  onUploadFiles,
  onOpenTrash,
  onRefresh,
}: SftpConnectionBarProps) {
  return (
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
              onChange={(e) => onHostChange(e.target.value)}
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
              onChange={(e) => onCustomHostChange(e.target.value)}
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
                onClick={onDisconnect}
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-rose-500/30 bg-rose-500/10 px-3 text-xs font-medium text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 transition-colors"
              >
                <Unplug className="h-3.5 w-3.5" />
                {t("sftpDisconnect", "断开连接")}
              </button>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => onConnect(pathInput)}
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
              onClick={() => onSelectBookmark(bm)}
              className="rounded-md border bg-background px-2 py-0.5 text-xs font-mono text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              {bm}
            </button>
          ))}
        </div>
      </div>

      {/* 最近连接记录快捷条 */}
      {recentConnections.length > 0 ? (
        <div
          data-testid="sftp-recent-connections"
          className="flex flex-wrap items-center gap-1.5 border-t pt-2 text-xs"
        >
          <div className="flex items-center gap-1 text-muted-foreground text-[11px] shrink-0">
            <History className="h-3.5 w-3.5 shrink-0" />
            <span>{t("sftpRecentConnections", "最近连接")}:</span>
          </div>
          {recentConnections.map((recHost) => {
            const isCurrent = isConnected && connectedHost === recHost;
            return (
              <button
                key={recHost}
                type="button"
                data-testid={`sftp-recent-item-${recHost}`}
                onClick={() => onQuickConnect(recHost)}
                disabled={connecting || isCurrent}
                title={t("sftpQuickConnectTo", "快捷连接至 {{host}}", {
                  host: recHost,
                })}
                className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 font-mono text-xs transition-colors ${
                  isCurrent
                    ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 cursor-default"
                    : "border-border/80 bg-background text-foreground hover:border-primary/50 hover:bg-primary/5 hover:text-primary"
                } disabled:opacity-50`}
              >
                <Server className="h-3 w-3 shrink-0 opacity-70" />
                <span>{recHost}</span>
              </button>
            );
          })}
          <button
            type="button"
            data-testid="sftp-clear-recent"
            onClick={onClearRecentConnections}
            title={t("sftpClearRecent", "清空")}
            className="inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-destructive transition-colors ml-auto sm:ml-1"
          >
            <Trash2 className="h-3 w-3" />
            <span>{t("sftpClearRecent", "清空")}</span>
          </button>
        </div>
      ) : null}

      {/* 路径与操作栏 */}
      <div className="flex items-center gap-2 border-t pt-2">
        <button
          type="button"
          onClick={onGoParent}
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
            onSubmitPath();
          }}
          className="flex-1"
        >
          <input
            type="text"
            value={pathInput}
            onChange={(e) => onPathInputChange(e.target.value)}
            placeholder="~ or /var/log"
            className="h-8 w-full rounded-md border bg-background px-3 font-mono text-xs outline-none"
          />
        </form>

        {/* 上传文件按钮 */}
        <button
          type="button"
          onClick={onUploadFiles}
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
          onClick={onOpenTrash}
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
          onClick={onRefresh}
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
  );
}
