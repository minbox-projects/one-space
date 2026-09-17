import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";
import type { FusionStatus } from "@/lib/apiFusion";

// 轻量 mock 核心重型组件，专注于测试 App 顶部栏与导航联动
vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher" />,
}));
vi.mock("@/components/MoreToolsHub", () => ({
  MoreToolsHub: () => <div data-testid="mock-more-tools" />,
}));
vi.mock("@/components/AiSessions", () => ({
  AiSessions: () => <div data-testid="mock-ai-sessions" />,
}));
vi.mock("@/components/Workspaces", () => ({
  Workspaces: () => <div data-testid="mock-workspaces" />,
}));
vi.mock("@/components/ApiFusion", () => ({
  ApiFusion: () => <div data-testid="mock-api-fusion-page">API Fusion Content</div>,
}));

describe("App 顶部 API 网关启动状态图标", () => {
  let eventHandlers: Record<string, ((event: { payload?: unknown }) => void)[]> = {};

  beforeEach(() => {
    resetTauriMocks();
    eventHandlers = {};

    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    // 捕获 listen 注册的回调
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

  function triggerEvent(eventName: string, payload?: unknown) {
    const handlers = eventHandlers[eventName] || [];
    for (const handler of handlers) {
      handler({ payload });
    }
  }

  function mockApiFusionStatus(status: Partial<FusionStatus>) {
    const fullStatus: FusionStatus = {
      running: false,
      enabled: false,
      port: 17688,
      local_base_url: "http://127.0.0.1:17688",
      provider_count: 1,
      auto_disabled_count: 0,
      key_count: 1,
      default_key_id: "k1",
      ...status,
    };

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_fusion_status") {
        return fullStatus;
      }
      if (command === "protocol_router_status") {
        return { enabled: false, running: false, port: 17860, route_count: 0 };
      }
      if (command === "get_storage_config") {
        return {
          ok: true,
          data: {
            language: "zh",
            storage_type: "local",
          },
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

    return fullStatus;
  }

  it("当 API 网关未启动（running: false）时，顶部栏不显示 API 网关图标", async () => {
    mockApiFusionStatus({ running: false });

    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    // 等待状态拉取完成
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_status");
    });

    expect(screen.queryByTestId("header-api-fusion-status")).not.toBeInTheDocument();
  });

  it("当 API 网关已启动（running: true）时，顶部栏显示绿色已启动图标且包含端口信息", async () => {
    mockApiFusionStatus({ running: true, port: 17688 });

    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    const gatewayIconBtn = await screen.findByTestId("header-api-fusion-status");
    expect(gatewayIconBtn).toBeInTheDocument();
    expect(gatewayIconBtn).toHaveClass("text-emerald-600");
    expect(gatewayIconBtn.getAttribute("title")).toContain("17688");
    expect(gatewayIconBtn.getAttribute("aria-label")).toContain("17688");
  });

  it("点击顶部 API 网关状态图标可跳转到 API 网关页面", async () => {
    const user = userEvent.setup();
    mockApiFusionStatus({ running: true, port: 17688 });

    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    const gatewayIconBtn = await screen.findByTestId("header-api-fusion-status");
    await user.click(gatewayIconBtn);

    // 页面跳转至 API Fusion 页面
    expect(await screen.findByTestId("mock-api-fusion-page")).toBeInTheDocument();
  });

  it("当收到 api-fusion-status-update 事件时能够刷新状态并展示图标", async () => {
    let currentRunning = false;

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_fusion_status") {
        return {
          running: currentRunning,
          enabled: true,
          port: 17688,
          local_base_url: "http://127.0.0.1:17688",
          provider_count: 1,
          auto_disabled_count: 0,
          key_count: 1,
          default_key_id: "k1",
        };
      }
      if (command === "protocol_router_status") {
        return { enabled: false, running: false, port: 17860, route_count: 0 };
      }
      return undefined;
    });

    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_status");
    });
    expect(screen.queryByTestId("header-api-fusion-status")).not.toBeInTheDocument();

    // 模拟网关启动并广播事件
    currentRunning = true;
    triggerEvent("api-fusion-status-update");

    await waitFor(() => {
      expect(screen.getByTestId("header-api-fusion-status")).toBeInTheDocument();
    });
  });
});
