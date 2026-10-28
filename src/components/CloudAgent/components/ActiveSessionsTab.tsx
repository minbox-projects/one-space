import { useState } from "react";
import {
  GitBranch,
  Folder,
  Clock,
  Power,
  Play,
  Loader2,
  Sparkles,
  Layers,
  FolderGit2,
  Copy,
  Check,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import type { CloudSession } from "@/lib/cloudAgent";

interface ActiveSessionsTabProps {
  t: TFunction;
  language: string;
  sessions: CloudSession[];
  loading: boolean;
  onLaunchTerminal: (sessionName: string) => Promise<void>;
  onKillSession: (sessionName: string) => Promise<void>;
  onOpenNewTaskModal: () => void;
}

export function ActiveSessionsTab({
  t,
  language,
  sessions,
  loading,
  onLaunchTerminal,
  onKillSession,
  onOpenNewTaskModal,
}: ActiveSessionsTabProps) {
  const [connectingSession, setConnectingSession] = useState<string | null>(null);
  const [killingSession, setKillingSession] = useState<string | null>(null);
  const [copiedSession, setCopiedSession] = useState<string | null>(null);
  const dateLocale = language === "zh" ? zhCN : enUS;

  const handleConnect = async (sessionName: string) => {
    try {
      setConnectingSession(sessionName);
      await onLaunchTerminal(sessionName);
    } finally {
      setConnectingSession(null);
    }
  };

  const handleKill = async (sessionName: string) => {
    if (!confirm(t("confirmKillSession", `确定要终止云端会话 [${sessionName}] 吗？`))) {
      return;
    }
    try {
      setKillingSession(sessionName);
      await onKillSession(sessionName);
    } finally {
      setKillingSession(null);
    }
  };

  const handleCopyPath = (path: string, sessionName: string) => {
    navigator.clipboard.writeText(path);
    setCopiedSession(sessionName);
    setTimeout(() => {
      setCopiedSession((curr) => (curr === sessionName ? null : curr));
    }, 2000);
  };

  if (loading && sessions.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-emerald-500" />
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="rounded-2xl border border-dashed border-slate-300 dark:border-slate-800 bg-white/50 dark:bg-slate-900/40 p-12 text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-2xl bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400 mb-4">
          <Layers className="h-6 w-6" />
        </div>
        <h3 className="text-base font-semibold text-slate-800 dark:text-slate-200">
          {t("noActiveSessionsTitle", "当前没有正在运行的云端开发会话")}
        </h3>
        <p className="mt-1 text-xs text-slate-500 dark:text-slate-400 max-w-sm mx-auto">
          {t(
            "noActiveSessionsDesc",
            "在云端创建 Git Worktree 特性隔离任务，或恢复离线的历史工作区，即可瞬间调起本地终端开展长程编程。"
          )}
        </p>
        <button
          type="button"
          onClick={onOpenNewTaskModal}
          className="mt-5 inline-flex items-center gap-2 rounded-xl bg-emerald-600 px-4 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all"
        >
          <Sparkles className="h-3.5 w-3.5" />
          {t("createNewFeatureTask", "新建云端特性任务")}
        </button>
      </div>
    );
  }

  const parseDate = (val: number | string | undefined) => {
    if (!val) return new Date();
    if (typeof val === "number") {
      return new Date(val > 1e11 ? val : val * 1000);
    }
    const d = new Date(val);
    return isNaN(d.getTime()) ? new Date() : d;
  };

  const resolveWorktreeName = (session: CloudSession) => {
    if (session.worktree_name) return session.worktree_name;
    if (session.working_dir?.includes("-worktrees/")) {
      const parts = session.working_dir.split("-worktrees/");
      if (parts.length > 1) return parts[1].replace(/^\/+|\/+$/g, "");
    }
    if (session.working_dir) {
      const segments = session.working_dir.split("/").filter(Boolean);
      return segments.pop() || "";
    }
    return "";
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {sessions.map((session) => {
          const isConnecting = connectingSession === session.session_name;
          const isKilling = killingSession === session.session_name;
          const startedAgo = formatDistanceToNow(parseDate(session.created_at), {
            addSuffix: true,
            locale: dateLocale,
          });
          const worktreeName = resolveWorktreeName(session);
          const isPathCopied = copiedSession === session.session_name;

          return (
            <div
              key={session.session_name}
              className="rounded-2xl border border-slate-200 bg-white p-4.5 shadow-sm transition-all hover:shadow-md hover:border-emerald-500/30 dark:border-slate-800 dark:bg-slate-900 flex flex-col justify-between"
            >
              <div>
                {/* 1. 卡片顶部：会话名称独占第一焦点，右侧紧凑展示已运行时长 */}
                <div className="flex items-start justify-between gap-3 mb-2.5">
                  <div className="flex items-start gap-2 min-w-0 flex-1">
                    <span className="flex h-2.5 w-2.5 rounded-full bg-emerald-500 ring-4 ring-emerald-500/20 shrink-0 mt-1" />
                    <div className="min-w-0 flex-1">
                      <h3
                        className="font-mono text-xs sm:text-sm font-bold text-slate-900 dark:text-white leading-snug break-all select-all hover:text-emerald-600 dark:hover:text-emerald-400 transition-colors"
                        title={session.session_name}
                      >
                        {session.session_name}
                      </h3>
                    </div>
                  </div>

                  <div className="flex items-center gap-1 text-[11px] text-slate-400 dark:text-slate-500 shrink-0">
                    <Clock className="h-3 w-3 shrink-0" />
                    <span>{startedAgo}</span>
                  </div>
                </div>

                {/* 2. 紧凑元数据胶囊栏：仓库、分支、Worktree 紧凑单行横排，不占多余高度 */}
                <div className="flex flex-wrap items-center gap-1.5 mb-3">
                  {/* 代码仓库 */}
                  <span
                    className="inline-flex items-center gap-1 rounded-md bg-slate-100 dark:bg-slate-800 px-2 py-0.5 text-[11px] font-medium text-slate-700 dark:text-slate-300 shrink-0"
                    title={session.repo_name || "仓库主干"}
                  >
                    <FolderGit2 className="h-3 w-3 text-slate-400" />
                    <span>{session.repo_name || "仓库主干"}</span>
                  </span>

                  {/* 特性分支 */}
                  {session.branch && (
                    <span
                      className="inline-flex items-center gap-1 rounded-md bg-emerald-500/10 dark:bg-emerald-500/20 px-2 py-0.5 text-[11px] font-mono font-semibold text-emerald-700 dark:text-emerald-300 border border-emerald-500/20 max-w-[200px] truncate"
                      title={session.branch}
                    >
                      <GitBranch className="h-3 w-3 text-emerald-600 dark:text-emerald-400 shrink-0" />
                      <span className="truncate">{session.branch}</span>
                    </span>
                  )}

                  {/* Worktree 名称 */}
                  {worktreeName && (
                    <span
                      className="inline-flex items-center gap-1 rounded-md bg-sky-500/10 dark:bg-sky-500/20 px-2 py-0.5 text-[11px] font-mono font-medium text-sky-700 dark:text-sky-300 border border-sky-500/20 max-w-[200px] truncate"
                      title={`Worktree: ${worktreeName}`}
                    >
                      <Layers className="h-3 w-3 text-sky-600 dark:text-sky-400 shrink-0" />
                      <span className="truncate">{worktreeName}</span>
                    </span>
                  )}

                  {/* Tmux PID */}
                  {session.pane_pid > 0 && (
                    <span className="inline-flex items-center rounded-md bg-slate-50 dark:bg-slate-800/50 px-1.5 py-0.5 text-[10px] font-mono text-slate-400">
                      pid: {session.pane_pid}
                    </span>
                  )}
                </div>

                {/* 3. 作业目录：轻量单行，带前端路径省略与一键复制 */}
                <div className="flex items-center justify-between gap-2 text-xs text-slate-500 dark:text-slate-400 mb-4 rounded-xl bg-slate-50/70 dark:bg-slate-800/40 px-2.5 py-1.5 border border-slate-100 dark:border-slate-800/80">
                  <div className="flex items-center gap-1.5 min-w-0 flex-1">
                    <Folder className="h-3.5 w-3.5 shrink-0 text-slate-400" />
                    <span
                      className="font-mono text-[11px] truncate flex-1 min-w-0"
                      style={{ direction: "rtl", textAlign: "left", unicodeBidi: "plaintext" }}
                      title={session.working_dir}
                    >
                      {session.working_dir}
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleCopyPath(session.working_dir, session.session_name)}
                    title={t("copyPathTooltip", "复制完整目录路径")}
                    className="p-1 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded-md transition-colors shrink-0"
                  >
                    {isPathCopied ? (
                      <Check className="h-3 w-3 text-emerald-500" />
                    ) : (
                      <Copy className="h-3 w-3" />
                    )}
                  </button>
                </div>
              </div>

              {/* 4. 底部操作栏 */}
              <div className="pt-3 border-t border-slate-100 dark:border-slate-800/80 flex items-center justify-between gap-2">
                <button
                  type="button"
                  disabled={isConnecting || isKilling}
                  onClick={() => handleConnect(session.session_name)}
                  className="flex-1 inline-flex items-center justify-center gap-1.5 rounded-xl bg-emerald-600 px-3.5 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all disabled:opacity-60"
                >
                  {isConnecting ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Play className="h-3.5 w-3.5 fill-current" />
                  )}
                  {t("resumeSessionTerminal", "恢复会话 (直连终端)")}
                </button>

                <button
                  type="button"
                  disabled={isConnecting || isKilling}
                  onClick={() => handleKill(session.session_name)}
                  title={t("killSessionTooltip", "终止此会话")}
                  className="flex h-8 w-8 items-center justify-center rounded-xl text-slate-400 hover:text-rose-600 hover:bg-rose-50 dark:hover:bg-rose-950/40 transition-all disabled:opacity-60 shrink-0"
                >
                  {isKilling ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin text-rose-500" />
                  ) : (
                    <Power className="h-4 w-4" />
                  )}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
