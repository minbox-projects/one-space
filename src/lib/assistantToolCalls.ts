import type { AssistantToolCall } from "@/lib/aiWorkspace";

function normalizeToken(token: string) {
  const lower = token.toLowerCase();
  if (lower === "mcp") return "MCP";
  if (lower === "api") return "API";
  if (lower === "url") return "URL";
  if (lower === "id") return "ID";
  return lower.charAt(0).toUpperCase() + lower.slice(1);
}

export function humanizeToolName(name?: string | null) {
  const value = name?.trim() || "";
  if (!value) return "";
  return value
    .split(/[._:/\-\s]+/)
    .filter(Boolean)
    .map(normalizeToken)
    .join(" ");
}

export function formatMcpServerLabel(
  serverId?: string | null,
  serverName?: string | null,
) {
  const trimmedId = serverId?.trim() || "";
  const trimmedName = serverName?.trim() || "";
  if (trimmedName && trimmedId) {
    return `${trimmedName} (${trimmedId})`;
  }
  return trimmedName || trimmedId;
}

export function mapMcpServerIdsToLabels(
  serverIds: string[],
  serverNameById: ReadonlyMap<string, string>,
) {
  return serverIds.map((serverId) =>
    formatMcpServerLabel(serverId, serverNameById.get(serverId)),
  );
}

export function getToolCallDisplayName(tool: Pick<
  AssistantToolCall,
  "display_name" | "original_tool_name" | "name"
>) {
  return (
    tool.display_name?.trim() ||
    humanizeToolName(tool.original_tool_name) ||
    humanizeToolName(tool.name) ||
    tool.original_tool_name?.trim() ||
    tool.name
  );
}

export function getToolCallMeta(tool: Pick<
  AssistantToolCall,
  "server_id" | "server_name" | "original_tool_name" | "name" | "display_name"
>) {
  const parts: string[] = [];
  const serverLabel = formatMcpServerLabel(tool.server_id, tool.server_name);
  if (serverLabel) {
    parts.push(serverLabel);
  }

  const actualToolName = tool.original_tool_name?.trim() || tool.name?.trim() || "";
  if (actualToolName) {
    const displayName = getToolCallDisplayName(tool).trim();
    if (!displayName || actualToolName !== displayName) {
      parts.push(actualToolName);
    }
  }

  return parts.join(" · ");
}

export function upsertToolCall(
  toolCalls: AssistantToolCall[],
  nextTool: AssistantToolCall,
) {
  const byId = nextTool.id?.trim();
  if (byId) {
    const existingIndex = toolCalls.findIndex((item) => item.id === byId);
    if (existingIndex >= 0) {
      const next = [...toolCalls];
      next[existingIndex] = nextTool;
      return next;
    }
  }

  const fallbackIndex = toolCalls.findIndex(
    (item) =>
      item.name === nextTool.name &&
      item.started_at === nextTool.started_at &&
      item.status !== nextTool.status,
  );
  if (fallbackIndex >= 0) {
    const next = [...toolCalls];
    next[fallbackIndex] = nextTool;
    return next;
  }

  return [...toolCalls, nextTool];
}
