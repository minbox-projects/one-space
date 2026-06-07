import type { TFunction } from "i18next";
import type { ActionDescriptor } from "@/lib/userActions";

export function buildWorkspaceSaveActionDescriptor(
  t: TFunction,
  input: {
    mode: "create" | "update";
    id?: string | null;
    rootPath?: string | null;
    name: string;
  },
): ActionDescriptor {
  return {
    source: "workspaces",
    category: input.mode === "create" ? "create" : "update",
    action:
      input.mode === "create" ? "create-workspace" : "update-workspace",
    target: { tab: "workspaces" },
    dedupeKey: `workspaces:${input.mode}:${input.id || input.rootPath || input.name}`,
    metadata: { workspace_id: input.id, root_path: input.rootPath },
    success: {
      title:
        input.mode === "create"
          ? t("workspaceCreated", "Workspace created")
          : t("workspaceUpdated", "Workspace updated"),
      summary:
        input.mode === "create"
          ? t("workspaceCreated", "Workspace created")
          : t("workspaceUpdated", "Workspace updated"),
    },
    error: {
      title:
        input.mode === "create"
          ? t("workspaceCreateFailed", "Failed to create workspace")
          : t("workspaceUpdateFailed", "Failed to update workspace"),
    },
  };
}

export function buildDeleteWorkspaceActionDescriptor(
  t: TFunction,
  input: { id: string; name: string },
): ActionDescriptor {
  return {
    source: "workspaces",
    category: "delete",
    action: "delete-workspace",
    target: { tab: "workspaces", entity_id: input.id },
    dedupeKey: `workspaces:delete:${input.id}`,
    metadata: { workspace_id: input.id },
    confirm: {
      message: t('workspaceDeleteConfirm', 'Delete workspace "{{name}}"?', {
        name: input.name,
      }),
      title: t("workspaceDeleteTitle", "Delete Workspace"),
      okLabel: t("delete", "Delete"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "error",
    },
    success: {
      title: t("workspaceDeleted", "Workspace deleted"),
      summary: t("workspaceDeleted", "Workspace deleted"),
    },
    error: {
      title: t("workspaceDeleteFailed", "Failed to delete workspace"),
    },
  };
}

export function buildLaunchWorkspaceSessionActionDescriptor(
  t: TFunction,
  input: { workspaceId: string; tool: string },
): ActionDescriptor {
  return {
    source: "workspaces",
    category: "launch",
    action: "launch-workspace-session",
    target: { tab: "workspaces", entity_id: input.workspaceId },
    dedupeKey: `workspaces:launch:${input.workspaceId}:${input.tool}`,
    metadata: { workspace_id: input.workspaceId, tool: input.tool },
    success: {
      title: t("workspaceLaunchSuccess", "New terminal session started"),
      summary: t("workspaceLaunchSuccess", "New terminal session started"),
    },
    error: {
      title: t("workspaceLaunchFailed", "Failed to start session"),
    },
  };
}

export function buildCopyWorkspaceActionDescriptor(
  t: TFunction,
  input: { workspaceId: string; targetRootPath: string },
): ActionDescriptor {
  return {
    source: "workspaces",
    category: "copy",
    action: "copy-workspace",
    target: { tab: "workspaces", entity_id: input.workspaceId },
    dedupeKey: `workspaces:copy:${input.workspaceId}:${input.targetRootPath}`,
    metadata: {
      workspace_id: input.workspaceId,
      target_root_path: input.targetRootPath,
    },
    success: {
      title: t("workspaceCopySuccess", "Workspace configuration copied"),
      summary: t("workspaceCopySuccess", "Workspace configuration copied"),
    },
    error: {
      title: t("workspaceCopyFailed", "Failed to copy workspace"),
    },
  };
}
