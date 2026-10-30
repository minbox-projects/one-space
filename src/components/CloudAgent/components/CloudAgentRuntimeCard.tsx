import { useMemo } from "react";
import {
  Activity,
  TerminalSquare,
  Layers,
  FolderGit2,
  HardDrive,
  RefreshCw,
  Sparkles,
  Cpu,
  GitBranch,
} from "lucide-react";
import type { TFunction } from "i18next";
import type {
  CloudAgentStatus,
  CloudAgentSettings,
  CloudSession,
  CloudWorktree,
  CloudAgentTask,
} from "@/lib/cloudAgent";
import type { CloudAgentTab } from "../hooks/useCloudAgent";

interface CloudAgentRuntimeCardProps {
  t: TFunction;
  status: CloudAgentStatus | null;
  settings: CloudAgentSettings;
  sessions: CloudSession[];
  worktrees: CloudWorktree[];
  tasks: CloudAgentTask[];
  loading: boolean;
  selectedHost?: string;
  onRefresh: () => void;
  onOpenNewTaskModal: () => void;
  onSelectTab: (tab: CloudAgentTab) => void;
}

export function CloudAgentRuntimeCard({
  t,
  status,
  settings,
  sessions,
  worktrees,
  loading,
  selectedHost,
  onRefresh,
  onOpenNewTaskModal,
  onSelectTab,
}: CloudAgentRuntimeCardProps) {
  const disk = status?.disk;
  const isDiskAlert =
    disk && disk.use_percent >= (settings.diskAlertThresholdPercent || 80);

  const gatewayHealthy =
    status?.gateway?.status === "healthy" || status?.gateway?.status === "ok";
  const gatewayPort = status?.gateway?.port || 17688;

  // 4 组核心统计指标计算
  const stats = useMemo(() => {
    const totalSessions = sessions.length;
    const activeCount = sessions.filter((s) => s.is_active !== false).length;
    const totalWindows = sessions.reduce(
      (acc, s) => acc + (s.windows_count && s.windows_count > 0 ? s.windows_count : 1),
      0
    );
    const worktreeCount = worktrees.length;
    const repoSet = new Set<string>();
    sessions.forEach((s) => {
      if (s.repo_name) repoSet.add(s.repo_name);
    });
    worktrees.forEach((w) => {
      if (w.repo_name) repoSet.add(w.repo_name);
    });
    const repoCount = repoSet.size;

    return {
      totalSessions,
      activeCount,
      totalWindows,
      worktreeCount,
      repoCount,
    };
  }, [sessions, worktrees]);

  return (
    <section
      className="space-y-3 rounded-xl border border-border bg-card p-4 shadow-sm"
      data-testid="cloud-agent-runtime-card"
    >
      {/* 顶部单行聚合栏：守护进程状态 + 云端网关胶囊 + 磁盘空间 + 刷新与新建任务控制 */}
      <div
        className="flex flex-wrap items-center justify-between gap-2.5"
        aria-label={t("cloudAgentRuntimeBar", "云端智能体运行时状态")}
      >
        {/* 左侧：服务状态与端点胶囊 */}
        <div className="flex flex-wrap items-center gap-2">
          {/* 守护进程运行状态指示灯与状态文字 */}
          <div className="flex items-center gap-1.5 shrink-0">
            <span className="relative flex h-2 w-2">
              {!selectedHost ? (
                <span className="relative inline-flex h-2 w-2 rounded-full bg-muted-foreground/40" />
              ) : status?.service_active ? (
                <>
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
                </>
              ) : (
                <span className="relative inline-flex h-2 w-2 rounded-full bg-rose-500" />
              )}
            </span>
            <span
              className={`text-xs font-semibold leading-none ${
                !selectedHost
                  ? "text-muted-foreground"
                  : status?.service_active
                  ? "text-emerald-600 dark:text-emerald-400"
                  : "text-rose-500"
              }`}
              data-testid="cloud-agent-daemon-state"
            >
              {!selectedHost
                ? t("daemonNoHost", "未配置云端主机")
                : status?.service_active
                ? t("daemonActive", "运行中")
                : t("daemonInactive", "离线")}
            </span>
          </div>

          <span className="text-muted-foreground/30 text-xs hidden sm:inline">·</span>

          {/* 云端 AI 网关状态胶囊 */}
          <div className="flex items-center gap-1.5 rounded-md border border-border/80 bg-muted/40 px-2 py-1 text-xs">
            <span className="shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
              Cloud Gateway
            </span>
            <div className="flex items-center gap-1.5 text-[11px]">
              <span
                className={`h-1.5 w-1.5 rounded-full shrink-0 ${
                  !selectedHost
                    ? "bg-muted-foreground/40"
                    : gatewayHealthy
                    ? "bg-emerald-500"
                    : "bg-amber-500"
                }`}
              />
              <span className="font-mono text-foreground/80">
                Port {gatewayPort}
              </span>
              <span className="text-muted-foreground/30">·</span>
              <span
                className={`font-medium ${
                  !selectedHost
                    ? "text-muted-foreground"
                    : gatewayHealthy
                    ? "text-emerald-600 dark:text-emerald-400"
                    : "text-amber-500"
                }`}
              >
                {!selectedHost
                  ? t("daemonNoHost", "未配置云端主机")
                  : gatewayHealthy
                  ? t("healthy", "在线")
                  : t("offlineOrStarting", "离线或启动中")}
              </span>
            </div>
          </div>

          {/* 云端磁盘空间胶囊 */}
          {disk && (
            <div className="flex items-center gap-2 rounded-lg border border-border/80 bg-muted/40 px-2.5 py-1 text-xs">
              <HardDrive
                className={`h-3.5 w-3.5 ${
                  isDiskAlert ? "text-amber-500" : "text-muted-foreground"
                }`}
              />
              <span
                className={`text-[11px] font-mono ${
                  isDiskAlert
                    ? "font-semibold text-amber-600 dark:text-amber-400"
                    : "text-muted-foreground"
                }`}
              >
                {disk.used_gb}G / {disk.total_gb}G ({disk.use_percent}%)
              </span>
              <div className="h-1.5 w-14 overflow-hidden rounded-full bg-muted">
                <div
                  className={`h-full rounded-full transition-all duration-300 ${
                    isDiskAlert ? "bg-amber-500" : "bg-emerald-500"
                  }`}
                  style={{ width: `${Math.min(100, disk.use_percent)}%` }}
                />
              </div>
            </div>
          )}
        </div>

        {/* 右侧：刷新数据与新建特性任务按钮 */}
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            disabled={loading}
            onClick={onRefresh}
            title={t("refresh", "刷新")}
            aria-label={t("refresh", "刷新")}
            data-testid="cloud-agent-refresh-btn"
            className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-border/80 bg-background text-muted-foreground shadow-2xs transition hover:bg-muted hover:text-foreground active:scale-98 disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
          </button>

          <button
            type="button"
            onClick={onOpenNewTaskModal}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-2xs transition hover:bg-primary/90 active:scale-98 cursor-pointer"
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span>{t("newFeatureTaskBtn", "新建特性任务")}</span>
          </button>
        </div>
      </div>

      {/* 4 组核心数据指标卡：活跃会话、终端作业窗口、特性隔离空间、涉及代码仓库 */}
      <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
        {/* 指标卡 1：活跃会话 */}
        <div
          data-testid="cloud-agent-metric-sessions"
          onClick={() => onSelectTab("sessions")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onSelectTab("sessions");
            }
          }}
          className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium group-hover:text-foreground">
              {t("activeSessionsMetricTitle", "活跃会话")}
            </span>
            <Activity className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground transition-colors" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {stats.activeCount}
            </span>
            <span className="text-xs text-muted-foreground">
              / {stats.totalSessions}
            </span>
          </div>
          <div className="mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground truncate">
            {stats.activeCount > 0 ? (
              <span className="inline-flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400 font-medium truncate">
                <span className="relative flex h-1.5 w-1.5">
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-emerald-500" />
                </span>
                <span className="truncate">{t("activeSessionsMetricDesc", "守护进程保活运行中")}</span>
              </span>
            ) : (
              <span className="truncate">{t("activeSessionsMetricIdle", "暂无活跃中的开发会话")}</span>
            )}
          </div>
        </div>

        {/* 指标卡 2：终端作业窗口 */}
        <div
          data-testid="cloud-agent-metric-windows"
          onClick={() => onSelectTab("sessions")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onSelectTab("sessions");
            }
          }}
          className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium group-hover:text-foreground">
              {t("terminalWindowsMetricTitle", "终端作业窗口")}
            </span>
            <TerminalSquare className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground transition-colors" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {stats.totalWindows}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("windowsUnit", "个窗口")}
            </span>
          </div>
          <div className="mt-1 flex items-center gap-1 text-[11px] text-muted-foreground truncate">
            <Cpu className="h-3 w-3 shrink-0 text-muted-foreground/70" />
            <span className="truncate">{t("terminalWindowsMetricDesc", "窗口协同运行中")}</span>
          </div>
        </div>

        {/* 指标卡 3：特性隔离空间 */}
        <div
          data-testid="cloud-agent-metric-worktrees"
          onClick={() => onSelectTab("worktrees")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onSelectTab("worktrees");
            }
          }}
          className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium group-hover:text-foreground">
              {t("isolatedWorktreesMetricTitle", "特性隔离空间")}
            </span>
            <Layers className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground transition-colors" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {stats.worktreeCount}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("worktreesUnit", "个工作区")}
            </span>
          </div>
          <div className="mt-1 flex items-center gap-1 text-[11px] text-muted-foreground truncate">
            <GitBranch className="h-3 w-3 shrink-0 text-muted-foreground/70" />
            <span className="truncate">{t("isolatedWorktreesMetricDesc", "Git 独立目录隔离")}</span>
          </div>
        </div>

        {/* 指标卡 4：涉及代码仓库 */}
        <div
          data-testid="cloud-agent-metric-repos"
          onClick={() => onSelectTab("worktrees")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onSelectTab("worktrees");
            }
          }}
          className="group flex flex-col justify-between rounded-lg border border-border/60 bg-muted/20 p-3 transition-colors hover:border-border hover:bg-muted/30 cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <div className="flex items-center justify-between text-muted-foreground">
            <span className="text-[11px] font-medium group-hover:text-foreground">
              {t("reposMetricTitle", "涉及代码仓库")}
            </span>
            <FolderGit2 className="h-3.5 w-3.5 text-muted-foreground/70 group-hover:text-foreground transition-colors" />
          </div>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-lg font-bold tracking-tight text-foreground">
              {stats.repoCount}
            </span>
            <span className="text-xs text-muted-foreground">
              {t("reposUnit", "个项目")}
            </span>
          </div>
          <div className="mt-1 flex items-center gap-1 text-[11px] text-muted-foreground truncate">
            <Sparkles className="h-3 w-3 shrink-0 text-muted-foreground/70" />
            <span className="truncate">{t("reposMetricDesc", "多工程并行推进")}</span>
          </div>
        </div>
      </div>
    </section>
  );
}
