import { GitPullRequest, AlertCircle, CheckCircle2, Clock, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import type { CloudAgentTask } from "@/lib/cloudAgent";

interface PrPipelineTabProps {
  t: TFunction;
  language: string;
  tasks: CloudAgentTask[];
  loading: boolean;
  onRefresh: () => void;
}

export function PrPipelineTab({
  t,
  language,
  tasks,
  loading,
  onRefresh,
}: PrPipelineTabProps) {
  const dateLocale = language === "zh" ? zhCN : enUS;

  if (loading && tasks.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-emerald-500" />
      </div>
    );
  }

  if (tasks.length === 0) {
    return (
      <div className="rounded-2xl border border-dashed border-slate-300 dark:border-slate-800 bg-white/50 dark:bg-slate-900/40 p-12 text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-2xl bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400 mb-4">
          <GitPullRequest className="h-6 w-6" />
        </div>
        <h3 className="text-base font-semibold text-slate-800 dark:text-slate-200">
          {t("noPipelineTasksTitle", "暂无 Issues 自动流水线任务记录")}
        </h3>
        <p className="mt-1 text-xs text-slate-500 dark:text-slate-400 max-w-sm mx-auto">
          {t(
            "noPipelineTasksDesc",
            "当监控的 GitHub 仓库被打上任务触发标签（如 bot:task）时，AI 云端智能体将自动接单、无头实施、创建 PR 并闭环记录流水。"
          )}
        </p>
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

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider">
          {t("recentTasksHistory", "近期任务流转历史")} ({tasks.length})
        </h3>
        <button
          type="button"
          onClick={onRefresh}
          className="inline-flex items-center gap-1.5 text-xs text-slate-500 hover:text-slate-900 dark:hover:text-white transition-colors"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("refresh", "刷新")}
        </button>
      </div>

      <div className="grid grid-cols-1 gap-3">
        {tasks.map((task) => {
          const startedAgo = formatDistanceToNow(parseDate(task.started_at), {
            addSuffix: true,
            locale: dateLocale,
          });

          return (
            <div
              key={task.id}
              className="rounded-2xl border border-slate-200 bg-white p-4.5 shadow-sm dark:border-slate-800 dark:bg-slate-900 hover:border-slate-300 dark:hover:border-slate-700 transition-all"
            >
              <div className="flex flex-col md:flex-row md:items-center justify-between gap-3">
                {/* 任务标题与 Issue 编号 */}
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center gap-2">
                    {task.status === "completed" && (
                      <span className="flex items-center gap-1 text-[11px] font-semibold text-emerald-600 bg-emerald-50 dark:bg-emerald-950/40 px-2 py-0.5 rounded-full">
                        <CheckCircle2 className="h-3 w-3" />
                        PR 已创建
                      </span>
                    )}
                    {task.status === "running" && (
                      <span className="flex items-center gap-1 text-[11px] font-semibold text-sky-600 bg-sky-50 dark:bg-sky-950/40 px-2 py-0.5 rounded-full animate-pulse">
                        <Loader2 className="h-3 w-3 animate-spin" />
                        实施中
                      </span>
                    )}
                    {task.status === "failed" && (
                      <span className="flex items-center gap-1 text-[11px] font-semibold text-rose-600 bg-rose-50 dark:bg-rose-950/40 px-2 py-0.5 rounded-full">
                        <AlertCircle className="h-3 w-3" />
                        执行失败
                      </span>
                    )}
                    {task.status === "no_changes" && (
                      <span className="flex items-center gap-1 text-[11px] font-semibold text-amber-600 bg-amber-50 dark:bg-amber-950/40 px-2 py-0.5 rounded-full">
                        无变更
                      </span>
                    )}

                    <span className="text-xs font-mono font-medium text-slate-500">
                      {task.repo}#{task.issue_number}
                    </span>
                  </div>

                  <h4 className="text-sm font-semibold text-slate-900 dark:text-white truncate">
                    {task.title}
                  </h4>

                  {task.error_message && (
                    <p className="text-xs text-rose-600 dark:text-rose-400 font-mono bg-rose-50/50 dark:bg-rose-950/30 p-2 rounded-xl mt-1">
                      {task.error_message}
                    </p>
                  )}
                </div>

                {/* 详细元信息与 PR 链接 */}
                <div className="flex items-center gap-4 text-xs text-slate-500 shrink-0">
                  <div className="text-right space-y-0.5">
                    {task.duration_seconds ? (
                      <div className="flex items-center gap-1 justify-end">
                        <Clock className="h-3 w-3" />
                        <span>耗时 {task.duration_seconds} 秒</span>
                      </div>
                    ) : null}
                    <div>{startedAgo}</div>
                  </div>

                  {task.pr_url && (
                    <a
                      href={task.pr_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1 rounded-xl bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 px-3 py-1.5 text-xs font-medium text-slate-800 dark:text-slate-200 transition-colors"
                    >
                      <GitPullRequest className="h-3.5 w-3.5 text-violet-600 dark:text-violet-400" />
                      {t("viewPullRequest", "查看 PR")}
                      <ExternalLink className="h-3 w-3 opacity-60" />
                    </a>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
