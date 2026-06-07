import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { buildCopyWorkspaceActionDescriptor, buildDeleteWorkspaceActionDescriptor, buildLaunchWorkspaceSessionActionDescriptor, buildWorkspaceSaveActionDescriptor } from "@/lib/actionDescriptors/workspaces";
import { runUserAction } from "@/lib/userActions";
import { safeRecordMessage } from "@/lib/messages";
import type { ActionContext } from "@/lib/userActions";
import type { TFunction } from "i18next";
import {
  buildSkillSelectionKey,
  buildSubagentSelectionKey,
  normalizeWorkspaceDetail,
  normalizeWorkspaceView,
  parseTagsInput,
} from "../helpers/workspaceHelpers";
import type {
  ApiResp,
  CopyableSkill,
  CopyableSubagent,
  DialogMode,
  InstalledSkill,
  InstalledSubagent,
  ModelId,
  WorkspaceDetail,
  WorkspaceFormState,
  WorkspaceRecord,
} from "../types";

export function useWorkspaceDialogs(args: {
  t: TFunction;
  isTauri: boolean;
  confirmDialog: ActionContext["confirm"];
  pushToast: ActionContext["pushToast"];
  loadWorkspaces: () => Promise<void>;
  loadWorkspaceDetail: (workspaceId: string, optimisticView?: any) => Promise<void>;
  refreshActiveWorkspace: () => Promise<void>;
  loadMcpServers: () => Promise<void>;
  activeWorkspaceRootPath?: string | null;
  activeWorkspaceId: string | null;
  setActiveWorkspaceId: (id: string | null) => void;
  setActiveDetail: (detail: WorkspaceDetail | null) => void;
  setActiveSessions: (sessions: any[]) => void;
  setMessage: (message: { type: "success" | "error"; text: string } | null) => void;
}) {
  const {
    t,
    isTauri,
    confirmDialog,
    pushToast,
    loadWorkspaces,
    loadWorkspaceDetail,
    refreshActiveWorkspace,
    loadMcpServers,
    activeWorkspaceRootPath,
    activeWorkspaceId,
    setActiveWorkspaceId,
    setActiveDetail,
    setActiveSessions,
    setMessage,
  } = args;

  const actionContext = useMemo(
    () => ({
      t,
      confirm: confirmDialog,
      pushToast,
      recordMessage: safeRecordMessage,
    }),
    [confirmDialog, pushToast, t],
  );

  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogMode, setDialogMode] = useState<DialogMode>("create");
  const [formSubmitting, setFormSubmitting] = useState(false);
  const [formError, setFormError] = useState("");
  const [formState, setFormState] = useState<WorkspaceFormState>({
    name: "",
    root_path: "",
    description: "",
    tags: "",
  });
  const [launchWorkspace, setLaunchWorkspace] = useState<WorkspaceRecord | null>(null);
  const [launchModel, setLaunchModel] = useState<ModelId>("claude");
  const [launchSubmitting, setLaunchSubmitting] = useState(false);
  const [copyWorkspace, setCopyWorkspace] = useState<WorkspaceRecord | null>(null);
  const [copyDetail, setCopyDetail] = useState<WorkspaceDetail | null>(null);
  const [copySkills, setCopySkills] = useState<CopyableSkill[]>([]);
  const [copySubagents, setCopySubagents] = useState<CopyableSubagent[]>([]);
  const [copyTargetRoot, setCopyTargetRoot] = useState("");
  const [copySelectedMcpIds, setCopySelectedMcpIds] = useState<string[]>([]);
  const [copySelectedSkills, setCopySelectedSkills] = useState<string[]>([]);
  const [copySelectedSubagents, setCopySelectedSubagents] = useState<string[]>([]);
  const [copySubmitting, setCopySubmitting] = useState(false);
  const [copyError, setCopyError] = useState("");
  const [copyLoading, setCopyLoading] = useState(false);
  const [copiedRootPath, setCopiedRootPath] = useState(false);
  const copiedRootPathTimeoutRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (copiedRootPathTimeoutRef.current !== null) {
        window.clearTimeout(copiedRootPathTimeoutRef.current);
      }
    };
  }, []);

  useEffect(() => {
    setCopiedRootPath(false);
    if (copiedRootPathTimeoutRef.current !== null) {
      window.clearTimeout(copiedRootPathTimeoutRef.current);
      copiedRootPathTimeoutRef.current = null;
    }
  }, [activeWorkspaceRootPath]);

  const openCreateDialog = useCallback(() => {
    setDialogMode("create");
    setFormState({
      name: "",
      root_path: "",
      description: "",
      tags: "",
    });
    setFormError("");
    setDialogOpen(true);
  }, []);

  const openEditDialog = useCallback((workspace: WorkspaceRecord) => {
    setDialogMode("edit");
    setFormState({
      id: workspace.id,
      name: workspace.name,
      root_path: workspace.root_path,
      description: workspace.description || "",
      tags: (workspace.tags || []).join(", "),
    });
    setFormError("");
    setDialogOpen(true);
  }, []);

  const handleWorkspaceSubmit = useCallback(async () => {
    if (!isTauri) return;
    const name = formState.name.trim();
    const rootPath = formState.root_path.trim();
    if (!name) {
      setFormError(t("workspaceNameRequired", "Workspace name is required."));
      return;
    }
    if (dialogMode === "create" && !rootPath) {
      setFormError(t("workspaceRootRequired", "Workspace directory is required."));
      return;
    }
    try {
      setFormSubmitting(true);
      const input = {
        id: formState.id,
        name,
        root_path: rootPath,
        description: formState.description.trim() || null,
        tags: parseTagsInput(formState.tags),
      };
      const resp = await runUserAction(
        actionContext,
        buildWorkspaceSaveActionDescriptor(t, {
          mode: dialogMode === "create" ? "create" : "update",
          id: input.id,
          rootPath: input.root_path,
          name: input.name,
        }),
        () =>
          dialogMode === "create"
            ? invoke<ApiResp<WorkspaceDetail>>("workspace_create", { input })
            : invoke<ApiResp<WorkspaceDetail>>("workspace_update_meta", { input }),
      );
      if (!resp) return;
      setDialogOpen(false);
      setFormError("");
      setMessage({
        type: "success",
        text: dialogMode === "create" ? t("workspaceCreated", "Workspace created") : t("workspaceUpdated", "Workspace updated"),
      });
      emit("refresh-counts").catch(() => {});
      await loadWorkspaces();
      await loadWorkspaceDetail(resp.data.workspace.workspace.id, normalizeWorkspaceView(resp.data.workspace));
    } catch (e: any) {
      setFormError(String(e));
    } finally {
      setFormSubmitting(false);
    }
  }, [actionContext, dialogMode, formState, isTauri, loadWorkspaceDetail, loadWorkspaces, setMessage, t]);

  const handleDeleteWorkspace = useCallback(
    async (workspace: WorkspaceRecord) => {
      if (!isTauri) return;
      try {
        const result = await runUserAction(
          actionContext,
          buildDeleteWorkspaceActionDescriptor(t, {
            id: workspace.id,
            name: workspace.name,
          }),
          () => invoke("workspace_delete", { workspaceId: workspace.id }),
        );
        if (result === null) return;
        emit("refresh-counts").catch(() => {});
        setMessage({ type: "success", text: t("workspaceDeleted", "Workspace deleted") });
        if (activeWorkspaceId === workspace.id) {
          setActiveWorkspaceId(null);
          setActiveDetail(null);
          setActiveSessions([]);
        }
        await loadWorkspaces();
      } catch (e: any) {
        setMessage({
          type: "error",
          text: t("workspaceDeleteFailed", "Failed to delete workspace: {{message}}", {
            message: String(e),
          }),
        });
      }
    },
    [actionContext, activeWorkspaceId, isTauri, loadWorkspaces, setActiveDetail, setActiveSessions, setActiveWorkspaceId, setMessage, t],
  );

  const openLaunchDialog = useCallback((workspace: WorkspaceRecord) => {
    setLaunchWorkspace(workspace);
    setLaunchModel("claude");
  }, []);

  const handleLaunchWorkspaceSession = useCallback(async () => {
    if (!launchWorkspace || !isTauri) return;
    try {
      setLaunchSubmitting(true);
      const result = await runUserAction(
        actionContext,
        buildLaunchWorkspaceSessionActionDescriptor(t, {
          workspaceId: launchWorkspace.id,
          tool: launchModel,
        }),
        () =>
          invoke("workspace_launch_session", {
            workspaceId: launchWorkspace.id,
            tool: launchModel,
          }),
      );
      if (result === null) return;
      emit("refresh-counts").catch(() => {});
      setMessage({
        type: "success",
        text: t("workspaceLaunchSuccess", "New terminal session started"),
      });
      setLaunchWorkspace(null);
      await Promise.all([loadWorkspaces(), refreshActiveWorkspace()]);
    } catch (e: any) {
      setMessage({
        type: "error",
        text: t("workspaceLaunchFailed", "Failed to start session: {{message}}", {
          message: String(e),
        }),
      });
    } finally {
      setLaunchSubmitting(false);
    }
  }, [actionContext, isTauri, launchModel, launchWorkspace, loadWorkspaces, refreshActiveWorkspace, setMessage, t]);

  const loadCopySources = useCallback(
    async (workspace: WorkspaceRecord) => {
      if (!isTauri) return;
      setCopyLoading(true);
      setCopyError("");
      try {
        const [, detailResp, skillsResp, subagentsResp] = await Promise.all([
          loadMcpServers().catch(() => {}),
          invoke<ApiResp<WorkspaceDetail>>("workspace_get", { workspaceId: workspace.id }),
          invoke<ApiResp<InstalledSkill[]>>("skills_list_installed", {
            model: null,
            scope: "project",
            projectRoot: workspace.root_path,
          }),
          invoke<ApiResp<InstalledSubagent[]>>("subagents_list_installed", {
            model: null,
            scope: "project",
            projectRoot: workspace.root_path,
          }),
        ]);
        const detailData = normalizeWorkspaceDetail(detailResp.data);
        const nextSkills = (skillsResp.data || []).map((item) => ({
          ...item,
          selection_key: buildSkillSelectionKey(item),
        }));
        const nextSubagents = (subagentsResp.data || []).map((item) => ({
          ...item,
          selection_key: buildSubagentSelectionKey(item),
        }));
        setCopyWorkspace(workspace);
        setCopyDetail(detailData);
        setCopySkills(nextSkills);
        setCopySubagents(nextSubagents);
        setCopySelectedMcpIds((detailData.mcp_bindings || []).map((item) => item.server_id));
        setCopySelectedSkills(nextSkills.map((item) => item.selection_key));
        setCopySelectedSubagents(nextSubagents.map((item) => item.selection_key));
        setCopyTargetRoot("");
      } catch (e: any) {
        setCopyError(
          t("workspaceCopyLoadFailed", "Failed to load copyable content: {{message}}", {
            message: String(e),
          }),
        );
      } finally {
        setCopyLoading(false);
      }
    },
    [isTauri, loadMcpServers, t],
  );

  const openCopyDialog = useCallback(
    async (workspace: WorkspaceRecord) => {
      await loadCopySources(workspace);
    },
    [loadCopySources],
  );

  const handleCopyWorkspace = useCallback(async () => {
    if (!copyWorkspace || !isTauri) return;
    if (!copyTargetRoot.trim()) {
      setCopyError(t("workspaceCopyTargetRequired", "Target directory is required."));
      return;
    }
    try {
      setCopySubmitting(true);
      setCopyError("");
      const result = await runUserAction(
        actionContext,
        buildCopyWorkspaceActionDescriptor(t, {
          workspaceId: copyWorkspace.id,
          targetRootPath: copyTargetRoot.trim(),
        }),
        () =>
          invoke<ApiResp<WorkspaceDetail>>("workspace_copy", {
            input: {
              source_workspace_id: copyWorkspace.id,
              target_root_path: copyTargetRoot.trim(),
              selected_mcp_server_ids: copySelectedMcpIds,
              selected_skills: copySkills
                .filter((item) => copySelectedSkills.includes(item.selection_key))
                .map((item) => ({
                  model: item.model,
                  source_id: item.source_id,
                  source_rel_path: item.source_rel_path,
                })),
              selected_subagents: copySubagents
                .filter((item) => copySelectedSubagents.includes(item.selection_key))
                .map((item) => ({
                  model: item.model,
                  source_id: item.source_id,
                  source_rel_path: item.source_rel_path,
                })),
            },
          }),
      );
      if (result === null) return;
      emit("refresh-counts").catch(() => {});
      setMessage({
        type: "success",
        text: t("workspaceCopySuccess", "Workspace configuration copied"),
      });
      setCopyWorkspace(null);
      setCopyDetail(null);
      setCopySkills([]);
      setCopySubagents([]);
      setCopyTargetRoot("");
      await loadWorkspaces();
    } catch (e: any) {
      setCopyError(String(e));
    } finally {
      setCopySubmitting(false);
    }
  }, [
    actionContext,
    copySelectedMcpIds,
    copySelectedSkills,
    copySelectedSubagents,
    copySkills,
    copySubagents,
    copyTargetRoot,
    copyWorkspace,
    isTauri,
    loadWorkspaces,
    setMessage,
    t,
  ]);

  const handleCopyActiveRootPath = useCallback(async (rootPath?: string | null) => {
    if (!rootPath) return;
    try {
      await navigator.clipboard.writeText(rootPath);
      setCopiedRootPath(true);
      if (copiedRootPathTimeoutRef.current !== null) {
        window.clearTimeout(copiedRootPathTimeoutRef.current);
      }
      copiedRootPathTimeoutRef.current = window.setTimeout(() => {
        setCopiedRootPath(false);
        copiedRootPathTimeoutRef.current = null;
      }, 2000);
    } catch (error) {
      console.error("failed to copy workspace root path", error);
      setMessage({
        type: "error",
        text: t("copyPathFailed", "Failed to copy path. Please copy manually."),
      });
    }
  }, [setMessage, t]);

  const toggleCopySelection = useCallback((kind: "mcp" | "skills" | "subagents", key: string) => {
    const updater = (prev: string[]) => (prev.includes(key) ? prev.filter((item) => item !== key) : [...prev, key]);
    if (kind === "mcp") {
      setCopySelectedMcpIds(updater);
      return;
    }
    if (kind === "skills") {
      setCopySelectedSkills(updater);
      return;
    }
    setCopySelectedSubagents(updater);
  }, []);

  const setAllCopySelections = useCallback(
    (kind: "mcp" | "skills" | "subagents", enabled: boolean) => {
      if (kind === "mcp") {
        setCopySelectedMcpIds(enabled ? (copyDetail?.mcp_bindings || []).map((item) => item.server_id) : []);
        return;
      }
      if (kind === "skills") {
        setCopySelectedSkills(enabled ? copySkills.map((item) => item.selection_key) : []);
        return;
      }
      setCopySelectedSubagents(enabled ? copySubagents.map((item) => item.selection_key) : []);
    },
    [copyDetail?.mcp_bindings, copySkills, copySubagents],
  );

  const browseWorkspaceRoot = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === "string") {
      setFormState((prev) => ({ ...prev, root_path: selected }));
      if (formError) setFormError("");
    }
  }, [formError]);

  const browseCopyTargetRoot = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === "string") {
      setCopyTargetRoot(selected);
      if (copyError) setCopyError("");
    }
  }, [copyError]);

  const formTitle =
    dialogMode === "create" ? t("workspaceCreateTitle", "Create Workspace") : t("workspaceEditTitle", "Edit Workspace");

  return {
    actionContext,
    dialogOpen,
    setDialogOpen,
    dialogMode,
    formSubmitting,
    formError,
    setFormError,
    formState,
    setFormState,
    formTitle,
    launchWorkspace,
    setLaunchWorkspace,
    launchModel,
    setLaunchModel,
    launchSubmitting,
    copyWorkspace,
    setCopyWorkspace,
    copyDetail,
    setCopyDetail,
    copySkills,
    setCopySkills,
    copySubagents,
    setCopySubagents,
    copyTargetRoot,
    setCopyTargetRoot,
    copySelectedMcpIds,
    copySelectedSkills,
    copySelectedSubagents,
    copySubmitting,
    copyError,
    setCopyError,
    copyLoading,
    copiedRootPath,
    openCreateDialog,
    openEditDialog,
    handleWorkspaceSubmit,
    handleDeleteWorkspace,
    openLaunchDialog,
    handleLaunchWorkspaceSession,
    openCopyDialog,
    handleCopyWorkspace,
    handleCopyActiveRootPath,
    toggleCopySelection,
    setAllCopySelections,
    browseWorkspaceRoot,
    browseCopyTargetRoot,
  };
}
