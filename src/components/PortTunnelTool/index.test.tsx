import { act, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PortTunnelTool } from "@/components/PortTunnelTool";
import type { TunnelConfig, TunnelRuntimeStatus, EnvironmentStatus } from "@/lib/portTunnel";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import { getToolboxTool } from "@/toolbox/registry";

const { pushToastMock } = vi.hoisted(() => ({ pushToastMock: vi.fn() }));

vi.mock("@/components/ToastProvider", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/ToastProvider")
  >("@/components/ToastProvider");
  return {
    ...actual,
    useToast: () => ({ pushToast: pushToastMock, dismissToast: vi.fn() }),
  };
});

vi.mock("qrcode.react", () => ({
  QRCodeSVG: ({ value }: { value: string }) => <div data-testid="qr-code">{value}</div>,
}));

vi.mock("@/lib/externalActions", () => ({
  openExternalUrl: vi.fn(),
}));

const mockTunnel: TunnelConfig = {
  id: "test-tunnel-1",
  name: "Frontend Vite",
  mode: "cloudflare",
  local_port: 5173,
  protocol: "http",
  auto_start: false,
};

const stoppedStatus: TunnelRuntimeStatus = {
  id: "test-tunnel-1",
  state: "stopped",
  public_url: undefined,
  error: undefined,
  started_at: undefined,
  active_connections: 0,
};

const envInstalled: EnvironmentStatus = {
  mode: "cloudflare",
  installed: true,
  binary_path: "/usr/local/bin/cloudflared",
  version: "cloudflared version 2024.1.0",
  recommended_command: "brew install cloudflared",
};

describe("PortTunnelTool", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "tunnel_list") return [mockTunnel];
      if (command === "tunnel_list_status") return [stoppedStatus];
      if (command === "tunnel_status") return stoppedStatus;
      if (command === "tunnel_check_environment") return envInstalled;
      if (command === "tunnel_get_logs") return ["[12:00:00] Initialized"];
      return null;
    });
  });

  it("renders the tool header with Globe icon and bilingual title", async () => {
    await act(async () => {
      renderWithProviders(<PortTunnelTool />);
    });

    const title = screen.getByRole("heading", { level: 2, name: /Port Tunnel|内网穿透/ });
    expect(title).toBeInTheDocument();

    const { iconClassName } = getToolboxTool("port-tunnel")!;
    const iconContainer = title.parentElement?.previousElementSibling;
    expect(iconContainer).toHaveClass(...iconClassName.split(" "));
  });

  it("displays the active tunnel card with local port and action buttons", async () => {
    await act(async () => {
      renderWithProviders(<PortTunnelTool />);
    });

    expect(screen.getByRole("heading", { level: 3, name: "Frontend Vite" })).toBeInTheDocument();
    expect(screen.getByText("127.0.0.1:5173")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start Tunnel|启动穿透/ })).toBeInTheDocument();
  });

  it("renders the public URL box and QR button when running", async () => {
    const runningStatus: TunnelRuntimeStatus = {
      id: "test-tunnel-1",
      state: "running",
      public_url: "https://vite-preview-xyz.trycloudflare.com",
      error: undefined,
      started_at: Date.now(),
      active_connections: 1,
    };

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "tunnel_list") return [mockTunnel];
      if (command === "tunnel_status") return runningStatus;
      if (command === "tunnel_check_environment") return envInstalled;
      if (command === "tunnel_get_logs") return ["[12:00:00] Tunnel online"];
      return null;
    });

    await act(async () => {
      renderWithProviders(<PortTunnelTool />);
    });

    expect(screen.getByText("https://vite-preview-xyz.trycloudflare.com")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Stop Tunnel|停止穿透/ })).toBeInTheDocument();
  });

  it("shows missing dependency banner when environment lacks binary", async () => {
    const envMissing: EnvironmentStatus = {
      mode: "cloudflare",
      installed: false,
      binary_path: undefined,
      version: undefined,
      recommended_command: "brew install cloudflared",
    };

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "tunnel_list") return [mockTunnel];
      if (command === "tunnel_status") return stoppedStatus;
      if (command === "tunnel_check_environment") return envMissing;
      if (command === "tunnel_get_logs") return [];
      return null;
    });

    await act(async () => {
      renderWithProviders(<PortTunnelTool />);
    });

    expect(screen.getByText(/brew install cloudflared/)).toBeInTheDocument();
  });

  it("renders a green pulsating dot in the tunnel list and card when tunnel is running", async () => {
    const tunnel2: TunnelConfig = {
      id: "test-tunnel-2",
      name: "Backend API",
      mode: "cloudflare",
      local_port: 8080,
      protocol: "http",
      auto_start: false,
    };
    const runningStatus2: TunnelRuntimeStatus = {
      id: "test-tunnel-2",
      state: "running",
      public_url: "https://api-xyz.trycloudflare.com",
      error: undefined,
      started_at: Date.now(),
      active_connections: 1,
    };

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "tunnel_list") return [mockTunnel, tunnel2];
      if (command === "tunnel_list_status") return [stoppedStatus, runningStatus2];
      if (command === "tunnel_status") return runningStatus2;
      if (command === "tunnel_check_environment") return envInstalled;
      if (command === "tunnel_get_logs") return [];
      return null;
    });

    await act(async () => {
      renderWithProviders(<PortTunnelTool isVisible={true} />);
    });

    // 自动聚焦到正在 running 的 tunnel-2
    expect(screen.getByRole("heading", { level: 3, name: "Backend API" })).toBeInTheDocument();
    const itemBtn = screen.getByTestId("port-tunnel-item-test-tunnel-2");
    expect(itemBtn).toBeInTheDocument();
    const pingDot = itemBtn.querySelector(".animate-ping.bg-emerald-400");
    const emeraldDot = itemBtn.querySelector(".bg-emerald-500");
    expect(pingDot).toBeInTheDocument();
    expect(emeraldDot).toBeInTheDocument();
  });

  it("calls openExternalUrl to launch system default browser when Open button is clicked", async () => {
    const { openExternalUrl } = await import("@/lib/externalActions");
    const runningStatus: TunnelRuntimeStatus = {
      id: "test-tunnel-1",
      state: "running",
      public_url: "https://vite-preview-xyz.trycloudflare.com",
      error: undefined,
      started_at: Date.now(),
      active_connections: 1,
    };

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "tunnel_list") return [mockTunnel];
      if (command === "tunnel_list_status") return [runningStatus];
      if (command === "tunnel_status") return runningStatus;
      if (command === "tunnel_check_environment") return envInstalled;
      if (command === "tunnel_get_logs") return [];
      return null;
    });

    await act(async () => {
      renderWithProviders(<PortTunnelTool isVisible={true} />);
    });

    const openBtn = screen.getByRole("button", { name: /Open|打开/ });
    await act(async () => {
      openBtn.click();
    });

    expect(openExternalUrl).toHaveBeenCalledWith("https://vite-preview-xyz.trycloudflare.com");
  });
});
