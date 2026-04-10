import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Pencil, Plus, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";
import type { SshTunnelGroupView } from "./types";

type SshTunnelGroupManagerDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  groups: SshTunnelGroupView[];
  submitting: boolean;
  onCreate: (name: string) => Promise<void> | void;
  onRename: (group: SshTunnelGroupView, name: string) => Promise<void> | void;
  onDelete: (group: SshTunnelGroupView) => Promise<void> | void;
};

export function SshTunnelGroupManagerDialog({
  open,
  onOpenChange,
  groups,
  submitting,
  onCreate,
  onRename,
  onDelete,
}: SshTunnelGroupManagerDialogProps) {
  const { t } = useTranslation();
  const [newName, setNewName] = useState("");
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");

  const editingGroup = useMemo(
    () => groups.find((group) => group.id === editingGroupId) || null,
    [groups, editingGroupId],
  );

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setNewName("");
      setEditingGroupId(null);
      setEditingName("");
    }
    onOpenChange(nextOpen);
  };

  const beginEdit = (group: SshTunnelGroupView) => {
    setEditingGroupId(group.id);
    setEditingName(group.name);
  };

  const cancelEdit = () => {
    setEditingGroupId(null);
    setEditingName("");
  };

  const submitCreate = async () => {
    try {
      await onCreate(newName);
      setNewName("");
    } catch {
      // Parent already surfaced the error.
    }
  };

  const submitRename = async () => {
    if (!editingGroup) return;
    try {
      await onRename(editingGroup, editingName);
      cancelEdit();
    } catch {
      // Parent already surfaced the error.
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      {open ? (
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("sshTunnelManageGroups", "管理分组")}</DialogTitle>
            <DialogDescription>
              {t(
                "sshTunnelManageGroupsDesc",
                "Environment groups only affect SSH tunnel ownership and the tabs-based filter on this page.",
              )}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="rounded-xl border bg-muted/20 p-4">
              <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {t("sshTunnelCreateGroup", "新建分组")}
              </div>
              <div className="mt-3 flex gap-2">
                <input
                  type="text"
                  value={newName}
                  onChange={(event) => setNewName(event.target.value)}
                  placeholder={t(
                    "sshTunnelGroupNamePlaceholder",
                    "例如：开发环境",
                  )}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                />
                <button
                  type="button"
                  onClick={() => void submitCreate()}
                  disabled={submitting}
                  className="inline-flex shrink-0 items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
                >
                  {submitting ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Plus className="h-4 w-4" />
                  )}
                  {t("add", "Add")}
                </button>
              </div>
            </div>

            <div className="space-y-3">
              {groups.map((group) => {
                const isEditing = editingGroupId === group.id;
                return (
                  <div
                    key={group.id}
                    className="flex flex-col gap-3 rounded-xl border bg-card p-4 md:flex-row md:items-center md:justify-between"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-sm font-semibold">
                          {group.is_default
                            ? t("sshTunnelDefaultGroup", "默认分组")
                            : group.name}
                        </span>
                        {group.is_default ? (
                          <span className="rounded-full border bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                            {t("default", "Default")}
                          </span>
                        ) : null}
                      </div>
                      {isEditing ? (
                        <div className="mt-3 flex gap-2">
                          <input
                            type="text"
                            value={editingName}
                            onChange={(event) => setEditingName(event.target.value)}
                            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                          />
                          <button
                            type="button"
                            onClick={() => void submitRename()}
                            disabled={submitting}
                            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
                          >
                            {t("save", "Save")}
                          </button>
                          <button
                            type="button"
                            onClick={cancelEdit}
                            disabled={submitting}
                            className="rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
                          >
                            {t("cancel", "Cancel")}
                          </button>
                        </div>
                      ) : (
                        <p className="mt-1 text-sm text-muted-foreground">
                          {group.is_default
                            ? t(
                                "sshTunnelDefaultGroupHint",
                                "Unassigned or fallback tunnels will return here automatically.",
                              )
                            : t(
                                "sshTunnelCustomGroupHint",
                                "Use this group to organize and filter related SSH tunnels.",
                              )}
                        </p>
                      )}
                    </div>

                    {!group.is_default && !isEditing ? (
                      <div className="flex shrink-0 gap-2">
                        <button
                          type="button"
                          onClick={() => beginEdit(group)}
                          disabled={submitting}
                          className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm font-medium transition-colors hover:bg-muted disabled:opacity-60"
                        >
                          <Pencil className="h-4 w-4" />
                          {t("edit", "Edit")}
                        </button>
                        <button
                          type="button"
                          onClick={() => void onDelete(group)}
                          disabled={submitting}
                          className="inline-flex items-center gap-2 rounded-md border border-destructive/20 px-3 py-2 text-sm font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-60"
                        >
                          <Trash2 className="h-4 w-4" />
                          {t("delete", "Delete")}
                        </button>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>

          <DialogFooter>
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="rounded-md border px-4 py-2 text-sm font-medium transition-colors hover:bg-muted"
            >
              {t("cancel", "Cancel")}
            </button>
          </DialogFooter>
        </DialogContent>
      ) : null}
    </Dialog>
  );
}
