import { screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher">Launcher Content</div>,
}));

/**
 * Anchored accessible-name matcher: sidebar buttons append their count badge to
 * the accessible name (e.g. "Skills 0"), so match the localized label prefix.
 */
function namePattern(label: string): RegExp {
  const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${escaped}(\\s|$)`);
}

describe("App 侧边栏菜单分组", () => {
  beforeEach(() => {
    resetTauriMocks();
    localStorage.clear();

    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_storage_config") {
        return {
          ok: true,
          data: { language: "zh", storage_type: "local" },
          meta: { schema_version: 1, revision: 1 },
        };
      }
      if (command === "get_dashboard_counts") {
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
          meta: { schema_version: 1, revision: 1 },
        };
      }
      return undefined;
    });
  });

  /** The label div's parent is the group container (`space-y-1.5`). */
  function groupContainer(label: RegExp): HTMLElement {
    const labelEl = screen.getByText(label);
    const container = labelEl.parentElement;
    expect(container).not.toBeNull();
    expect(container).toHaveClass("space-y-1.5");
    return container as HTMLElement;
  }

  it("将 skills、mcp-servers、subagents 归入工具分组并保留原有工具项", async () => {
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );
    await screen.findByText("OneSpace");

    const toolsGroup = groupContainer(/^(工具|Tools)$/);

    // Moved items
    expect(
      within(toolsGroup).getByRole("button", {
        name: namePattern(i18n.t("skills", "Skills")),
      }),
    ).toBeInTheDocument();
    expect(
      within(toolsGroup).getByRole("button", {
        name: namePattern("MCP Servers"),
      }),
    ).toBeInTheDocument();
    expect(
      within(toolsGroup).getByRole("button", {
        name: namePattern(i18n.t("subagents", "Subagents")),
      }),
    ).toBeInTheDocument();

    // Existing registry-derived sidebar entries
    expect(
      within(toolsGroup).getByRole("button", {
        name: namePattern(i18n.t("snippets", "Snippets")),
      }),
    ).toBeInTheDocument();
    expect(
      within(toolsGroup).getByRole("button", {
        name: namePattern(i18n.t("notes", "Notes")),
      }),
    ).toBeInTheDocument();

    // Existing More Tools entry (hardcoded zh/en label in App)
    expect(
      within(toolsGroup).getByRole("button", {
        name: /^(更多工具|More Tools)$/,
      }),
    ).toBeInTheDocument();
  });

  it("AI 能力分组仅保留原有能力项，三个迁移项不再属于该分组", async () => {
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );
    await screen.findByText("OneSpace");

    const capabilitiesGroup = groupContainer(/^(AI 能力|AI Capabilities)$/);

    expect(
      within(capabilitiesGroup).getByRole("button", {
        name: namePattern(i18n.t("cliEnvironments", "AI Terminal Environments")),
      }),
    ).toBeInTheDocument();
    expect(
      within(capabilitiesGroup).getByRole("button", {
        name: namePattern(i18n.t("aiGateway", "AI Gateway")),
      }),
    ).toBeInTheDocument();
    expect(
      within(capabilitiesGroup).getByRole("button", {
        name: namePattern(i18n.t("aiUsageStatsMenu", "AI Usage Stats")),
      }),
    ).toBeInTheDocument();

    expect(
      within(capabilitiesGroup).queryByRole("button", {
        name: namePattern(i18n.t("skills", "Skills")),
      }),
    ).not.toBeInTheDocument();
    expect(
      within(capabilitiesGroup).queryByRole("button", {
        name: namePattern("MCP Servers"),
      }),
    ).not.toBeInTheDocument();
    expect(
      within(capabilitiesGroup).queryByRole("button", {
        name: namePattern(i18n.t("subagents", "Subagents")),
      }),
    ).not.toBeInTheDocument();
  });
});
