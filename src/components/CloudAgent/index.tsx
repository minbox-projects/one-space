import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Terminal, GitBranch, GitPullRequest, Network, Server, Settings } from "lucide-react";
import { useToast } from "../ToastProvider";
import { CloudAgentHeader } from "./components/CloudAgentHeader";
import { CloudAgentRuntimeCard } from "./components/CloudAgentRuntimeCard";
import { ActiveSessionsTab } from "./components/ActiveSessionsTab";
import { WorktreesHistoryTab } from "./components/WorktreesHistoryTab";
import { PrPipelineTab } from "./components/PrPipelineTab";
import { GatewayPanel } from "./components/GatewayPanel";
import { NewSessionDialog } from "./components/NewSessionDialog";
import { CloudEmptyState } from "./components/CloudEmptyState";
import { useCloudAgent, type CloudAgentTab } from "./hooks/useCloudAgent";

export function CloudAgent({ isVisible = true }: { isVisible?: boolean }) {
  const { t, i18n } = useTranslation();
  const { pushToast } = useToast();
  const notify = useCallback(
    (type: "success" | "error", message: string) => {
      pushToast({ kind: type, title: message });
    },
    [pushToast]
  );
  const controller = useCloudAgent(notify);

  const {
    activeTab,
    setActiveTab,
    settings,
    selectedHost,
    setSelectedHost,
    status,
    sessions,
    worktrees,
    tasks,
    loading,
    refreshAll,
    launchTerminal,
    killSession,
    syncGateway,
    createAndLaunchSession,
    showNewTaskModal,
    setShowNewTaskModal,
  } = controller;

  if (!isVisible) return null;

  const tabs: Array<{
    id: CloudAgentTab;
    label: string;
    icon: typeof Terminal;
    count?: number;
    hasAlert?: boolean;
  }> = [
    {
      id: "sessions",
      label: t("activeSessionsTab", "活跃会话"),
      icon: Terminal,
      count: sessions.length,
    },
    {
      id: "worktrees",
      label: t("worktreesHistoryTab", "历史工作区"),
      icon: GitBranch,
      count: worktrees.length,
    },
    {
      id: "pipeline",
      label: t("prPipelineTab", "Issues 流水线"),
      icon: GitPullRequest,
      count: tasks.length,
      hasAlert: tasks.some((task) => task.status === "failed"),
    },
    {
      id: "gateway",
      label: t("cloudGatewayTab", "网关同步"),
      icon: Network,
      count: status?.gateway?.providers?.length,
      hasAlert:
        Boolean(status?.gateway?.status) &&
        status?.gateway?.status !== "healthy" &&
        status?.gateway?.status !== "ok",
    },
  ];

  return (
    <div
      className="h-full overflow-y-auto bg-background text-foreground"
      data-testid="cloud-agent-console"
    >
      <div className="mx-auto max-w-7xl space-y-4 p-6">
        {/* 顶部紧凑标题与主机选择栏 */}
        <CloudAgentHeader
          t={t}
          selectedHost={selectedHost}
          setSelectedHost={setSelectedHost}
        />

        {/* 全局常驻云端运行时状态卡片（对齐 AI 网关 RuntimeStatusCard 风格） */}
        <CloudAgentRuntimeCard
          t={t}
          status={status}
          settings={settings}
          sessions={sessions}
          worktrees={worktrees}
          tasks={tasks}
          loading={loading}
          selectedHost={selectedHost}
          onRefresh={() => void refreshAll()}
          onOpenNewTaskModal={() => setShowNewTaskModal(true)}
          onSelectTab={setActiveTab}
        />

        {/* 工作区 Tabs 标签页导航（对齐 AI 网关标签页设计） */}
        <div
          role="tablist"
          aria-label={t("cloudAgentWorkspaceTabs", "云端智能体工作区标签")}
          className="flex flex-wrap items-center gap-1.5 rounded-xl border border-border bg-muted/50 p-1.5 shadow-xs"
        >
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => setActiveTab(tab.id)}
                className={`group inline-flex h-9 items-center gap-2 rounded-lg px-3.5 text-xs font-medium transition-all cursor-pointer ${
                  isActive
                    ? "bg-background text-foreground font-semibold shadow-xs ring-1 ring-border/80 dark:ring-border"
                    : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
                }`}
              >
                <Icon
                  className={`h-4 w-4 transition-colors ${
                    isActive
                      ? "text-primary"
                      : "text-muted-foreground/70 group-hover:text-foreground"
                  }`}
                />
                <span>{tab.label}</span>
                {tab.count !== undefined ? (
                  <span
                    className={`rounded-full px-2 py-0.5 text-[11px] font-semibold transition-colors ${
                      isActive
                        ? "bg-primary/10 text-primary"
                        : "bg-muted text-muted-foreground group-hover:bg-muted/80 group-hover:text-foreground"
                    }`}
                  >
                    {tab.count}
                  </span>
                ) : null}
                {tab.hasAlert ? (
                  <span
                    className="relative flex h-2 w-2"
                    title={t("cloudAgentHasAlert", "存在需关注项")}
                  >
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
                    <span className="relative inline-flex h-2 w-2 rounded-full bg-amber-500" />
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>

        {/* 标签页内容呈现 */}
        {!selectedHost ? (
          <div className="py-6">
            <CloudEmptyState
              icon={Server}
              title={t("noHostSelectedTitle", "尚未选择或配置云端主机")}
              description={t(
                "noHostSelectedDesc",
                "请在上方右上角选择目标云端 SSH 主机，或进入设置面板配置默认主机。"
              )}
              action={{
                label: t("goToSettings", "前往设置"),
                icon: Settings,
                onClick: () => {
                  const win = window as any;
                  win.setSettingsTab?.("cloud-agent");
                  win.setSettingsOpen?.(true);
                },
              }}
            />
          </div>
        ) : (
          <>
            {/* Tab 1: 活跃会话 */}
            {activeTab === "sessions" && (
              <div
                role="tabpanel"
                aria-label={t("activeSessionsTab", "活跃会话")}
              >
                <ActiveSessionsTab
                  t={t}
                  language={i18n.language}
                  sessions={sessions}
                  loading={loading}
                  onLaunchTerminal={(sName) => launchTerminal(sName)}
                  onKillSession={(sName) => killSession(sName)}
                  onOpenNewTaskModal={() => setShowNewTaskModal(true)}
                />
              </div>
            )}

            {/* Tab 2: 历史工作区 */}
            {activeTab === "worktrees" && (
              <div
                role="tabpanel"
                aria-label={t("worktreesHistoryTab", "历史工作区")}
              >
                <WorktreesHistoryTab
                  t={t}
                  language={i18n.language}
                  worktrees={worktrees}
                  loading={loading}
                  onLaunchTerminal={(target, branch) =>
                    launchTerminal(target, branch)
                  }
                  onOpenNewTaskModal={() => setShowNewTaskModal(true)}
                />
              </div>
            )}

            {/* Tab 3: Issues 流水线 */}
            {activeTab === "pipeline" && (
              <div
                role="tabpanel"
                aria-label={t("prPipelineTab", "Issues 流水线")}
              >
                <PrPipelineTab
                  t={t}
                  language={i18n.language}
                  tasks={tasks}
                  loading={loading}
                  onRefresh={() => refreshAll()}
                  onLaunchTerminal={(target, branch) =>
                    launchTerminal(target, branch)
                  }
                />
              </div>
            )}

            {/* Tab 4: 网关同步 */}
            {activeTab === "gateway" && (
              <div
                role="tabpanel"
                aria-label={t("cloudGatewayTab", "网关同步")}
              >
                <GatewayPanel
                  t={t}
                  status={status}
                  loading={loading}
                  onSyncGateway={() => syncGateway()}
                />
              </div>
            )}
          </>
        )}
      </div>

      {/* 新建特性任务标准 Radix UI 弹窗 */}
      <NewSessionDialog
        t={t}
        settings={settings}
        isOpen={showNewTaskModal}
        onClose={() => setShowNewTaskModal(false)}
        onSubmit={createAndLaunchSession}
      />
    </div>
  );
}
