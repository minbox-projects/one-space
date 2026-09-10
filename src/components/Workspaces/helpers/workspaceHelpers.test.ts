import { describe, expect, it } from "vitest";
import {
  collectWorkspaceTags,
  deriveWorkspaceAvailableMcpEntries,
  deriveWorkspaceEffectiveMcpEntriesByModel,
  deriveWorkspaceGlobalMcpEntries,
  deriveWorkspaceProjectMcpEntries,
  filterWorkspacesByTags,
  getMcpEnabledModelsFromSwitch,
  normalizeMcpModelSwitchState,
  normalizeWorkspaceDetail,
  normalizeWorkspaceView,
} from "@/components/Workspaces/helpers/workspaceHelpers";
import { DEFAULT_MCP_MODEL_SWITCH_STATE, TOOL_OPTIONS } from "@/components/Workspaces/types";

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

  it("includes Antigravity in the tool matrix with a default-disabled MCP switch", () => {
    expect(TOOL_OPTIONS).toContainEqual({ id: "antigravity", label: "Antigravity" });
    expect(TOOL_OPTIONS.map((tool) => tool.id)).not.toContain("gemini");
    expect(DEFAULT_MCP_MODEL_SWITCH_STATE.antigravity).toBe(false);
    expect(normalizeMcpModelSwitchState({}).antigravity).toBe(false);
    expect(normalizeMcpModelSwitchState({ antigravity: true }).antigravity).toBe(true);
    expect(getMcpEnabledModelsFromSwitch(undefined)).not.toContain("antigravity");
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
      "srv-1": { claude: true, antigravity: false, codex: false, opencode: false },
      "srv-2": { claude: false, antigravity: true, codex: false, opencode: false },
    });
    const projectEntries = deriveWorkspaceProjectMcpEntries(activeDetail, servers);
    const effective = deriveWorkspaceEffectiveMcpEntriesByModel(globalEntries, projectEntries);
    expect(getMcpEnabledModelsFromSwitch({ claude: true, antigravity: false, codex: false, opencode: true })).toEqual(["claude", "opencode"]);
    expect(effective.claude).toHaveLength(1);
    expect(effective.claude[0].scope).toBe("project");
    expect(effective.antigravity.map((entry) => entry.server.id)).toEqual(["srv-2"]);

    const catalog = deriveWorkspaceAvailableMcpEntries({
      activeDetail,
      activeMcpModel: "claude",
      mcpServers: servers,
      mcpModelSwitchStates: {
        "srv-1": { claude: false, antigravity: false, codex: false, opencode: false },
        "srv-2": { claude: true, antigravity: false, codex: false, opencode: false },
      },
    });
    expect(catalog.find((item) => item.server.id === "srv-1")?.status).toBe("enabled_for_model");
    expect(catalog.find((item) => item.server.id === "srv-2")?.status).toBe("enabled_user_level");
  });
});
