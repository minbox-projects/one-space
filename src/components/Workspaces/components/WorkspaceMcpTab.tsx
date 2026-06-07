import { Info, Loader2, Server, Settings2, Trash2 } from "lucide-react";
import { ToolIcon } from "../../AiEnvironments";
import { getMcpConnectionText, getScopeBadgeClassName } from "../helpers/workspaceHelpers";
import type { MCPServer, ModelId, WorkspaceMcpCatalogEntry, WorkspaceMcpEntry } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceMcpTab(args: {
  t: TFunction;
  activeMcpModel: ModelId;
  setActiveMcpModel: (model: ModelId) => void;
  mcpLoading: boolean;
  mcpInitialized: boolean;
  workspaceInstalledCountsByModel: Record<ModelId, number>;
  workspaceInstalledCards: WorkspaceMcpEntry[];
  activeMcpLoadRule: string;
  workspaceAvailableMcpEntries: WorkspaceMcpCatalogEntry[];
  formatEnabledModels: (models: string[]) => string;
  getWorkspaceMcpStatusMeta: (status: WorkspaceMcpCatalogEntry["status"]) => { label: string; className: string };
  onManageGlobalServers: () => void;
  onManageUserLevel: () => void;
  onBrowseGlobalServers: () => void;
  onOpenMcpInstallDialog: (server: MCPServer) => void;
  onUninstallWorkspaceMcpForModel: (serverId: string, model: ModelId) => void;
  onEnableWorkspaceMcpForActiveModel: (server: MCPServer) => void;
}) {
  const {
    t,
    activeMcpModel,
    setActiveMcpModel,
    mcpLoading,
    mcpInitialized,
    workspaceInstalledCountsByModel,
    workspaceInstalledCards,
    activeMcpLoadRule,
    workspaceAvailableMcpEntries,
    formatEnabledModels,
    getWorkspaceMcpStatusMeta,
    onManageGlobalServers,
    onManageUserLevel,
    onBrowseGlobalServers,
    onOpenMcpInstallDialog,
    onUninstallWorkspaceMcpForModel,
    onEnableWorkspaceMcpForActiveModel,
  } = args;

  return (
    <div className="space-y-4">
      {mcpLoading && !mcpInitialized ? (
        <div className="flex min-h-[16rem] flex-col items-center justify-center rounded-xl border bg-card text-muted-foreground">
          <Loader2 className="mb-3 h-8 w-8 animate-spin" />
          <p>{t("loading", "Loading...")}</p>
        </div>
      ) : (
        <>
          <div className="space-y-3">
            <div className="rounded-xl border bg-card p-4">
              <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                <div className="min-w-0">
                  <h3 className="text-lg font-semibold tracking-tight">{t("mcpServers", "MCP Servers")}</h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {t(
                      "workspaceMcpSectionDesc",
                      "Review MCP servers already enabled in this workspace by model, and adjust which models can use each server.",
                    )}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2 lg:justify-end">
                  {mcpLoading && mcpInitialized && (
                    <div className="inline-flex w-fit items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs text-muted-foreground">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      {t("loading", "Loading...")}
                    </div>
                  )}
                  <button
                    type="button"
                    onClick={onManageGlobalServers}
                    className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
                  >
                    <Settings2 className="h-4 w-4" />
                    {t("workspaceMcpManageGlobalServers", "Manage Global Servers")}
                  </button>
                </div>
              </div>
            </div>

            <div className="border rounded-xl bg-card p-3">
              <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                {[
                  { id: "claude" as const, label: "Claude Code" },
                  { id: "gemini" as const, label: "Gemini" },
                  { id: "codex" as const, label: "Codex" },
                  { id: "opencode" as const, label: "OpenCode" },
                ].map((tool) => (
                  <button
                    key={`workspace-mcp-model-${tool.id}`}
                    type="button"
                    onClick={() => setActiveMcpModel(tool.id)}
                    className={`rounded-lg border px-4 py-3 text-left transition-all ${
                      activeMcpModel === tool.id ? "border-primary bg-primary/5" : "hover:bg-muted/40 hover:-translate-y-0.5"
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <ToolIcon tool={tool.id} className="h-5 w-5" />
                      <span className="text-sm font-semibold">{tool.label}</span>
                    </div>
                    <div className="mt-2.5 text-sm leading-none text-muted-foreground">
                      {t("mcpInstalledCountForModel", "Enabled {{count}} MCP servers", {
                        count: workspaceInstalledCountsByModel[tool.id],
                      })}
                    </div>
                  </button>
                ))}
              </div>
            </div>
            <div className="flex items-start gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
              <Info className="mt-0.5 h-4 w-4 shrink-0" />
              <p>
                <span className="font-medium text-foreground">{t("workspaceEffectiveLoadRule", "Effective load rule")}:</span>{" "}
                {activeMcpLoadRule}
              </p>
            </div>
          </div>

          {workspaceInstalledCards.length === 0 ? (
            <div className="text-center py-12">
              <Server className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <h3 className="text-lg font-semibold mb-2">{t("mcpNoServersForModelTitle", "No enabled MCP for this model")}</h3>
              <p className="text-muted-foreground">
                {t("workspaceMcpNoInstalledForModelDesc", "This workspace has not enabled any MCP servers for the selected model yet.")}
              </p>
              <div className="mt-4 flex flex-wrap justify-center gap-2">
                <button
                  type="button"
                  onClick={onBrowseGlobalServers}
                  className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                >
                  <Settings2 className="h-4 w-4" />
                  {t("workspaceMcpBrowseGlobalServers", "Browse Global Servers")}
                </button>
              </div>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {workspaceInstalledCards.map(({ server, scope, enabled_models }) => (
                <div
                  key={`workspace-mcp-installed-${activeMcpModel}-${scope}-${server.id}`}
                  className="group border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="p-2 rounded-md bg-primary/10 text-primary">
                      <Server className="w-4 h-4" />
                    </div>
                    <div className="flex flex-col items-end gap-1">
                      <span className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${getScopeBadgeClassName(scope)}`}>
                        {scope === "global" ? t("workspaceScopeUser", "User-level") : t("workspaceScopeDirectory", "Directory-level")}
                      </span>
                      <span className="text-[10px] text-muted-foreground uppercase">{server.transport || "stdio"}</span>
                    </div>
                  </div>

                  <h4 className="mt-3 font-semibold text-sm line-clamp-1">{server.name}</h4>
                  <p className="mt-1 text-xs text-muted-foreground line-clamp-2">
                    {server.description?.trim() || t("workspaceMcpNoDescription", "No description")}
                  </p>

                  <div className="mt-3 text-[11px] text-muted-foreground font-mono line-clamp-1">{getMcpConnectionText(server)}</div>
                  <div className="mt-2 text-[11px] text-muted-foreground">
                    {t("workspaceMcpEnabledModels", "Enabled models")}: {formatEnabledModels(enabled_models)}
                  </div>

                  <div className="mt-3 flex items-center justify-between gap-2">
                    {scope === "global" ? (
                      <button
                        type="button"
                        onClick={onManageUserLevel}
                        className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs font-medium hover:bg-muted"
                      >
                        <Settings2 className="h-3.5 w-3.5" />
                        {t("workspaceManageUserLevel", "Manage User-level")}
                      </button>
                    ) : (
                      <>
                        <button
                          type="button"
                          onClick={() => onOpenMcpInstallDialog(server)}
                          className="inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-xs font-medium hover:bg-muted"
                        >
                          <Settings2 className="h-3.5 w-3.5" />
                          {t("workspaceMcpManageModels", "Manage Models")}
                        </button>
                        <button
                          type="button"
                          onClick={() => onUninstallWorkspaceMcpForModel(server.id, activeMcpModel)}
                          className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1.5 text-xs font-medium text-destructive hover:bg-destructive/10"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          {t("workspaceMcpDisableCurrentModel", "Disable current model")}
                        </button>
                      </>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}

          <div className="rounded-xl border bg-card p-4">
            <div className="flex flex-col gap-1">
              <h3 className="text-base font-semibold tracking-tight">{t("workspaceMcpAvailableSectionTitle", "Add MCP to This Workspace")}</h3>
              <p className="text-sm text-muted-foreground">
                {t(
                  "workspaceMcpAvailableSectionDesc",
                  "Choose from global MCP server definitions and enable them for the current workspace model.",
                )}
              </p>
            </div>
          </div>

          {workspaceAvailableMcpEntries.length === 0 ? (
            <div className="text-center py-10">
              <Server className="mx-auto mb-4 h-14 w-14 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">
                {t("workspaceMcpEmpty", "No MCP servers available yet. Add global MCP servers first, then bind them to this workspace.")}
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {workspaceAvailableMcpEntries.map(({ server, enabled_models, status }) => {
                const statusMeta = getWorkspaceMcpStatusMeta(status);
                const enabledForCurrentModel = status === "enabled_for_model";
                const enabledAtUserLevel = status === "enabled_user_level";
                const enabledForOtherModels = status === "bound_other_models";
                return (
                  <div
                    key={`workspace-mcp-catalog-${activeMcpModel}-${server.id}`}
                    className="group rounded-xl border bg-card p-4 transition-all duration-200 hover:-translate-y-0.5 hover:border-primary/30 hover:shadow-md"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="rounded-md bg-primary/10 p-2 text-primary">
                        <Server className="h-4 w-4" />
                      </div>
                      <div className="flex flex-col items-end gap-1">
                        <span className={`rounded-full border px-2 py-0.5 text-[10px] ${statusMeta.className}`}>{statusMeta.label}</span>
                        <span className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${getScopeBadgeClassName("global")}`}>
                          {t("workspaceScopeUser", "User-level")}
                        </span>
                      </div>
                    </div>

                    <h4 className="mt-3 line-clamp-1 text-sm font-semibold">{server.name}</h4>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                      {server.description?.trim() || t("workspaceMcpNoDescription", "No description")}
                    </p>

                    <div className="mt-3 line-clamp-1 font-mono text-[11px] text-muted-foreground">{getMcpConnectionText(server)}</div>
                    <div className="mt-2 text-[11px] text-muted-foreground">
                      {t("workspaceMcpEnabledModels", "Enabled models")}: {formatEnabledModels(enabled_models)}
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-2">
                      <button
                        type="button"
                        onClick={() => {
                          if (enabledForCurrentModel) {
                            onOpenMcpInstallDialog(server);
                            return;
                          }
                          onEnableWorkspaceMcpForActiveModel(server);
                        }}
                        className={`inline-flex items-center gap-2 rounded-md px-3 py-1.5 text-xs font-medium ${
                          enabledForCurrentModel ? "border hover:bg-muted" : "bg-primary text-primary-foreground hover:bg-primary/90"
                        }`}
                      >
                        <Settings2 className="h-3.5 w-3.5" />
                        {enabledForCurrentModel
                          ? t("workspaceMcpManageModels", "Manage Models")
                          : enabledAtUserLevel
                            ? t("workspaceMcpPromoteToDirectoryLevel", "Enable Directory-level")
                            : t("workspaceMcpEnableCurrentModel", "Enable Current Model")}
                      </button>
                      {enabledForOtherModels && (
                        <button
                          type="button"
                          onClick={() => onOpenMcpInstallDialog(server)}
                          className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1.5 text-xs font-medium hover:bg-muted"
                        >
                          <Settings2 className="h-3.5 w-3.5" />
                          {t("workspaceMcpManageModels", "Manage Models")}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}
    </div>
  );
}
