import { describe, expect, it } from "vitest";
import {
  deriveSshTunnelHeaderSummary,
  deriveSshTunnelLauncherSummary,
} from "@/lib/sshTunnelSummary";
import type {
  SshTunnelGroupView,
  SshTunnelRuntimeView,
  SshTunnelsSnapshot,
  SshTunnelView,
} from "@/components/sshTunnels/types";

const defaultGroup: SshTunnelGroupView = {
  id: "default",
  name: "Default Group",
  created_at: 0,
  updated_at: 0,
  is_default: true,
};

function makeTunnel(
  overrides: Partial<SshTunnelView> & Pick<SshTunnelView, "id" | "name">,
): SshTunnelView {
  return {
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
    ...overrides,
  };
}

function makeRuntime(
  overrides: Partial<SshTunnelRuntimeView> & Pick<SshTunnelRuntimeView, "id" | "status">,
): SshTunnelRuntimeView {
  return {
    active_client_count: 0,
    mode: "local",
    summary: `runtime-${overrides.id}`,
    resolved_server_host: null,
    listening_addr: null,
    last_error: null,
    ...overrides,
  };
}

function makeSnapshot(
  overrides: Partial<SshTunnelsSnapshot> = {},
): SshTunnelsSnapshot {
  return {
    groups: [defaultGroup],
    tunnels: [],
    runtime: [],
    ...overrides,
  };
}

describe("deriveSshTunnelHeaderSummary", () => {
  it("空快照返回中性汇总", () => {
    expect(
      deriveSshTunnelHeaderSummary(makeSnapshot({ groups: [] })),
    ).toEqual({
      hasTunnels: false,
      connectedCount: 0,
      disconnectedCount: 0,
      connectingCount: 0,
      reconnectingCount: 0,
      totalCount: 0,
      hasErrors: false,
      hasConnecting: false,
      errorTunnelNames: [],
    });
  });

  it("按运行时状态统计连接、错误与重连数量", () => {
    const summary = deriveSshTunnelHeaderSummary(
      makeSnapshot({
        tunnels: [
          makeTunnel({ id: "a", name: "Alpha" }),
          makeTunnel({ id: "b", name: "Beta" }),
          makeTunnel({ id: "c", name: "Gamma" }),
        ],
        runtime: [
          makeRuntime({ id: "a", status: "connected" }),
          makeRuntime({ id: "b", status: "error", last_error: "boom" }),
          makeRuntime({ id: "c", status: "reconnecting" }),
        ],
      }),
    );

    expect(summary.hasTunnels).toBe(true);
    expect(summary.connectedCount).toBe(1);
    expect(summary.disconnectedCount).toBe(1);
    expect(summary.connectingCount).toBe(0);
    expect(summary.reconnectingCount).toBe(1);
    expect(summary.totalCount).toBe(3);
    expect(summary.hasErrors).toBe(true);
    expect(summary.hasConnecting).toBe(true);
    expect(summary.errorTunnelNames).toEqual(["Beta"]);
  });

  it("错误运行时缺少匹配隧道时回退到运行时摘要", () => {
    const summary = deriveSshTunnelHeaderSummary(
      makeSnapshot({
        tunnels: [makeTunnel({ id: "known", name: "Known" })],
        runtime: [
          makeRuntime({ id: "known", status: "error" }),
          makeRuntime({
            id: "orphan",
            status: "error",
            summary: "Orphan failure",
          }),
        ],
      }),
    );

    expect(summary.disconnectedCount).toBe(2);
    expect(summary.errorTunnelNames).toEqual(["Known", "Orphan failure"]);
  });

  it("connecting 状态计入 hasConnecting", () => {
    const summary = deriveSshTunnelHeaderSummary(
      makeSnapshot({
        tunnels: [makeTunnel({ id: "a", name: "Alpha" })],
        runtime: [makeRuntime({ id: "a", status: "connecting" })],
      }),
    );

    expect(summary.connectingCount).toBe(1);
    expect(summary.reconnectingCount).toBe(0);
    expect(summary.hasConnecting).toBe(true);
    expect(summary.hasErrors).toBe(false);
  });
});

describe("deriveSshTunnelLauncherSummary", () => {
  it("自动连接隧道出错时状态为 failed", () => {
    const summary = deriveSshTunnelLauncherSummary(
      makeSnapshot({
        tunnels: [
          makeTunnel({ id: "auto-ok", name: "Auto OK", auto_connect: true }),
          makeTunnel({ id: "auto-fail", name: "Auto Fail", auto_connect: true }),
          makeTunnel({ id: "manual", name: "Manual" }),
        ],
        runtime: [
          makeRuntime({ id: "auto-ok", status: "connected" }),
          makeRuntime({ id: "auto-fail", status: "error", last_error: "nope" }),
          makeRuntime({ id: "manual", status: "connected" }),
        ],
      }),
    );

    expect(summary.state).toBe("failed");
    expect(summary.connectedCount).toBe(2);
    expect(summary.autoConnectingCount).toBe(0);
    expect(summary.autoConnectFailedCount).toBe(1);
  });

  it("自动连接隧道连接中时状态为 connecting", () => {
    const summary = deriveSshTunnelLauncherSummary(
      makeSnapshot({
        tunnels: [
          makeTunnel({ id: "auto", name: "Auto", auto_connect: true }),
        ],
        runtime: [makeRuntime({ id: "auto", status: "connecting" })],
      }),
    );

    expect(summary.state).toBe("connecting");
    expect(summary.autoConnectingCount).toBe(1);
    expect(summary.autoConnectFailedCount).toBe(0);
  });

  it("无自动连接失败时状态为 connected", () => {
    const summary = deriveSshTunnelLauncherSummary(
      makeSnapshot({
        tunnels: [
          makeTunnel({ id: "auto", name: "Auto", auto_connect: true }),
        ],
        runtime: [makeRuntime({ id: "auto", status: "connected" })],
      }),
    );

    expect(summary.state).toBe("connected");
    expect(summary.connectedCount).toBe(1);
    expect(summary.autoConnectingCount).toBe(0);
    expect(summary.autoConnectFailedCount).toBe(0);
  });

  it("运行时缺失但隧道记录保留上次错误时计入自动连接失败", () => {
    const summary = deriveSshTunnelLauncherSummary(
      makeSnapshot({
        tunnels: [
          makeTunnel({
            id: "auto",
            name: "Auto",
            auto_connect: true,
            last_error: "stored failure",
          }),
        ],
        runtime: [],
      }),
    );

    expect(summary.state).toBe("failed");
    expect(summary.autoConnectFailedCount).toBe(1);
  });
});
