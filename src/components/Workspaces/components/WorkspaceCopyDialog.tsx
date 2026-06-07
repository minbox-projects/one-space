import { Bot, FolderOpen, Loader2, Server, Sparkles } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/dialog";
import type { CopyableSkill, CopyableSubagent, MCPServer, WorkspaceDetail, WorkspaceRecord } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceCopyDialog(args: {
  t: TFunction;
  workspace: WorkspaceRecord | null;
  copyDetail: WorkspaceDetail | null;
  copySkills: CopyableSkill[];
  copySubagents: CopyableSubagent[];
  copyTargetRoot: string;
  copySelectedMcpIds: string[];
  copySelectedSkills: string[];
  copySelectedSubagents: string[];
  copySubmitting: boolean;
  copyError: string;
  copyLoading: boolean;
  mcpServers: MCPServer[];
  onOpenChange: (open: boolean) => void;
  onTargetRootChange: (value: string) => void;
  onBrowseTargetRoot: () => void;
  onToggleSelection: (kind: "mcp" | "skills" | "subagents", key: string) => void;
  onSetAllSelections: (kind: "mcp" | "skills" | "subagents", enabled: boolean) => void;
  onSubmit: () => void;
}) {
  const {
    t,
    workspace,
    copyDetail,
    copySkills,
    copySubagents,
    copyTargetRoot,
    copySelectedMcpIds,
    copySelectedSkills,
    copySelectedSubagents,
    copySubmitting,
    copyError,
    copyLoading,
    mcpServers,
    onOpenChange,
    onTargetRootChange,
    onBrowseTargetRoot,
    onToggleSelection,
    onSetAllSelections,
    onSubmit,
  } = args;

  return (
    <Dialog open={Boolean(workspace)} onOpenChange={onOpenChange}>
      {workspace && (
        <DialogContent className="max-w-3xl max-h-[85vh] overflow-hidden">
          <DialogHeader>
            <DialogTitle>{t("workspaceCopyTitle", "Copy Workspace Config")}</DialogTitle>
            <DialogDescription>
              {t("workspaceCopyDesc", "Choose what to copy from {{name}} and where to create or update the target workspace.", {
                name: workspace.name,
              })}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 overflow-auto pr-1">
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t("workspaceCopyTarget", "Target Directory")}</label>
              <div className="flex gap-2">
                <input
                  value={copyTargetRoot}
                  onChange={(event) => onTargetRootChange(event.target.value)}
                  className="h-10 w-full rounded-md border px-3 text-sm"
                  placeholder={t("workspaceCopyTargetPlaceholder", "Choose a target folder")}
                />
                <button
                  type="button"
                  onClick={onBrowseTargetRoot}
                  className="inline-flex items-center justify-center rounded-md border px-3 hover:bg-muted"
                >
                  <FolderOpen className="h-4 w-4" />
                </button>
              </div>
            </div>

            {copyLoading ? (
              <div className="flex items-center gap-2 rounded-xl border bg-card p-4 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                {t("loading", "Loading...")}
              </div>
            ) : (
              <>
                <div className="rounded-xl border bg-card p-4">
                  <div className="mb-3 flex items-center justify-between">
                    <div className="inline-flex items-center gap-2 text-sm font-medium">
                      <Server className="h-4 w-4" />
                      MCP
                    </div>
                    <div className="flex items-center gap-2 text-xs">
                      <button type="button" onClick={() => onSetAllSelections("mcp", true)} className="hover:text-foreground">
                        {t("selectAll", "Select All")}
                      </button>
                      <button type="button" onClick={() => onSetAllSelections("mcp", false)} className="hover:text-foreground">
                        {t("clear", "Clear")}
                      </button>
                    </div>
                  </div>
                  <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                    {(copyDetail?.mcp_bindings || []).length === 0 ? (
                      <div className="text-sm text-muted-foreground">{t("workspaceCopyEmptyMcp", "No workspace MCP bindings")}</div>
                    ) : (
                      (copyDetail?.mcp_bindings || []).map((binding) => {
                        const server = mcpServers.find((item) => item.id === binding.server_id);
                        const selected = copySelectedMcpIds.includes(binding.server_id);
                        return (
                          <button
                            key={binding.server_id}
                            type="button"
                            onClick={() => onToggleSelection("mcp", binding.server_id)}
                            className={`rounded-lg border p-3 text-left ${selected ? "border-primary bg-primary/10" : "hover:bg-muted"}`}
                          >
                            <div className="font-medium">{server?.name || binding.server_id}</div>
                            <div className="mt-1 text-xs text-muted-foreground">{(binding.enabled_models || []).join(", ") || "-"}</div>
                          </button>
                        );
                      })
                    )}
                  </div>
                </div>

                <div className="rounded-xl border bg-card p-4">
                  <div className="mb-3 flex items-center justify-between">
                    <div className="inline-flex items-center gap-2 text-sm font-medium">
                      <Sparkles className="h-4 w-4" />
                      {t("skills", "Skills")}
                    </div>
                    <div className="flex items-center gap-2 text-xs">
                      <button type="button" onClick={() => onSetAllSelections("skills", true)} className="hover:text-foreground">
                        {t("selectAll", "Select All")}
                      </button>
                      <button type="button" onClick={() => onSetAllSelections("skills", false)} className="hover:text-foreground">
                        {t("clear", "Clear")}
                      </button>
                    </div>
                  </div>
                  <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                    {copySkills.length === 0 ? (
                      <div className="text-sm text-muted-foreground">{t("workspaceCopyEmptySkills", "No project skills")}</div>
                    ) : (
                      copySkills.map((item) => {
                        const selected = copySelectedSkills.includes(item.selection_key);
                        return (
                          <button
                            key={item.selection_key}
                            type="button"
                            onClick={() => onToggleSelection("skills", item.selection_key)}
                            className={`rounded-lg border p-3 text-left ${selected ? "border-primary bg-primary/10" : "hover:bg-muted"}`}
                          >
                            <div className="font-medium">{item.name}</div>
                            <div className="mt-1 text-xs text-muted-foreground">
                              {item.model} · {item.source_rel_path}
                            </div>
                          </button>
                        );
                      })
                    )}
                  </div>
                </div>

                <div className="rounded-xl border bg-card p-4">
                  <div className="mb-3 flex items-center justify-between">
                    <div className="inline-flex items-center gap-2 text-sm font-medium">
                      <Bot className="h-4 w-4" />
                      {t("subagents", "Subagents")}
                    </div>
                    <div className="flex items-center gap-2 text-xs">
                      <button type="button" onClick={() => onSetAllSelections("subagents", true)} className="hover:text-foreground">
                        {t("selectAll", "Select All")}
                      </button>
                      <button type="button" onClick={() => onSetAllSelections("subagents", false)} className="hover:text-foreground">
                        {t("clear", "Clear")}
                      </button>
                    </div>
                  </div>
                  <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                    {copySubagents.length === 0 ? (
                      <div className="text-sm text-muted-foreground">{t("workspaceCopyEmptySubagents", "No project subagents")}</div>
                    ) : (
                      copySubagents.map((item) => {
                        const selected = copySelectedSubagents.includes(item.selection_key);
                        return (
                          <button
                            key={item.selection_key}
                            type="button"
                            onClick={() => onToggleSelection("subagents", item.selection_key)}
                            className={`rounded-lg border p-3 text-left ${selected ? "border-primary bg-primary/10" : "hover:bg-muted"}`}
                          >
                            <div className="font-medium">{item.name}</div>
                            <div className="mt-1 text-xs text-muted-foreground">
                              {item.model} · {item.source_rel_path}
                            </div>
                          </button>
                        );
                      })
                    )}
                  </div>
                </div>
              </>
            )}

            {copyError && <p className="text-sm text-destructive">{copyError}</p>}
          </div>
          <DialogFooter>
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
              disabled={copySubmitting}
            >
              {t("cancel", "Cancel")}
            </button>
            <button
              type="button"
              onClick={onSubmit}
              className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              disabled={copySubmitting || copyLoading}
            >
              {copySubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
              {t("copy", "Copy")}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
