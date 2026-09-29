import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { OmniSearch } from "@/components/OmniSearch";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const apiMeta = { schema_version: 1, revision: 1 };

// cmdk, used by the command dialog, requires ResizeObserver which jsdom does
// not implement. Provide the minimal surface used during render.
class ResizeObserverStub {
  constructor(_callback: ResizeObserverCallback) {}
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver =
    ResizeObserverStub as unknown as typeof ResizeObserver;
}

function workflowCalls() {
  return invokeMock.mock.calls.filter(
    ([command]) => typeof command === "string" && command.startsWith("workflows_"),
  );
}

describe("OmniSearch workflow removal", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "sessions_list":
          return {
            ok: true,
            data: [
              {
                id: "s1",
                name: "Demo Session",
                working_dir: "/tmp/one-space-demo",
                model_type: "claude",
                tool_session_id: "t1",
                created_at: 0,
              },
            ],
            meta: apiMeta,
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
              },
            ],
            meta: apiMeta,
          };
        case "workflows_runs_list":
          return { ok: true, data: [], meta: apiMeta };
        case "get_storage_config":
          return {};
        case "launcher_list":
          return { ok: true, data: [], meta: apiMeta };
        case "get_ssh_hosts":
          return [];
        case "read_snippets":
          return "[]";
        case "read_bookmarks":
          return "[]";
        case "read_notes":
          return "[]";
        case "skills_list_installed":
          return { ok: true, data: [], meta: apiMeta };
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });
  });

  it("AC-004 search returns no workflow results and loads no workflow command", async () => {
    const user = userEvent.setup();
    renderWithProviders(<OmniSearch open setOpen={() => {}} />);

    const input = await screen.findByPlaceholderText("Search...");
    await user.type(input, "Demo");

    // A non-workflow result proves the search finished loading its items; the
    // workflow group would render in the same commit when it still exists.
    expect(await screen.findByText("Demo Session")).toBeInTheDocument();

    expect(screen.queryByText("Run: Demo Flow")).not.toBeInTheDocument();
    expect(screen.queryByText("Workflow Presets")).not.toBeInTheDocument();
    expect(workflowCalls()).toEqual([]);
  });
});
