import { describe, expect, it } from "vitest";
import {
  collectWorkspaceTags,
  deriveWorkspaceAvailableMcpEntries,
  deriveWorkspaceEffectiveMcpEntriesByModel,
  deriveWorkspaceGlobalMcpEntries,
  deriveWorkspaceProjectMcpEntries,
  filterWorkspacesByTags,
  getMcpEnabledModelsFromSwitch,
  normalizeWorkspaceDetail,
  normalizeWorkspaceView,
} from "@/components/Workspaces/helpers/workspaceHelpers";

describe("workspaceHelpers", () => {
  it("normalizes workspace records and details", () => {
    const view = normalizeWorkspaceView({
      workspace: { id: 1, name: "Demo", root_path: "/tmp/demo", tags: ["a", "a", "b"], source: "manual" },
      session_count: "2",
    });
    const detail = normalizeWorkspaceDetail({
      workspace: view,
      mcp_bindings: [{ workspace_id: 1, server_id: "srv", enabled_models: ["claude", "claude", "codex"] }],
    });
    expect(view.workspace.id).toBe("1");
    expect(view.session_count).toBe(2);
    expect(detail.mcp_bindings[0].enabled_models).toEqual(["claude", "codex"]);
  });

  it("collects and filters workspace tags case-insensitively", () => {
    const workspaces = [
      normalizeWorkspaceView({ id: "1", name: "One", root_path: "/a", tags: ["Frontend", "Work"], source: "manual" }),
      normalizeWorkspaceView({ id: "2", name: "Two", root_path: "/b", tags: ["personal"], source: "manual" }),
    ];
    expect(collectWorkspaceTags(workspaces)).toEqual(["Frontend", "personal", "Work"]);
    expect(filterWorkspacesByTags(workspaces, ["frontend"])).toHaveLength(1);
    expect(filterWorkspacesByTags(workspaces, ["personal"])[0].workspace.id).toBe("2");
  });

  it("derives effective MCP precedence and catalog status", () => {
    const activeDetail = normalizeWorkspaceDetail({
      workspace: { workspace: { id: "1", name: "Demo", root_path: "/tmp", tags: [], source: "manual" }, session_count: 0 },
      mcp_bindings: [{ workspace_id: "1", server_id: "srv-1", enabled_models: ["claude"] }],
    });
    const servers = [
      { id: "srv-1", name: "Alpha", transport: "stdio" as const },
      { id: "srv-2", name: "Beta", transport: "stdio" as const },
    ];
    const globalEntries = deriveWorkspaceGlobalMcpEntries(servers, {
      "srv-1": { claude: true, gemini: false, codex: false, opencode: false },
      "srv-2": { claude: false, gemini: true, codex: false, opencode: false },
    });
    const projectEntries = deriveWorkspaceProjectMcpEntries(activeDetail, servers);
    const effective = deriveWorkspaceEffectiveMcpEntriesByModel(globalEntries, projectEntries);
    expect(getMcpEnabledModelsFromSwitch({ claude: true, gemini: false, codex: false, opencode: true })).toEqual(["claude", "opencode"]);
    expect(effective.claude).toHaveLength(1);
    expect(effective.claude[0].scope).toBe("project");

    const catalog = deriveWorkspaceAvailableMcpEntries({
      activeDetail,
      activeMcpModel: "claude",
      mcpServers: servers,
      mcpModelSwitchStates: {
        "srv-1": { claude: false, gemini: false, codex: false, opencode: false },
        "srv-2": { claude: true, gemini: false, codex: false, opencode: false },
      },
    });
    expect(catalog.find((item) => item.server.id === "srv-1")?.status).toBe("enabled_for_model");
    expect(catalog.find((item) => item.server.id === "srv-2")?.status).toBe("enabled_user_level");
  });
});
