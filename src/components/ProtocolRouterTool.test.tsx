import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProtocolRouterTool } from "@/components/ProtocolRouterTool";
import type {
  ProtocolRoute,
  ProtocolRouterStatsSummary,
} from "@/lib/protocolRouter";
import { setNativeWindowVisible } from "@/lib/runtimeStatus";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

const route: ProtocolRoute = {
  id: "route-1",
  name: "Claude Route",
  claude_provider_id: "provider-1",
  claude_provider_name: "Provider One",
  upstream_provider_id: "upstream-1",
  upstream_provider_name: "Upstream One",
  base_url: "https://upstream.example/v1",
  auth_header: null,
  api_key: "",
  wire_api: "open_ai_chat",
  default_model: "gpt-4o-mini",
  mappings: [],
  enabled: true,
};

const emptyStats: ProtocolRouterStatsSummary = {
  total_calls: 0,
  input_tokens: 0,
  output_tokens: 0,
  total_tokens: 0,
  by_route: [],
  by_provider: [],
  by_model: [],
  calls: [],
};

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

function deleteTauriInternals() {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
}

function restoreTauriInternals() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
}

describe("ProtocolRouterTool", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  afterEach(() => {
    restoreTauriInternals();
    setNativeWindowVisible(true);
  });

  it("在 Tauri 之外仍渲染视图且不调用路由测试命令", async () => {
    deleteTauriInternals();

    renderWithProviders(<ProtocolRouterTool isVisible />);
    await settle();

    expect(
      screen.getByRole("heading", {
        level: 1,
        name: /协议路由|Protocol Router/,
      }),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "protocol_router_test_connection",
    );
  });

  it("统计窗口控件只渲染一次", async () => {
    deleteTauriInternals();

    renderWithProviders(<ProtocolRouterTool isVisible />);
    await settle();

    expect(
      screen.getAllByRole("button", { name: /^(今天|Today)$/ }),
    ).toHaveLength(1);
  });

  it("无请求空状态通过共享 ToolEmptyState 边界渲染", async () => {
    deleteTauriInternals();

    renderWithProviders(<ProtocolRouterTool isVisible />);
    await settle();

    const emptyState = screen.getByTestId("protocol-router-empty-requests");
    expect(emptyState).toHaveTextContent(
      /还没有请求记录|No requests in the selected view/,
    );
  });

  it("在 Tauri 中点击路由测试会调用测试连接命令", async () => {
    invokeMock.mockImplementation(
      async (command: string) => {
        if (command === "protocol_router_get_config") {
          return {
            enabled: true,
            port: 17687,
            token: "token",
            retention_days: 30,
            routes: [route],
          };
        }
        if (command === "protocol_router_status") {
          return { running: true, enabled: true, port: 17687, route_count: 1 };
        }
        if (command === "protocol_router_stats") {
          return emptyStats;
        }
        if (command === "protocol_router_test_connection") {
          return {
            ts: 0,
            route_id: route.id,
            provider: "Provider One",
            model: "gpt-4o-mini",
            endpoint: "/v1/chat/completions",
            wire_api: "open_ai_chat",
            status: 200,
            latency_ms: 12,
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
          };
        }
        return undefined;
      },
    );

    renderWithProviders(<ProtocolRouterTool isVisible />);
    await settle();

    fireEvent.click(
      screen.getByRole("button", { name: /^(测试连接|Test Connection)$/ }),
    );
    await settle();

    expect(invokeMock).toHaveBeenCalledWith(
      "protocol_router_test_connection",
      { input: { route_id: route.id, model: route.default_model } },
    );
  });

  it("隐藏时推迟路由状态事件刷新并在显示后只追赶一次", async () => {
    const handlers: Record<string, (event: { payload?: unknown }) => void> = {};
    listenMock.mockImplementation(
      async (eventName: string, handler: (event: { payload?: unknown }) => void) => {
        handlers[eventName] = handler;
        return vi.fn();
      },
    );
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "protocol_router_get_config") {
        return {
          enabled: true,
          port: 17687,
          token: "token",
          retention_days: 30,
          routes: [route],
        };
      }
      if (command === "protocol_router_status") {
        return { running: true, enabled: true, port: 17687, route_count: 1 };
      }
      if (command === "protocol_router_stats") {
        return emptyStats;
      }
      return undefined;
    });

    renderWithProviders(<ProtocolRouterTool isVisible />);
    await settle();
    await waitFor(() => {
      expect(handlers["protocol-router-status-update"]).toBeTruthy();
      expect(
        screen.getByRole("heading", {
          level: 1,
          name: /协议路由|Protocol Router/,
        }),
      ).toBeInTheDocument();
    });

    const configCalls = () =>
      invokeMock.mock.calls.filter(
        (call) => call[0] === "protocol_router_get_config",
      ).length;
    const configCallsBefore = configCalls();

    // Hidden native window: status updates trigger no reload.
    act(() => {
      setNativeWindowVisible(false);
    });
    await act(async () => {
      handlers["protocol-router-status-update"]({ payload: undefined });
      handlers["protocol-router-status-update"]({ payload: undefined });
      await Promise.resolve();
    });
    expect(configCalls()).toBe(configCallsBefore);

    // Showing the window performs exactly one coalesced catch-up reload.
    await act(async () => {
      setNativeWindowVisible(true);
    });
    await waitFor(() => {
      expect(configCalls()).toBe(configCallsBefore + 1);
    });
    await settle();
    expect(configCalls()).toBe(configCallsBefore + 1);
  });
});
