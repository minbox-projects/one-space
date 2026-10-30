import { useState, useMemo } from "react";
import {
  Terminal,
  Clock,
  Power,
  Play,
  Loader2,
  GitBranch,
  Folder,
  Copy,
  Check,
  Search,
  Filter,
  X,
  Plus,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import { formatWorkingDirTail, type CloudSession } from "@/lib/cloudAgent";
import { CloudEmptyState } from "./CloudEmptyState";

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
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedRepo, setSelectedRepo] = useState<string>("all");

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

  // 提取可用仓库筛选列表
  const repoOptions = useMemo(() => {
    const set = new Set<string>();
    sessions.forEach((s) => {
      if (s.repo_name) set.add(s.repo_name);
    });
    return Array.from(set).sort();
  }, [sessions]);

  // 状态与关键词复合过滤
  const filteredSessions = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return sessions.filter((session) => {
      if (statusFilter === "active" && session.is_active === false) return false;
      if (statusFilter === "idle" && session.is_active !== false) return false;
      if (selectedRepo !== "all" && session.repo_name !== selectedRepo) return false;

      if (q) {
        const matchesName = session.session_name.toLowerCase().includes(q);
        const matchesRepo = (session.repo_name || "").toLowerCase().includes(q);
        const matchesBranch = (session.branch || "").toLowerCase().includes(q);
        const matchesPath = (session.working_dir || "").toLowerCase().includes(q);
        const matchesOpenCode = (session.opencode_session_id || "").toLowerCase().includes(q);
        return matchesName || matchesRepo || matchesBranch || matchesPath || matchesOpenCode;
      }
      return true;
    });
  }, [sessions, statusFilter, selectedRepo, searchQuery]);

  if (loading && sessions.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-emerald-500" />
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <CloudEmptyState
        icon={Terminal}
        title={t("noActiveSessionsTitle", "当前没有正在运行的云端开发会话")}
        description={t(
          "noActiveSessionsDesc",
          "在云端创建 Git Worktree 特性隔离任务，或恢复离线的历史工作区，即可瞬间调起本地终端开展长程编程。"
        )}
        action={{
          label: t("createNewFeatureTask", "新任务"),
          icon: Plus,
          onClick: onOpenNewTaskModal,
        }}
      />
    );
  }

  return (
    <div className="space-y-4">
      {/* 统一操作过滤栏（对齐 AI 网关 UpstreamProviderList 风格） */}
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-card p-3 shadow-2xs">
        {/* 左侧：搜索框与状态筛选药丸组 */}
        <div className="flex flex-wrap items-center gap-2.5 flex-1 min-w-[280px]">
          {/* 搜索输入框 */}
          <div className="relative min-w-[200px] max-w-sm flex-1">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("searchSessionsPlaceholder", "搜索会话、分支、仓库或 OpenCode ID...")}
              className="w-full rounded-lg border border-input bg-background pl-8 pr-7 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>

          {/* 状态药丸过滤组 */}
          <div className="flex items-center gap-1 rounded-lg border border-border/70 bg-muted/30 p-0.5">
            <button
              type="button"
              onClick={() => setStatusFilter("all")}
              className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all cursor-pointer ${
                statusFilter === "all"
                  ? "bg-background text-foreground font-semibold shadow-2xs"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <span>{t("allSessionsFilter", "全部会话")}</span>
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("active")}
              className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all cursor-pointer ${
                statusFilter === "active"
                  ? "bg-background text-foreground font-semibold shadow-2xs"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <span>{t("activeSessionsFilter", "活跃中")}</span>
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("idle")}
              className={`rounded-md px-2.5 py-1 text-xs font-medium transition-all cursor-pointer ${
                statusFilter === "idle"
                  ? "bg-background text-foreground font-semibold shadow-2xs"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <span>{t("idleSessionsFilter", "已离线")}</span>
            </button>
          </div>
        </div>

        {/* 右侧：代码仓库筛选下拉 */}
        {repoOptions.length > 1 && (
          <div className="flex items-center gap-2">
            <Filter className="h-3.5 w-3.5 text-muted-foreground" />
            <select
              value={selectedRepo}
              onChange={(e) => setSelectedRepo(e.target.value)}
              className="rounded-lg border border-input bg-background px-2.5 py-1.5 text-xs text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <option value="all">{t("allRepositories", "所有代码仓库")}</option>
              {repoOptions.map((repo) => (
                <option key={repo} value={repo}>
                  {repo}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>

      {/* 无过滤匹配状态 */}
      {filteredSessions.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-card/50 p-10 text-center">
          <p className="text-sm font-medium text-foreground">
            {t("noMatchingSessions", "未找到符合条件的云端会话")}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("tryAdjustingFilter", "请尝试调整关键词或筛选状态")}
          </p>
          <button
            type="button"
            onClick={() => {
              setSearchQuery("");
              setStatusFilter("all");
              setSelectedRepo("all");
            }}
            className="mt-3 rounded-lg border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-muted"
          >
            {t("resetFilters", "重置筛选条件")}
          </button>
        </div>
      ) : (
        /* 会话卡片列表 */
        <div className="grid grid-cols-1 gap-3">
          {filteredSessions.map((session) => {
            const isConnecting = connectingSession === session.session_name;
            const isKilling = killingSession === session.session_name;
            const timeAgo = formatDistanceToNow(parseDate(session.created_at), {
              addSuffix: true,
              locale: dateLocale,
            });
            const isActive = session.is_active !== false;
            const formattedDir = formatWorkingDirTail(session.working_dir || "");

            return (
              <div
                key={session.session_name}
                className="group relative rounded-xl border border-border bg-card p-4 shadow-2xs transition-all hover:border-border/80 hover:shadow-xs"
              >
                <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  {/* 左侧：会话身份与元数据 */}
                  <div className="min-w-0 flex-1 space-y-1.5">
                    {/* 会话名称与状态徽标 */}
                    <div className="flex flex-wrap items-center gap-2">
                      <div className="flex items-center gap-1.5">
                        <span className="relative flex h-2 w-2">
                          {isActive ? (
                            <>
                              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                              <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
                            </>
                          ) : (
                            <span className="relative inline-flex h-2 w-2 rounded-full bg-muted-foreground/40" />
                          )}
                        </span>
                        <h4 className="font-mono text-sm font-semibold tracking-tight text-foreground truncate">
                          {session.session_name}
                        </h4>
                      </div>

                      {/* 仓库名称徽标 */}
                      {session.repo_name && (
                        <span className="inline-flex items-center gap-1 rounded-md border border-border/80 bg-muted/40 px-2 py-0.5 text-[11px] font-medium text-foreground">
                          <Folder className="h-3 w-3 text-muted-foreground" />
                          {session.repo_name}
                        </span>
                      )}

                      {/* 分支标识徽标 */}
                      {session.branch && (
                        <span className="inline-flex items-center gap-1 rounded-md border border-primary/20 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                          <GitBranch className="h-3 w-3" />
                          {session.branch}
                        </span>
                      )}

                      {/* OpenCode 会话标识 */}
                      {session.opencode_session_id && (
                        <span className="font-mono text-[11px] text-muted-foreground">
                          {session.opencode_session_id}
                        </span>
                      )}
                    </div>

                    {/* 下部属性行：工作目录 + 运行时间 + 窗口数 */}
                    <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                      {/* 工作目录 */}
                      {session.working_dir && (
                        <div className="flex items-center gap-1 font-mono text-[11px]">
                          <span
                            title={session.working_dir}
                            className="text-foreground/80 hover:text-foreground select-all"
                          >
                            {formattedDir.display}
                          </span>
                          <button
                            type="button"
                            onClick={() =>
                              handleCopy(
                                session.working_dir,
                                `dir-${session.session_name}`
                              )
                            }
                            title={t("copyPathTooltip", "复制完整目录路径")}
                            className="inline-flex h-4 w-4 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground cursor-pointer"
                          >
                            {copiedTarget === `dir-${session.session_name}` ? (
                              <Check className="h-3 w-3 text-emerald-500" />
                            ) : (
                              <Copy className="h-3 w-3" />
                            )}
                          </button>
                        </div>
                      )}

                      <span className="text-muted-foreground/30">·</span>

                      {/* 创建与运行时长 */}
                      <div className="flex items-center gap-1 text-[11px]">
                        <Clock className="h-3 w-3 text-muted-foreground/70" />
                        <span>{timeAgo}</span>
                      </div>

                      {/* 终端窗口与进程 */}
                      {session.windows_count !== undefined && (
                        <>
                          <span className="text-muted-foreground/30">·</span>
                          <span className="text-[11px]">
                            {session.windows_count} {t("windowsUnit", "个窗口")}
                          </span>
                        </>
                      )}

                      {session.pane_pid ? (
                        <>
                          <span className="text-muted-foreground/30">·</span>
                          <span className="font-mono text-[11px]">
                            PID: {session.pane_pid}
                          </span>
                        </>
                      ) : null}
                    </div>
                  </div>

                  {/* 右侧：会话控制动作 */}
                  <div className="flex items-center gap-2 shrink-0 pt-2 sm:pt-0">
                    <button
                      type="button"
                      disabled={isConnecting || isKilling}
                      onClick={() => handleConnect(session.session_name)}
                      className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-2xs hover:bg-primary/90 active:scale-98 transition-all disabled:opacity-50 cursor-pointer"
                    >
                      {isConnecting ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Play className="h-3.5 w-3.5 fill-current" />
                      )}
                      <span>{t("resumeSessionTerminal", "恢复会话 (直连终端)")}</span>
                    </button>

                    <button
                      type="button"
                      disabled={isConnecting || isKilling}
                      onClick={() => handleKill(session.session_name)}
                      title={t("killSessionTooltip", "终止此会话")}
                      aria-label={t("killSessionTooltip", "终止此会话")}
                      className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-border/80 bg-background text-muted-foreground hover:border-destructive/30 hover:bg-destructive/10 hover:text-destructive transition-all disabled:opacity-50 cursor-pointer shadow-2xs"
                    >
                      {isKilling ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin text-destructive" />
                      ) : (
                        <Power className="h-3.5 w-3.5" />
                      )}
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
