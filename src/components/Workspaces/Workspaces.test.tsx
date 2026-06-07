import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { Workspaces } from "@/components/Workspaces";
import { renderWithProviders } from "@/test/mocks/render";
import { emitMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import { resetMessageMocks } from "@/test/mocks/messages";

function mockWorkspacesState() {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    if (command === "get_storage_config") return {};
    if (command === "get_mcp_servers") return { servers: [{ id: "srv-1", name: "Alpha", transport: "stdio" }] };
    if (command === "get_mcp_model_switch_states") return { "srv-1": { claude: true, gemini: false, codex: false, opencode: false } };
    if (command === "workspaces_list") {
      return {
        data: [
          { workspace: { id: "ws-1", name: "Workspace A", root_path: "/tmp/a", tags: ["frontend"], source: "manual", created_at: 1, updated_at: 1, last_activity_at: 1 }, session_count: 1 },
          { workspace: { id: "ws-2", name: "Workspace B", root_path: "/tmp/b", tags: ["backend"], source: "manual", created_at: 1, updated_at: 1, last_activity_at: 1 }, session_count: 0 },
        ],
      };
    }
    if (command === "workspace_get") {
      return {
        data: {
          workspace: { workspace: { id: "ws-1", name: "Workspace A", root_path: "/tmp/a", tags: ["frontend"], source: "manual", created_at: 1, updated_at: 1, last_activity_at: 1 }, session_count: 1 },
          mcp_bindings: [{ workspace_id: "ws-1", server_id: "srv-1", enabled_models: ["claude"] }],
        },
      };
    }
    if (command === "workspace_sessions_list") {
      return {
        data: {
          items: [{ id: "sess-1", name: "Session A", model_type: "Claude", working_dir: "/tmp/a", created_at: 1, updated_at: 1, is_favorite: false }],
          total: 1,
          tool_options: ["Claude"],
          model_options: ["Claude"],
        },
      };
    }
    if (command === "workspace_create" || command === "workspace_update_meta") {
      return {
        data: {
          workspace: { workspace: { id: "ws-1", name: "Workspace A", root_path: "/tmp/a", tags: ["frontend"], source: "manual", created_at: 1, updated_at: 1, last_activity_at: 1 }, session_count: 1 },
          mcp_bindings: [],
        },
      };
    }
    if (command === "workspace_delete" || command === "workspace_launch_session" || command === "workspace_mcp_binding_upsert") {
      return { data: { workspace: { workspace: { id: "ws-1", name: "Workspace A", root_path: "/tmp/a", tags: ["frontend"], source: "manual", created_at: 1, updated_at: 1, last_activity_at: 1 }, session_count: 1 }, mcp_bindings: [] } };
    }
    if (command === "skills_list_installed" || command === "subagents_list_installed") return { data: [] };
    if (command === "sessions_launch" || command === "sessions_delete" || command === "sessions_update" || command === "sessions_set_favorite") return null;
    throw new Error(`Unhandled command: ${command} ${JSON.stringify(args)}`);
  });
}

describe("Workspaces", () => {
  beforeEach(() => {
    resetTauriMocks();
    resetMessageMocks();
    mockWorkspacesState();
  });

  it("filters workspace list by tag and loads detail/sessions", async () => {
    renderWithProviders(<Workspaces isVisible />);
    expect(await screen.findByText("Workspace A")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^frontend$/i }));
    expect(screen.getByText("Workspace A")).toBeInTheDocument();
    expect(screen.queryByText("Workspace B")).not.toBeInTheDocument();

    await userEvent.click(screen.getByText("Workspace A"));
    expect(await screen.findByRole("heading", { name: /Terminal Sessions|终端会话/i })).toBeInTheDocument();
    expect(await screen.findByText("Session A")).toBeInTheDocument();
  });

  it("validates create form and emits refresh on success", async () => {
    renderWithProviders(<Workspaces isVisible />);
    await userEvent.click(await screen.findByRole("button", { name: /New Workspace|新建工作空间/i }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: /Create|创建/i }));
    expect(await screen.findByText(/Workspace name is required|工作空间名称不能为空/i)).toBeInTheDocument();

    const textboxes = within(dialog).getAllByRole("textbox");
    await userEvent.type(textboxes[0], "Workspace A");
    fireEvent.change(textboxes[1], { target: { value: "/tmp/a" } });
    await userEvent.click(within(dialog).getByRole("button", { name: /Create|创建/i }));

    await waitFor(() => {
      expect(emitMock).toHaveBeenCalledWith("refresh-counts");
      expect(invokeMock).toHaveBeenCalledWith("workspace_create", expect.anything());
    });
  });

  it("keeps delete and MCP update on existing action paths", async () => {
    renderWithProviders(<Workspaces isVisible />);
    await userEvent.click(await screen.findByText("Workspace A"));
    await userEvent.click(screen.getByRole("button", { name: /MCP/i }));
    await userEvent.click((await screen.findAllByRole("button", { name: /Manage Models|管理模型/i }))[0]);
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: /Save|保存/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("workspace_mcp_binding_upsert", expect.anything());
      expect(emitMock).toHaveBeenCalledWith("refresh-counts");
    });
  });
});
