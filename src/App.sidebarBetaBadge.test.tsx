import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const { getVersionMock } = vi.hoisted(() => ({
  getVersionMock: vi.fn(async () => "0.1.58-beta.2"),
}));

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher">Launcher Content</div>,
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: getVersionMock,
}));

describe("App sidebar beta badge", () => {
  beforeEach(() => {
    resetTauriMocks();
    localStorage.clear();
    getVersionMock.mockReset();
    getVersionMock.mockResolvedValue("0.1.58-beta.2");

    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_storage_config") {
        return {
          ok: true,
          data: { language: "en", storage_type: "local" },
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

  it("shows beta badge in expanded state", async () => {
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    const badge = await screen.findByTestId("app-beta-badge");
    expect(badge).toHaveTextContent("BETA");
    expect(badge.getAttribute("title")).toBeTruthy();
  });

  it("keeps badge after collapsing", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    const collapseBtn = await screen.findByRole("button", { name: /Collapse sidebar/ });
    await user.click(collapseBtn);

    expect(screen.getByTestId("app-beta-badge")).toBeInTheDocument();
  });

  it("no badge for stable version", async () => {
    getVersionMock.mockResolvedValue("0.1.57");
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await screen.findByText("OneSpace");
    expect(screen.queryByTestId("app-beta-badge")).not.toBeInTheDocument();
  });

  it("no badge and no crash when getVersion rejects", async () => {
    getVersionMock.mockRejectedValue(new Error("boom"));
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await screen.findByText("OneSpace");
    expect(screen.queryByTestId("app-beta-badge")).not.toBeInTheDocument();
  });

  it("no badge for malformed version", async () => {
    getVersionMock.mockResolvedValue("not-a-version");
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await screen.findByText("OneSpace");
    expect(screen.queryByTestId("app-beta-badge")).not.toBeInTheDocument();
  });
});
