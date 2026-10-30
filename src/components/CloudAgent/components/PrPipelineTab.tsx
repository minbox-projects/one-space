import { useState } from "react";
import {
  GitPullRequest,
  AlertCircle,
  CheckCircle2,
  Clock,
  ExternalLink,
  Loader2,
  RefreshCw,
  GitBranch,
  Copy,
  Check,
  Info,
  FolderGit2,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import type { CloudAgentTask } from "@/lib/cloudAgent";
import { CloudEmptyState } from "./CloudEmptyState";

interface PrPipelineTabProps {
  t: TFunction;
  language: string;
  tasks: CloudAgentTask[];
  loading: boolean;
  onRefresh: () => void;
}

/**
 * 智能推导 Issue 的完整外部访问链接
 */
export function resolveIssueUrl(task: CloudAgentTask): string | null {
  if (!task.issue_number) return null;

  // 1. 优先从 pr_url 推导（保留真实的 GitHub 域名与完整 owner/repo 路径）
  if (task.pr_url && task.pr_url.includes("/pull/")) {
    return task.pr_url.replace(/\/pull\/\d+.*$/, `/issues/${task.issue_number}`);
  }

  // 2. 检查 repo 是否本身就是 URL 或 SSH 路径
  if (task.repo) {
    const rawRepo = task.repo.trim();
    if (rawRepo.startsWith("http://") || rawRepo.startsWith("https://")) {
      const cleanUrl = rawRepo.replace(/\.git$/, "").replace(/\/+$/, "");
      return `${cleanUrl}/issues/${task.issue_number}`;
    }
    if (rawRepo.startsWith("git@")) {
      const parts = rawRepo.replace(/\.git$/, "").split(":");
      if (parts.length === 2) {
        const host = parts[0].replace("git@", "");
        return `https://${host}/${parts[1]}/issues/${task.issue_number}`;
      }
    }
    // 3. 常规 owner/repo 或单名格式
    return `https://github.com/${rawRepo}/issues/${task.issue_number}`;
  }

  return null;
}

export function PrPipelineTab({
  t,
  language,
  tasks,
  loading,
  onRefresh,
}: PrPipelineTabProps) {
  const [copiedTaskId, setCopiedTaskId] = useState<string | null>(null);
  const dateLocale = language === "zh" ? zhCN : enUS;

  const handleCopyError = (taskId: string, text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedTaskId(taskId);
    setTimeout(() => {
      setCopiedTaskId((curr) => (curr === taskId ? null : curr));
    }, 2000);
  };

  if (loading && tasks.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-emerald-500" />
      </div>
    );
  }

  if (tasks.length === 0) {
    return (
      <CloudEmptyState
        icon={GitPullRequest}
        title={t("noPipelineTasksTitle", "暂无 Issues 自动流水线任务记录")}
        description={t(
          "noPipelineTasksDesc",
          "当监控的 GitHub 仓库被打上任务触发标签（如 bot:task）时，AI 云端智能体将自动接单、无头实施、创建 PR 并闭环记录流水。"
        )}
        action={{
          label: t("refreshPipelineTasks", "刷新任务流水"),
          icon: RefreshCw,
          onClick: onRefresh,
          loading,
        }}
      />
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

  return (
    <div className="space-y-4">
      {/* 列表头部统计与操作 */}
      <div className="flex items-center justify-between px-1">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("recentTasksHistory", "近期任务流转历史")} ({tasks.length})
        </h3>
        <button
          type="button"
          onClick={onRefresh}
          className="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground cursor-pointer"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("refresh", "刷新")}
        </button>
      </div>

      {/* 任务卡片列表 */}
      <div className="grid grid-cols-1 gap-3">
        {tasks.map((task, index) => {
          const startedAgo = formatDistanceToNow(parseDate(task.started_at), {
            addSuffix: true,
            locale: dateLocale,
          });

          // 如果处于靠后的任务卡片，浮层向上展开避免超出视口底部；否则向下展开
          const isNearBottom = index >= tasks.length - 1 && tasks.length > 1;
          const tooltipPositionClass = isNearBottom
            ? "bottom-full mb-2.5"
            : "top-full mt-2.5";

          const hasBranchOrPr = Boolean(task.branch || task.pr_url);
          const issueUrl = resolveIssueUrl(task);

          return (
            <div
              key={task.id}
              className="group/card rounded-xl border border-border/80 bg-card p-4 sm:p-4.5 shadow-2xs transition-all hover:border-border hover:shadow-xs"
            >
              <div className="flex flex-col">
                {/* 1. 顶栏区域：状态标识与时间/耗时元数据，解耦拥挤 */}
                <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-border/40">
                  {/* 左侧：任务状态徽章与所属仓库#Issue 标识（支持点击跳转） */}
                  <div className="flex flex-wrap items-center gap-2.5">
                    {task.status === "completed" && (
                      <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1 text-xs font-semibold text-emerald-600 dark:text-emerald-400">
                        <CheckCircle2 className="h-3.5 w-3.5" />
                        {t("pipelineStatusPrCreated", "PR 已创建")}
                      </span>
                    )}
                    {task.status === "running" && (
                      <span className="inline-flex items-center gap-1.5 rounded-full border border-sky-500/20 bg-sky-500/10 px-2.5 py-1 text-xs font-semibold text-sky-600 animate-pulse dark:text-sky-400">
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        {t("pipelineStatusRunning", "实施中")}
                      </span>
                    )}
                    {task.status === "failed" && (
                      <span className="inline-flex items-center gap-1.5 rounded-full border border-rose-500/20 bg-rose-500/10 px-2.5 py-1 text-xs font-semibold text-rose-600 dark:text-rose-400">
                        <AlertCircle className="h-3.5 w-3.5" />
                        {t("pipelineStatusFailed", "执行失败")}
                      </span>
                    )}
                    {task.status === "no_changes" && (
                      <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/20 bg-amber-500/10 px-2.5 py-1 text-xs font-semibold text-amber-600 dark:text-amber-400">
                        {t("pipelineStatusNoChanges", "无变更")}
                      </span>
                    )}

                    {/* 代码仓库与 Issue 标识（支持点击在新窗口打开 Issue 页面） */}
                    {issueUrl ? (
                      <a
                        href={issueUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="group/issuelink inline-flex items-center gap-1.5 rounded-lg border border-border/80 bg-muted/60 px-2.5 py-1 font-mono text-xs font-medium text-foreground transition-all hover:border-primary/40 hover:bg-muted hover:text-primary cursor-pointer"
                        title={t("viewIssueTooltip", {
                          number: task.issue_number,
                          defaultValue: `在新窗口打开 Issue #${task.issue_number}`,
                        })}
                        data-testid="pipeline-task-issue-link"
                      >
                        <FolderGit2 className="h-3.5 w-3.5 text-muted-foreground group-hover/issuelink:text-primary transition-colors" />
                        <span>{task.repo}#{task.issue_number}</span>
                        <ExternalLink className="h-3 w-3 opacity-40 group-hover/issuelink:opacity-100 transition-opacity" />
                      </a>
                    ) : (
                      <span className="inline-flex items-center gap-1.5 rounded-lg border border-border/80 bg-muted/60 px-2.5 py-1 font-mono text-xs font-medium text-foreground">
                        <FolderGit2 className="h-3.5 w-3.5 text-muted-foreground" />
                        <span>{task.repo}#{task.issue_number}</span>
                      </span>
                    )}
                  </div>

                  {/* 右侧：耗时与相对启动时间 */}
                  <div className="flex items-center gap-2 text-xs text-muted-foreground shrink-0">
                    {task.duration_seconds ? (
                      <div className="flex items-center gap-1 text-foreground/80">
                        <Clock className="h-3.5 w-3.5 text-muted-foreground" />
                        <span>
                          {t("taskDurationSeconds", {
                            seconds: task.duration_seconds,
                            defaultValue: `耗时 ${task.duration_seconds} 秒`,
                          })}
                        </span>
                      </div>
                    ) : null}
                    {task.duration_seconds ? <span className="text-border">·</span> : null}
                    <span>{startedAgo}</span>
                  </div>
                </div>

                {/* 2. 主体区域：居中突出任务标题（支持点击跳转）与独立的分支/PR 操作栏 */}
                <div className="pt-3 pb-1 space-y-2.5">
                  {/* 任务主标题：点击直接跳转至 Issue 页面 */}
                  {issueUrl ? (
                    <a
                      href={issueUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sm font-semibold text-foreground tracking-tight leading-snug truncate hover:text-primary transition-colors inline-block max-w-full"
                      title={task.title}
                      data-testid="pipeline-task-title-link"
                    >
                      {task.title}
                    </a>
                  ) : (
                    <h4
                      className="text-sm font-semibold text-foreground tracking-tight leading-snug truncate"
                      title={task.title}
                    >
                      {task.title}
                    </h4>
                  )}

                  {/* 特性分支与 PR 查看按钮 */}
                  {hasBranchOrPr && (
                    <div className="flex flex-wrap items-center justify-between gap-3 pt-0.5">
                      {/* 特性分支标识 */}
                      {task.branch ? (
                        <div className="flex items-center gap-2">
                          <span
                            className="inline-flex items-center gap-1.5 rounded-lg border border-primary/20 bg-primary/10 px-2.5 py-1 font-mono text-xs font-medium text-primary"
                            title={t("branchLabel", "分支")}
                          >
                            <GitBranch className="h-3.5 w-3.5 text-primary shrink-0" />
                            <span className="max-w-[240px] truncate">{task.branch}</span>
                          </span>
                        </div>
                      ) : <div />}

                      {/* 查看 PR 链接按钮 */}
                      {task.pr_url && (
                        <a
                          href={task.pr_url}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="inline-flex items-center gap-1.5 rounded-lg border border-border/80 bg-background px-3 py-1 text-xs font-medium text-foreground hover:bg-muted hover:text-primary transition-colors shadow-2xs shrink-0"
                        >
                          <GitPullRequest className="h-3.5 w-3.5 text-primary" />
                          <span>{t("viewPullRequest", "查看 PR")}</span>
                          <ExternalLink className="h-3 w-3 opacity-60" />
                        </a>
                      )}
                    </div>
                  )}
                </div>

                {/* 3. 底栏区域：错误信息单行收敛展示与悬浮完整错误浮层 */}
                {task.error_message && (
                  <div className="mt-3 pt-3 border-t border-border/60">
                    <div className="relative group/error">
                      {/* 单行紧凑告警栏，内边距舒适，边界留白充足 */}
                      <div
                        tabIndex={0}
                        role="button"
                        aria-label={t("hoverToViewErrorDetail", "滑过查看完整错误")}
                        className="flex items-center gap-2.5 rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/15 cursor-pointer focus:outline-none focus:ring-1 focus:ring-destructive/30"
                      >
                        <AlertCircle className="h-3.5 w-3.5 shrink-0 text-destructive" />
                        <span
                          className="truncate font-mono flex-1 text-left"
                          title={task.error_message}
                          data-testid="pipeline-task-error-collapsed"
                        >
                          {task.error_message}
                        </span>
                        <span className="shrink-0 text-[11px] font-sans opacity-75 flex items-center gap-1 select-none">
                          <Info className="h-3 w-3" />
                          <span>{t("hoverToViewErrorDetail", "滑过查看完整错误")}</span>
                        </span>
                      </div>

                      {/* 悬浮展示完整错误信息的浮层 */}
                      <div
                        role="tooltip"
                        data-testid="pipeline-task-error-tooltip"
                        className={`pointer-events-auto absolute left-0 ${tooltipPositionClass} hidden group-hover/error:flex group-focus-within/error:flex flex-col gap-2.5 rounded-xl border border-slate-700/80 bg-slate-900 p-3.5 text-left text-slate-100 shadow-2xl z-50 w-full max-w-xl animate-in fade-in zoom-in-95 duration-150`}
                      >
                        <div className="flex items-center justify-between border-b border-slate-800 pb-2">
                          <div className="flex items-center gap-1.5 text-xs font-semibold text-rose-400">
                            <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                            <span>{t("errorDetailTitle", "错误详细信息")}</span>
                          </div>
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleCopyError(task.id, task.error_message!);
                            }}
                            className="inline-flex items-center gap-1 rounded-lg bg-slate-800 hover:bg-slate-700 px-2 py-1 text-[11px] font-medium text-slate-300 hover:text-white transition-colors cursor-pointer"
                            title={t("copyErrorMessage", "复制完整错误")}
                          >
                            {copiedTaskId === task.id ? (
                              <>
                                <Check className="h-3 w-3 text-emerald-400" />
                                <span className="text-emerald-400">
                                  {t("copiedErrorMessage", "已复制错误信息")}
                                </span>
                              </>
                            ) : (
                              <>
                                <Copy className="h-3 w-3" />
                                <span>{t("copyErrorMessage", "复制完整错误")}</span>
                              </>
                            )}
                          </button>
                        </div>

                        <div className="max-h-56 overflow-y-auto rounded-lg bg-slate-950 p-3 font-mono text-xs text-slate-200 whitespace-pre-wrap break-all select-text border border-slate-800/80 leading-relaxed">
                          {task.error_message}
                        </div>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
