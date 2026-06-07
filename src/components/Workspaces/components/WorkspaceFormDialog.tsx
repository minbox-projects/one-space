import { FolderOpen, Loader2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/dialog";
import type { WorkspaceFormState } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceFormDialog(args: {
  t: TFunction;
  open: boolean;
  mode: "create" | "edit";
  title: string;
  submitting: boolean;
  error: string;
  formState: WorkspaceFormState;
  onOpenChange: (open: boolean) => void;
  onChange: (updater: (prev: WorkspaceFormState) => WorkspaceFormState) => void;
  onBrowseRootPath: () => void;
  onSubmit: () => void;
}) {
  const { t, open, mode, title, submitting, error, formState, onOpenChange, onChange, onBrowseRootPath, onSubmit } = args;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && (
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>
              {mode === "create"
                ? t("workspaceCreateDesc", "Name and directory are required. Description and tags are optional.")
                : t("workspaceEditDesc", "Only name, description, and tags can be updated. Directory is read-only.")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t("name", "Name")}</label>
              <input
                value={formState.name}
                onChange={(event) => onChange((prev) => ({ ...prev, name: event.target.value }))}
                className="h-10 w-full rounded-md border px-3 text-sm"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t("workingDirectory", "Directory")}</label>
              <div className="flex gap-2">
                <input
                  value={formState.root_path}
                  onChange={(event) => {
                    if (mode === "edit") return;
                    onChange((prev) => ({ ...prev, root_path: event.target.value }));
                  }}
                  readOnly={mode === "edit"}
                  className={`h-10 w-full rounded-md border px-3 text-sm ${mode === "edit" ? "bg-muted/60 text-muted-foreground" : ""}`}
                />
                {mode === "create" && (
                  <button
                    type="button"
                    onClick={onBrowseRootPath}
                    className="inline-flex items-center justify-center rounded-md border px-3 hover:bg-muted"
                  >
                    <FolderOpen className="h-4 w-4" />
                  </button>
                )}
              </div>
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t("description", "Description")}</label>
              <textarea
                value={formState.description}
                onChange={(event) => onChange((prev) => ({ ...prev, description: event.target.value }))}
                rows={3}
                className="w-full rounded-md border px-3 py-2 text-sm"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t("tags", "Tags")}</label>
              <input
                value={formState.tags}
                onChange={(event) => onChange((prev) => ({ ...prev, tags: event.target.value }))}
                placeholder={t("workspaceTagsPlaceholder", "frontend, work, personal")}
                className="h-10 w-full rounded-md border px-3 text-sm"
              />
            </div>
            {error && <p className="text-sm text-destructive">{error}</p>}
          </div>
          <DialogFooter>
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
              disabled={submitting}
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              onClick={onSubmit}
              className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              disabled={submitting}
            >
              {submitting && <Loader2 className="h-4 w-4 animate-spin" />}
              {mode === "create" ? t("create", "Create") : t("save", "Save")}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
