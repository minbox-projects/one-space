import { useState, useMemo } from "react";
import {
  Activity,
  TerminalSquare,
  Cpu,
  Layers,
  FolderGit2,
  Sparkles,
  Clock,
  Power,
  Play,
  Loader2,
  GitBranch,
  Folder,
  Copy,
  Check,
  Terminal,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import { formatWorkingDirTail, type CloudSession } from "@/lib/cloudAgent";

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
  const [copiedTarget, setCopiedTarget] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<"all" | "active" | "idle">("all");

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

  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedTarget(id);
    setTimeout(() => {
      setCopiedTarget((curr) => (curr === id ? null : curr));
    }, 2000);
  };

  const parseDate = (val: number | string | undefined) => {
    if (!val) return new Date();
    if (typeof val === "number") {
      return new Date(val > 1e11 ? val : val * 1000);
    }
    const d = new Date(val);
    return isNaN(d.getTime()) ? new Date() : d;
  };

  // 4 组核心统计指标计算（对齐 AI 网关风格）
  const stats = useMemo(() => {
    const totalSessions = sessions.length;
    const activeCount = sessions.filter((s) => s.is_active !== false).length;
    const idleCount = sessions.filter((s) => s.is_active === false).length;
    const totalWindows = sessions.reduce(
      (acc, s) => acc + (s.windows_count && s.windows_count > 0 ? s.windows_count : 1),
      0
    );
    const worktreeCount = sessions.filter((s) =>
      Boolean(s.worktree_name || s.working_dir?.includes("-worktrees/"))
    ).length;
    const repoSet = new Set<string>();
    sessions.forEach((s) => {
      if (s.repo_name) repoSet.add(s.repo_name);
    });
    const repoCount = repoSet.size;

    return {
      totalSessions,
      activeCount,
      idleCount,
      totalWindows,
      worktreeCount,
      repoCount,
    };
  }, [sessions]);

  // 状态过滤
  const filteredSessions = useMemo(() => {
    return sessions.filter((session) => {
      if (statusFilter === "active" && session.is_active === false) return false;
      if (statusFilter === "idle" && session.is_active !== false) return false;
      return true;
    });
  }, [sessions, statusFilter]);

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

  return (
    <div className="space-y-5">
      {/* 1. 顶部 4 栏核心指标统计卡片（参考 AI 网关 RuntimeStatusCard 风格） */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {/* 指标卡 1：活跃会话 */}
        <div className="flex flex-col justify-between rounded-xl border border-slate-200/80 bg-white p-4 shadow-2xs transition-colors dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400">
            <span className="text-[11px] font-medium">
              {t("activeSessionsMetricTitle", "活跃会话")}
            </span>
            <Activity className="h-4 w-4 text-emerald-500" />
          </div>
          <div className="mt-2.5 flex items-baseline gap-1.5">
            <span className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.activeCount}
            </span>
            <span className="text-xs text-slate-400">
              / {stats.totalSessions}
            </span>
          </div>
          <div className="mt-1.5 flex items-center gap-1.5 text-[11px] text-emerald-600 dark:text-emerald-400">
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
              <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
            </span>
            <span className="truncate">{t("activeSessionsMetricDesc", "守护进程保活运行中")}</span>
          </div>
        </div>

        {/* 指标卡 2：终端作业窗口 */}
        <div className="flex flex-col justify-between rounded-xl border border-slate-200/80 bg-white p-4 shadow-2xs transition-colors dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400">
            <span className="text-[11px] font-medium">
              {t("terminalWindowsMetricTitle", "终端作业窗口")}
            </span>
            <TerminalSquare className="h-4 w-4 text-sky-500" />
          </div>
          <div className="mt-2.5 flex items-baseline gap-1.5">
            <span className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.totalWindows}
            </span>
            <span className="text-xs text-slate-400">
              {t("windowsUnit", "个窗口")}
            </span>
          </div>
          <div className="mt-1.5 flex items-center gap-1 text-[11px] text-slate-500 dark:text-slate-400 truncate">
            <Cpu className="h-3.5 w-3.5 shrink-0 text-slate-400" />
            <span className="truncate">{t("terminalWindowsMetricDesc", "窗口协同运行中")}</span>
          </div>
        </div>

        {/* 指标卡 3：特性隔离空间 */}
        <div className="flex flex-col justify-between rounded-xl border border-slate-200/80 bg-white p-4 shadow-2xs transition-colors dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400">
            <span className="text-[11px] font-medium">
              {t("isolatedWorktreesMetricTitle", "特性隔离空间")}
            </span>
            <Layers className="h-4 w-4 text-indigo-500" />
          </div>
          <div className="mt-2.5 flex items-baseline gap-1.5">
            <span className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.worktreeCount}
            </span>
            <span className="text-xs text-slate-400">
              {t("worktreesUnit", "个工作区")}
            </span>
          </div>
          <div className="mt-1.5 flex items-center gap-1 text-[11px] text-slate-500 dark:text-slate-400 truncate">
            <GitBranch className="h-3.5 w-3.5 shrink-0 text-slate-400" />
            <span className="truncate">{t("isolatedWorktreesMetricDesc", "Git 独立目录隔离")}</span>
          </div>
        </div>

        {/* 指标卡 4：涉及代码仓库 */}
        <div className="flex flex-col justify-between rounded-xl border border-slate-200/80 bg-white p-4 shadow-2xs transition-colors dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400">
            <span className="text-[11px] font-medium">
              {t("reposMetricTitle", "涉及代码仓库")}
            </span>
            <FolderGit2 className="h-4 w-4 text-amber-500" />
          </div>
          <div className="mt-2.5 flex items-baseline gap-1.5">
            <span className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              {stats.repoCount}
            </span>
            <span className="text-xs text-slate-400">
              {t("reposUnit", "个项目")}
            </span>
          </div>
          <div className="mt-1.5 flex items-center gap-1 text-[11px] text-slate-500 dark:text-slate-400 truncate">
            <Sparkles className="h-3.5 w-3.5 shrink-0 text-slate-400" />
            <span className="truncate">{t("reposMetricDesc", "多工程并行推进")}</span>
          </div>
        </div>
      </div>

      {/* 2. 状态快捷过滤药丸组（对齐 AI 网关 UpstreamProviderList 风格，无冗余输入框） */}
      <div className="flex items-center justify-between pt-1 pb-0.5">
        <div
          role="group"
          aria-label={t("filterSessionsAria", "按状态筛选会话")}
          className="inline-flex items-center rounded-lg border border-slate-200/60 dark:border-slate-800 bg-slate-100/60 dark:bg-slate-800/60 p-0.5 text-xs shadow-inner"
        >
          <button
            type="button"
            aria-pressed={statusFilter === "all"}
            onClick={() => setStatusFilter("all")}
            className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-all ${
              statusFilter === "all"
                ? "bg-white text-slate-900 shadow-sm border border-slate-200/50 dark:bg-slate-900 dark:text-white dark:border-slate-700 font-semibold"
                : "text-slate-500 hover:text-slate-800 hover:bg-slate-200/50 dark:text-slate-400 dark:hover:text-slate-200"
            }`}
          >
            <span>{t("allSessionsFilter", "全部会话")}</span>
            <span
              className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                statusFilter === "all"
                  ? "bg-slate-100 text-slate-800 dark:bg-slate-800 dark:text-slate-200 font-semibold"
                  : "bg-slate-200/60 text-slate-500 dark:bg-slate-800/60 dark:text-slate-400"
              }`}
            >
              {stats.totalSessions}
            </span>
          </button>

          <button
            type="button"
            aria-pressed={statusFilter === "active"}
            onClick={() => setStatusFilter("active")}
            className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-all ${
              statusFilter === "active"
                ? "bg-white text-slate-900 shadow-sm border border-slate-200/50 dark:bg-slate-900 dark:text-white dark:border-slate-700 font-semibold"
                : "text-slate-500 hover:text-slate-800 hover:bg-slate-200/50 dark:text-slate-400 dark:hover:text-slate-200"
            }`}
          >
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
            <span>{t("activeSessionsFilter", "活跃中")}</span>
            <span
              className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                statusFilter === "active"
                  ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 font-semibold"
                  : "bg-slate-200/60 text-slate-500 dark:bg-slate-800/60 dark:text-slate-400"
              }`}
            >
              {stats.activeCount}
            </span>
          </button>

          <button
            type="button"
            aria-pressed={statusFilter === "idle"}
            onClick={() => setStatusFilter("idle")}
            className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-all ${
              statusFilter === "idle"
                ? "bg-white text-slate-900 shadow-sm border border-slate-200/50 dark:bg-slate-900 dark:text-white dark:border-slate-700 font-semibold"
                : "text-slate-500 hover:text-slate-800 hover:bg-slate-200/50 dark:text-slate-400 dark:hover:text-slate-200"
            }`}
          >
            <span className="h-1.5 w-1.5 rounded-full bg-slate-400" />
            <span>{t("idleSessionsFilter", "已离线")}</span>
            <span
              className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                statusFilter === "idle"
                  ? "bg-slate-500/15 text-slate-700 dark:text-slate-300 font-semibold"
                  : "bg-slate-200/60 text-slate-500 dark:bg-slate-800/60 dark:text-slate-400"
              }`}
            >
              {stats.idleCount}
            </span>
          </button>
        </div>
      </div>

      {/* 3. 三段式大留白会话卡片网格系统 */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
        {filteredSessions.map((session) => {
          const isConnecting = connectingSession === session.session_name;
          const isKilling = killingSession === session.session_name;
          const startedAgo = formatDistanceToNow(parseDate(session.created_at), {
            addSuffix: true,
            locale: dateLocale,
          });
          const isPathCopied = copiedTarget === `path:${session.session_name}`;
          const isNameCopied = copiedTarget === `name:${session.session_name}`;
          const isOpencodeIdCopied = copiedTarget === `opencode:${session.session_name}`;
          const isAttachCopied = copiedTarget === `attach:${session.session_name}`;
          const dirTail = formatWorkingDirTail(session.working_dir, 38);

          return (
            <div
              key={session.session_name}
              className="group flex flex-col justify-between overflow-hidden rounded-2xl border border-slate-200/80 bg-white shadow-xs transition-all duration-200 hover:border-emerald-500/40 hover:shadow-md dark:border-slate-800 dark:bg-slate-900"
            >
              {/* 段落 1：卡片顶栏（移除“活跃运行中”文字，仅保留呼吸指示灯图标） */}
              <div className="flex items-center justify-between gap-3 border-b border-slate-100 bg-slate-50/60 px-5 py-3.5 dark:border-slate-800/80 dark:bg-slate-800/40">
                <div className="flex items-center gap-2.5 min-w-0 flex-1">
                  {/* 仅保留呼吸指示灯图标 */}
                  <span
                    className="relative flex h-2.5 w-2.5 shrink-0"
                    title={t("statusRunning", "活跃运行中")}
                  >
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                    <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-emerald-500" />
                  </span>

                  {/* 仓库名称徽标 */}
                  <span
                    className="inline-flex items-center gap-1.5 text-xs font-bold text-slate-800 dark:text-slate-200 truncate"
                    title={session.repo_name || "仓库主干"}
                  >
                    <FolderGit2 className="h-3.5 w-3.5 text-slate-400 shrink-0" />
                    <span className="truncate">{session.repo_name || "仓库主干"}</span>
                  </span>
                </div>

                <div className="flex items-center gap-1.5 text-slate-400 dark:text-slate-500 shrink-0">
                  <div className="flex items-center gap-1 text-[11px]">
                    <Clock className="h-3 w-3 shrink-0" />
                    <span>{startedAgo}</span>
                  </div>

                  <button
                    type="button"
                    disabled={isConnecting || isKilling}
                    onClick={() => handleKill(session.session_name)}
                    title={t("killSessionTooltip", "终止此会话")}
                    className="flex h-7 w-7 items-center justify-center rounded-lg text-slate-400 hover:text-rose-600 hover:bg-rose-50 dark:hover:bg-rose-950/40 transition-all disabled:opacity-60"
                  >
                    {isKilling ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin text-rose-500" />
                    ) : (
                      <Power className="h-3.5 w-3.5" />
                    )}
                  </button>
                </div>
              </div>

              {/* 段落 2：宽裕主体区（大留白、特性分支、OpenCode 会话 ID、完整会话名） */}
              <div className="flex-1 p-5 sm:p-5.5 space-y-3.5">
                {/* 1. 特性分支（常规字体，移除 Worktree） */}
                <div className="flex items-center gap-2 min-w-0">
                  <GitBranch className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />
                  <span
                    className="font-mono text-xs sm:text-sm text-slate-700 dark:text-slate-300 truncate select-all"
                    title={session.branch || session.repo_name}
                  >
                    {session.branch || session.repo_name}
                  </span>
                </div>

                {/* 2. OpenCode 会话 ID 行 */}
                <div className="flex items-center justify-between gap-2 text-xs">
                  <div className="flex items-center gap-1.5 min-w-0 flex-1 text-slate-500 dark:text-slate-400">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400 shrink-0">
                      OpenCode ID:
                    </span>
                    <span
                      className="font-mono text-[11px] font-medium text-slate-700 dark:text-slate-300 break-all select-all min-w-0"
                      title={session.opencode_session_id || "—"}
                    >
                      {session.opencode_session_id || "—"}
                    </span>
                    {session.opencode_session_id && (
                      <button
                        type="button"
                        onClick={() =>
                          handleCopy(
                            session.opencode_session_id!,
                            `opencode:${session.session_name}`
                          )
                        }
                        title={
                          isOpencodeIdCopied
                            ? t("copiedSessionIdTooltip", "已复制 OpenCode 会话 ID")
                            : t("copySessionIdTooltip", "复制 OpenCode 会话 ID")
                        }
                        className="p-1 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded-md transition-colors shrink-0"
                      >
                        {isOpencodeIdCopied ? (
                          <Check className="h-3 w-3 text-emerald-500" />
                        ) : (
                          <Copy className="h-3 w-3" />
                        )}
                      </button>
                    )}
                  </div>
                </div>

                {/* 3. 会话名称（完整显示全，不截断） + PID + 窗口数 */}
                <div className="flex items-start justify-between gap-2 text-xs">
                  <div className="flex items-start gap-1.5 min-w-0 flex-1 text-slate-600 dark:text-slate-300">
                    <Terminal className="h-3.5 w-3.5 text-slate-400 shrink-0 mt-0.5" />
                    <div className="min-w-0 flex-1">
                      <span
                        className="font-mono text-[11px] font-medium break-all select-all text-slate-800 dark:text-slate-200 leading-snug"
                        title={session.session_name}
                      >
                        {session.session_name}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleCopy(session.session_name, `name:${session.session_name}`)}
                      title={
                        isNameCopied
                          ? t("copiedSessionNameTooltip", "已复制会话名")
                          : t("copySessionNameTooltip", "复制会话名称")
                      }
                      className="p-1 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded-md transition-colors shrink-0 mt-[-2px]"
                    >
                      {isNameCopied ? (
                        <Check className="h-3 w-3 text-emerald-500" />
                      ) : (
                        <Copy className="h-3 w-3" />
                      )}
                    </button>
                  </div>

                  <div className="flex items-center gap-1.5 shrink-0 text-[10px] font-mono text-slate-500 dark:text-slate-400 mt-0.5">
                    {session.pane_pid > 0 && (
                      <span className="rounded bg-slate-100 dark:bg-slate-800 px-1.5 py-0.5 font-medium">
                        pid: {session.pane_pid}
                      </span>
                    )}
                    <span className="rounded bg-slate-100 dark:bg-slate-800 px-1.5 py-0.5 font-medium">
                      {t("windowsCountLabel", {
                        count: session.windows_count || 1,
                        defaultValue: `${session.windows_count || 1} 窗口`,
                      })}
                    </span>
                  </div>
                </div>

                {/* 4. 工作目录路径：前部省略，后部完整可见，一键复制 */}
                <div className="flex items-center justify-between gap-2 text-xs text-slate-500 dark:text-slate-400 pt-0.5">
                  <div className="flex items-center gap-1.5 min-w-0 flex-1">
                    <Folder className="h-3.5 w-3.5 shrink-0 text-slate-400" />
                    <span
                      className="font-mono text-[11px] truncate flex-1 min-w-0"
                      title={session.working_dir}
                    >
                      {dirTail.display}
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleCopy(session.working_dir, `path:${session.session_name}`)}
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

              {/* 段落 3：舒适底栏（标准高度按钮、充裕留白、恢复主按钮 + 附加次按钮） */}
              <div className="flex items-center justify-between gap-2.5 border-t border-slate-100 bg-slate-50/40 px-5 py-3.5 dark:border-slate-800/80 dark:bg-slate-800/20">
                <button
                  type="button"
                  disabled={isConnecting || isKilling}
                  onClick={() => handleConnect(session.session_name)}
                  className="flex-1 inline-flex h-9 items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-emerald-600 to-teal-600 px-4 text-xs font-semibold text-white shadow-sm hover:from-emerald-500 hover:to-teal-500 transition-all disabled:opacity-60"
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
                  onClick={() =>
                    handleCopy(`tmux attach -t ${session.session_name}`, `attach:${session.session_name}`)
                  }
                  title={
                    isAttachCopied
                      ? t("copiedAttachCommandTooltip", "已复制附加指令")
                      : t("copyAttachCommandTooltip", "复制 tmux attach 命令")
                  }
                  className="inline-flex h-9 w-9 items-center justify-center rounded-xl border border-slate-200/80 dark:border-slate-800 bg-white dark:bg-slate-800 text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200 transition-colors shrink-0 shadow-2xs"
                >
                  {isAttachCopied ? (
                    <Check className="h-3.5 w-3.5 text-emerald-500" />
                  ) : (
                    <TerminalSquare className="h-4 w-4" />
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
