import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { AiSessions } from "@/components/AiSessions";
import { renderWithProviders } from "@/test/mocks/render";
import { dialogOpenMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const apiMeta = { schema_version: 1, revision: 1 };

function workflowCalls() {
  return invokeMock.mock.calls.filter(
    ([command]) => typeof command === "string" && command.startsWith("workflows_"),
  );
}

describe("AiSessions workflow removal", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "check_cli_installed":
          return true;
        case "sessions_list":
          return { ok: true, data: [], meta: apiMeta };
        case "get_storage_config":
          return {};
        case "service_providers_list":
          return { ok: true, data: { providers: [] }, meta: apiMeta };
        case "workflows_presets_list":
          return { ok: true, data: [], meta: apiMeta };
        case "sessions_create":
          return { ok: true, data: {}, meta: apiMeta };
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });
  });

  it("AC-001 renders no workflow control or preset selector on the sessions page", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiSessions isVisible />);

    expect(
      screen.queryByRole("button", { name: "Workflow Presets" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Workflow" }),
    ).not.toBeInTheDocument();

    await user.click(await screen.findByRole("button", { name: "New Session" }));
    expect(
      await screen.findByRole("button", { name: "Launch" }),
    ).toBeInTheDocument();

    expect(screen.queryByText("Workflow Preset")).not.toBeInTheDocument();
    expect(screen.queryByText("No preset (manual)")).not.toBeInTheDocument();

    // AC-001 also requires the session list container itself to keep rendering;
    // pin it via its empty state so it cannot be confused with the removed controls.
    expect(
      await screen.findByText("No active AI terminal sessions found."),
    ).toBeInTheDocument();

    // The manual create path still renders the command select and directory input.
    expect(screen.getByText("AI Command")).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "Claude Code" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Working Directory")).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText("Select a project directory..."),
    ).toBeInTheDocument();
  });

  it("AC-002 manual session creation still invokes sessions_create with no workflow command", async () => {
    const user = userEvent.setup();
    dialogOpenMock.mockResolvedValue("/tmp/one-space-demo");
    renderWithProviders(<AiSessions isVisible />);

    await user.click(await screen.findByRole("button", { name: "New Session" }));
    await screen.findByRole("button", { name: "Launch" });

    // AC-002 only constrains the manual create submission itself, so scope the
    // workflow-command check to the create action rather than to mount.
    invokeMock.mockClear();

    await user.click(screen.getByRole("button", { name: "Browse" }));
    await waitFor(() =>
      expect(
        screen.getByDisplayValue("/tmp/one-space-demo"),
      ).toBeInTheDocument(),
    );

    await user.click(screen.getByRole("button", { name: "Launch" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_create", {
        session: {
          name: "",
          working_dir: "/tmp/one-space-demo",
          tool: "claude",
          status: "active",
        },
      });
    });

    expect(workflowCalls()).toEqual([]);
  });
});
