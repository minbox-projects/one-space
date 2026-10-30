import { useState, useMemo } from "react";
import {
  GitBranch,
  GitCommit,
  Folder,
  Play,
  Clock,
  Loader2,
  Plus,
  Search,
  Copy,
  Check,
  Filter,
  X,
  User,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import type { CloudWorktree } from "@/lib/cloudAgent";
import { CloudEmptyState } from "./CloudEmptyState";

interface WorktreesHistoryTabProps {
  t: TFunction;
  language: string;
  worktrees: CloudWorktree[];
  loading: boolean;
  onLaunchTerminal: (target: string, branch?: string) => Promise<void>;
  onOpenNewTaskModal: () => void;
}

export function WorktreesHistoryTab({
  t,
  language,
  worktrees,
  loading,
  onLaunchTerminal,
  onOpenNewTaskModal,
}: WorktreesHistoryTabProps) {
  const [resumingTarget, setResumingTarget] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "active" | "idle">("all");
  const [selectedRepo, setSelectedRepo] = useState<string>("all");
  const [copiedTarget, setCopiedTarget] = useState<string | null>(null);

  const dateLocale = language === "zh" ? zhCN : enUS;

  // 提取仓库选项
  const repoOptions = useMemo(() => {
    const set = new Set<string>();
    worktrees.forEach((w) => {
      if (w.repo_name) set.add(w.repo_name);
    });
    return Array.from(set).sort();
  }, [worktrees]);

  // 统计数值
  const stats = useMemo(() => {
    const total = worktrees.length;
    const active = worktrees.filter((w) => w.has_active_session).length;
    const idle = total - active;
    return { total, active, idle };
  }, [worktrees]);

  // 提取 worktree 专属名称
  const extractWorktreeName = (wt: CloudWorktree) => {
    if (wt.branch) return wt.branch;
    if (wt.worktree_path?.includes("-worktrees/")) {
      const parts = wt.worktree_path.split("-worktrees/");
      if (parts.length > 1) return parts[1].replace(/^\/+|\/+$/g, "");
    }
    if (wt.worktree_path) {
      const segs = wt.worktree_path.split("/").filter(Boolean);
      return segs.pop() || "";
    }
    return wt.repo_name;
  };

  // 复制工具函数
  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedTarget(id);
    setTimeout(() => {
      setCopiedTarget((curr) => (curr === id ? null : curr));
    }, 2000);
  };

  // 唤起终端连入现场
  const handleResume = async (wt: CloudWorktree) => {
    try {
      setResumingTarget(wt.worktree_path);
      if (wt.has_active_session) {
        await onLaunchTerminal(wt.session_name);
      } else {
        await onLaunchTerminal(wt.repo_name, wt.branch);
      }
    } finally {
      setResumingTarget(null);
    }
  };

  const parseDate = (val: number | string | undefined) => {
    if (!val) return new Date();
    if (typeof val === "number") {
      return new Date(val > 1e11 ? val : val * 1000);
    }
    const d = new Date(val);
    return isNaN(d.getTime()) ? new Date() : d;
  };

  // 复合过滤工作区
  const filteredWorktrees = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return worktrees.filter((wt) => {
      if (statusFilter === "active" && !wt.has_active_session) return false;
      if (statusFilter === "idle" && wt.has_active_session) return false;
      if (selectedRepo !== "all" && wt.repo_name !== selectedRepo) return false;

      if (q) {
        const matchesRepo = (wt.repo_name || "").toLowerCase().includes(q);
        const matchesBranch = (wt.branch || "").toLowerCase().includes(q);
        const matchesPath = (wt.worktree_path || "").toLowerCase().includes(q);
        const matchesCommit = (wt.last_commit?.message || "").toLowerCase().includes(q);
        const matchesAuthor = (wt.last_commit?.author || "").toLowerCase().includes(q);
        const matchesHash = (wt.last_commit?.hash || "").toLowerCase().includes(q);
        return (
          matchesRepo ||
          matchesBranch ||
          matchesPath ||
          matchesCommit ||
          matchesAuthor ||
          matchesHash
        );
      }
      return true;
    });
  }, [worktrees, statusFilter, selectedRepo, searchQuery]);

  if (loading && worktrees.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-emerald-500" />
      </div>
    );
  }

  if (worktrees.length === 0) {
    return (
      <CloudEmptyState
        icon={GitBranch}
        title={t("noWorktreesTitle", "暂无 Git Worktree 历史工作区")}
        description={t(
          "noWorktreesDesc",
          "在云端创建的所有特性分支隔离环境将长期保存在此，即使会话退出也可随时一键重新拉起现场。"
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
      {/* 顶部操作过滤与统计工具栏 */}
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-card p-3 shadow-2xs">
        {/* 左侧：搜索框与状态筛选药丸组 */}
        <div className="flex flex-wrap items-center gap-2.5 flex-1 min-w-[280px]">
          {/* 搜索框 */}
          <div className="relative min-w-[200px] max-w-sm flex-1">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("searchWorktreesPlaceholder", "搜索分支 / 特性 / Commit...")}
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

          {/* 状态统计与筛选药丸组 */}
          <div className="flex items-center gap-1 rounded-lg border border-border/70 bg-muted/30 p-0.5 text-xs">
            <button
              type="button"
              onClick={() => setStatusFilter("all")}
              className={`rounded-md px-2.5 py-1 font-medium transition-all cursor-pointer ${
                statusFilter === "all"
                  ? "bg-background text-foreground font-semibold shadow-2xs"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <span>{t("allWorktreesCount", "全部工作区")}</span>
              <span className="ml-1.5 font-semibold">{stats.total}</span>
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("active")}
              className={`rounded-md px-2.5 py-1 font-medium transition-all cursor-pointer ${
                statusFilter === "active"
                  ? "bg-background text-foreground font-semibold shadow-2xs"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <span>{t("activeSessionsCount", "活跃运行")}</span>
              <span className="ml-1.5 font-semibold text-emerald-600 dark:text-emerald-400">{stats.active}</span>
            </button>
            <button
              type="button"
              onClick={() => setStatusFilter("idle")}
              className={`rounded-md px-2.5 py-1 font-medium transition-all cursor-pointer ${
                statusFilter === "idle"
                  ? "bg-background text-foreground font-semibold shadow-2xs"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <span>{t("idleWorktreesCount", "已离线")}</span>
              <span className="ml-1.5 font-semibold text-muted-foreground">{stats.idle}</span>
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

      {/* 无匹配状态 */}
      {filteredWorktrees.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-card/50 p-10 text-center">
          <p className="text-sm font-medium text-foreground">
            {t("noMatchingWorktrees", "未找到符合条件的工作区")}
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
        /* 工作区卡片列表 */
        <div className="grid grid-cols-1 gap-3">
          {filteredWorktrees.map((wt) => {
            const isResuming = resumingTarget === wt.worktree_path;
            const wtName = extractWorktreeName(wt);
            const modifiedAgo = formatDistanceToNow(parseDate(wt.last_modified), {
              addSuffix: true,
              locale: dateLocale,
            });

            return (
              <div
                key={wt.worktree_path}
                className="group relative rounded-xl border border-border bg-card p-4 shadow-2xs transition-all hover:border-border/80 hover:shadow-xs"
              >
                <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                  {/* 左侧：工作区身份与最新提交信息 */}
                  <div className="min-w-0 flex-1 space-y-2">
                    {/* 头部：状态灯 + 分支/Worktree名称 + 仓库徽标 */}
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="relative flex h-2 w-2">
                        {wt.has_active_session ? (
                          <>
                            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                            <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
                          </>
                        ) : (
                          <span className="relative inline-flex h-2 w-2 rounded-full bg-muted-foreground/40" />
                        )}
                      </span>

                      <h4 className="font-mono text-sm font-semibold tracking-tight text-foreground truncate">
                        {wtName}
                      </h4>

                      {/* 仓库名称徽标 */}
                      {wt.repo_name && (
                        <span className="inline-flex items-center gap-1 rounded-md border border-border/80 bg-muted/40 px-2 py-0.5 text-[11px] font-medium text-foreground">
                          <Folder className="h-3 w-3 text-muted-foreground" />
                          {wt.repo_name}
                        </span>
                      )}

                      {/* 分支标识 */}
                      {wt.branch && (
                        <span className="inline-flex items-center gap-1 rounded-md border border-primary/20 bg-primary/10 px-2 py-0.5 text-[11px] font-mono text-primary font-medium">
                          <GitBranch className="h-3 w-3" />
                          {wt.branch}
                        </span>
                      )}

                      {/* 状态徽标 */}
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${
                          wt.has_active_session
                            ? "bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400"
                            : "bg-muted text-muted-foreground"
                        }`}
                      >
                        {wt.has_active_session
                          ? t("statusRunning", "活跃运行中")
                          : t("statusIdle", "已离线 / 空闲")}
                      </span>
                    </div>

                    {/* 中部：最新 Git 提交信息卡（若有） */}
                    {wt.last_commit ? (
                      <div className="flex flex-wrap items-center gap-2 rounded-lg border border-border/60 bg-muted/30 px-2.5 py-1.5 text-xs text-foreground/90">
                        <GitCommit className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                        {wt.last_commit.hash && (
                          <div className="flex items-center gap-1 font-mono text-[11px] text-muted-foreground">
                            <span>{wt.last_commit.hash.slice(0, 7)}</span>
                            <button
                              type="button"
                              onClick={() =>
                                handleCopy(
                                  wt.last_commit!.hash,
                                  `hash-${wt.worktree_path}`
                                )
                              }
                              title={t("copyHashTooltip", "复制 Commit Hash")}
                              className="inline-flex h-3.5 w-3.5 items-center justify-center rounded text-muted-foreground hover:text-foreground cursor-pointer"
                            >
                              {copiedTarget === `hash-${wt.worktree_path}` ? (
                                <Check className="h-3 w-3 text-emerald-500" />
                              ) : (
                                <Copy className="h-3 w-3" />
                              )}
                            </button>
                          </div>
                        )}
                        <span className="text-muted-foreground/40">·</span>
                        <span className="font-medium truncate max-w-md">
                          {wt.last_commit.message}
                        </span>
                        {wt.last_commit.author && (
                          <>
                            <span className="text-muted-foreground/40">·</span>
                            <span className="flex items-center gap-1 text-[11px] text-muted-foreground">
                              <User className="h-3 w-3" />
                              {wt.last_commit.author}
                            </span>
                          </>
                        )}
                      </div>
                    ) : (
                      <div className="text-[11px] text-muted-foreground italic">
                        {t("noCommitHistory", "暂无最新 Git 提交记录")}
                      </div>
                    )}

                    {/* 底部属性：路径 + 最后修改时间 */}
                    <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                      <div className="flex items-center gap-1 font-mono text-[11px]">
                        <span
                          title={wt.worktree_path}
                          className="text-foreground/80 hover:text-foreground select-all truncate max-w-sm"
                        >
                          {wt.worktree_path}
                        </span>
                        <button
                          type="button"
                          onClick={() =>
                            handleCopy(wt.worktree_path, `path-${wt.worktree_path}`)
                          }
                          title={t("copyPathTooltip", "复制完整目录路径")}
                          className="inline-flex h-4 w-4 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground cursor-pointer"
                        >
                          {copiedTarget === `path-${wt.worktree_path}` ? (
                            <Check className="h-3 w-3 text-emerald-500" />
                          ) : (
                            <Copy className="h-3 w-3" />
                          )}
                        </button>
                      </div>

                      <span className="text-muted-foreground/30">·</span>

                      <div className="flex items-center gap-1 text-[11px]">
                        <Clock className="h-3 w-3 text-muted-foreground/70" />
                        <span>
                          {t("lastModifiedAt", "最后修改")}: {modifiedAgo}
                        </span>
                      </div>
                    </div>
                  </div>

                  {/* 右侧：进入现场 / 重新拉起动作 */}
                  <div className="flex items-center gap-2 shrink-0 pt-2 md:pt-0">
                    <button
                      type="button"
                      disabled={isResuming}
                      onClick={() => handleResume(wt)}
                      className={`inline-flex h-8 items-center gap-1.5 rounded-lg px-3 text-xs font-medium shadow-2xs transition-all disabled:opacity-50 cursor-pointer ${
                        wt.has_active_session
                          ? "bg-primary text-primary-foreground hover:bg-primary/90 active:scale-98"
                          : "border border-border/80 bg-background text-foreground hover:bg-muted active:scale-98"
                      }`}
                    >
                      {isResuming ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Play className="h-3.5 w-3.5 fill-current" />
                      )}
                      <span>
                        {wt.has_active_session
                          ? t("resumeLiveSession", "进入运行现场 (直连终端)")
                          : t("reviveWorktreeSession", "重新拉起现场 (直连终端)")}
                      </span>
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
