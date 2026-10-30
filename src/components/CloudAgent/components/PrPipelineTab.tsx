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
  FolderGit2,
  User,
  Terminal,
  ChevronDown,
  ChevronUp,
  Tag,
  MessageSquare,
  FileDiff,
  Hand,
  XCircle,
  Info,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";
import type { TFunction } from "i18next";
import {
  type CloudAgentTask,
  formatFriendlyDuration,
  extractPrNumber,
  formatGitCheckoutCommand,
  formatIssueLabelTail,
} from "@/lib/cloudAgent";
import { CloudEmptyState } from "./CloudEmptyState";

interface PrPipelineTabProps {
  t: TFunction;
  language: string;
  tasks: CloudAgentTask[];
  loading: boolean;
  onRefresh: () => void;
  onLaunchTerminal?: (target: string, branch?: string) => Promise<void>;
  onPlanAttach?: (jobId: string) => void | Promise<void>;
  onPlanCancel?: (jobId: string) => void | Promise<void>;
}

function formatPlanDetail(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
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

export type StepState =
  | "completed"
  | "current"
  | "failed"
  | "skipped"
  | "pending"
  | "cancelled";

export interface PipelineStepItem {
  id: string;
  label: string;
  state: StepState;
}

/**
 * 根据任务状态推导 5 阶段流水线节点的渲染状态
 */
export function derivePipelineSteps(
  task: CloudAgentTask,
  t: TFunction
): PipelineStepItem[] {
  if (task.source === "plan") {
    const planSteps: PipelineStepItem[] = [
      { id: "accepted", label: t("planStepAccepted", "接单"), state: "pending" },
      {
        id: "preparing",
        label: t("planStepPreparing", "准备仓库"),
        state: "pending",
      },
      {
        id: "running",
        label: t("planStepRunning", "实施"),
        state: "pending",
      },
      {
        id: "publishing",
        label: t("planStepPublishing", "发布 PR"),
        state: "pending",
      },
      {
        id: "completed",
        label: t("planStepCompleted", "完成"),
        state: "pending",
      },
    ];

    const status = (task.status || "").toLowerCase();
    const stage = (task.stage || task.status || "").toLowerCase();
    const stageIndex: Record<string, number> = {
      accepted: 0,
      preparing: 1,
      running: 2,
      waiting_input: 2,
      publishing: 3,
      completed: 4,
      no_changes: 4,
    };

    if (status === "completed") {
      return planSteps.map((step) => ({ ...step, state: "completed" }));
    }

    if (status === "no_changes") {
      planSteps[0].state = "completed";
      planSteps[1].state = "completed";
      planSteps[2].state = "completed";
      planSteps[3].state = "skipped";
      planSteps[4].state = "completed";
      return planSteps;
    }

    const index = Math.min(stageIndex[stage] ?? 0, planSteps.length - 1);

    if (status === "failed" || status === "cancelled") {
      for (let i = 0; i < index; i += 1) {
        planSteps[i].state = "completed";
      }
      planSteps[index].state =
        status === "cancelled" ? "cancelled" : "failed";
      return planSteps;
    }

    for (let i = 0; i < index; i += 1) {
      planSteps[i].state = "completed";
    }
    if (index < planSteps.length - 1) {
      planSteps[index].state = "current";
    } else {
      planSteps[index].state = "completed";
    }
    return planSteps;
  }

  const steps: PipelineStepItem[] = [
    { id: "pick", label: t("stepPick", "接单鉴权"), state: "completed" },
    { id: "branch", label: t("stepBranch", "创建 Worktree"), state: "pending" },
    { id: "opencode", label: t("stepOpenCode", "实施"), state: "pending" },
    { id: "diff", label: t("stepDiff", "推送"), state: "pending" },
    { id: "pr", label: t("stepPr", "PR 创建"), state: "pending" },
  ];

  if (task.status === "completed") {
    return steps.map((s) => ({ ...s, state: "completed" }));
  }

  if (task.status === "no_changes") {
    steps[0].state = "completed";
    steps[1].state = "completed";
    steps[2].state = "completed";
    steps[3].state = "skipped";
    steps[4].state = "completed";
    return steps;
  }

  if (task.status === "running") {
    steps[0].state = "completed";
    const curStep = task.step?.toLowerCase() || "";
    if (curStep.includes("clone") || curStep.includes("branch")) {
      steps[1].state = "current";
    } else if (curStep.includes("push") || curStep.includes("commit")) {
      steps[1].state = "completed";
      steps[2].state = "completed";
      steps[3].state = "current";
    } else if (curStep.includes("pr")) {
      steps[1].state = "completed";
      steps[2].state = "completed";
      steps[3].state = "completed";
      steps[4].state = "current";
    } else {
      if (task.branch) {
        steps[1].state = "completed";
        steps[2].state = "current";
      } else {
        steps[1].state = "current";
      }
    }
    return steps;
  }

  if (task.status === "failed") {
    steps[0].state = "completed";
    const curStep = task.step?.toLowerCase() || "";
    if (curStep.includes("clone") || curStep.includes("branch")) {
      steps[1].state = "failed";
    } else if (curStep.includes("push") || curStep.includes("commit")) {
      steps[1].state = "completed";
      steps[2].state = "completed";
      steps[3].state = "failed";
    } else if (curStep.includes("pr")) {
      steps[1].state = "completed";
      steps[2].state = "completed";
      steps[3].state = "completed";
      steps[4].state = "failed";
    } else {
      if (task.branch) {
        steps[1].state = "completed";
        steps[2].state = "failed";
      } else {
        steps[1].state = "failed";
      }
    }
    return steps;
  }

  return steps;
}

export function PrPipelineTab({
  t,
  language,
  tasks,
  loading,
  onRefresh,
  onLaunchTerminal,
  onPlanAttach,
  onPlanCancel,
}: PrPipelineTabProps) {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [expandedErrors, setExpandedErrors] = useState<Record<string, boolean>>({});
  const [resumingTaskId, setResumingTaskId] = useState<string | null>(null);

  const dateLocale = language === "zh" ? zhCN : enUS;

  const handleCopy = (key: string, text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(key);
    setTimeout(() => {
      setCopiedKey((curr) => (curr === key ? null : curr));
    }, 2000);
  };

  const toggleErrorExpand = (taskId: string) => {
    setExpandedErrors((prev) => ({
      ...prev,
      [taskId]: !prev[taskId],
    }));
  };

  const handleLaunchTerminal = async (task: CloudAgentTask) => {
    if (!onLaunchTerminal) return;
    try {
      setResumingTaskId(task.id);
      await onLaunchTerminal(task.repo, task.branch || undefined);
    } finally {
      setResumingTaskId(null);
    }
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
    <div className="space-y-3.5">
      {/* 列表头部统计与操作 */}
      <div className="flex items-center justify-between px-1">
        <div className="flex items-center gap-2">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("recentTasksHistory", "近期任务流转历史")} ({tasks.length})
          </h3>
        </div>
        <button
          type="button"
          onClick={onRefresh}
          className="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground cursor-pointer"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("refresh", "刷新")}
        </button>
      </div>

      {/* 任务卡片列表：支持大屏/桌面端一行双卡片布局 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
        {tasks.map((task) => {
          const startedAgo = formatDistanceToNow(parseDate(task.started_at), {
            addSuffix: true,
            locale: dateLocale,
          });

          const issueUrl = resolveIssueUrl(task);
          const prNumber = extractPrNumber(task.pr_url);
          const checkoutCmd = task.branch ? formatGitCheckoutCommand(task.branch) : "";
          const friendlyDuration = formatFriendlyDuration(task.duration_seconds, language);
          const pipelineSteps = derivePipelineSteps(task, t);
          const isErrorExpanded = Boolean(expandedErrors[task.id]);
          const isPlanTask = task.source === "plan";
          const planStatus = (task.status || "").toLowerCase();
          const planStage = (task.stage || task.status || "").toLowerCase();
          const planTerminal = [
            "completed",
            "no_changes",
            "failed",
            "cancelled",
          ].includes(planStatus);
          const planLive = ["running", "waiting_input", "publishing"].includes(
            planStage
          );
          const questionText = formatPlanDetail(task.question);
          const decisionText = formatPlanDetail(task.decision);

          // 触发类型显示
          const isFollowUp = task.trigger_type === "follow_up";
          const hasFooter = Boolean(checkoutCmd || task.pr_url);

          return (
            <div
              key={task.id}
              className="group/card flex flex-col justify-between rounded-xl border border-border/80 bg-card p-3.5 sm:p-4 shadow-2xs transition-all hover:border-border hover:shadow-xs space-y-2.5"
            >
              <div className="space-y-2.5">
                {/* 1. 顶栏：状态标识、触发源、尝试轮次、仓库标识与 Issue 链接（单行不折行） */}
                <div className="flex items-center gap-1.5 pb-2 border-b border-border/40 min-w-0 overflow-hidden whitespace-nowrap">
                  {/* 状态徽标 */}
                  {task.status === "completed" && (
                    <span className="inline-flex items-center gap-1 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-xs font-semibold text-emerald-600 dark:text-emerald-400 shrink-0 whitespace-nowrap">
                      <CheckCircle2 className="h-3.5 w-3.5" />
                      {t("pipelineStatusPrCreated", "PR 已创建")}
                    </span>
                  )}
                  {task.status === "running" && (
                    <span className="inline-flex items-center gap-1 rounded-full border border-sky-500/20 bg-sky-500/10 px-2 py-0.5 text-xs font-semibold text-sky-600 animate-pulse dark:text-sky-400 shrink-0 whitespace-nowrap">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      {t("pipelineStatusRunning", "实施中")}
                    </span>
                  )}
                  {task.status === "failed" && (
                    <span className="inline-flex items-center gap-1 rounded-full border border-rose-500/20 bg-rose-500/10 px-2 py-0.5 text-xs font-semibold text-rose-600 dark:text-rose-400 shrink-0 whitespace-nowrap">
                      <AlertCircle className="h-3.5 w-3.5" />
                      {t("pipelineStatusFailed", "执行失败")}
                    </span>
                  )}
                  {task.status === "no_changes" && (
                    <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-xs font-semibold text-amber-600 dark:text-amber-400 shrink-0 whitespace-nowrap">
                      <AlertCircle className="h-3.5 w-3.5" />
                      {t("pipelineStatusNoChanges", "无变更")}
                    </span>
                  )}
                  {isPlanTask && task.status === "waiting_input" && (
                    <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-xs font-semibold text-amber-600 dark:text-amber-400 shrink-0 whitespace-nowrap">
                      <AlertCircle className="h-3.5 w-3.5" />
                      {t("pipelineStatusWaitingInput", "等待人工输入")}
                    </span>
                  )}
                  {isPlanTask && task.status === "cancelled" && (
                    <span className="inline-flex items-center gap-1 rounded-full border border-border bg-muted/60 px-2 py-0.5 text-xs font-semibold text-muted-foreground shrink-0 whitespace-nowrap">
                      <XCircle className="h-3.5 w-3.5" />
                      {t("pipelineStatusCancelled", "已取消")}
                    </span>
                  )}

                  {/* 触发方式标识 */}
                  {!isPlanTask ? (
                    <span
                      className={`inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[10.5px] font-medium border shrink-0 whitespace-nowrap ${
                        isFollowUp
                          ? "border-purple-500/20 bg-purple-500/10 text-purple-600 dark:text-purple-400"
                          : "border-blue-500/20 bg-blue-500/10 text-blue-600 dark:text-blue-400"
                      }`}
                      title={
                        isFollowUp
                          ? t("triggerFollowUpDesc", "协作者评论 /agent 发起的追随任务")
                          : t("triggerFirstRunDesc", "Issue 关联标签首次接单")
                      }
                    >
                      {isFollowUp ? (
                        <MessageSquare className="h-2.5 w-2.5" />
                      ) : (
                        <Tag className="h-2.5 w-2.5" />
                      )}
                      <span>
                        {isFollowUp
                          ? t("triggerFollowUp", "追加指令")
                          : t("triggerFirstRun", "标签接单")}
                      </span>
                    </span>
                  ) : null}

                  {/* 尝试轮次 */}
                  {task.attempts !== undefined && task.attempts !== null ? (
                    <span
                      className="inline-flex items-center rounded-md border border-border/70 bg-muted/50 px-1.5 py-0.5 text-[10.5px] font-mono text-muted-foreground shrink-0 whitespace-nowrap"
                      title={t("attemptsTooltip", {
                        current: task.attempts,
                        max: task.max_attempts || 3,
                        defaultValue: `已尝试 ${task.attempts}/${task.max_attempts || 3} 次`,
                      })}
                    >
                      <span>
                        {t("attemptBadge", {
                          current: task.attempts,
                          max: task.max_attempts || 3,
                          defaultValue: `尝试 ${task.attempts}/${task.max_attempts || 3}`,
                        })}
                      </span>
                    </span>
                  ) : null}

                  {/* 代码仓库与 Issue 标识（支持点击在新窗口打开 Issue 页面，隐藏前部分组织/URL，完整显示后部与编号） */}
                  {isPlanTask ? (
                    <span
                      className="inline-flex items-center gap-1 rounded-md border border-sky-500/20 bg-sky-500/10 px-2 py-0.5 font-mono text-xs font-medium text-sky-700 dark:text-sky-300 min-w-0 shrink truncate whitespace-nowrap"
                      data-testid="plan-task-meta"
                      title={task.plan_title || task.plan_id || task.repo}
                    >
                      <GitPullRequest className="h-3 w-3 shrink-0" />
                      <span className="shrink-0">
                        {t("planTaskSourcePlan", "本地计划")}
                      </span>
                      {task.plan_id ? (
                        <span className="truncate max-w-[180px]">
                          {t("planTaskPlanId", "计划")}: {task.plan_id}
                        </span>
                      ) : null}
                      {task.workspace_id ? (
                        <span className="truncate max-w-[150px] text-muted-foreground">
                          {t("planTaskWorkspace", "工作区")}: {task.workspace_id}
                        </span>
                      ) : null}
                    </span>
                  ) : issueUrl ? (
                    <a
                      href={issueUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="group/issuelink inline-flex items-center gap-1 rounded-md border border-border/80 bg-muted/60 px-2 py-0.5 font-mono text-xs font-medium text-foreground transition-all hover:border-primary/40 hover:bg-muted hover:text-primary cursor-pointer min-w-0 shrink truncate whitespace-nowrap"
                      title={t("viewIssueTooltip", {
                        number: task.issue_number,
                        defaultValue: `在新窗口打开 Issue #${task.issue_number} (${task.repo})`,
                      })}
                      data-testid="pipeline-task-issue-link"
                    >
                      <FolderGit2 className="h-3 w-3 text-muted-foreground group-hover/issuelink:text-primary transition-colors shrink-0" />
                      <span className="truncate max-w-[200px]">
                        {formatIssueLabelTail(task.repo, task.issue_number ?? "")}
                      </span>
                      <ExternalLink className="h-2.5 w-2.5 opacity-40 group-hover/issuelink:opacity-100 transition-opacity shrink-0" />
                    </a>
                  ) : (
                    <span className="inline-flex items-center gap-1 rounded-md border border-border/80 bg-muted/60 px-2 py-0.5 font-mono text-xs font-medium text-foreground min-w-0 shrink truncate whitespace-nowrap">
                      <FolderGit2 className="h-3 w-3 text-muted-foreground shrink-0" />
                      <span className="truncate max-w-[200px]">
                        {formatIssueLabelTail(task.repo, task.issue_number ?? "")}
                      </span>
                    </span>
                  )}
                </div>

                {/* 2. 主体：Issue 标题、交付成果胶囊条与时间操作行（单行截断，绝不换行） */}
                <div className="space-y-2 min-w-0">
                  {/* 任务主标题 */}
                  <div className="min-w-0">
                    {issueUrl ? (
                      <a
                        href={issueUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-sm font-semibold text-foreground tracking-tight leading-snug truncate whitespace-nowrap block hover:text-primary transition-colors max-w-full"
                        title={task.title}
                        data-testid="pipeline-task-title-link"
                      >
                        {task.title}
                      </a>
                    ) : (
                      <h4
                        className="text-sm font-semibold text-foreground tracking-tight leading-snug truncate whitespace-nowrap block"
                        title={task.title}
                      >
                        {task.title}
                      </h4>
                    )}
                  </div>

                  {isPlanTask ? (
                    <div className="space-y-1.5 rounded-lg border border-border/50 bg-muted/20 p-2">
                      <div className="flex flex-wrap items-center gap-1.5 text-[10.5px] text-muted-foreground">
                        <span
                          className="inline-flex items-center gap-1 rounded-md border border-sky-500/20 bg-sky-500/10 px-1.5 py-0.5 font-mono font-semibold text-sky-700 dark:text-sky-300"
                          data-testid="plan-task-stage"
                          title={t("planTaskStage", "当前阶段")}
                        >
                          <Info className="h-3 w-3 shrink-0" />
                          {task.stage || task.status}
                        </span>
                        {task.base_commit || task.final_commit ? (
                          <span
                            className="inline-flex items-center gap-1 rounded-md border border-border/60 bg-background px-1.5 py-0.5 font-mono text-foreground"
                            data-testid="plan-task-commits"
                            title={t("planTaskCommits", "基线 / 最终提交")}
                          >
                            <GitBranch className="h-3 w-3 shrink-0" />
                            <span>
                              {task.base_commit
                                ? task.base_commit.slice(0, 7)
                                : "—"}
                              {" → "}
                              {task.final_commit
                                ? task.final_commit.slice(0, 7)
                                : "—"}
                            </span>
                          </span>
                        ) : null}
                      </div>

                      {questionText ? (
                        <p
                          className="break-words text-[10.5px] text-amber-700 dark:text-amber-300"
                          data-testid="plan-task-question"
                        >
                          {t("planTaskQuestion", "待回答")}: {questionText}
                        </p>
                      ) : null}

                      {decisionText ? (
                        <p
                          className="break-words text-[10.5px] text-emerald-700 dark:text-emerald-300"
                          data-testid="plan-task-decision"
                        >
                          {t("planTaskDecision", "自动决策")}: {decisionText}
                        </p>
                      ) : null}

                      {task.pr_url ? (
                        <a
                          href={task.pr_url}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="inline-flex max-w-full items-center gap-1 break-all text-[10.5px] font-medium text-primary hover:underline"
                          data-testid="plan-task-pr"
                          title={t("planTaskPr", "计划任务 PR")}
                        >
                          <GitPullRequest className="h-3 w-3 shrink-0" />
                          <span className="break-all">{task.pr_url}</span>
                        </a>
                      ) : null}

                      <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                        {planLive && onPlanAttach ? (
                          <button
                            type="button"
                            onClick={() => void onPlanAttach(task.id)}
                            className="inline-flex items-center gap-1 rounded-md border border-primary/30 bg-primary/10 px-2 py-1 text-[10.5px] font-medium text-primary transition-colors hover:bg-primary/20"
                            data-testid="plan-task-takeover"
                            title={t(
                              "planTaskTakeover",
                              "在原生终端接管该实时会话"
                            )}
                          >
                            <Hand className="h-3 w-3 shrink-0" />
                            {t("planTaskTakeoverLabel", "接管会话")}
                          </button>
                        ) : null}
                        {!planTerminal && onPlanCancel ? (
                          <button
                            type="button"
                            onClick={() => void onPlanCancel(task.id)}
                            className="inline-flex items-center gap-1 rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1 text-[10.5px] font-medium text-destructive transition-colors hover:bg-destructive/20"
                            data-testid="plan-task-cancel"
                            title={t("planTaskCancelTooltip", "取消该计划任务")}
                          >
                            <XCircle className="h-3 w-3 shrink-0" />
                            {t("planTaskCancel", "取消任务")}
                          </button>
                        ) : null}
                      </div>
                    </div>
                  ) : null}

                  {/* 交付成果胶囊条：PR、分支、变动量、作者（单行横向排布） */}
                  {(task.pr_url || task.branch || task.diff_stat || task.author) && (
                    <div className="flex items-center gap-1.5 text-xs overflow-x-auto no-scrollbar py-0.5 whitespace-nowrap min-w-0">
                      {/* PR 胶囊 */}
                      {task.pr_url ? (
                        <a
                          href={task.pr_url}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="inline-flex items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 font-medium text-emerald-600 dark:text-emerald-400 hover:bg-emerald-500/15 transition-colors shrink-0"
                          title={t("viewPullRequest", "查看 PR")}
                          data-testid="pipeline-task-pr-badge"
                        >
                          <GitPullRequest className="h-3 w-3 shrink-0" />
                          <span>{prNumber ? `PR ${prNumber}` : t("pullRequestLabel", "Pull Request")}</span>
                          <ExternalLink className="h-2.5 w-2.5 opacity-60 shrink-0" />
                        </a>
                      ) : null}

                      {/* 特性分支胶囊（支持一键复制） */}
                      {task.branch ? (
                        <div
                          className="inline-flex items-center gap-1 rounded-md border border-border/80 bg-muted/50 px-2 py-0.5 font-mono text-muted-foreground text-[11px] shrink-0"
                          title={t("branchLabel", "特性分支")}
                        >
                          <GitBranch className="h-3 w-3 text-primary shrink-0" />
                          <span className="max-w-[140px] truncate text-foreground font-medium">
                            {task.branch}
                          </span>
                          <button
                            type="button"
                            onClick={() => handleCopy(`${task.id}_branch`, task.branch!)}
                            className="text-muted-foreground hover:text-foreground transition-colors cursor-pointer ml-0.5 shrink-0"
                            title={t("copyBranch", "复制分支名")}
                            aria-label={t("copyBranch", "复制分支名")}
                          >
                            {copiedKey === `${task.id}_branch` ? (
                              <Check className="h-2.5 w-2.5 text-emerald-500" />
                            ) : (
                              <Copy className="h-2.5 w-2.5" />
                            )}
                          </button>
                        </div>
                      ) : null}

                      {/* 代码改动量统计 */}
                      {task.diff_stat ? (
                        <span
                          className="inline-flex items-center gap-1 rounded-md border border-border/60 bg-muted/40 px-1.5 py-0.5 text-muted-foreground font-mono text-[10.5px] shrink-0"
                          title={t("codeDiffStat", "代码变动统计")}
                        >
                          <FileDiff className="h-3 w-3 text-muted-foreground shrink-0" />
                          <span className="max-w-[160px] truncate">{task.diff_stat}</span>
                        </span>
                      ) : null}

                      {/* Issue 提出人 */}
                      {task.author ? (
                        <span
                          className="inline-flex items-center gap-1 rounded-md border border-border/50 bg-muted/30 px-1.5 py-0.5 text-muted-foreground text-[10.5px] shrink-0"
                          title={t("issueAuthor", "Issue 提出人")}
                        >
                          <User className="h-2.5 w-2.5 opacity-70 shrink-0" />
                          <span>@{task.author}</span>
                        </span>
                      ) : null}
                    </div>
                  )}

                  {/* 耗时与相对时间（居左）+ 终端现场按钮（居右，图标白色） */}
                  <div className="flex items-center justify-between gap-2 min-w-0 pt-0.5">
                    {/* 左侧：耗时与相对启动时间（文字居左） */}
                    <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground min-w-0 shrink truncate whitespace-nowrap">
                      {task.duration_seconds !== null && task.duration_seconds !== undefined ? (
                        <div className="flex items-center gap-1 text-foreground/80 shrink-0">
                          <Clock className="h-3 w-3 text-muted-foreground shrink-0" />
                          <span>
                            {t("friendlyDuration", {
                              duration: friendlyDuration,
                              defaultValue: `耗时 ${friendlyDuration}`,
                            })}
                          </span>
                        </div>
                      ) : null}
                      {task.duration_seconds !== null && task.duration_seconds !== undefined ? (
                        <span className="text-border shrink-0">·</span>
                      ) : null}
                      <span className="truncate">{startedAgo}</span>
                    </div>

                    {/* 右侧：终端现场按钮（按钮居右，白色 Terminal 图标） */}
                    {onLaunchTerminal && (
                      <button
                        type="button"
                        disabled={resumingTaskId === task.id}
                        onClick={() => handleLaunchTerminal(task)}
                        className="inline-flex items-center gap-1.5 rounded-md border border-slate-800/90 bg-slate-900 text-slate-100 hover:bg-slate-800 hover:text-white dark:border-emerald-500/40 dark:bg-emerald-950/70 dark:text-emerald-300 dark:hover:bg-emerald-900/80 px-2.5 py-1 text-xs font-semibold shadow-xs transition-all hover:scale-[1.02] active:scale-[0.98] cursor-pointer disabled:opacity-50 shrink-0 ml-auto whitespace-nowrap"
                        title={t("launchTerminalTooltip", "在原生终端中接入该分支或工作现场")}
                        data-testid="pipeline-task-terminal-btn"
                      >
                        {resumingTaskId === task.id ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin text-white shrink-0" />
                        ) : (
                          <Terminal className="h-3.5 w-3.5 text-white shrink-0" />
                        )}
                        <span>{t("connectTerminal", "终端现场")}</span>
                      </button>
                    )}
                  </div>
                </div>

                {/* 3. 迷你流水线节点流指示条（平均分配步骤之间的空白间隔） */}
                <div
                  className="rounded-lg border border-border/50 bg-muted/30 px-2.5 py-1.5"
                  data-testid="pipeline-steps-flow"
                >
                  <div className="flex items-center justify-between w-full overflow-x-auto pb-0.5 sm:pb-0">
                    {pipelineSteps.map((step, idx) => {
                      const isLast = idx === pipelineSteps.length - 1;
                      return (
                        <div
                          key={step.id}
                          className={`flex items-center ${isLast ? "shrink-0" : "flex-1 min-w-0"}`}
                        >
                          <div className="flex items-center gap-1 shrink-0">
                            {/* 节点图标 */}
                            {step.state === "completed" && (
                              <CheckCircle2 className="h-3 w-3 text-emerald-500 shrink-0" />
                            )}
                            {step.state === "current" && (
                              <Loader2 className="h-3 w-3 text-sky-500 animate-spin shrink-0" />
                            )}
                            {step.state === "failed" && (
                              <AlertCircle className="h-3 w-3 text-rose-500 shrink-0" />
                            )}
                            {step.state === "cancelled" && (
                              <XCircle className="h-3 w-3 text-muted-foreground shrink-0" />
                            )}
                            {step.state === "skipped" && (
                              <span className="h-3 w-3 rounded-full border border-amber-500/40 bg-amber-500/20 text-[8px] font-bold text-amber-500 flex items-center justify-center shrink-0">
                                -
                              </span>
                            )}
                            {step.state === "pending" && (
                              <span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/30 mx-0.5 shrink-0" />
                            )}

                            {/* 节点文字 */}
                            <span
                              className={`text-[10.5px] font-medium whitespace-nowrap ${
                                step.state === "completed"
                                  ? "text-foreground font-normal"
                                  : step.state === "current"
                                  ? "text-sky-600 dark:text-sky-400 font-semibold"
                                  : step.state === "failed"
                                  ? "text-rose-600 dark:text-rose-400 font-semibold"
                                  : step.state === "cancelled"
                                  ? "text-muted-foreground line-through"
                                  : step.state === "skipped"
                                  ? "text-amber-600 dark:text-amber-400"
                                  : "text-muted-foreground/60"
                              }`}
                            >
                              {step.label}
                            </span>
                          </div>

                          {/* 连接线：连接各节点并在步骤之间平均分配剩余空白 */}
                          {!isLast && (
                            <div
                              className={`h-[1px] flex-1 mx-2 transition-colors ${
                                step.state === "completed"
                                  ? "bg-emerald-500/40"
                                  : "bg-border/60"
                              }`}
                            />
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>

                {/* 4. 异常诊断区域：单行不换行，支持横向平滑滚动 */}
                {task.error_message && (
                  <div
                    className="rounded-lg border border-destructive/30 bg-destructive/5 p-2 text-xs text-destructive space-y-1.5 min-w-0"
                    data-testid="pipeline-task-error-box"
                  >
                    <div className="flex items-center justify-between gap-1.5 min-w-0 whitespace-nowrap">
                      <div className="flex items-center gap-1.5 min-w-0 flex-1 overflow-hidden">
                        <AlertCircle className="h-3.5 w-3.5 shrink-0 text-destructive" />
                        <span className="font-semibold text-[11px] shrink-0 whitespace-nowrap">{t("executionError", "异常")}</span>
                        <span className="font-mono truncate text-[11px] text-muted-foreground dark:text-destructive/80 min-w-0 flex-1 whitespace-nowrap">
                          {task.error_message.split("\n")[0]}
                        </span>
                      </div>

                      <div className="flex items-center gap-1 shrink-0 whitespace-nowrap">
                        <button
                          type="button"
                          onClick={() => handleCopy(`${task.id}_error`, task.error_message!)}
                          className="inline-flex items-center gap-1 rounded bg-destructive/10 hover:bg-destructive/20 px-1.5 py-0.5 text-[10px] font-medium text-destructive transition-colors cursor-pointer shrink-0 whitespace-nowrap"
                          title={t("copyErrorMessage", "复制完整错误")}
                        >
                          {copiedKey === `${task.id}_error` ? (
                            <>
                              <Check className="h-2.5 w-2.5 text-emerald-500" />
                              <span className="text-emerald-500">{t("copied", "已复制")}</span>
                            </>
                          ) : (
                            <>
                              <Copy className="h-2.5 w-2.5" />
                              <span>{t("copyError", "复制错误")}</span>
                            </>
                          )}
                        </button>

                        <button
                          type="button"
                          onClick={() => toggleErrorExpand(task.id)}
                          className="inline-flex items-center gap-0.5 rounded hover:bg-destructive/15 px-1 py-0.5 text-[10px] text-destructive transition-colors cursor-pointer shrink-0 whitespace-nowrap"
                          aria-expanded={isErrorExpanded}
                        >
                          <span>{isErrorExpanded ? t("collapse", "收起") : t("expand", "展开详情")}</span>
                          {isErrorExpanded ? (
                            <ChevronUp className="h-2.5 w-2.5" />
                          ) : (
                            <ChevronDown className="h-2.5 w-2.5" />
                          )}
                        </button>
                      </div>
                    </div>

                    {/* 展开的完整 Traceback：单行代码不软折行，支持水平滚动 */}
                    {isErrorExpanded && (
                      <div
                        className="mt-1.5 max-h-48 overflow-y-auto overflow-x-auto rounded-md bg-slate-950 p-2.5 font-mono text-[10.5px] text-slate-200 whitespace-pre select-text border border-slate-800 leading-relaxed"
                        data-testid="pipeline-task-error-expanded"
                      >
                        {task.error_message}
                      </div>
                    )}
                  </div>
                )}
              </div>

              {/* 5. 底栏：简明检出命令与查看 PR 操作（已移除重复的查看 Issue 与迁移终端现场） */}
              {hasFooter && (
                <div className="flex items-center justify-between gap-2 pt-2 border-t border-border/40 text-xs min-w-0 whitespace-nowrap">
                  {/* 左侧：本地 Git 检出命令快速复制 */}
                  {checkoutCmd ? (
                    <div className="flex items-center gap-1 min-w-0 flex-1 overflow-hidden">
                      <span className="text-muted-foreground font-mono select-none text-[10.5px] shrink-0">$</span>
                      <button
                        type="button"
                        onClick={() => handleCopy(`${task.id}_checkout`, checkoutCmd)}
                        className="group/checkout flex items-center gap-1.5 rounded-md border border-border/80 bg-muted/40 hover:bg-muted px-2 py-0.5 font-mono text-[10.5px] text-muted-foreground hover:text-foreground transition-all cursor-pointer truncate max-w-full"
                        title={t("clickToCopyCheckout", "点击复制完整本地检出命令")}
                        data-testid="pipeline-task-copy-checkout"
                      >
                        <span className="truncate">git checkout {task.branch}</span>
                        {copiedKey === `${task.id}_checkout` ? (
                          <Check className="h-2.5 w-2.5 text-emerald-500 shrink-0" />
                        ) : (
                          <Copy className="h-2.5 w-2.5 opacity-60 group-hover/checkout:opacity-100 shrink-0" />
                        )}
                      </button>
                    </div>
                  ) : (
                    <div />
                  )}

                  {/* 右侧：查看 PR 按钮 */}
                  {task.pr_url && (
                    <a
                      href={task.pr_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-xs font-medium text-primary hover:bg-primary/20 transition-colors shadow-2xs shrink-0 whitespace-nowrap ml-auto"
                    >
                      <GitPullRequest className="h-3 w-3 shrink-0" />
                      <span>{t("viewPullRequest", "查看 PR")}</span>
                      <ExternalLink className="h-2.5 w-2.5 opacity-70 shrink-0" />
                    </a>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
