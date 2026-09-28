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
});
