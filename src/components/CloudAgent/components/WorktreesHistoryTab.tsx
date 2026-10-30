import { useState, useMemo } from "react";
import {
  GitBranch,
  GitCommit,
  Folder,
  Play,
  Clock,
  Loader2,
  Sparkles,
  Layers,
  Search,
  Copy,
  Check,
  FolderGit2,
  Terminal,
  Filter,
  X,
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

  // 过滤后的列表
  const filteredWorktrees = useMemo(() => {
    return worktrees.filter((wt) => {
      // 状态过滤
      if (statusFilter === "active" && !wt.has_active_session) return false;
      if (statusFilter === "idle" && wt.has_active_session) return false;

      // 仓库过滤
      if (selectedRepo !== "all" && wt.repo_name !== selectedRepo) return false;

      // 关键字搜索过滤
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase().trim();
        const wtName = extractWorktreeName(wt).toLowerCase();
        const repo = (wt.repo_name || "").toLowerCase();
        const branch = (wt.branch || "").toLowerCase();
        const path = (wt.worktree_path || "").toLowerCase();
        const sessionName = (wt.session_name || "").toLowerCase();
        const commitMsg = (wt.last_commit?.message || "").toLowerCase();
        const commitHash = (wt.last_commit?.hash || "").toLowerCase();

        return (
          wtName.includes(q) ||
          repo.includes(q) ||
          branch.includes(q) ||
          path.includes(q) ||
          sessionName.includes(q) ||
          commitMsg.includes(q) ||
          commitHash.includes(q)
        );
      }

      return true;
    });
  }, [worktrees, statusFilter, selectedRepo, searchQuery]);

  if (loading && worktrees.length === 0) {
    return (
      <div className="flex h-72 items-center justify-center">
        <Loader2 className="h-7 w-7 animate-spin text-emerald-500" />
      </div>
    );
  }

  if (worktrees.length === 0) {
    return (
      <CloudEmptyState
        icon={Layers}
        title={t("noWorktreesTitle", "暂无 Git Worktree 历史工作区")}
        description={t(
          "noWorktreesDesc",
          "在云端创建的所有特性分支隔离环境将长期保存在此，即使会话退出也可随时一键重新拉起现场。"
        )}
        action={{
          label: t("createNewFeatureTask", "新建云端特性任务"),
          icon: Sparkles,
          onClick: onOpenNewTaskModal,
        }}
      />
    );
  }

  return (
    <div className="space-y-4">
      {/* 顶部统计与多功能筛选控制栏 */}
      <div className="rounded-2xl border border-slate-200/80 bg-white/80 p-3.5 shadow-sm backdrop-blur-sm dark:border-slate-800/80 dark:bg-slate-900/80 flex flex-wrap items-center justify-between gap-3">
        {/* 左侧：统计指示胶囊 */}
        <div className="flex items-center gap-2">
          <span className="inline-flex items-center gap-1.5 rounded-xl bg-slate-100 dark:bg-slate-800 px-3 py-1 text-xs font-medium text-slate-700 dark:text-slate-300">
            <Layers className="h-3.5 w-3.5 text-slate-500 dark:text-slate-400" />
            <span>{t("allWorktreesCount", "全部工作区")}</span>
            <span className="rounded-md bg-white dark:bg-slate-700 px-1.5 py-0.2 text-[11px] font-bold text-slate-800 dark:text-slate-200">
              {stats.total}
            </span>
          </span>

          <button
            type="button"
            onClick={() => setStatusFilter(statusFilter === "active" ? "all" : "active")}
            className={`inline-flex items-center gap-1.5 rounded-xl px-3 py-1 text-xs font-medium transition-all ${
              statusFilter === "active"
                ? "bg-emerald-600 text-white shadow-sm"
                : "bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/20 dark:bg-emerald-500/20 dark:text-emerald-300"
            }`}
          >
            <span className="h-2 w-2 rounded-full bg-emerald-400 animate-pulse" />
            <span>{t("activeSessionsCount", "活跃运行")}</span>
            <span className="font-bold ml-0.5">{stats.active}</span>
          </button>

          <button
            type="button"
            onClick={() => setStatusFilter(statusFilter === "idle" ? "all" : "idle")}
            className={`inline-flex items-center gap-1.5 rounded-xl px-3 py-1 text-xs font-medium transition-all ${
              statusFilter === "idle"
                ? "bg-slate-700 text-white shadow-sm dark:bg-slate-300 dark:text-slate-900"
                : "bg-slate-100 text-slate-600 hover:bg-slate-200/80 dark:bg-slate-800 dark:text-slate-400"
            }`}
          >
            <span className="h-2 w-2 rounded-full bg-slate-400" />
            <span>{t("idleWorktreesCount", "已离线")}</span>
            <span className="font-bold ml-0.5">{stats.idle}</span>
          </button>
        </div>

        {/* 右侧：搜索过滤与新建入口 */}
        <div className="flex flex-1 sm:flex-initial items-center gap-2 min-w-[280px]">
          {/* 仓库下拉筛选（如果有多仓库） */}
          {repoOptions.length > 1 && (
            <div className="relative">
              <select
                value={selectedRepo}
                onChange={(e) => setSelectedRepo(e.target.value)}
                aria-label={t("filterByRepo", "按代码仓库筛选")}
                className="h-8 rounded-xl border border-slate-200 bg-slate-50/70 pl-2.5 pr-7 text-xs font-medium text-slate-700 focus:border-emerald-500 focus:bg-white focus:outline-none dark:border-slate-800 dark:bg-slate-800/70 dark:text-slate-300"
              >
                <option value="all">{t("allRepositories", "所有代码仓库")}</option>
                {repoOptions.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* 实时搜索框 */}
          <div className="relative flex-1 sm:w-48">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-slate-400" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("searchWorktreesPlaceholder", "搜索分支 / 特性 / Commit...")}
              className="h-8 w-full rounded-xl border border-slate-200 bg-slate-50/70 pl-8 pr-7 text-xs text-slate-800 placeholder:text-slate-400 focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-1 focus:ring-emerald-500/20 dark:border-slate-800 dark:bg-slate-800/70 dark:text-slate-200"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>

          {/* 新建特性任务主按钮 */}
          <button
            type="button"
            onClick={onOpenNewTaskModal}
            className="h-8 inline-flex items-center gap-1.5 rounded-xl bg-emerald-600 px-3 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all shrink-0"
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">{t("createNewTaskBtn", "新建特性任务")}</span>
          </button>
        </div>
      </div>

      {/* 搜索无结果提示 */}
      {filteredWorktrees.length === 0 && (
        <div className="rounded-2xl border border-slate-200 bg-white/50 dark:border-slate-800 dark:bg-slate-900/40 p-10 text-center">
          <Filter className="mx-auto h-8 w-8 text-slate-400 mb-2 opacity-60" />
          <h4 className="text-sm font-semibold text-slate-700 dark:text-slate-300">
            {t("noMatchingWorktrees", "未找到符合条件的工作区")}
          </h4>
          <p className="mt-1 text-xs text-slate-400">
            {t("tryAdjustingFilter", "请尝试调整关键词或筛选状态")}
          </p>
          <button
            type="button"
            onClick={() => {
              setSearchQuery("");
              setStatusFilter("all");
              setSelectedRepo("all");
            }}
            className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-emerald-600 hover:underline dark:text-emerald-400"
          >
            {t("resetFilters", "重置筛选条件")}
          </button>
        </div>
      )}

      {/* 卡片网格系统 */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredWorktrees.map((wt) => {
          const isResuming = resumingTarget === wt.worktree_path;
          const updatedAgo = formatDistanceToNow(parseDate(wt.last_modified), {
            addSuffix: true,
            locale: dateLocale,
          });
          const worktreeName = extractWorktreeName(wt);
          const isPathCopied = copiedTarget === `path:${wt.worktree_path}`;
          const isHashCopied = copiedTarget === `hash:${wt.last_commit?.hash}`;

          return (
            <div
              key={wt.worktree_path}
              className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm transition-all hover:shadow-md hover:border-emerald-500/30 dark:border-slate-800 dark:bg-slate-900 flex flex-col justify-between group"
            >
              <div>
                {/* 顶栏：代码仓库与运行状态 */}
                <div className="flex items-center justify-between gap-2 mb-3">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="flex h-6 w-6 items-center justify-center rounded-lg bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-300 shrink-0">
                      <FolderGit2 className="h-3.5 w-3.5" />
                    </span>
                    <span
                      className="truncate text-xs font-bold text-slate-900 dark:text-white"
                      title={wt.repo_name}
                    >
                      {wt.repo_name}
                    </span>
                  </div>

                  {wt.has_active_session ? (
                    <span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-[11px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400 shrink-0">
                      <span className="h-2 w-2 rounded-full bg-emerald-500 animate-pulse" />
                      {t("statusRunning", "活跃运行中")}
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1 rounded-full bg-slate-100 px-2.5 py-0.5 text-[10px] font-medium text-slate-500 dark:bg-slate-800 dark:text-slate-400 shrink-0">
                      <span className="h-1.5 w-1.5 rounded-full bg-slate-400" />
                      {t("statusIdle", "已离线 / 空闲")}
                    </span>
                  )}
                </div>

                {/* 分支名称 (独占一行，长分支名折行完整呈现，支持选中复制) */}
                {wt.branch && (
                  <div className="mb-2.5 flex items-start gap-2 rounded-xl bg-emerald-500/10 dark:bg-emerald-500/20 px-3 py-1.5 text-xs font-mono font-medium text-emerald-700 dark:text-emerald-300 border border-emerald-500/20">
                    <GitBranch className="h-3.5 w-3.5 shrink-0 text-emerald-600 dark:text-emerald-400 mt-0.5" />
                    <div className="min-w-0 flex-1">
                      <div className="text-[10px] uppercase tracking-wider text-emerald-800/80 dark:text-emerald-200/80 font-semibold mb-0.5">
                        {t("branchLabel", "分支")}
                      </div>
                      <div className="break-all select-all leading-snug font-semibold">
                        {wt.branch}
                      </div>
                    </div>
                  </div>
                )}

                {/* Worktree 专属名称 (独立高亮显示) */}
                <div className="mb-2.5 flex items-start gap-2 rounded-xl bg-slate-50 dark:bg-slate-800/60 px-3 py-1.5 text-xs font-mono text-slate-700 dark:text-slate-300 border border-slate-200/70 dark:border-slate-800/80">
                  <Layers className="h-3.5 w-3.5 shrink-0 text-slate-500 dark:text-slate-400 mt-0.5" />
                  <div className="min-w-0 flex-1">
                    <div className="text-[10px] uppercase tracking-wider text-slate-500 dark:text-slate-400 font-semibold mb-0.5">
                      Worktree
                    </div>
                    <div className="break-all select-all font-semibold leading-snug text-slate-900 dark:text-slate-100">
                      {worktreeName}
                    </div>
                  </div>
                </div>

                {/* 最新 Commit 摘要区块 */}
                {wt.last_commit ? (
                  <div className="mb-3 rounded-xl bg-slate-50/70 dark:bg-slate-800/40 p-2.5 border border-slate-100 dark:border-slate-800/80 space-y-1">
                    <div className="flex items-center justify-between gap-1 text-[11px]">
                      <div className="flex items-center gap-1.5 min-w-0 font-mono">
                        <GitCommit className="h-3 w-3 text-slate-400 shrink-0" />
                        <button
                          type="button"
                          onClick={() =>
                            handleCopy(wt.last_commit!.hash, `hash:${wt.last_commit!.hash}`)
                          }
                          title={t("copyHashTooltip", "复制 Commit Hash")}
                          className="font-semibold text-emerald-700 dark:text-emerald-400 hover:underline inline-flex items-center gap-1"
                        >
                          <span>{wt.last_commit.hash}</span>
                          {isHashCopied ? (
                            <Check className="h-2.5 w-2.5 text-emerald-500" />
                          ) : (
                            <Copy className="h-2.5 w-2.5 text-slate-400 opacity-60" />
                          )}
                        </button>
                      </div>
                      <span className="truncate text-slate-400 text-[10px]">
                        {wt.last_commit.author}
                      </span>
                    </div>

                    <p
                      className="text-xs text-slate-600 dark:text-slate-300 truncate leading-relaxed"
                      title={wt.last_commit.message}
                    >
                      {wt.last_commit.message}
                    </p>
                  </div>
                ) : (
                  <div className="mb-3 rounded-xl bg-slate-50/40 dark:bg-slate-800/30 px-3 py-2 border border-dashed border-slate-200/80 dark:border-slate-800/60 text-[11px] text-slate-400 italic">
                    {t("noCommitHistory", "暂无最新 Git 提交记录")}
                  </div>
                )}

                {/* 工作区路径与最后更新 */}
                <div className="space-y-1.5 text-xs text-slate-500 dark:text-slate-400 mb-4">
                  <div className="flex items-center justify-between gap-1 min-w-0">
                    <div className="flex items-center gap-1.5 min-w-0 flex-1">
                      <Folder className="h-3.5 w-3.5 shrink-0 text-slate-400" />
                      <span
                        className="font-mono text-[11px] truncate flex-1 min-w-0"
                        style={{ direction: "rtl", textAlign: "left", unicodeBidi: "plaintext" }}
                        title={wt.worktree_path}
                      >
                        {wt.worktree_path}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleCopy(wt.worktree_path, `path:${wt.worktree_path}`)}
                      title={t("copyPathTooltip", "复制完整目录路径")}
                      className="p-1 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded-md transition-colors"
                    >
                      {isPathCopied ? (
                        <Check className="h-3 w-3 text-emerald-500" />
                      ) : (
                        <Copy className="h-3 w-3" />
                      )}
                    </button>
                  </div>

                  <div className="flex items-center gap-1.5 text-[11px] text-slate-400">
                    <Clock className="h-3 w-3 shrink-0" />
                    <span>
                      {t("lastModifiedAt", "最后修改")}: {updatedAgo}
                    </span>
                  </div>
                </div>
              </div>

              {/* 底部操作栏 */}
              <div className="pt-3 border-t border-slate-100 dark:border-slate-800/80 flex items-center justify-between gap-2">
                <button
                  type="button"
                  disabled={isResuming}
                  onClick={() => handleResume(wt)}
                  className={`flex-1 inline-flex items-center justify-center gap-1.5 rounded-xl px-3.5 py-2 text-xs font-medium text-white shadow-sm transition-all disabled:opacity-60 ${
                    wt.has_active_session
                      ? "bg-emerald-600 hover:bg-emerald-700"
                      : "bg-slate-800 hover:bg-slate-900 dark:bg-emerald-700 dark:hover:bg-emerald-600"
                  }`}
                >
                  {isResuming ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : wt.has_active_session ? (
                    <Play className="h-3.5 w-3.5 fill-current" />
                  ) : (
                    <Terminal className="h-3.5 w-3.5" />
                  )}
                  {wt.has_active_session
                    ? t("resumeLiveSession", "进入运行现场 (直连终端)")
                    : t("reviveWorktreeSession", "重新拉起现场 (直连终端)")}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
