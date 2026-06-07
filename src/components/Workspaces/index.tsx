import { emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, Plus, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "../ConfirmDialogProvider";
import { TerminalPermissionConfirmDialog } from "../TerminalPermissionConfirmDialog";
import { useToast } from "../ToastProvider";
import type {
  CapabilityTargetTab,
  WorkspaceCapabilityContext,
  WorkspaceCapabilityEntry,
} from "../workspaceCapabilityContext";
import { WorkspaceCopyDialog } from "./components/WorkspaceCopyDialog";
import { WorkspaceDetailHeader } from "./components/WorkspaceDetailHeader";
import { WorkspaceFormDialog } from "./components/WorkspaceFormDialog";
import { WorkspaceLaunchDialog } from "./components/WorkspaceLaunchDialog";
import { WorkspaceListSection } from "./components/WorkspaceListSection";
import { WorkspaceMcpDialog } from "./components/WorkspaceMcpDialog";
import { WorkspaceMcpTab } from "./components/WorkspaceMcpTab";
import { WorkspaceSessionsTab } from "./components/WorkspaceSessionsTab";
import { useWorkspaceCollection } from "./hooks/useWorkspaceCollection";
import { useWorkspaceDetail } from "./hooks/useWorkspaceDetail";
import { useWorkspaceDialogs } from "./hooks/useWorkspaceDialogs";
import { useWorkspaceMcpTab } from "./hooks/useWorkspaceMcpTab";
import { WorkspaceSkillsPanel } from "./WorkspaceSkillsPanel";
import { WorkspaceSubagentsPanel } from "./WorkspaceSubagentsPanel";
import {
  formatInvokeError,
  getInvokeErrorCode,
  type AiModelId as PermAiModelId,
  type TerminalPermissionMode,
} from "@/lib/terminalPermissions";
import { getSourceBadgeDescription, getSourceBadgeLabel, getSourceBadgeTranslationKeys } from "./helpers/workspaceHelpers";
import type { WorkspaceRecord, WorkspaceTab } from "./types";
import type { AiSessionListItem } from "../AiSessionsList";

function createWorkspaceCapabilityContext(
  workspace: WorkspaceRecord,
  entry: WorkspaceCapabilityEntry,
): WorkspaceCapabilityContext {
  return {
    workspaceId: workspace.id,
    workspaceName: workspace.name,
    rootPath: workspace.root_path,
    persistence: "one_shot",
    entry,
  };
}

export function Workspaces({
  isVisible = false,
  onNavigateToCapability,
}: {
  isVisible?: boolean;
  onNavigateToCapability?: (
    targetTab: CapabilityTargetTab,
    context: WorkspaceCapabilityContext,
  ) => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const { pushToast } = useToast();
  const isTauri = "__TAURI_INTERNALS__" in window;
  const [activeTab, setActiveTab] = useState<WorkspaceTab>("sessions");
  const [message, setMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [permissionDialogOpen, setPermissionDialogOpen] = useState(false);
  const [permissionDialogSession, setPermissionDialogSession] = useState<AiSessionListItem | null>(null);

  const detail = useWorkspaceDetail({
    isVisible,
    isTauri,
    activeTab,
    setMessage,
  });

  const collection = useWorkspaceCollection({
    isVisible,
    isTauri,
    activeWorkspaceId: detail.activeWorkspaceId,
    onActiveWorkspaceRemoved: detail.clearActiveWorkspace,
    onRefreshActiveWorkspace: detail.refreshActiveWorkspace,
    setMessage,
  });

  const mcpTab = useWorkspaceMcpTab({
    isVisible,
    isTauri,
    activeTab,
    activeWorkspaceId: detail.activeWorkspaceId,
    activeWorkspace: detail.activeDetail?.workspace.workspace || null,
    activeDetail: detail.activeDetail,
    setActiveDetail: (next) => detail.setActiveDetail(next),
    setMessage,
    loadWorkspaces: collection.loadWorkspaces,
  });

  const activeWorkspace = detail.activeDetail?.workspace.workspace || null;

  const dialogs = useWorkspaceDialogs({
    t,
    isTauri,
    confirmDialog,
    pushToast,
    loadWorkspaces: collection.loadWorkspaces,
    loadWorkspaceDetail: detail.loadWorkspaceDetail,
    refreshActiveWorkspace: detail.refreshActiveWorkspace,
    loadMcpServers: () => mcpTab.loadMcpServers(),
    activeWorkspaceRootPath: activeWorkspace?.root_path,
    activeWorkspaceId: detail.activeWorkspaceId,
    setActiveWorkspaceId: detail.setActiveWorkspaceId,
    setActiveDetail: detail.setActiveDetail,
    setActiveSessions: detail.setActiveSessions,
    setMessage,
  });

  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => setMessage(null), 3000);
    return () => window.clearTimeout(timer);
  }, [message]);

  const loadPermissionConfig = useCallback(async () => {
    if (!isTauri) return;
    try {
      await invoke<Record<string, unknown>>("get_storage_config");
    } catch (e) {
      console.error("Failed to load permission config", e);
    }
  }, [isTauri]);

  useEffect(() => {
    if (!isVisible) return;
    void loadPermissionConfig();
  }, [isVisible, loadPermissionConfig]);

  const navigateToCapability = useCallback(
    (targetTab: CapabilityTargetTab, workspace: WorkspaceRecord, entry: WorkspaceCapabilityEntry) => {
      onNavigateToCapability?.(targetTab, createWorkspaceCapabilityContext(workspace, entry));
    },
    [onNavigateToCapability],
  );

  const handleSelectTab = useCallback((nextTab: WorkspaceTab) => {
    if (nextTab === "sessions") {
      detail.setSessionsLoading(true);
    }
    if (nextTab === "mcp") {
      mcpTab.setMcpLoading(true);
    }
    setActiveTab(nextTab);
  }, [detail, mcpTab]);

  const handleWorkspaceSessionDelete = useCallback(async (sessionId: string) => {
    if (!isTauri) return;
    try {
      await invoke("sessions_delete", { sessionId });
      emit("refresh-counts").catch(() => {});
      await Promise.all([collection.loadWorkspaces(), detail.refreshActiveWorkspace()]);
    } catch (e: any) {
      setMessage({
        type: "error",
        text: t("workspaceSessionDeleteFailed", "Failed to delete session: {{message}}", {
          message: String(e),
        }),
      });
    }
  }, [collection, detail, isTauri, t]);

  const handleWorkspaceSessionRename = useCallback(async (session: AiSessionListItem, nextName: string) => {
    if (!isTauri) return;
    try {
      await invoke("sessions_update", {
        session: {
          id: session.id,
          name: nextName.trim(),
          working_dir: session.working_dir,
          tool: session.model_type,
        },
      });
      await Promise.all([collection.loadWorkspaces(), detail.refreshActiveWorkspace()]);
    } catch (e: any) {
      setMessage({
        type: "error",
        text: t("workspaceSessionRenameFailed", "Failed to rename session: {{message}}", {
          message: String(e),
        }),
      });
    }
  }, [collection, detail, isTauri, t]);

  const handleWorkspaceSessionLaunch = useCallback(async (session: AiSessionListItem) => {
    if (!isTauri) return;
    try {
      await invoke("sessions_launch", { sessionId: session.id });
      await detail.refreshActiveWorkspace();
    } catch (e: unknown) {
      const code = getInvokeErrorCode(e);
      if (code === "PERMISSION_CONFIRMATION_REQUIRED") {
        setPermissionDialogSession(session);
        setPermissionDialogOpen(true);
      } else {
        setMessage({
          type: "error",
          text: t("workspaceSessionLaunchFailed", "Failed to launch session: {{message}}", {
            message: formatInvokeError(e),
          }),
        });
      }
    }
  }, [collection, detail, isTauri, t]);

  const handleWorkspacePermissionConfirm = useCallback(async (mode: TerminalPermissionMode) => {
    if (!permissionDialogSession) return;
    setPermissionDialogOpen(false);
    const session = permissionDialogSession;
    setPermissionDialogSession(null);
    try {
      await invoke("sessions_launch", { sessionId: session.id, permissionMode: mode });
      await detail.refreshActiveWorkspace();
    } catch (e: unknown) {
      setMessage({
        type: "error",
        text: t("workspaceSessionLaunchFailed", "Failed to launch session: {{message}}", {
          message: formatInvokeError(e),
        }),
      });
    }
  }, [collection, detail, permissionDialogSession, t]);

  const handleWorkspacePermissionCancel = useCallback(() => {
    setPermissionDialogOpen(false);
    setPermissionDialogSession(null);
  }, []);

  const handleWorkspaceSessionFavoriteChange = useCallback(async (session: AiSessionListItem, favorite: boolean) => {
    if (!isTauri) return;
    try {
      await invoke("sessions_set_favorite", { sessionId: session.id, favorite });
      await Promise.all([collection.loadWorkspaces(), detail.refreshActiveWorkspace()]);
    } catch (e: unknown) {
      setMessage({
        type: "error",
        text: t("workspaceSessionFavoriteFailed", "Failed to update favorite: {{message}}", {
          message: formatInvokeError(e),
        }),
      });
    }
  }, [collection, detail, isTauri, t]);

  const activeWorkspaceSourceBadgeKeys = activeWorkspace ? getSourceBadgeTranslationKeys(activeWorkspace.source) : null;
  const activeWorkspaceSourceBadgeLabel = activeWorkspace && activeWorkspaceSourceBadgeKeys
    ? t(activeWorkspaceSourceBadgeKeys.label, getSourceBadgeLabel(activeWorkspace.source))
    : "";
  const activeWorkspaceSourceBadgeDescription = activeWorkspace && activeWorkspaceSourceBadgeKeys
    ? t(activeWorkspaceSourceBadgeKeys.description, getSourceBadgeDescription(activeWorkspace.source))
    : "";

  if (!isTauri) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t("notInTauri", "This feature is only available in the desktop app.")}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-bold tracking-tight">{t("workspaces", "Workspaces")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {activeWorkspace
              ? t(
                  "workspaceDetailDesc",
                  "Review {{name}} directory, metadata, terminal sessions, and installed project capabilities in one place.",
                  { name: activeWorkspace.name },
                )
              : t(
                  "workspaceListDesc",
                  "Use workspaces to organize each local project folder together with its sessions, MCP, Skills, and Subagents.",
                )}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2 md:justify-end">
          {message && (
            <div
              className={`rounded-md border px-2.5 py-1.5 text-xs ${
                message.type === "error"
                  ? "border-destructive/20 bg-destructive/10 text-destructive"
                  : "border-green-500/20 bg-green-500/10 text-green-700"
              }`}
            >
              {message.text}
            </div>
          )}
          <button
            type="button"
            onClick={() => {
              void Promise.all([
                collection.loadWorkspaces(),
                detail.activeWorkspaceId ? detail.refreshActiveWorkspace() : Promise.resolve(),
              ]);
            }}
            className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted"
          >
            {collection.loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
            {t("refresh", "Refresh")}
          </button>
          <button
            type="button"
            onClick={dialogs.openCreateDialog}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
          >
            <Plus className="h-4 w-4" />
            {t("workspaceCreate", "New Workspace")}
          </button>
        </div>
      </div>

      {!activeWorkspace ? (
        <WorkspaceListSection
          t={t}
          loading={collection.loading}
          workspacesInitialized={collection.workspacesInitialized}
          workspaces={collection.workspaces}
          allTags={collection.allTags}
          selectedTags={collection.selectedTags}
          selectedWorkspaceTags={collection.selectedWorkspaceTags}
          visibleWorkspaces={collection.visibleWorkspaces}
          onClearTags={() => collection.setSelectedTags([])}
          onToggleTag={collection.toggleTagFilter}
          onSelectWorkspace={(workspaceId, view) => {
            setActiveTab("sessions");
            void detail.loadWorkspaceDetail(workspaceId, view);
          }}
          onEditWorkspace={dialogs.openEditDialog}
          onCopyWorkspace={(workspace) => {
            void dialogs.openCopyDialog(workspace);
          }}
          onDeleteWorkspace={(workspace) => {
            void dialogs.handleDeleteWorkspace(workspace);
          }}
          onLaunchWorkspace={dialogs.openLaunchDialog}
        />
      ) : (
        <>
          <WorkspaceDetailHeader
            t={t}
            activeWorkspace={activeWorkspace}
            activeDetail={detail.activeDetail}
            copiedRootPath={dialogs.copiedRootPath}
            sourceBadgeLabel={activeWorkspaceSourceBadgeLabel}
            sourceBadgeDescription={activeWorkspaceSourceBadgeDescription}
            onBack={detail.clearActiveWorkspace}
            onEdit={() => dialogs.openEditDialog(activeWorkspace)}
            onCopyConfig={() => {
              void dialogs.openCopyDialog(activeWorkspace);
            }}
            onCopyRootPath={() => {
              void dialogs.handleCopyActiveRootPath(activeWorkspace.root_path);
            }}
          />

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
              {[
                { id: "sessions" as const, label: t("terminalSessions", "Terminal Sessions") },
                { id: "mcp" as const, label: "MCP" },
                { id: "skills" as const, label: t("skills", "Skills") },
                { id: "subagents" as const, label: t("subagents", "Subagents") },
              ].map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => handleSelectTab(item.id)}
                  className={`rounded-md px-3 py-1.5 text-sm ${activeTab === item.id ? "bg-black text-white" : "bg-white text-black"}`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>

          {activeTab === "sessions" && (
            <WorkspaceSessionsTab
              t={t}
              activeSessions={detail.activeSessions}
              sessionsLoading={detail.sessionsLoading}
              sessionsInitialized={detail.sessionsInitialized}
              sessionQuery={detail.sessionQuery}
              sessionsTotal={detail.sessionsTotal}
              sessionToolOptions={detail.sessionToolOptions}
              sessionModelOptions={detail.sessionModelOptions}
              onQueryChange={detail.setSessionQuery}
              onLaunch={handleWorkspaceSessionLaunch}
              onDelete={handleWorkspaceSessionDelete}
              onRename={handleWorkspaceSessionRename}
              onFavoriteChange={handleWorkspaceSessionFavoriteChange}
              onQuickLaunch={() => dialogs.openLaunchDialog(activeWorkspace)}
            />
          )}

          {activeTab === "mcp" && (
            <WorkspaceMcpTab
              t={t}
              activeMcpModel={mcpTab.activeMcpModel}
              setActiveMcpModel={mcpTab.setActiveMcpModel}
              mcpLoading={mcpTab.mcpLoading}
              mcpInitialized={mcpTab.mcpInitialized}
              workspaceInstalledCountsByModel={mcpTab.workspaceInstalledCountsByModel}
              workspaceInstalledCards={mcpTab.workspaceInstalledCards}
              activeMcpLoadRule={mcpTab.activeMcpLoadRule}
              workspaceAvailableMcpEntries={mcpTab.workspaceAvailableMcpEntries}
              formatEnabledModels={mcpTab.formatEnabledModels}
              getWorkspaceMcpStatusMeta={mcpTab.getWorkspaceMcpStatusMeta}
              onManageGlobalServers={() => navigateToCapability("mcp-servers", activeWorkspace, "recommended")}
              onManageUserLevel={() => navigateToCapability("mcp-servers", activeWorkspace, "installed")}
              onBrowseGlobalServers={() => navigateToCapability("mcp-servers", activeWorkspace, "recommended")}
              onOpenMcpInstallDialog={(server) => {
                void mcpTab.openMcpInstallDialog(server);
              }}
              onUninstallWorkspaceMcpForModel={(serverId, model) => {
                void mcpTab.handleUninstallWorkspaceMcpForModel(serverId, model);
              }}
              onEnableWorkspaceMcpForActiveModel={(server) => {
                void mcpTab.handleEnableWorkspaceMcpForActiveModel(server);
              }}
            />
          )}

          {activeTab === "skills" && (
            <WorkspaceSkillsPanel
              isVisible={isVisible && activeTab === "skills"}
              rootPath={activeWorkspace.root_path}
              onNavigateToGlobalPage={(entry) => {
                navigateToCapability("skills", activeWorkspace, entry);
              }}
            />
          )}

          {activeTab === "subagents" && (
            <WorkspaceSubagentsPanel
              isVisible={isVisible && activeTab === "subagents"}
              rootPath={activeWorkspace.root_path}
              onNavigateToGlobalPage={(entry) => {
                navigateToCapability("subagents", activeWorkspace, entry);
              }}
            />
          )}
        </>
      )}

      <WorkspaceFormDialog
        t={t}
        open={dialogs.dialogOpen}
        mode={dialogs.dialogMode}
        title={dialogs.formTitle}
        submitting={dialogs.formSubmitting}
        error={dialogs.formError}
        formState={dialogs.formState}
        onOpenChange={dialogs.setDialogOpen}
        onChange={(updater) => {
          dialogs.setFormState((prev) => {
            const next = updater(prev);
            if (dialogs.formError) dialogs.setFormError("");
            return next;
          });
        }}
        onBrowseRootPath={() => {
          void dialogs.browseWorkspaceRoot();
        }}
        onSubmit={() => {
          void dialogs.handleWorkspaceSubmit();
        }}
      />

      <WorkspaceLaunchDialog
        t={t}
        workspace={dialogs.launchWorkspace}
        launchModel={dialogs.launchModel}
        submitting={dialogs.launchSubmitting}
        onOpenChange={(open) => !open && dialogs.setLaunchWorkspace(null)}
        onSelectModel={dialogs.setLaunchModel}
        onSubmit={() => {
          void dialogs.handleLaunchWorkspaceSession();
        }}
      />

      <WorkspaceCopyDialog
        t={t}
        workspace={dialogs.copyWorkspace}
        copyDetail={dialogs.copyDetail}
        copySkills={dialogs.copySkills}
        copySubagents={dialogs.copySubagents}
        copyTargetRoot={dialogs.copyTargetRoot}
        copySelectedMcpIds={dialogs.copySelectedMcpIds}
        copySelectedSkills={dialogs.copySelectedSkills}
        copySelectedSubagents={dialogs.copySelectedSubagents}
        copySubmitting={dialogs.copySubmitting}
        copyError={dialogs.copyError}
        copyLoading={dialogs.copyLoading}
        mcpServers={mcpTab.mcpServers}
        onOpenChange={(open) => {
          if (!open && !dialogs.copySubmitting) {
            dialogs.setCopyWorkspace(null);
            dialogs.setCopyDetail(null);
            dialogs.setCopySkills([]);
            dialogs.setCopySubagents([]);
            dialogs.setCopyTargetRoot("");
            dialogs.setCopyError("");
          }
        }}
        onTargetRootChange={(value) => {
          dialogs.setCopyTargetRoot(value);
          if (dialogs.copyError) dialogs.setCopyError("");
        }}
        onBrowseTargetRoot={() => {
          void dialogs.browseCopyTargetRoot();
        }}
        onToggleSelection={dialogs.toggleCopySelection}
        onSetAllSelections={dialogs.setAllCopySelections}
        onSubmit={() => {
          void dialogs.handleCopyWorkspace();
        }}
      />

      <WorkspaceMcpDialog
        t={t}
        activeWorkspaceName={activeWorkspace?.name || ""}
        server={mcpTab.mcpDialogServer}
        models={mcpTab.mcpDialogModels}
        submitting={mcpTab.mcpDialogSubmitting}
        error={mcpTab.mcpDialogError}
        onOpenChange={(open) => {
          if (!open && !mcpTab.mcpDialogSubmitting) {
            mcpTab.setMcpDialogServer(null);
            mcpTab.setMcpDialogModels([]);
            mcpTab.setMcpDialogError("");
          }
        }}
        onToggleModel={mcpTab.toggleMcpDialogModel}
        onSubmit={() => {
          void mcpTab.handleSaveMcpDialog();
        }}
      />

      {permissionDialogSession && (
        <TerminalPermissionConfirmDialog
          open={permissionDialogOpen}
          toolId={(permissionDialogSession.model_type.toLowerCase() || "claude") as PermAiModelId}
          toolLabel={permissionDialogSession.model_type}
          onConfirm={handleWorkspacePermissionConfirm}
          onCancel={handleWorkspacePermissionCancel}
        />
      )}
    </div>
  );
}
