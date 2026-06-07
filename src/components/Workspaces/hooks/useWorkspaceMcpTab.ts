import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  deriveWorkspaceAvailableMcpEntries,
  deriveWorkspaceEffectiveMcpEntriesByModel,
  deriveWorkspaceGlobalMcpEntries,
  deriveWorkspaceProjectMcpEntries,
  formatEnabledModels,
  normalizeWorkspaceDetail,
  normalizeMcpModelSwitchState,
  normalizeMcpServer,
} from "../helpers/workspaceHelpers";
import {
  DEFAULT_MCP_MODEL_SWITCH_STATE,
  TOOL_OPTIONS,
  type ApiResp,
  type MCPModelSwitchState,
  type MCPStateResp,
  type MCPServer,
  type ModelId,
  type WorkspaceDetail,
} from "../types";

export function useWorkspaceMcpTab(args: {
  isVisible: boolean;
  isTauri: boolean;
  activeTab: "sessions" | "mcp" | "skills" | "subagents";
  activeWorkspaceId: string | null;
  activeWorkspace: { id: string; name: string } | null;
  activeDetail: WorkspaceDetail | null;
  setActiveDetail: (detail: WorkspaceDetail) => void;
  setMessage: (message: { type: "success" | "error"; text: string } | null) => void;
  loadWorkspaces: () => Promise<void>;
}) {
  const { isVisible, isTauri, activeTab, activeWorkspaceId, activeWorkspace, activeDetail, setActiveDetail, setMessage, loadWorkspaces } = args;
  const { t } = useTranslation();
  const [activeMcpModel, setActiveMcpModel] = useState<ModelId>("claude");
  const [mcpServers, setMcpServers] = useState<MCPServer[]>([]);
  const [mcpModelSwitchStates, setMcpModelSwitchStates] = useState<Record<string, MCPModelSwitchState>>({});
  const [mcpLoading, setMcpLoading] = useState(false);
  const [mcpInitialized, setMcpInitialized] = useState(false);
  const [mcpDialogServer, setMcpDialogServer] = useState<MCPServer | null>(null);
  const [mcpDialogModels, setMcpDialogModels] = useState<ModelId[]>([]);
  const [mcpDialogSubmitting, setMcpDialogSubmitting] = useState(false);
  const [mcpDialogError, setMcpDialogError] = useState("");

  const loadMcpServers = useCallback(async (force = false) => {
    if (!isTauri) return;
    if (!force && mcpServers.length > 0) {
      setMcpInitialized(true);
      setMcpLoading(false);
      return;
    }
    const startedAt = Date.now();
    try {
      setMcpLoading(true);
      const resp = await invoke<MCPStateResp>("get_mcp_servers");
      const nextServers = Array.isArray(resp?.servers) ? resp.servers.map((server) => normalizeMcpServer(server)) : [];
      setMcpServers(nextServers);
      const defaultSwitches = nextServers.reduce<Record<string, MCPModelSwitchState>>((acc, server) => {
        acc[server.id] = { ...DEFAULT_MCP_MODEL_SWITCH_STATE };
        return acc;
      }, {});
      if (nextServers.length > 0) {
        try {
          const switches = await invoke<Record<string, MCPModelSwitchState>>("get_mcp_model_switch_states");
          const normalizedSwitches = Object.entries(switches || {}).reduce<Record<string, MCPModelSwitchState>>(
            (acc, [serverId, state]) => {
              acc[serverId] = normalizeMcpModelSwitchState(state);
              return acc;
            },
            {},
          );
          setMcpModelSwitchStates({ ...defaultSwitches, ...normalizedSwitches });
        } catch (e) {
          console.error("Failed to load MCP model switches", e);
          setMcpModelSwitchStates(defaultSwitches);
        }
      } else {
        setMcpModelSwitchStates({});
      }
      setMcpInitialized(true);
    } catch (e) {
      console.error("Failed to load MCP servers", e);
      setMcpInitialized(true);
    } finally {
      const elapsed = Date.now() - startedAt;
      if (elapsed < 200) {
        await new Promise((resolve) => window.setTimeout(resolve, 200 - elapsed));
      }
      setMcpLoading(false);
    }
  }, [isTauri, mcpServers.length]);

  useEffect(() => {
    if (!isVisible || !activeWorkspace || activeTab !== "mcp") return;
    void loadMcpServers(true);
  }, [activeTab, activeWorkspace, isVisible, loadMcpServers]);

  const mcpBindingMap = useMemo(() => {
    const next = new Map<string, string[]>();
    (activeDetail?.mcp_bindings || []).forEach((binding) => {
      next.set(binding.server_id, binding.enabled_models || []);
    });
    return next;
  }, [activeDetail]);

  const workspaceProjectMcpEntries = useMemo(
    () => deriveWorkspaceProjectMcpEntries(activeDetail, mcpServers),
    [activeDetail, mcpServers],
  );

  const workspaceGlobalMcpEntries = useMemo(
    () => deriveWorkspaceGlobalMcpEntries(mcpServers, mcpModelSwitchStates),
    [mcpModelSwitchStates, mcpServers],
  );

  const workspaceEffectiveMcpEntriesByModel = useMemo(
    () => deriveWorkspaceEffectiveMcpEntriesByModel(workspaceGlobalMcpEntries, workspaceProjectMcpEntries),
    [workspaceGlobalMcpEntries, workspaceProjectMcpEntries],
  );

  const workspaceInstalledCountsByModel = useMemo(
    () => ({
      claude: workspaceEffectiveMcpEntriesByModel.claude.length,
      gemini: workspaceEffectiveMcpEntriesByModel.gemini.length,
      codex: workspaceEffectiveMcpEntriesByModel.codex.length,
      opencode: workspaceEffectiveMcpEntriesByModel.opencode.length,
    }),
    [workspaceEffectiveMcpEntriesByModel],
  );

  const workspaceInstalledCards = useMemo(
    () => workspaceEffectiveMcpEntriesByModel[activeMcpModel] || [],
    [activeMcpModel, workspaceEffectiveMcpEntriesByModel],
  );

  const activeMcpLoadRule = useMemo(() => {
    switch (activeMcpModel) {
      case "claude":
        return t(
          "workspaceMcpLoadRuleClaude",
          "Claude Code merges MCP by scope. Same-name servers resolve as local > project > user > plugin/connectors; different names are kept side by side.",
        );
      case "gemini":
        return t(
          "workspaceMcpLoadRuleGemini",
          "Gemini merges mcpServers from system, workspace, and user settings. Same-name servers resolve as system > workspace > user.",
        );
      case "codex":
        return t(
          "workspaceMcpLoadRuleCodex",
          "Codex reads user config plus trusted project .codex/config.toml files. Same-name MCP keys from the closest project config override user config.",
        );
      case "opencode":
      default:
        return t(
          "workspaceMcpLoadRuleOpenCode",
          "OpenCode merges config files instead of replacing them. Project opencode.json overrides global MCP keys with the same name; non-conflicting keys remain.",
        );
    }
  }, [activeMcpModel, t]);

  const workspaceAvailableMcpEntries = useMemo(
    () =>
      deriveWorkspaceAvailableMcpEntries({
        activeDetail,
        activeMcpModel,
        mcpServers,
        mcpModelSwitchStates,
      }),
    [activeDetail, activeMcpModel, mcpModelSwitchStates, mcpServers],
  );

  const getWorkspaceMcpStatusMeta = useCallback(
    (status: "enabled_for_model" | "enabled_user_level" | "bound_other_models" | "not_bound") => {
      if (status === "enabled_for_model") {
        return {
          label: t("workspaceMcpStatusEnabledForModel", "Enabled for current model"),
          className: "border-emerald-500/30 bg-emerald-500/10 text-emerald-700",
        };
      }
      if (status === "bound_other_models") {
        return {
          label: t("workspaceMcpStatusBoundOtherModels", "Enabled for other models"),
          className: "border-amber-500/30 bg-amber-500/10 text-amber-700",
        };
      }
      if (status === "enabled_user_level") {
        return {
          label: t("workspaceMcpStatusEnabledUserLevel", "Enabled at user level"),
          className: "border-blue-500/30 bg-blue-500/10 text-blue-700",
        };
      }
      return {
        label: t("workspaceMcpStatusNotBound", "Not enabled yet"),
        className: "border-border bg-muted/30 text-muted-foreground",
      };
    },
    [t],
  );

  const saveWorkspaceMcpBinding = useCallback(
    async (serverId: string, nextModels: ModelId[]) => {
      if (!activeWorkspaceId || !isTauri) return null;
      try {
        const resp = await invoke<ApiResp<WorkspaceDetail>>("workspace_mcp_binding_upsert", {
          input: {
            workspace_id: activeWorkspaceId,
            server_id: serverId,
            enabled_models: nextModels,
          },
        });
        const detail = resp.data && typeof resp.data === "object" ? normalizeWorkspaceDetail(resp.data) : null;
        if (!detail) return null;
        setActiveDetail(detail);
        emit("refresh-counts").catch(() => {});
        await loadWorkspaces();
        return detail;
      } catch (e: any) {
        setMessage({
          type: "error",
          text: t("workspaceMcpUpdateFailed", "Failed to update MCP binding: {{message}}", {
            message: String(e),
          }),
        });
        return null;
      }
    },
    [activeWorkspaceId, isTauri, loadWorkspaces, setActiveDetail, setMessage, t],
  );

  const openMcpInstallDialog = useCallback(
    async (server: MCPServer) => {
      if (!isTauri) return;
      await loadMcpServers();
      const currentModels = (mcpBindingMap.get(server.id) || []).filter((model): model is ModelId =>
        TOOL_OPTIONS.some((item) => item.id === model),
      );
      setMcpDialogServer(server);
      setMcpDialogModels(currentModels.length > 0 ? currentModels : [activeMcpModel]);
      setMcpDialogError("");
    },
    [activeMcpModel, isTauri, loadMcpServers, mcpBindingMap],
  );

  const toggleMcpDialogModel = useCallback(
    (model: ModelId) => {
      setMcpDialogModels((prev) => (prev.includes(model) ? prev.filter((item) => item !== model) : [...prev, model]));
      if (mcpDialogError) {
        setMcpDialogError("");
      }
    },
    [mcpDialogError],
  );

  const handleSaveMcpDialog = useCallback(async () => {
    if (!mcpDialogServer) return;
    if (mcpDialogModels.length === 0) {
      setMcpDialogError(t("workspaceMcpInstallModelsRequired", "Choose at least one model."));
      return;
    }
    try {
      setMcpDialogSubmitting(true);
      const nextModels = TOOL_OPTIONS.map((item) => item.id).filter((item) => mcpDialogModels.includes(item));
      const detail = await saveWorkspaceMcpBinding(mcpDialogServer.id, nextModels);
      if (!detail) return;
      setMcpDialogServer(null);
      setMcpDialogModels([]);
      setMcpDialogError("");
      setActiveMcpModel(nextModels[0] || "claude");
    } finally {
      setMcpDialogSubmitting(false);
    }
  }, [mcpDialogModels, mcpDialogServer, saveWorkspaceMcpBinding, t]);

  const handleUninstallWorkspaceMcpForModel = useCallback(
    async (serverId: string, model: ModelId) => {
      const currentModels = new Set(mcpBindingMap.get(serverId) || []);
      currentModels.delete(model);
      const nextModels = TOOL_OPTIONS.map((item) => item.id).filter((item) => currentModels.has(item));
      await saveWorkspaceMcpBinding(serverId, nextModels);
    },
    [mcpBindingMap, saveWorkspaceMcpBinding],
  );

  const handleEnableWorkspaceMcpForActiveModel = useCallback(
    async (server: MCPServer) => {
      const currentModels = new Set(
        (mcpBindingMap.get(server.id) || []).filter((model): model is ModelId =>
          TOOL_OPTIONS.some((item) => item.id === model),
        ),
      );
      currentModels.add(activeMcpModel);
      const nextModels = TOOL_OPTIONS.map((item) => item.id).filter((item) => currentModels.has(item));
      await saveWorkspaceMcpBinding(server.id, nextModels);
    },
    [activeMcpModel, mcpBindingMap, saveWorkspaceMcpBinding],
  );

  return {
    activeMcpModel,
    setActiveMcpModel,
    mcpServers,
    mcpLoading,
    setMcpLoading,
    mcpInitialized,
    mcpDialogServer,
    setMcpDialogServer,
    mcpDialogModels,
    setMcpDialogModels,
    mcpDialogSubmitting,
    mcpDialogError,
    setMcpDialogError,
    workspaceInstalledCountsByModel,
    workspaceInstalledCards,
    activeMcpLoadRule,
    workspaceAvailableMcpEntries,
    mcpBindingMap,
    loadMcpServers,
    openMcpInstallDialog,
    toggleMcpDialogModel,
    handleSaveMcpDialog,
    handleUninstallWorkspaceMcpForModel,
    handleEnableWorkspaceMcpForActiveModel,
    formatEnabledModels: (models: string[]) => formatEnabledModels(t, models),
    getWorkspaceMcpStatusMeta,
  };
}
