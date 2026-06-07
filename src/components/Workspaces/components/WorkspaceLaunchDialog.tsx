import { Loader2 } from "lucide-react";
import { ToolIcon } from "../../AiEnvironments";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/dialog";
import { TOOL_OPTIONS, type ModelId, type WorkspaceRecord } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceLaunchDialog(args: {
  t: TFunction;
  workspace: WorkspaceRecord | null;
  launchModel: ModelId;
  submitting: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectModel: (model: ModelId) => void;
  onSubmit: () => void;
}) {
  const { t, workspace, launchModel, submitting, onOpenChange, onSelectModel, onSubmit } = args;

  return (
    <Dialog open={Boolean(workspace)} onOpenChange={onOpenChange}>
      {workspace && (
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("workspaceLaunchDialogTitle", "Choose a model")}</DialogTitle>
            <DialogDescription>
              {t("workspaceLaunchDialogDesc", "Start a new AI terminal session in {{name}}", {
                name: workspace.name,
              })}
            </DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {TOOL_OPTIONS.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => onSelectModel(item.id)}
                className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
                  launchModel === item.id ? "border-primary bg-primary/10 text-primary" : "hover:bg-muted"
                }`}
              >
                <ToolIcon tool={item.id} className="h-5 w-5" />
                <span className="font-medium">{item.label}</span>
              </button>
            ))}
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
              {t("launch", "Launch")}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
