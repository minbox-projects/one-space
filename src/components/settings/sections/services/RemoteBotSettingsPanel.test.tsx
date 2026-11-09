import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { TFunction } from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import { RemoteBotSettingsPanel } from "./RemoteBotSettingsPanel";

function flatWorkspace(index: number) {
  return {
    id: `ws-${index}`,
    name: `Workspace ${index}`,
    root_path: `/tmp/workspace-${index}`,
    tags: [],
    source: "session_auto",
    created_at: 1,
    updated_at: 1,
    last_activity_at: index,
    session_count: 0,
  };
}

describe("RemoteBotSettingsPanel featured workspace picker", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("loads every system workspace in the flat workspaces_list payload and filters by keyword", async () => {
    const workspaces = Array.from({ length: 110 }, (_, index) => flatWorkspace(index + 1));

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "remote_bot_get_config") {
        return {
          enabled: false,
          bot_token: "",
          allowed_chat_ids: [],
          active_workspace: null,
          auto_approve_permissions: true,
          enable_natural_dialogue: true,
          opencode_bin_path: null,
          workspace_directories: [],
          updated_at: 1,
        };
      }
      if (command === "remote_bot_get_status") {
        return { running: false };
      }
      if (command === "remote_bot_list_installed_opencode") {
        return [];
      }
      if (command === "workspaces_list") {
        return {
          ok: true,
          data: workspaces,
          meta: { schema_version: 1, revision: 1 },
        };
      }
      return undefined;
    });

    const user = userEvent.setup();
    renderWithProviders(
      <RemoteBotSettingsPanel t={i18n.t.bind(i18n) as unknown as TFunction} />
    );

    await user.click(await screen.findByRole("button", { name: "添加工作区" }));

    expect(
      await screen.findByText("OneSpace 系统工作空间 (共 110 个)", { selector: "span" })
    ).toBeInTheDocument();
    expect(screen.getByText("Workspace 110")).toBeInTheDocument();

    await user.type(screen.getByPlaceholderText("搜索名称或标签..."), "Workspace 110");

    await waitFor(() => {
      expect(screen.getByText("Workspace 110")).toBeInTheDocument();
      expect(screen.queryByText("Workspace 1")).not.toBeInTheDocument();
    });
  });
});
