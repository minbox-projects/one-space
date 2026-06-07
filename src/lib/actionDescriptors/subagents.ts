import type { TFunction } from "i18next";
import type { ActionDescriptor } from "@/lib/userActions";

export function buildUninstallSubagentActionDescriptor(
  t: TFunction,
  input: { model: string; id: string; name: string },
): ActionDescriptor {
  return {
    source: "subagents",
    category: "delete",
    action: "uninstall-subagent",
    target: { tab: "subagents", entity_id: input.id },
    dedupeKey: `subagents:uninstall:${input.model}:${input.id}`,
    confirm: {
      message: t("confirmDelete", { name: input.name }),
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "error",
    },
    success: {
      title: t("subagentUninstalledMessageTitle", "Subagent uninstalled"),
      summary: t("subagentUninstalledMessageSummary", "Removed {{name}}.", {
        name: input.name,
      }),
    },
    error: {
      title: t(
        "subagentUninstallFailedMessageTitle",
        "Failed to uninstall subagent",
      ),
    },
  };
}
