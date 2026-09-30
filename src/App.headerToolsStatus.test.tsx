import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher" />,
}));
vi.mock("@/components/MoreToolsHub", () => ({
  MoreToolsHub: ({
    activeTool,
    sshTunnelTab,
  }: {
    activeTool?: string | null;
    sshTunnelTab?: string;
  }) => (
    <div
      data-testid={`mock-more-tools-${activeTool ?? "hub"}`}
      data-ssh-tunnel-tab={sshTunnelTab}
    />
  ),
}));
vi.mock("@/components/AiSessions", () => ({
  AiSessions: () => <div data-testid="mock-ai-sessions" />,
}));
vi.mock("@/components/Workspaces", () => ({
  Workspaces: () => <div data-testid="mock-workspaces" />,
}));
vi.mock("@/components/AiGateway", () => ({
  AiGateway: () => <div data-testid="mock-ai-gateway-page">AI Gateway Page</div>,
}));

describe("App 窗口顶部右侧各工具状态图标", () => {
  let eventHandlers: Record<string, ((event: { payload?: unknown }) => void)[]> = {};

  beforeEach(() => {
    resetTauriMocks();
    eventHandlers = {};

    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    (listenMock as unknown as { mockImplementation: (fn: (eventName: string, handler: (event: { payload?: unknown }) => void) => Promise<() => void>) => void }).mockImplementation(async (eventName: string, handler: (event: { payload?: unknown }) => void) => {
      if (!eventHandlers[eventName]) {
        eventHandlers[eventName] = [];
      }
      eventHandlers[eventName].push(handler);
      return () => {
        eventHandlers[eventName] = eventHandlers[eventName].filter((h) => h !== handler);
      };
    });
  });

  function setupInvokeMock({
    gateway = { running: false },
    router = { enabled: false, running: false, port: 17860, route_count: 0 },
    tunnels = { runtime: [], tunnels: [] },
    sharing = { running: false, files: [] },
  }: {
    gateway?: { running: boolean; port?: number };
    router?: { enabled: boolean; running: boolean; port: number; route_count: number };
    tunnels?: { runtime: { status: string; name?: string }[]; tunnels: { name: string }[] };
    sharing?: { running: boolean; files: string[] };
  } = {}) {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "ai_gateway_status") {
        return {
          running: gateway.running,
          enabled: true,
          port: gateway.port ?? 17688,
          local_base_url: "http://127.0.0.1:17688/v1",
          provider_count: 1,
          auto_disabled_count: 0,
          key_count: 1,
          default_key_id: "k1",
        };
      }
      if (command === "protocol_router_status") {
        return router;
      }
      if (command === "ssh_tunnels_snapshot") {
        return tunnels;
      }
      if (command === "file_sharing_status") {
        return sharing;
      }
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
  }

  describe("协议路由状态图标", () => {
    it("未启用时不渲染状态图标", async () => {
      setupInvokeMock({ router: { enabled: false, running: false, port: 17860, route_count: 0 } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("protocol_router_status"));
      expect(screen.queryByTestId("header-protocol-router-status")).not.toBeInTheDocument();
    });

    it("启用且运行时呈现绿色状态及呼吸点，并包含路由数和端口", async () => {
      setupInvokeMock({ router: { enabled: true, running: true, port: 17860, route_count: 3 } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-protocol-router-status");
      expect(btn).toBeInTheDocument();
      expect(btn).toHaveClass("text-emerald-600");
      expect(btn.getAttribute("title")).toContain("17860");
      expect(btn.getAttribute("title")).toContain("3");
    });

    it("启用但停止时呈现琥珀色告警状态", async () => {
      setupInvokeMock({ router: { enabled: true, running: false, port: 17860, route_count: 3 } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-protocol-router-status");
      expect(btn).toBeInTheDocument();
      expect(btn).toHaveClass("text-amber-600");
    });

    it("点击协议路由状态图标可跳转至对应页面", async () => {
      const user = userEvent.setup();
      setupInvokeMock({ router: { enabled: true, running: true, port: 17860, route_count: 2 } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-protocol-router-status");
      await user.click(btn);
      expect(await screen.findByTestId("mock-more-tools-protocol-router")).toBeInTheDocument();
    });
  });

  describe("SSH 隧道状态图标", () => {
    it("无已连接隧道时不渲染图标", async () => {
      setupInvokeMock({ tunnels: { runtime: [], tunnels: [{ name: "t1" }] } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ssh_tunnels_snapshot"));
      expect(screen.queryByTestId("header-ssh-tunnels-status")).not.toBeInTheDocument();
    });

    it("正常连接时呈现绿色激活态", async () => {
      setupInvokeMock({
        tunnels: {
          runtime: [{ status: "connected", name: "t1" }],
          tunnels: [{ name: "t1" }],
        },
      });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-ssh-tunnels-status");
      expect(btn).toBeInTheDocument();
      expect(btn).toHaveClass("text-emerald-600");
    });

    it("有连接中隧道时呈现琥珀色状态", async () => {
      setupInvokeMock({
        tunnels: {
          runtime: [
            { status: "connected", name: "t1" },
            { status: "connecting", name: "t2" },
          ],
          tunnels: [{ name: "t1" }, { name: "t2" }],
        },
      });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-ssh-tunnels-status");
      expect(btn).toBeInTheDocument();
      expect(btn).toHaveClass("text-amber-600");
    });

    it("发生断开异常时呈现红色警示态", async () => {
      setupInvokeMock({
        tunnels: {
          runtime: [
            { status: "connected", name: "t1" },
            { status: "error", name: "t2" },
          ],
          tunnels: [{ name: "t1" }, { name: "t2" }],
        },
      });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-ssh-tunnels-status");
      expect(btn).toBeInTheDocument();
      expect(btn).toHaveClass("text-destructive");
    });

    it("点击 SSH 隧道状态图标可跳转至对应页面", async () => {
      const user = userEvent.setup();
      setupInvokeMock({
        tunnels: {
          runtime: [{ status: "connected", name: "t1" }],
          tunnels: [{ name: "t1" }],
        },
      });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-ssh-tunnels-status");
      await user.click(btn);
      const detail = await screen.findByTestId("mock-more-tools-ssh-tunnels");
      expect(detail).toBeInTheDocument();
      expect(detail).toHaveAttribute("data-ssh-tunnel-tab", "__connected__");
    });
  });

  describe("文件共享状态图标", () => {
    it("未运行共享时不渲染图标", async () => {
      setupInvokeMock({ sharing: { running: false, files: [] } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("file_sharing_status"));
      expect(screen.queryByTestId("header-file-sharing-status")).not.toBeInTheDocument();
    });

    it("正在共享时呈现绿色状态及呼吸点，并支持点击跳转", async () => {
      const user = userEvent.setup();
      setupInvokeMock({ sharing: { running: true, files: ["/path/to/file1.png", "/path/to/file2.txt"] } });
      renderWithProviders(
        <ThemeProvider>
          <App />
        </ThemeProvider>,
      );

      const btn = await screen.findByTestId("header-file-sharing-status");
      expect(btn).toBeInTheDocument();
      expect(btn).toHaveClass("text-emerald-600");
      expect(btn.getAttribute("title")).toContain("2");

      await user.click(btn);
      expect(await screen.findByTestId("mock-more-tools-file-sharing")).toBeInTheDocument();
    });
  });
});
