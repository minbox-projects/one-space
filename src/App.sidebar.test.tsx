import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher">Launcher Content</div>,
}));

describe("App 侧边栏折叠与展开功能", () => {
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

  it("默认处于展开态，展示 OneSpace 标题及折叠按钮", async () => {
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    expect(await screen.findByText("OneSpace")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Collapse sidebar/ })).toBeInTheDocument();
  });

  it("点击折叠按钮后，侧边栏切换为紧凑折叠态并隐藏标题", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    const collapseBtn = await screen.findByRole("button", { name: /Collapse sidebar/ });
    await user.click(collapseBtn);

    // 标题文本消失，展开按钮出现
    expect(screen.queryByText("OneSpace")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Expand sidebar/ })).toBeInTheDocument();

    // 点击展开按钮恢复
    const expandBtn = screen.getByRole("button", { name: /Expand sidebar/ });
    await user.click(expandBtn);
    expect(await screen.findByText("OneSpace")).toBeInTheDocument();
  });

  it("通过键盘快捷键 Cmd+B / Ctrl+B 触发折叠与展开", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    expect(await screen.findByText("OneSpace")).toBeInTheDocument();

    // 触发 Cmd+B (Meta+b)
    await user.keyboard("{Meta>}b{/Meta}");
    expect(screen.queryByText("OneSpace")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Expand sidebar/ })).toBeInTheDocument();

    // 再次触发恢复
    await user.keyboard("{Meta>}b{/Meta}");
    expect(await screen.findByText("OneSpace")).toBeInTheDocument();
  });
});
