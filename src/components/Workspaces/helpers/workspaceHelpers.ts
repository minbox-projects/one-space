import type { TFunction } from "i18next";
import {
  DEFAULT_MCP_MODEL_SWITCH_STATE,
  TOOL_OPTIONS,
  type MCPModelSwitchState,
  type MCPServer,
  type ModelId,
  type WorkspaceDetail,
  type WorkspaceMcpCatalogEntry,
  type WorkspaceMcpEntry,
  type WorkspaceRecord,
  type WorkspaceView,
} from "../types";

export function formatTs(ts?: number) {
  if (!ts) return "--";
  return new Date(ts * 1000).toLocaleString();
}

export function parseTagsInput(value: string) {
  return Array.from(
    new Set(
      value
        .split(/[,\n]/)
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

export function buildSkillSelectionKey(item: {
  model: string;
  source_id: string;
  source_rel_path: string;
}) {
  return `${item.model}::${item.source_id}::${item.source_rel_path}`;
}

export function buildSubagentSelectionKey(item: {
  model: string;
  source_id: string;
  source_rel_path: string;
}) {
  return `${item.model}::${item.source_id}::${item.source_rel_path}`;
}

export function normalizeText(value: unknown, fallback = "") {
  if (typeof value === "string") return value;
  if (value == null) return fallback;
  return String(value);
}

export function normalizeOptionalText(value: unknown) {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed ? value : null;
}

export function normalizeStringArray(value: unknown) {
  const source = Array.isArray(value) ? value : typeof value === "string" ? [value] : [];
  return Array.from(
    new Set(
      source
        .map((item) => normalizeText(item).trim())
        .filter(Boolean),
    ),
  );
}

export function compactWorkspaceRootPath(path: string) {
  const trimmed = normalizeText(path).trim();
  if (!trimmed) return "";

  const homePrefix =
    trimmed.match(/^\/Users\/[^/]+(?=\/|$)/)?.[0] ?? trimmed.match(/^\/home\/[^/]+(?=\/|$)/)?.[0];

  return homePrefix ? `~${trimmed.slice(homePrefix.length)}` : trimmed;
}

export function normalizeWorkspaceRecord(raw: any): WorkspaceRecord {
  return {
    id: normalizeText(raw?.id),
    name: normalizeText(raw?.name),
    root_path: normalizeText(raw?.root_path),
    description: normalizeOptionalText(raw?.description),
    tags: normalizeStringArray(raw?.tags),
    source: normalizeText(raw?.source),
    created_at: Number(raw?.created_at) || 0,
    updated_at: Number(raw?.updated_at) || 0,
    last_activity_at: Number(raw?.last_activity_at) || 0,
  };
}

export function normalizeWorkspaceView(raw: any): WorkspaceView {
  return {
    workspace: normalizeWorkspaceRecord(raw?.workspace ?? raw),
    session_count: Number(raw?.session_count) || 0,
  };
}

export function normalizeWorkspaceDetail(raw: any): WorkspaceDetail {
  const bindings = Array.isArray(raw?.mcp_bindings)
    ? raw.mcp_bindings.map((binding: any) => ({
        workspace_id: normalizeText(binding?.workspace_id),
        server_id: normalizeText(binding?.server_id),
        enabled_models: normalizeStringArray(binding?.enabled_models),
        created_at: Number(binding?.created_at) || 0,
        updated_at: Number(binding?.updated_at) || 0,
      }))
    : [];

  return {
    workspace: normalizeWorkspaceView(raw?.workspace),
    mcp_bindings: bindings,
  };
}

export function createOptimisticWorkspaceDetail(
  view: WorkspaceView,
  previous: WorkspaceDetail | null,
): WorkspaceDetail {
  const previousWorkspaceId = previous?.workspace.workspace.id;
  return {
    workspace: view,
    mcp_bindings: previousWorkspaceId === view.workspace.id ? previous?.mcp_bindings || [] : [],
  };
}

export function normalizeMcpServer(raw: any): MCPServer {
  const transport = normalizeText(raw?.transport, "stdio").trim().toLowerCase();
  return {
    id: normalizeText(raw?.id),
    name: normalizeText(raw?.name, normalizeText(raw?.id)),
    config_key: normalizeOptionalText(raw?.config_key) || undefined,
    description: normalizeOptionalText(raw?.description) || undefined,
    transport:
      transport === "http" || transport === "sse" || transport === "stdio"
        ? transport
        : "stdio",
    command: normalizeOptionalText(raw?.command) || undefined,
    args: Array.isArray(raw?.args)
      ? raw.args.map((item: unknown) => normalizeText(item)).filter(Boolean)
      : undefined,
    url: normalizeOptionalText(raw?.url) || undefined,
    http_url: normalizeOptionalText(raw?.http_url) || undefined,
  };
}

export function sortMcpServersByName(a: MCPServer, b: MCPServer) {
  return normalizeText(a?.name).localeCompare(normalizeText(b?.name), undefined, {
    sensitivity: "base",
  });
}

export function normalizeMcpModelSwitchState(raw: any): MCPModelSwitchState {
  return {
    claude: Boolean(raw?.claude),
    gemini: Boolean(raw?.gemini),
    codex: Boolean(raw?.codex),
    opencode: Boolean(raw?.opencode),
  };
}

export function getMcpMergeKey(server: MCPServer) {
  return normalizeText(server.config_key || server.name || server.id)
    .trim()
    .toLowerCase();
}

export function getMcpEnabledModelsFromSwitch(state: MCPModelSwitchState | undefined) {
  const normalized = state || DEFAULT_MCP_MODEL_SWITCH_STATE;
  return TOOL_OPTIONS.flatMap((tool) => (normalized[tool.id] ? [tool.id] : []));
}

export function getMcpConnectionText(server: MCPServer) {
  const command = normalizeText(server.command).trim();
  if (command) {
    const args = Array.isArray(server.args) ? server.args.join(" ") : "";
    return `${command}${args ? ` ${args}` : ""}`;
  }
  return normalizeText(server.http_url || server.url, "-");
}

export function getSourceBadgeLabel(source: string) {
  const normalized = String(source || "").trim().toLowerCase();
  if (normalized === "session_auto") return "Auto";
  if (normalized === "copy_target") return "Copied";
  return "Manual";
}

export function getSourceBadgeDescription(source: string) {
  const normalized = String(source || "").trim().toLowerCase();
  if (normalized === "session_auto") {
    return "Created automatically from an existing AI session working directory.";
  }
  if (normalized === "copy_target") {
    return "Created as the target workspace when copying configuration from another workspace.";
  }
  return "Created manually from the workspace manager.";
}

export function getSourceBadgeClassName(source: string) {
  const normalized = String(source || "").trim().toLowerCase();
  if (normalized === "session_auto") {
    return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:border-emerald-400/30 dark:bg-emerald-400/10 dark:text-emerald-300";
  }
  return "border-border text-muted-foreground";
}

export function getScopeBadgeClassName(scope?: "global" | "project") {
  return scope === "global"
    ? "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300"
    : "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
}

export function getSourceBadgeTranslationKeys(source: string) {
  const normalized = String(source || "").trim().toLowerCase();
  if (normalized === "session_auto") {
    return {
      label: "workspaceSourceAuto",
      description: "workspaceSourceAutoDesc",
    };
  }
  if (normalized === "copy_target") {
    return {
      label: "workspaceSourceCopied",
      description: "workspaceSourceCopiedDesc",
    };
  }
  return {
    label: "workspaceSourceManual",
    description: "workspaceSourceManualDesc",
  };
}

export function collectWorkspaceTags(workspaces: WorkspaceView[]) {
  return Array.from(new Set(workspaces.flatMap((item) => item.workspace.tags || []).filter(Boolean))).sort((a, b) =>
    a.localeCompare(b),
  );
}

export function filterWorkspacesByTags(workspaces: WorkspaceView[], selectedTags: string[]) {
  if (selectedTags.length === 0) {
    return workspaces;
  }
  const selected = new Set(selectedTags.map((item) => item.trim().toLowerCase()).filter(Boolean));
  return workspaces.filter((item) =>
    (item.workspace.tags || []).some((tag) => selected.has(tag.trim().toLowerCase())),
  );
}

export function deriveWorkspaceProjectMcpEntries(
  activeDetail: WorkspaceDetail | null,
  mcpServers: MCPServer[],
): WorkspaceMcpEntry[] {
  const serverMap = new Map(mcpServers.map((server) => [server.id, server]));
  return (activeDetail?.mcp_bindings || [])
    .map((binding) => ({
      server:
        serverMap.get(binding.server_id) || {
          id: binding.server_id,
          name: binding.server_id,
          transport: "stdio" as const,
        },
      binding,
      scope: "project" as const,
      enabled_models: (binding.enabled_models || []).filter((model): model is ModelId =>
        TOOL_OPTIONS.some((tool) => tool.id === model),
      ),
    }))
    .sort((a, b) => sortMcpServersByName(a.server, b.server));
}

export function deriveWorkspaceGlobalMcpEntries(
  mcpServers: MCPServer[],
  mcpModelSwitchStates: Record<string, MCPModelSwitchState>,
) {
  return mcpServers
    .map((server) => ({
      server,
      binding: null,
      scope: "global" as const,
      enabled_models: getMcpEnabledModelsFromSwitch(mcpModelSwitchStates[server.id]),
    }))
    .filter((entry) => entry.enabled_models.length > 0)
    .sort((a, b) => sortMcpServersByName(a.server, b.server));
}

export function deriveWorkspaceEffectiveMcpEntriesByModel(
  workspaceGlobalMcpEntries: WorkspaceMcpEntry[],
  workspaceProjectMcpEntries: WorkspaceMcpEntry[],
) {
  const next: Record<ModelId, WorkspaceMcpEntry[]> = {
    claude: [],
    gemini: [],
    codex: [],
    opencode: [],
  };

  TOOL_OPTIONS.forEach((tool) => {
    const byKey = new Map<string, WorkspaceMcpEntry>();
    workspaceGlobalMcpEntries.forEach((entry) => {
      if (!entry.enabled_models.includes(tool.id)) return;
      const key = getMcpMergeKey(entry.server);
      if (key) byKey.set(key, entry);
    });
    workspaceProjectMcpEntries.forEach((entry) => {
      if (!entry.enabled_models.includes(tool.id)) return;
      const key = getMcpMergeKey(entry.server);
      if (key) byKey.set(key, entry);
    });
    next[tool.id] = Array.from(byKey.values()).sort((a, b) => sortMcpServersByName(a.server, b.server));
  });

  return next;
}

export function deriveWorkspaceAvailableMcpEntries(args: {
  activeDetail: WorkspaceDetail | null;
  activeMcpModel: ModelId;
  mcpServers: MCPServer[];
  mcpModelSwitchStates: Record<string, MCPModelSwitchState>;
}): WorkspaceMcpCatalogEntry[] {
  const { activeDetail, activeMcpModel, mcpServers, mcpModelSwitchStates } = args;
  return [...mcpServers].sort(sortMcpServersByName).map((server) => {
    const binding = (activeDetail?.mcp_bindings || []).find((item) => item.server_id === server.id) || null;
    const enabledModels = (binding?.enabled_models || []).filter((model): model is ModelId =>
      TOOL_OPTIONS.some((tool) => tool.id === model),
    );
    const globalEnabledModels = getMcpEnabledModelsFromSwitch(mcpModelSwitchStates[server.id]);
    const status: WorkspaceMcpCatalogEntry["status"] = enabledModels.includes(activeMcpModel)
      ? "enabled_for_model"
      : globalEnabledModels.includes(activeMcpModel)
        ? "enabled_user_level"
        : enabledModels.length > 0
          ? "bound_other_models"
          : "not_bound";
    return {
      server,
      binding,
      scope: "global" as const,
      enabled_models: enabledModels.length > 0 ? enabledModels : globalEnabledModels,
      status,
    };
  });
}

export function formatEnabledModels(t: TFunction, models: string[]) {
  if (models.length === 0) {
    return t("workspaceMcpNoEnabledModels", "No models enabled");
  }
  return models
    .map((model) => TOOL_OPTIONS.find((item) => item.id === model)?.label || model)
    .join(" · ");
}
