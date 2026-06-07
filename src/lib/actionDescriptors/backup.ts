import type { TFunction } from "i18next";
import type { ActionDescriptor } from "@/lib/userActions";

export function buildCreateBackupActionDescriptor(
  t: TFunction,
  activeTool: string,
): ActionDescriptor {
  return {
    source: "backup",
    category: "backup",
    action: "create-backup",
    target: { tab: "more-tools", section: "backup", entity_id: activeTool },
    dedupeKey: `backup:create:${activeTool}`,
    metadata: { active_tool: activeTool },
    confirm: {
      message: t("confirmCreateBackup", "Create a manual backup now?"),
      title: t("createBackup", "Create Backup"),
      okLabel: t("createBackup", "Create Backup"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "warning",
    },
    success: {
      title: t("backupCreatedMessageTitle", "Backup created"),
      summary: t(
        "backupCreatedMessageSummary",
        "Backup created successfully.",
      ),
    },
    error: {
      title: t("backupCreateFailedMessageTitle", "Failed to create backup"),
    },
  };
}

export function buildRestoreBackupActionDescriptor(
  t: TFunction,
  entryId: string,
): ActionDescriptor {
  return {
    source: "backup",
    category: "restore",
    action: "restore-backup",
    target: { tab: "more-tools", section: "backup", entity_id: entryId },
    dedupeKey: `backup:restore:${entryId}`,
    metadata: { entry_id: entryId },
    confirm: {
      message: t("confirmRestoreBackup"),
      title: t("restore", "Restore"),
      okLabel: t("restore", "Restore"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "warning",
    },
    success: {
      title: t("backupRestoreSuccessMessageTitle", "Backup restored"),
      summary: t(
        "backupRestoreSuccessMessageSummary",
        "Backup {{entryId}} restored successfully.",
        { entryId },
      ),
    },
    error: {
      title: t("backupRestoreFailedMessageTitle", "Failed to restore backup"),
    },
  };
}

export function buildDeleteBackupActionDescriptor(
  t: TFunction,
  entryId: string,
): ActionDescriptor {
  return {
    source: "backup",
    category: "delete",
    action: "delete-backup",
    target: { tab: "more-tools", section: "backup", entity_id: entryId },
    dedupeKey: `backup:delete:${entryId}`,
    metadata: { entry_id: entryId },
    confirm: {
      message: t("confirmDeleteBackup"),
      title: t("delete", "Delete"),
      okLabel: t("ok", "OK"),
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

export function buildCleanupBackupsActionDescriptor(
  t: TFunction,
  retentionDays: number,
): ActionDescriptor {
  return {
    source: "backup",
    category: "cleanup",
    action: "cleanup-backups",
    target: { tab: "more-tools", section: "backup" },
    dedupeKey: `backup:cleanup:${retentionDays}`,
    metadata: { retention_days: retentionDays },
    confirm: {
      message: t("confirmCleanupBackups", { days: retentionDays }),
      title: t("cleanupOld", "Cleanup Old"),
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "warning",
    },
    success: {
      title: t("backupCleanupSuccessTitle", "Backups cleaned"),
      summary: t(
        "backupCleanupSuccessSummary",
        "Removed old backups successfully.",
      ),
    },
    error: {
      title: t("backupCleanupFailedTitle", "Failed to clean backups"),
    },
  };
}
