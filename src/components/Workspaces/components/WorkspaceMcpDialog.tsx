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
import { TOOL_OPTIONS, type MCPServer, type ModelId } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceMcpDialog(args: {
  t: TFunction;
  activeWorkspaceName: string;
  server: MCPServer | null;
  models: ModelId[];
  submitting: boolean;
  error: string;
  onOpenChange: (open: boolean) => void;
  onToggleModel: (model: ModelId) => void;
  onSubmit: () => void;
}) {
  const { t, activeWorkspaceName, server, models, submitting, error, onOpenChange, onToggleModel, onSubmit } = args;

  return (
    <Dialog open={Boolean(server)} onOpenChange={onOpenChange}>
      {server && (
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("workspaceMcpInstallDialogTitle", "Manage workspace MCP models")}</DialogTitle>
            <DialogDescription>
              {t("workspaceMcpInstallDialogDesc", "Choose which models in {{name}} should enable {{server}}.", {
                name: activeWorkspaceName,
                server: server.name,
              })}
            </DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {TOOL_OPTIONS.map((tool) => {
              const selected = models.includes(tool.id);
              return (
                <button
                  key={`workspace-mcp-dialog-${tool.id}`}
                  type="button"
                  onClick={() => onToggleModel(tool.id)}
                  className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
                    selected ? "border-primary bg-primary/10 text-primary" : "hover:bg-muted"
                  }`}
                >
                  <ToolIcon tool={tool.id} className="h-5 w-5" />
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{tool.label}</div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      {selected ? t("selected", "Selected") : t("clickToSelect", "Click to select")}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
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
              {t("save", "Save")}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
