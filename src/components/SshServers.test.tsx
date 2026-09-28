import { act, fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { SshServers } from "@/components/SshServers";
import { renderWithProviders } from "@/test/mocks/render";
import {
  dialogOpenMock,
  invokeMock,
  resetTauriMocks,
} from "@/test/mocks/tauri";

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

const hosts = [
  { name: "alpha", host_name: "alpha.example", user: "root", port: 22 },
  { name: "beta", host_name: "beta.example", user: "root", port: 22 },
];

const history = [
  {
    id: "h-alpha",
    type: "config",
    name: "alpha",
    host_name: "alpha.example",
    user: "root",
    port: 22,
    last_connected: 0,
  },
  {
    id: "h-beta",
    type: "config",
    name: "beta",
    host_name: "beta.example",
    user: "root",
    port: 22,
    last_connected: 0,
  },
];

describe("SshServers", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("历史过滤仍然隐藏被忽略的条目", async () => {
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "get_ssh_hosts") return hosts;
        if (command === "get_secret") {
          if (args?.key === "onespace_ssh_history") {
            return JSON.stringify(history);
          }
          if (args?.key === "onespace_ssh_ignored") {
            return JSON.stringify(["beta"]);
          }
          if (args?.key === "onespace_ssh_favorites") {
            return JSON.stringify([]);
          }
        }
        return null;
      },
    );

    renderWithProviders(<SshServers />);
    await settle();

    fireEvent.click(screen.getByRole("button", { name: /历史|History/ }));

    expect(await screen.findByText("alpha")).toBeInTheDocument();
    expect(screen.queryByText("beta")).not.toBeInTheDocument();
  });

  it("密钥文件选择失败时展示可见错误而非仅写控制台", async () => {
    dialogOpenMock.mockRejectedValue(new Error("key file unavailable"));
    invokeMock.mockImplementation(
      async (command: string) => {
        if (command === "get_ssh_hosts") return [];
        return null;
      },
    );

    const { container } = renderWithProviders(<SshServers />);
    await settle();

    fireEvent.click(screen.getByRole("button", { name: /自定义|Custom/ }));
    fireEvent.click(
      screen.getByRole("radio", { name: /身份验证密钥文件|Identity Key File/ }),
    );
    fireEvent.click(screen.getByRole("button", { name: /浏览|Browse/ }));
    await settle();

    const alert = screen.queryByRole("alert");
    const legacyBanner = container.querySelector(".bg-destructive\\/15");
    const errorNode = alert ?? legacyBanner;

    expect(errorNode).not.toBeNull();
    expect(errorNode?.textContent?.trim().length ?? 0).toBeGreaterThan(0);
  });

  it("Tauri 运行时缺失时自定义历史连接不调用后端也不崩溃", async () => {
    resetTauriMocks();
    const customHistory = [
      {
        id: "h-custom",
        type: "custom",
        name: "custom-entry",
        host_name: "custom.example",
        user: "root",
        port: 22,
        last_connected: 0,
      },
    ];
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "get_ssh_hosts") return hosts;
        if (command === "get_secret") {
          if (args?.key === "onespace_ssh_history") {
            return JSON.stringify(customHistory);
          }
          if (args?.key === "onespace_ssh_ignored") {
            return JSON.stringify([]);
          }
          if (args?.key === "onespace_ssh_favorites") {
            return JSON.stringify([]);
          }
        }
        return null;
      },
    );

    const unhandled: unknown[] = [];
    const onUnhandled = (event: PromiseRejectionEvent) => {
      unhandled.push(event.reason);
      event.preventDefault();
    };
    window.addEventListener("unhandledrejection", onUnhandled);

    try {
      renderWithProviders(<SshServers />);
      await settle();

      // 应用已加载历史后失去 Tauri 运行时: 连接动作必须在调用点重新守卫,
      // 而不是依赖渲染时捕获的 isTauri。
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;

      fireEvent.click(screen.getByRole("button", { name: /历史|History/ }));
      await settle();
      const row = await screen.findByText("custom-entry");
      const invokesBeforeConnect = invokeMock.mock.calls.length;

      // 历史行的整张卡片就是连接控件。
      fireEvent.click(row);
      await settle();

      expect(
        invokeMock.mock.calls
          .slice(invokesBeforeConnect)
          .map(([command]) => command),
      ).toEqual([]);
      expect(unhandled).toEqual([]);
      expect(screen.getByText("custom-entry")).toBeInTheDocument();
    } finally {
      window.removeEventListener("unhandledrejection", onUnhandled);
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        value: {},
        configurable: true,
      });
    }
  });
});
