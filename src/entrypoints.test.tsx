import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import QuickAiApp from "@/QuickAiApp";
import { ThemeProvider } from "@/components/ThemeProvider";
import { resolveEntryKind } from "@/main";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

/**
 * REQ-006 / AC-006 entry-selection contract (frozen Step 6).
 *
 * `resolveEntryKind` is the public selector evaluated before any application
 * module loads, so a quick-AI window never pulls in the main-window/toolbox UI.
 * The behavioral checks render the two public entry components over the same
 * Tauri IPC mock the existing App.* suites use; heavy child views are mocked so
 * the assertion observes only entry composition.
 */

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher">Launcher Content</div>,
}));

const apiMeta = { schema_version: 1, revision: 1 };

describe("entry selection (REQ-006/AC-006)", () => {
  beforeEach(() => {
    resetTauriMocks();
    localStorage.clear();
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "get_storage_config":
          return {
            default_ai_dir: "/tmp/one-space-quick",
            default_ai_model: "claude",
          };
        case "get_dashboard_counts":
        case "dashboard_counts":
          return {
            ok: true,
            data: {
              launcher: 0,
              workspaces: 0,
              sessions: 0,
              ssh: 0,
              snippets: 0,
              bookmarks: 0,
              notes: 0,
              ai_news: 0,
              environments: 0,
              skills: 0,
              subagents: 0,
              mcp_servers: 0,
            },
            meta: apiMeta,
          };
        case "resize_window":
        case "hide_quick_ai_window":
          return null;
        case "sessions_create":
          return { ok: true, data: {}, meta: apiMeta };
        default:
          return undefined;
      }
    });
  });

  it("selects the quick-ai entry only for the quick-ai view parameter", () => {
    expect(resolveEntryKind("?view=quick-ai")).toBe("quick-ai");
    expect(resolveEntryKind("?view=quick-ai&foo=bar")).toBe("quick-ai");
  });

  it("falls back to the main entry for an empty or unrelated query", () => {
    expect(resolveEntryKind("")).toBe("main");
    expect(resolveEntryKind("?view=main")).toBe("main");
    expect(resolveEntryKind("?view=other")).toBe("main");
    expect(resolveEntryKind("?foo=bar")).toBe("main");
    expect(resolveEntryKind("?view=quick")).toBe("main");
  });

  it("renders the quick bar without any main-shell markers", async () => {
    renderWithProviders(<QuickAiApp />);

    // The quick session bar is present (provider selector).
    expect(await screen.findByRole("combobox")).toBeInTheDocument();
    // No main-window sidebar/navigation surface.
    expect(screen.queryByText("OneSpace")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Collapse sidebar/ }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });

  it("renders the main shell for the main entry", async () => {
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    expect(await screen.findByText("OneSpace")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Collapse sidebar/ }),
    ).toBeInTheDocument();
  });

  // REQ-006 / AC-006: a mounted hidden tab keeps its draft. The main shell
  // keeps a visited tab mounted (hidden) instead of unmounting it, so switching
  // away and back must preserve the real Notes editor draft.
  it("preserves a Notes draft while another tab is shown and restores it", async () => {
    const note = {
      id: "note-draft-1",
      title: "Existing Note",
      content: "original body",
      created_at: 1,
      updated_at: 1,
    };
    const base = invokeMock.getMockImplementation() as
      | ((command: string, args?: unknown) => Promise<unknown>)
      | undefined;
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "read_notes") {
        return JSON.stringify([note]);
      }
      if (command === "save_notes") {
        return null;
      }
      if (command === "read_snippets") {
        return "[]";
      }
      if (command === "save_snippets") {
        return null;
      }
      return base ? base(command, args) : undefined;
    });

    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(await screen.findByRole("button", { name: /备忘录/ }));
    await user.click(await screen.findByText("Existing Note"));

    const textarea = await screen.findByPlaceholderText("写点什么...");
    expect(textarea).toHaveValue("original body");

    await user.clear(textarea);
    await user.type(textarea, "draft-while-hidden");
    expect(textarea).toHaveValue("draft-while-hidden");

    // Switch to another tab: the Notes component remains mounted (hidden) and
    // its editor draft is not discarded.
    await user.click(screen.getByRole("button", { name: /代码片段/ }));
    expect(screen.queryByPlaceholderText("写点什么...")).toBeInTheDocument();

    // Returning to Notes restores the same mounted editor with the draft intact.
    await user.click(screen.getByRole("button", { name: /备忘录/ }));
    expect(await screen.findByPlaceholderText("写点什么...")).toHaveValue(
      "draft-while-hidden",
    );
  });
});
