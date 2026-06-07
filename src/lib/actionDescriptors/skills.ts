import type { TFunction } from "i18next";
import type { ActionDescriptor } from "@/lib/userActions";

export function buildUninstallSkillActionDescriptor(
  t: TFunction,
  input: { model: string; id: string; name: string },
): ActionDescriptor {
  return {
    source: "skills",
    category: "delete",
    action: "uninstall-skill",
    target: { tab: "skills", entity_id: input.id },
    dedupeKey: `skills:uninstall:${input.model}:${input.id}`,
    confirm: {
      message: t("confirmDelete", { name: input.name }),
      okLabel: t("ok", "OK"),
      cancelLabel: t("cancel", "Cancel"),
      kind: "error",
    },
    success: {
      title: t("skillUninstalledMessageTitle", "Skill uninstalled"),
      summary: t("skillUninstalledMessageSummary", "Removed {{name}}.", {
        name: input.name,
      }),
    },
    error: {
      title: t(
        "skillUninstallFailedMessageTitle",
        "Failed to uninstall skill",
      ),
    },
  };
}
