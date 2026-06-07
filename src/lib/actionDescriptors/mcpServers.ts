import type { TFunction } from "i18next";
import type { ActionDescriptor } from "@/lib/userActions";

export function buildSaveMcpServerActionDescriptor(
  t: TFunction,
  input: { id: string; transport: string },
): ActionDescriptor {
  return {
    source: "mcp_servers",
    category: "save",
    action: "save-server",
    target: { tab: "mcp-servers", entity_id: input.id },
    dedupeKey: `mcp:save:${input.id}`,
    metadata: { server_id: input.id, transport: input.transport },
    success: {
      title: t("saveSuccess", "Saved"),
      summary: t("saveSuccess", "Saved"),
    },
    error: {
      title: t("saveFailed", "Save failed"),
    },
  };
}

export function buildDeleteMcpServerActionDescriptor(
  t: TFunction,
  id: string,
): ActionDescriptor {
  return {
    source: "mcp_servers",
    category: "delete",
    action: "delete-server",
    target: { tab: "mcp-servers", entity_id: id },
    dedupeKey: `mcp:delete:${id}`,
    metadata: { server_id: id },
    confirm: {
      message: t("confirmDeleteMcp"),
      title: t("delete", "Delete"),
      okLabel: t("delete", "Delete"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "error",
    },
    success: {
      title: t("deleteSuccess", "Deleted"),
      summary: t("deleteSuccess", "Deleted"),
    },
    error: {
      title: t("deleteFailed", "Delete failed"),
    },
  };
}
