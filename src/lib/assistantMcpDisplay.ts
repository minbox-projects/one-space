import type {
  ManagedMcpCatalogResponse,
  ManagedMcpServerCatalogItem,
  McpImpactTag,
} from "@/lib/aiWorkspace";

export interface McpServerCardItem {
  serverId: string;
  name: string;
  meta: string;
  connectionStatus: "ready" | "failed" | "unchecked" | string;
  connectionLabel: string;
  summary: string;
  previewSummary: string;
  toolNames: string[];
  impactLabels: string[];
}

type TranslateFn = (
  key: string,
  defaultValue: string,
  options?: Record<string, unknown>,
) => string;

export function mcpCategoryLabel(
  category: ManagedMcpServerCatalogItem["category"],
  t: TranslateFn,
) {
  switch (category) {
    case "search":
      return t("mcpCategorySearch", "Search");
    case "docs":
      return t("mcpCategoryDocs", "Docs");
    case "workspace":
      return t("mcpCategoryWorkspace", "Workspace");
    case "automation":
      return t("mcpCategoryAutomation", "Automation");
    default:
      return t("mcpCategoryIntegration", "Integration");
  }
}

export function mcpImpactLabel(tag: McpImpactTag, t: TranslateFn) {
  switch (tag) {
    case "network":
      return t("mcpImpactNetwork", "Network");
    case "remote_api":
      return t("mcpImpactRemoteApi", "Remote API");
    case "credentials":
      return t("mcpImpactCredentials", "Credentials");
    case "workspace_read":
      return t("mcpImpactWorkspaceRead", "Workspace Read");
    case "workspace_write":
      return t("mcpImpactWorkspaceWrite", "Workspace Write");
    case "data_access":
      return t("mcpImpactDataAccess", "Data Access");
    case "local_state":
      return t("mcpImpactLocalState", "Local State");
    case "browser_automation":
      return t("mcpImpactBrowser", "Browser");
    case "trusted":
      return t("mcpImpactTrusted", "Trusted");
    default:
      return tag;
  }
}

export function mcpPreviewSummary(
  item: ManagedMcpServerCatalogItem,
  t: TranslateFn,
) {
  if (item.tool_preview.status === "ready") {
    return t("mcpPreviewReady", "{{count}} tools cached", {
      count: item.tool_preview.tool_count,
    });
  }
  if (item.tool_preview.status === "failed") {
    return item.tool_preview.error || t("mcpPreviewFailed", "Preview failed");
  }
  return t("mcpPreviewUnchecked", "Preview not fetched yet");
}

export function mcpConnectionStatusLabel(
  status: string,
  t: TranslateFn,
) {
  switch (status) {
    case "ready":
      return t("mcpConnectionStatusReady", "Connected");
    case "failed":
      return t("mcpConnectionStatusFailed", "Connection Failed");
    default:
      return t("mcpConnectionStatusUnchecked", "Not Checked");
  }
}

export function buildMcpServerCardItems(
  catalog: ManagedMcpCatalogResponse | null,
  selectedIds: string[],
  t: TranslateFn,
): McpServerCardItem[] {
  const itemsById = new Map((catalog?.items || []).map((item) => [item.server_id, item]));
  return selectedIds.map((serverId, index) => {
    const item =
      itemsById.get(serverId) || {
        server_id: serverId,
        config_key: "",
        name: t("mcpCustomServerName", "Custom MCP {{index}}", {
          index: index + 1,
        }),
        description: "",
        transport: "unknown",
        category: "integration" as const,
        capability_summary: t(
          "mcpMissingCatalogDesc",
          "This assistant references a server that is not available in the managed catalog.",
        ),
        capability_tags: [],
        impact_tags: [],
        impact_note: null,
        tool_preview: {
          status: "unchecked",
          checked_at: null,
          error: null,
          tool_count: 0,
          tools: [],
        },
      };

    return {
      serverId: item.server_id,
      name: item.name,
      meta: `${mcpCategoryLabel(item.category, t)} · ${item.transport}`,
      connectionStatus: item.tool_preview.status,
      connectionLabel: mcpConnectionStatusLabel(item.tool_preview.status, t),
      summary: item.capability_summary || item.description || item.server_id,
      previewSummary: mcpPreviewSummary(item, t),
      toolNames: item.tool_preview.tools.slice(0, 3).map((tool) => tool.name),
      impactLabels: item.impact_tags.map((tag) => mcpImpactLabel(tag, t)),
    };
  });
}
