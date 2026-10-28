import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useToast } from "../ToastProvider";
import { CloudAgentHeader } from "./components/CloudAgentHeader";
import { ActiveSessionsTab } from "./components/ActiveSessionsTab";
import { WorktreesHistoryTab } from "./components/WorktreesHistoryTab";
import { PrPipelineTab } from "./components/PrPipelineTab";
import { GatewayPanel } from "./components/GatewayPanel";
import { NewSessionModal } from "./components/NewSessionModal";
import { useCloudAgent } from "./hooks/useCloudAgent";

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

  return (
    <div className="flex h-full flex-col bg-slate-50 text-slate-800 dark:bg-slate-950 dark:text-slate-100">
      {/* 顶部主工具栏与状态仪表 */}
      <CloudAgentHeader
        t={t}
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        selectedHost={selectedHost}
        setSelectedHost={setSelectedHost}
        status={status}
        settings={settings}
        sessionsCount={sessions.length}
        worktreesCount={worktrees.length}
        tasksCount={tasks.length}
        loading={loading}
        onRefresh={() => refreshAll()}
        onOpenNewTaskModal={() => setShowNewTaskModal(true)}
        onSyncGateway={() => syncGateway()}
      />

      {/* 主工作区 */}
      <div className="flex-1 overflow-y-auto p-6">
        <div className="mx-auto max-w-6xl">
          {activeTab === "sessions" && (
            <ActiveSessionsTab
              t={t}
              language={i18n.language}
              sessions={sessions}
              loading={loading}
              onLaunchTerminal={(sName) => launchTerminal(sName)}
              onKillSession={(sName) => killSession(sName)}
              onOpenNewTaskModal={() => setShowNewTaskModal(true)}
            />
          )}

          {activeTab === "worktrees" && (
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
          )}

          {activeTab === "pipeline" && (
            <PrPipelineTab
              t={t}
              language={i18n.language}
              tasks={tasks}
              loading={loading}
              onRefresh={() => refreshAll()}
            />
          )}

          {activeTab === "gateway" && (
            <GatewayPanel
              t={t}
              status={status}
              loading={loading}
              onSyncGateway={() => syncGateway()}
            />
          )}
        </div>
      </div>

      {/* 新建特性任务弹窗 */}
      <NewSessionModal
        t={t}
        settings={settings}
        isOpen={showNewTaskModal}
        onClose={() => setShowNewTaskModal(false)}
        onSubmit={createAndLaunchSession}
      />
    </div>
  );
}
