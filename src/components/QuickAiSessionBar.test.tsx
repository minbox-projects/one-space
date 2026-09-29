import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { QuickAiSessionBar } from "@/components/QuickAiSessionBar";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const apiMeta = { schema_version: 1, revision: 1 };

function workflowCalls() {
  return invokeMock.mock.calls.filter(
    ([command]) => typeof command === "string" && command.startsWith("workflows_"),
  );
}

describe("QuickAiSessionBar workflow removal", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "get_storage_config":
          return {
            default_ai_dir: "/tmp/one-space-quick",
            default_ai_model: "claude",
          };
        case "workflows_presets_list":
          return {
            ok: true,
            data: [
              {
                id: "wf-1",
                name: "Demo Flow",
                tool: "claude",
                working_dir: "/tmp/demo",
                launch_scope: "shared",
              },
            ],
            meta: apiMeta,
          };
        case "sessions_create":
          return { ok: true, data: {}, meta: apiMeta };
        case "hide_quick_ai_window":
          return null;
        case "resize_window":
          return null;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });
  });

  it("AC-003 quick bar launches a plain session and renders no preset selector", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickAiSessionBar />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_storage_config"),
    );
    expect(workflowCalls()).toEqual([]);

    await user.click(screen.getByTitle("Expand Options"));
    expect(screen.queryByText("Workflow Preset")).not.toBeInTheDocument();

    await user.click(screen.getByTitle("Launch AI Session"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_create", {
        session: {
          name: "",
          working_dir: "/tmp/one-space-quick",
          tool: "claude",
          status: "active",
        },
      });
    });
    expect(workflowCalls()).toEqual([]);
  });
});
