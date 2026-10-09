import type { ReactElement } from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SshTunnels } from "@/components/SshTunnels";
import { ConfirmDialogProvider } from "@/components/ConfirmDialogProvider";
import { ToastProvider } from "@/components/ToastProvider";
import type {
  SshTunnelGroupView,
  SshTunnelRuntimeView,
  SshTunnelsSnapshot,
  SshTunnelView,
} from "@/components/sshTunnels/types";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const defaultGroup: SshTunnelGroupView = {
  id: "default",
  name: "Default Group",
  created_at: 0,
  updated_at: 0,
  is_default: true,
};

const tunnelAlpha: SshTunnelView = {
  id: "tunnel-alpha",
  name: "Alpha Tunnel",
  group_id: "default",
  source_kind: "saved_host",
  saved_host_name: "alpha-host",
  custom: null,
  forward: {
    mode: "local",
    local_bind_host: "127.0.0.1",
    local_port: 5432,
    target_host: "127.0.0.1",
    target_port: 5432,
  },
  auto_connect: false,
  auto_reconnect: true,
  created_at: 0,
  updated_at: 0,
  last_connected_at: null,
  last_error: null,
};

function runtime(
  status: SshTunnelRuntimeView["status"],
  summary: string,
  lastError: string | null = null,
): SshTunnelRuntimeView {
  return {
    id: tunnelAlpha.id,
    status,
    active_client_count: 0,
    mode: "local",
    summary,
    resolved_server_host: null,
    listening_addr: null,
    last_error: lastError,
  };
}

const emptySnapshot: SshTunnelsSnapshot = {
  groups: [defaultGroup],
  tunnels: [],
  runtime: [],
};

const errorSnapshot: SshTunnelsSnapshot = {
  groups: [defaultGroup],
  tunnels: [tunnelAlpha],
  runtime: [runtime("error", "Alpha failed", "Connection refused")],
};

const disconnectedSnapshot: SshTunnelsSnapshot = {
  groups: [defaultGroup],
  tunnels: [tunnelAlpha],
  runtime: [runtime("disconnected", "Alpha idle")],
};

const connectedSnapshot: SshTunnelsSnapshot = {
  groups: [defaultGroup],
  tunnels: [tunnelAlpha],
  runtime: [runtime("connected", "Alpha connected")],
};

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

function renderTunnels(ui: ReactElement) {
  return render(ui, {
    wrapper: ({ children }) => (
      <ToastProvider>
        <ConfirmDialogProvider>{children}</ConfirmDialogProvider>
      </ToastProvider>
    ),
  });
}

function mockSnapshotInvokes(snapshot: SshTunnelsSnapshot) {
  invokeMock.mockImplementation(
    async (command: string) => {
      if (command === "get_ssh_hosts") return [];
      if (command === "ssh_tunnels_snapshot") return snapshot;
      if (command === "ssh_tunnels_refresh_status") return snapshot.runtime;
      return undefined;
    },
  );
}

describe("SshTunnels", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("隐藏时不轮询 ssh_tunnels_refresh_status，变为可见后恢复", async () => {
    vi.useFakeTimers();
    mockSnapshotInvokes(errorSnapshot);

    const refreshCallCount = () =>
      invokeMock.mock.calls.filter(
        (call) => call[0] === "ssh_tunnels_refresh_status",
      ).length;

    const { rerender } = renderTunnels(<SshTunnels isVisible={false} />);
    await settle();

    invokeMock.mockClear();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(refreshCallCount()).toBe(0);

    rerender(<SshTunnels isVisible />);
    await settle();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(refreshCallCount()).toBeGreaterThan(0);

    invokeMock.mockClear();
    rerender(<SshTunnels isVisible={false} />);
    await settle();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(refreshCallCount()).toBe(0);
  });

  it("原生窗口隐藏时暂停页面轮询，重新可见只做一次追赶刷新", async () => {
    vi.useFakeTimers();
    mockSnapshotInvokes(connectedSnapshot);

    const refreshCallCount = () =>
      invokeMock.mock.calls.filter(
        (call) => call[0] === "ssh_tunnels_refresh_status",
      ).length;

    const visibility = { state: "visible" as DocumentVisibilityState };
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => visibility.state,
    });

    try {
      renderTunnels(<SshTunnels isVisible />);
      await settle();

      // 前置条件：isVisible 与原生窗口均可见时，一个轮询间隔产生一次刷新。
      invokeMock.mockClear();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5_000);
      });
      expect(refreshCallCount()).toBe(1);

      // 原生窗口（document）隐藏后，两个轮询间隔内不得再刷新。
      visibility.state = "hidden";
      await act(async () => {
        document.dispatchEvent(new Event("visibilitychange"));
      });
      invokeMock.mockClear();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_000);
      });
      expect(refreshCallCount()).toBe(0);

      // 重新可见：不推进计时器也应有且仅有一次合并的追赶刷新。
      invokeMock.mockClear();
      visibility.state = "visible";
      await act(async () => {
        document.dispatchEvent(new Event("visibilitychange"));
      });
      expect(refreshCallCount()).toBe(1);
    } finally {
      delete (document as { visibilityState?: unknown }).visibilityState;
    }
  });

  it("过期的保存探测不再覆盖运行时错误", async () => {
    vi.useFakeTimers();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_ssh_hosts") return [];
      if (command === "ssh_tunnels_snapshot") return errorSnapshot;
      if (command === "ssh_tunnels_refresh_status") return errorSnapshot.runtime;
      if (command === "ssh_tunnel_probe_saved") {
        return {
          ok: true,
          mode: "local",
          summary: "check",
          message: "probe ok",
        };
      }
      return undefined;
    });

    const { rerender } = renderTunnels(<SshTunnels isVisible />);
    await settle();

    expect(
      screen.getAllByText(/连接被拒绝|Connection refused/).length,
    ).toBeGreaterThan(0);

    fireEvent.click(
      screen.getByRole("button", { name: /检测连接|Detect Connection/ }),
    );
    await settle();

    expect(screen.getAllByText("probe ok").length).toBeGreaterThan(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10 * 60_000);
    });
    rerender(<SshTunnels isVisible />);
    await settle();

    expect(screen.queryAllByText("probe ok")).toHaveLength(0);
    expect(
      screen.getAllByText(/连接被拒绝|Connection refused/).length,
    ).toBeGreaterThan(0);
  });

  it("草稿探测失败时展示本地化消息而非原始错误文本", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_ssh_hosts") return [];
      if (command === "ssh_tunnels_snapshot") return emptySnapshot;
      if (command === "ssh_tunnels_refresh_status") return [];
      if (command === "ssh_tunnel_probe_draft") {
        throw new Error("Connection refused");
      }
      return undefined;
    });

    renderTunnels(<SshTunnels isVisible />);
    await settle();

    fireEvent.click(
      screen.getAllByRole("button", { name: /新建隧道|New Tunnel/ })[0],
    );
    await settle();

    const dialog = screen.getByRole("dialog");
    fireEvent.click(
      within(dialog).getByRole("button", {
        name: /检测连接|Detect Connection/,
      }),
    );
    await settle();

    expect(screen.getAllByText(/连接被拒绝/).length).toBeGreaterThan(0);
    expect(screen.queryAllByText(/Connection refused/)).toHaveLength(0);
  });

  it("分组连接部分失败时报告部分失败消息", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_ssh_hosts") return [];
      if (command === "ssh_tunnels_snapshot") return disconnectedSnapshot;
      if (command === "ssh_tunnels_refresh_status") {
        return disconnectedSnapshot.runtime;
      }
      if (command === "ssh_tunnel_group_connect") {
        return {
          operation: "connect",
          group_id: "default",
          group_name: "Default Group",
          success_count: 0,
          failed_count: 1,
          skipped_count: 0,
          total_count: 1,
          failures: [
            {
              tunnel_id: tunnelAlpha.id,
              tunnel_name: tunnelAlpha.name,
              error: "boom",
            },
          ],
        };
      }
      return undefined;
    });

    renderTunnels(<SshTunnels isVisible />);
    await settle();

    fireEvent.click(
      screen.getByRole("button", { name: /^(操作|Group actions|Actions)$/ }),
    );
    fireEvent.click(
      screen.getByRole("menuitem", { name: /全部连接|Connect all/ }),
    );
    await settle();

    expect(invokeMock).toHaveBeenCalledWith("ssh_tunnel_group_connect", {
      groupId: "default",
    });
    expect(
      screen.getByText(/部分连接成功|Connect partially|partially failed/i),
    ).toBeInTheDocument();
  });

  it("分组断开部分失败时报告部分失败消息", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_ssh_hosts") return [];
      if (command === "ssh_tunnels_snapshot") return connectedSnapshot;
      if (command === "ssh_tunnels_refresh_status") {
        return connectedSnapshot.runtime;
      }
      if (command === "ssh_tunnel_group_disconnect") {
        return {
          operation: "disconnect",
          group_id: "default",
          group_name: "Default Group",
          success_count: 0,
          failed_count: 1,
          skipped_count: 0,
          total_count: 1,
          failures: [
            {
              tunnel_id: tunnelAlpha.id,
              tunnel_name: tunnelAlpha.name,
              error: "boom",
            },
          ],
        };
      }
      return undefined;
    });

    renderTunnels(<SshTunnels isVisible />);
    await settle();

    fireEvent.click(
      screen.getByRole("button", { name: /^(操作|Group actions|Actions)$/ }),
    );
    fireEvent.click(
      screen.getByRole("menuitem", { name: /全部断开|Disconnect all/ }),
    );
    await settle();

    expect(invokeMock).toHaveBeenCalledWith("ssh_tunnel_group_disconnect", {
      groupId: "default",
    });
    expect(
      screen.getByText(/部分断开成功|Disconnect partially|partially failed/i),
    ).toBeInTheDocument();
  });

  it("分组 Tab 根据其名下隧道运行状态展示连接成功或失败徽标", async () => {
    const devGroup: SshTunnelGroupView = {
      id: "group-dev",
      name: "Development",
      created_at: 10,
      updated_at: 10,
      is_default: false,
    };
    const tunnelBeta: SshTunnelView = {
      id: "tunnel-beta",
      name: "Beta Tunnel",
      group_id: "group-dev",
      source_kind: "saved_host",
      saved_host_name: "beta-host",
      custom: null,
      forward: {
        mode: "local",
        local_bind_host: "127.0.0.1",
        local_port: 6379,
        target_host: "127.0.0.1",
        target_port: 6379,
      },
      auto_connect: false,
      auto_reconnect: true,
      created_at: 10,
      updated_at: 10,
      last_connected_at: null,
      last_error: null,
    };

    const multiGroupSnapshot: SshTunnelsSnapshot = {
      groups: [defaultGroup, devGroup],
      tunnels: [tunnelAlpha, tunnelBeta],
      runtime: [
        runtime("connected", "Alpha running"),
        {
          id: tunnelBeta.id,
          status: "error",
          active_client_count: 0,
          mode: "local",
          summary: "Beta connection error",
          resolved_server_host: null,
          listening_addr: null,
          last_error: "Connection refused",
        },
      ],
    };

    mockSnapshotInvokes(multiGroupSnapshot);
    renderTunnels(<SshTunnels isVisible />);
    await settle();

    const defaultCount = screen.getByTestId("group-tab-count-default");
    const devCount = screen.getByTestId("group-tab-count-group-dev");
    expect(defaultCount).toHaveTextContent("1");
    expect(devCount).toHaveTextContent("1");

    // 初始状态下：默认分组被选中（黑底），其已连接徽标自适应切换为白色文字与半透明白背景（text-white bg-white/20）
    expect(defaultCount.className).toContain("text-white");
    expect(defaultCount.className).toContain("bg-white/20");
    // dev 分组未选中（白底），其错误徽标展示为红色（text-destructive）
    expect(devCount.className).toContain("text-destructive");

    // 点击切换选中 dev 分组后：
    const devTab = screen.getByTestId("ssh-tunnel-group-tab-group-dev");
    fireEvent.click(devTab);
    await settle();
    // dev 分组变为选中（黑底），其错误徽标切换为白色文字与半透明白背景
    expect(devCount.className).toContain("text-white");
    expect(devCount.className).toContain("bg-white/20");
    // 默认分组变为未选中（白底），其已连接徽标切换为绿色（text-emerald-700 bg-emerald-50）
    expect(defaultCount.className).toContain("text-emerald-700");
    expect(defaultCount.className).toContain("bg-emerald-50");
  });

  it("响应 initialTab 属性自动切换并选中「已连接」全局视图", async () => {
    mockSnapshotInvokes(connectedSnapshot);
    const { rerender } = renderTunnels(<SshTunnels isVisible initialTab="__connected__" />);
    await settle();

    // 已连接全局视图处于激活态（黑底白字）
    const connectedBtn = screen.getByTestId("ssh-tunnel-connected-view-tab");
    expect(connectedBtn.className).toContain("bg-black text-white");

    // 切换到默认分组
    const defaultTab = screen.getByTestId("ssh-tunnel-group-tab-default");
    fireEvent.click(defaultTab);
    await settle();
    expect(defaultTab.className).toContain("bg-black text-white");
    expect(connectedBtn.className).not.toContain("bg-black text-white");

    // 再次通过 initialTab 与 navigationNonce 触发切换
    rerender(<SshTunnels isVisible initialTab="__connected__" navigationNonce={2} />);
    await settle();
    expect(connectedBtn.className).toContain("bg-black text-white");
  });

  it("全局「已连接」胶囊按钮展示跨分组活跃隧道，并在无活跃隧道时展示专属空状态", async () => {
    const devGroup: SshTunnelGroupView = {
      id: "group-dev",
      name: "Development",
      created_at: 10,
      updated_at: 10,
      is_default: false,
    };
    const tunnelBeta: SshTunnelView = {
      id: "tunnel-beta",
      name: "Beta Tunnel",
      group_id: "group-dev",
      source_kind: "saved_host",
      saved_host_name: "beta-host",
      custom: null,
      forward: {
        mode: "local",
        local_bind_host: "127.0.0.1",
        local_port: 6379,
        target_host: "127.0.0.1",
        target_port: 6379,
      },
      auto_connect: false,
      auto_reconnect: true,
      created_at: 10,
      updated_at: 10,
      last_connected_at: null,
      last_error: null,
    };
    const tunnelGamma: SshTunnelView = {
      id: "tunnel-gamma",
      name: "Gamma Tunnel",
      group_id: "group-dev",
      source_kind: "saved_host",
      saved_host_name: "gamma-host",
      custom: null,
      forward: {
        mode: "dynamic",
        local_bind_host: "127.0.0.1",
        local_port: 1080,
      },
      auto_connect: false,
      auto_reconnect: true,
      created_at: 20,
      updated_at: 20,
      last_connected_at: null,
      last_error: null,
    };

    const multiGroupSnapshot: SshTunnelsSnapshot = {
      groups: [defaultGroup, devGroup],
      tunnels: [tunnelAlpha, tunnelBeta, tunnelGamma],
      runtime: [
        runtime("connected", "Alpha running"),
        {
          id: tunnelBeta.id,
          status: "connecting",
          active_client_count: 0,
          mode: "local",
          summary: "Beta connecting...",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
        {
          id: tunnelGamma.id,
          status: "disconnected",
          active_client_count: 0,
          mode: "dynamic",
          summary: "Gamma idle",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
      ],
    };

    mockSnapshotInvokes(multiGroupSnapshot);
    renderTunnels(<SshTunnels isVisible />);
    await settle();

    // 默认分组下只展示 Alpha Tunnel
    expect(screen.getByText("Alpha Tunnel")).toBeInTheDocument();
    expect(screen.queryByText("Beta Tunnel")).not.toBeInTheDocument();
    expect(screen.queryByText("Gamma Tunnel")).not.toBeInTheDocument();

    // 切换到「已连接」全局视图
    const connectedBtn = screen.getByTestId("ssh-tunnel-connected-view-tab");
    expect(connectedBtn).toHaveTextContent("2");
    fireEvent.click(connectedBtn);
    await settle();

    // 跨分组同时展示 Alpha Tunnel 和 Beta Tunnel，不展示断开的 Gamma Tunnel
    expect(screen.getByText("Alpha Tunnel")).toBeInTheDocument();
    expect(screen.getByText("Beta Tunnel")).toBeInTheDocument();
    expect(screen.queryByText("Gamma Tunnel")).not.toBeInTheDocument();

    // 模拟无任何连接时并点击刷新
    const allDisconnectedSnapshot: SshTunnelsSnapshot = {
      groups: [defaultGroup, devGroup],
      tunnels: [tunnelAlpha, tunnelBeta],
      runtime: [
        runtime("disconnected", "Alpha idle"),
        {
          id: tunnelBeta.id,
          status: "disconnected",
          active_client_count: 0,
          mode: "local",
          summary: "Beta idle",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
      ],
    };
    mockSnapshotInvokes(allDisconnectedSnapshot);
    fireEvent.click(screen.getByRole("button", { name: /刷新|Refresh/ }));
    await settle();

    expect(
      screen.getByText(/当前暂无正在连接或运行中的 SSH 隧道|No active or connected SSH tunnels currently/i),
    ).toBeInTheDocument();
    expect(connectedBtn).toHaveTextContent("0");
  });

  it("在「已连接」全局视图下操作菜单支持全部断开", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_ssh_hosts") return [];
      if (command === "ssh_tunnels_snapshot") return connectedSnapshot;
      if (command === "ssh_tunnels_refresh_status") {
        return connectedSnapshot.runtime;
      }
      if (command === "ssh_tunnels_disconnect_all") {
        return {
          operation: "disconnect",
          group_id: "all",
          group_name: "All Tunnels",
          success_count: 1,
          failed_count: 0,
          skipped_count: 0,
          total_count: 1,
          failures: [],
        };
      }
      return undefined;
    });

    renderTunnels(<SshTunnels isVisible />);
    await settle();

    // 点击切换到「已连接」全局视图
    fireEvent.click(screen.getByTestId("ssh-tunnel-connected-view-tab"));
    await settle();

    // 打开操作菜单
    fireEvent.click(
      screen.getByRole("button", { name: /^(操作|Group actions|Actions)$/ }),
    );
    await settle();

    const disconnectAllBtn = screen.getByRole("menuitem", {
      name: /断开全部运行中隧道|Disconnect all active tunnels/,
    });
    fireEvent.click(disconnectAllBtn);
    await settle();

    expect(invokeMock).toHaveBeenCalledWith("ssh_tunnels_disconnect_all", undefined);
    expect(
      screen.getByText(/全部隧道已断开|All tunnels disconnected/i),
    ).toBeInTheDocument();
  });

  it("在「已连接」视图中根据所属分组进行分组展示，并且空分组不展示", async () => {
    const stagingGroup: SshTunnelGroupView = {
      id: "group-staging",
      name: "Staging",
      created_at: 5,
      updated_at: 5,
      is_default: false,
    };
    const prodGroup: SshTunnelGroupView = {
      id: "group-prod",
      name: "Production",
      created_at: 10,
      updated_at: 10,
      is_default: false,
    };
    const emptyGroup: SshTunnelGroupView = {
      id: "group-empty",
      name: "Empty Group",
      created_at: 15,
      updated_at: 15,
      is_default: false,
    };

    const stagingTunnel1: SshTunnelView = {
      ...tunnelAlpha,
      id: "tunnel-stg-1",
      name: "Staging Redis",
      group_id: "group-staging",
    };
    const stagingTunnel2: SshTunnelView = {
      ...tunnelAlpha,
      id: "tunnel-stg-2",
      name: "Staging DB",
      group_id: "group-staging",
    };
    const prodTunnel: SshTunnelView = {
      ...tunnelAlpha,
      id: "tunnel-prod-1",
      name: "Prod Web",
      group_id: "group-prod",
    };
    const disconnectedTunnel: SshTunnelView = {
      ...tunnelAlpha,
      id: "tunnel-empty-1",
      name: "Empty Offline",
      group_id: "group-empty",
    };

    const testSnapshot: SshTunnelsSnapshot = {
      groups: [defaultGroup, stagingGroup, prodGroup, emptyGroup],
      tunnels: [stagingTunnel1, stagingTunnel2, prodTunnel, disconnectedTunnel],
      runtime: [
        {
          id: stagingTunnel1.id,
          status: "connected",
          active_client_count: 0,
          mode: "local",
          summary: "Staging Redis connected",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
        {
          id: stagingTunnel2.id,
          status: "connecting",
          active_client_count: 0,
          mode: "local",
          summary: "Staging DB connecting",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
        {
          id: prodTunnel.id,
          status: "connected",
          active_client_count: 0,
          mode: "local",
          summary: "Prod Web connected",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
        {
          id: disconnectedTunnel.id,
          status: "disconnected",
          active_client_count: 0,
          mode: "local",
          summary: "Offline",
          resolved_server_host: null,
          listening_addr: null,
          last_error: null,
        },
      ],
    };

    mockSnapshotInvokes(testSnapshot);
    renderTunnels(<SshTunnels isVisible />);
    await settle();

    // 切换到「已连接」全局视图
    fireEvent.click(screen.getByTestId("ssh-tunnel-connected-view-tab"));
    await settle();

    // 验证 staging 分组存在，且展示 2 个连接
    const stgSection = screen.getByTestId("ssh-tunnel-connected-group-group-staging");
    expect(stgSection).toBeInTheDocument();
    expect(within(stgSection).getByText("Staging")).toBeInTheDocument();
    expect(within(stgSection).getByText("2")).toBeInTheDocument();
    expect(within(stgSection).getByText("Staging Redis")).toBeInTheDocument();
    expect(within(stgSection).getByText("Staging DB")).toBeInTheDocument();

    // 验证 prod 分组存在，且展示 1 个连接
    const prodSection = screen.getByTestId("ssh-tunnel-connected-group-group-prod");
    expect(prodSection).toBeInTheDocument();
    expect(within(prodSection).getByText("Production")).toBeInTheDocument();
    expect(within(prodSection).getByText("1")).toBeInTheDocument();
    expect(within(prodSection).getByText("Prod Web")).toBeInTheDocument();

    // 验证没有任何活跃连接的分组（default 和 emptyGroup）不渲染分组区块
    expect(screen.queryByTestId("ssh-tunnel-connected-group-default")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ssh-tunnel-connected-group-group-empty")).not.toBeInTheDocument();

    // 验证新优化的分组头部包含快捷跳转操作并支持点击切换到对应分组
    const viewAllButtons = within(stgSection).getAllByRole("button", { name: /View all|查看全部/i });
    expect(viewAllButtons.length).toBeGreaterThan(0);
    fireEvent.click(viewAllButtons[0]);
    await settle();
    const stgTab = screen.getByTestId("ssh-tunnel-group-tab-group-staging");
    expect(stgTab.className).toContain("bg-black text-white");
  });

  it("点击常用端口按钮可打开管理常用端口弹窗", async () => {
    const testSnapshot: SshTunnelsSnapshot = {
      groups: [defaultGroup],
      tunnels: [],
      runtime: [],
      common_ports: [
        {
          id: "mysql-port",
          name: "MySQL",
          localPort: 3306,
          remotePort: 3306,
          description: "Database",
          created_at: 1,
          updated_at: 1,
        },
      ],
    };
    mockSnapshotInvokes(testSnapshot);
    renderTunnels(<SshTunnels isVisible />);
    await settle();

    // 点击顶部“常用端口”按钮
    const commonPortsButton = screen.getByRole("button", { name: /Common Ports|常用端口/i });
    expect(commonPortsButton).toBeInTheDocument();
    fireEvent.click(commonPortsButton);
    await settle();

    // 检查管理弹窗是否展示了 MySQL 3306 → 3306
    expect(screen.getByText("3306 → 3306")).toBeInTheDocument();
    expect(screen.getByText("MySQL")).toBeInTheDocument();
  });

  it("新建隧道时只保留单个常用端口按钮，选中后同时作用于本地与目标端口", async () => {
    const testSnapshot: SshTunnelsSnapshot = {
      groups: [defaultGroup],
      tunnels: [],
      runtime: [],
      common_ports: [
        {
          id: "pg-port",
          name: "PostgreSQL",
          localPort: 5432,
          remotePort: 5432,
          description: "Postgres DB",
          created_at: 1,
          updated_at: 1,
        },
      ],
    };
    mockSnapshotInvokes(testSnapshot);
    renderTunnels(<SshTunnels isVisible />);
    await settle();

    // 打开新建隧道弹窗
    const newTunnelButtons = screen.getAllByRole("button", { name: /New Tunnel|新建隧道/i });
    fireEvent.click(newTunnelButtons[0]);
    await settle();

    // 验证新建隧道弹窗中只保留了一个常用端口下拉按钮
    const portSelectButtons = screen.getAllByTitle(/Select from common ports|从常用端口中选择/i);
    expect(portSelectButtons).toHaveLength(1);

    // 点击该唯一的常用端口选择按钮
    fireEvent.click(portSelectButtons[0]);
    await settle();

    // 在下拉菜单中选择 PostgreSQL (5432 → 5432)
    const pgOption = screen.getByRole("button", { name: /PostgreSQL.*5432/i });
    fireEvent.click(pgOption);
    await settle();

    // 检查 Target Port、Local Port 以及 Name 的自动联动填充
    const portInputs = screen.getAllByDisplayValue("5432");
    expect(portInputs).toHaveLength(2);

    const nameInput = screen.getByDisplayValue("PostgreSQL (5432)");
    expect(nameInput).toBeInTheDocument();
  });
});
