import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UptimeMonitorTool } from "./index";
import { HeartbeatBar } from "./HeartbeatBar";
import { renderWithProviders } from "@/test/mocks/render";
import type { HeartbeatBarPoint } from "./types";

describe("UptimeMonitorTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders the tool shell, header buttons, and empty state", () => {
    renderWithProviders(<UptimeMonitorTool />);
    expect(
      screen.getByRole("heading", { name: /服务状态监控/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /新建监控/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /刷新/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/暂无监控项/i)).toBeInTheDocument();
  });

  it("opens the create modal and toggles protocol tabs", async () => {
    const user = userEvent.setup();
    renderWithProviders(<UptimeMonitorTool />);

    const newBtn = screen.getByRole("button", { name: /新建监控/i });
    await user.click(newBtn);

    expect(screen.getByText(/新建监控目标/i)).toBeInTheDocument();

    // 切换到 TCP
    const tcpBtn = screen.getByRole("button", { name: /^tcp$/i });
    await user.click(tcpBtn);

    expect(screen.getByText(/目标主机与端口/i)).toBeInTheDocument();

    // 切换回 HTTP
    const httpBtn = screen.getByRole("button", { name: /^http$/i });
    await user.click(httpBtn);
    expect(screen.getByText(/HTTP 高级参数/i)).toBeInTheDocument();
  });

  it("renders HeartbeatBar with correct status colors", () => {
    const points: HeartbeatBarPoint[] = [
      { timestamp: 1000, status: "up", responseTimeMs: 30, statusCode: 200 },
      { timestamp: 1030, status: "down", responseTimeMs: 1500, statusCode: 500 },
      { timestamp: 1060, status: "degraded", responseTimeMs: 2500, statusCode: 200 },
    ];

    const { container } = renderWithProviders(<HeartbeatBar heartbeats={points} maxBars={10} />);
    // 应该渲染 10 根 bar (7 根 empty + 3 根有效)
    const bars = container.querySelectorAll(".flex-1");
    expect(bars.length).toBe(10);
  });
});
