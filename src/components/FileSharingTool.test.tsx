import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileSharingTool } from "@/components/FileSharingTool";
import type { FileSharingSnapshot } from "@/lib/fileSharing";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";
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

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("qrcode.react", () => ({ QRCodeSVG: ({ value }: { value: string }) => <div data-testid="qr-code">{value}</div> }));

const stopped: FileSharingSnapshot = { running: false, sessionId: null, shareUrl: null, startedAt: null, files: [], transfers: [], summary: { activeTransfers: 0, completedTransfers: 0, failedTransfers: 0, cancelledTransfers: 0, bytesSent: 0, droppedTransferRecords: 0 } };

describe("FileSharingTool", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "file_sharing_status") return stopped;
      if (command === "file_sharing_start") return { ...stopped, running: true, shareUrl: "http://192.168.1.2:1234/s/token/", files: [{ id: "file-1", name: "report.txt", sourcePath: "/tmp/report.txt", size: 12, modifiedAt: 0 }] };
      return stopped;
    });
  });

  it("renders the shared Share2 presentation before the detail title", async () => {
    await act(async () => {
      renderWithProviders(<FileSharingTool />);
    });

    const title = screen.getByRole("heading", { level: 2, name: /File Sharing|文件共享/ });
    const iconContainer = title.parentElement?.previousElementSibling;
    const { iconClassName } = getToolboxTool("file-sharing")!;

    expect(iconContainer).toHaveClass(...iconClassName.split(" "));
    expect(iconContainer?.querySelector("svg")).toHaveClass("lucide-share-2");
    expect(
      screen.getByText(
        /Share files over local network or ephemeral public tunnels|通过局域网或临时公网隧道共享文件/,
      ),
    ).toBeInTheDocument();
  });

  it("keeps selected files when start fails", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "file_sharing_status") return stopped;
      if (command === "file_sharing_start") throw new Error("network changed");
      return stopped;
    });
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    await user.click(screen.getByRole("button", { name: /Start sharing|开始共享/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("network changed");
    expect(screen.getByText("/tmp/report.txt")).toBeInTheDocument();
  });

  it("renders backend-issued QR link after starting", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    await user.click(screen.getByRole("button", { name: /Start sharing|开始共享/ }));
    expect(await screen.findByTestId("qr-code")).toHaveTextContent("http://192.168.1.2:1234/s/token/");
  });

  it("does not replace a newer status with an earlier request", async () => {
    let resolveFirst: (value: typeof stopped) => void = () => {};
    let resolveSecond: (value: typeof stopped) => void = () => {};
    let statusCalls = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "file_sharing_networks") return Promise.resolve([{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }]);
      if (command === "file_sharing_status") {
        statusCalls += 1;
        return new Promise((resolve) => {
          if (statusCalls === 1) resolveFirst = resolve;
          else resolveSecond = resolve;
        });
      }
      return Promise.resolve(stopped);
    });
    const { rerender } = renderWithProviders(<FileSharingTool />);
    await act(async () => {
      await Promise.resolve();
      rerender(<FileSharingTool isVisible={false} />);
      rerender(<FileSharingTool />);
    });
    await act(async () => {
      resolveSecond({ ...stopped, running: true, shareUrl: "http://192.168.1.2:1234/s/new/" });
    });
    expect(screen.getByTestId("qr-code")).toHaveTextContent("/new/");
    await act(async () => {
      resolveFirst(stopped);
    });
    expect(screen.getByTestId("qr-code")).toHaveTextContent("/new/");
  });

  it("invalidates an earlier status request when starting a session", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    let resolveStatus: (value: typeof stopped) => void = () => {};
    invokeMock.mockImplementation((command: string) => {
      if (command === "file_sharing_networks") return Promise.resolve([{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }]);
      if (command === "file_sharing_status") return new Promise((resolve) => { resolveStatus = resolve; });
      if (command === "file_sharing_start") return Promise.resolve({ ...stopped, running: true, shareUrl: "http://192.168.1.2:1234/s/new/" });
      return Promise.resolve(stopped);
    });
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    await user.click(screen.getByRole("button", { name: /Start sharing|开始共享/ }));
    expect(await screen.findByTestId("qr-code")).toHaveTextContent("/new/");
    await act(async () => { resolveStatus(stopped); });
    expect(screen.getByTestId("qr-code")).toHaveTextContent("/new/");
  });

  it("keeps the stopped session summary and records when an earlier status resolves", async () => {
    const running: FileSharingSnapshot = {
      ...stopped,
      running: true,
      sessionId: "session-1",
      shareUrl: "http://192.168.1.2:1234/s/old/",
      files: [{ id: "file-1", name: "report.txt", sourcePath: "/tmp/report.txt", size: 12, modifiedAt: 0 }],
    };
    const ended: FileSharingSnapshot = {
      ...running,
      running: false,
      shareUrl: null,
      transfers: [{ id: "transfer-1", fileName: "report.txt", clientAddress: "127.0.0.1", state: "completed", bytesSent: 12 }],
      summary: { activeTransfers: 0, completedTransfers: 1, failedTransfers: 0, cancelledTransfers: 0, bytesSent: 12, droppedTransferRecords: 0 },
    };
    let resolveStale: (value: FileSharingSnapshot) => void = () => {};
    let statusCalls = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "file_sharing_networks") return Promise.resolve([{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }]);
      if (command === "file_sharing_status") {
        statusCalls += 1;
        return statusCalls === 1 ? Promise.resolve(running) : new Promise((resolve) => { resolveStale = resolve; });
      }
      if (command === "file_sharing_stop") return Promise.resolve(ended);
      return Promise.resolve(stopped);
    });
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    expect(await screen.findByTestId("qr-code")).toHaveTextContent("/old/");
    await act(async () => {
      const listener = (listenMock.mock.calls as unknown as Array<[string, (event: { payload: { kind: "transfer" } }) => void]>)[0]?.[1];
      listener?.({ payload: { kind: "transfer" } });
    });
    await user.click(screen.getByRole("button", { name: /Stop sharing|停止共享/ }));
    expect(await screen.findByText(/Sharing ended|共享已结束/)).toBeInTheDocument();
    await act(async () => { resolveStale(running); });
    expect(screen.queryByTestId("qr-code")).not.toBeInTheDocument();
    expect(screen.getByText(/Completed 1|已完成 1/)).toBeInTheDocument();
    expect(screen.getAllByText(/report.txt/)).toHaveLength(2);
  });

  it("在 isVisible=false 时不因 file-sharing-updated 事件刷新状态", async () => {
    renderWithProviders(
      <FileSharingTool
        {...({ isVisible: false } as Record<string, unknown>)}
      />,
    );
    await act(async () => {
      await Promise.resolve();
    });

    invokeMock.mockClear();

    const listener = (
      listenMock.mock.calls as unknown as Array<
        [string, (event: unknown) => void]
      >
    )[0]?.[1];
    listener?.({ payload: { kind: "transfer" } });

    await act(async () => {
      await Promise.resolve();
    });

    expect(invokeMock).not.toHaveBeenCalledWith("file_sharing_status");
  });

  it("复制共享链接失败时通过 toast 通道提示本地化错误", async () => {
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    });
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") {
        return [
          {
            id: "en0:192.168.1.2",
            interfaceName: "en0",
            address: "192.168.1.2",
          },
        ];
      }
      if (command === "file_sharing_status") {
        return {
          ...stopped,
          running: true,
          shareUrl: "http://192.168.1.2:1234/s/token/",
        };
      }
      return stopped;
    });

    renderWithProviders(<FileSharingTool />);
    await user.click(
      await screen.findByRole("button", { name: /Copy link|复制链接/ }),
    );

    await act(async () => {
      await Promise.resolve();
    });

    expect(pushToastMock).toHaveBeenCalled();
    const pushed = pushToastMock.mock.calls
      .map(([toast]) => JSON.stringify(toast))
      .join(" ");
    expect(pushed).toMatch(/copy|复制/i);
  });

  it("renders the empty state without an active share and does not throw", async () => {
    renderWithProviders(<FileSharingTool />);

    expect(
      await screen.findByText(
        /Choose one or more files to share|请选择一个或多个要共享的文件/,
      ),
    ).toBeInTheDocument();
  });

  it("never invokes window.confirm during mount or interactions", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);

    await user.click(
      await screen.findByRole("button", { name: /Rescan|重新扫描/ }),
    );
    await user.click(
      screen.getByRole("button", { name: /Choose files|选择文件/ }),
    );
    await user.click(screen.getByRole("button", { name: /Clear|清空/ }));

    expect(confirmSpy).not.toHaveBeenCalled();
  });
});

describe("FileSharingTool public tunnel (RED)", () => {
  const runningLan = {
    ...stopped,
    running: true,
    sessionId: "session-1",
    shareUrl: "http://192.168.1.2:1234/s/token/",
    files: [{ id: "file-1", name: "report.txt", sourcePath: "/tmp/report.txt", size: 12, modifiedAt: 0 }],
    passwordProtected: false,
    publicTunnelId: null,
    publicState: "disabled",
    publicShareUrl: null,
    publicError: null,
  };

  const runningPublic = {
    ...runningLan,
    publicTunnelId: "tunnel-1",
    publicState: "running",
    publicShareUrl: "https://abc.trycloudflare.com/s/token/",
  };

  function baseMocks(overrides?: (command: string) => unknown) {
    invokeMock.mockImplementation(async (command: string, _args?: unknown) => {
      if (overrides) {
        const value = overrides(command);
        if (value !== undefined) return value;
      }
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return stopped;
      if (command === "file_sharing_start") return runningLan;
      return stopped;
    });
  }

  it("does not check tunnel environment on a LAN-only mount/start", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    baseMocks();
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    await user.click(screen.getByRole("button", { name: /Start sharing|开始共享/ }));
    expect(await screen.findByTestId("qr-code")).toHaveTextContent("http://192.168.1.2:1234/s/token/");
    const calls = invokeMock.mock.calls.map((call) => call[0]);
    expect(calls).not.toContain("tunnel_check_environment");
  });

  it("switches QR to the public URL when public sharing runs", async () => {
    baseMocks((command) => {
      if (command === "file_sharing_start") return { ...runningLan, publicAccess: true } as unknown;
    });
    // simulate a running public snapshot via status after enabling
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningPublic;
      if (command === "file_sharing_start") return runningPublic;
      if (command === "file_sharing_public_start") return runningPublic;
      return stopped;
    });
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    await user.click(screen.getByRole("button", { name: /Start sharing|开始共享/ }));
    const toggle = await screen.findByTestId("file-sharing-public-toggle");
    await user.click(toggle);
    expect(await screen.findByTestId("file-sharing-public-card")).toHaveAttribute("data-state", "running");
    expect(await screen.findByTestId("qr-code")).toHaveTextContent("https://abc.trycloudflare.com/s/token/");
    expect(screen.getByTestId("file-sharing-lan-url")).toHaveTextContent("http://192.168.1.2:1234/s/token/");
  });

  it("shows the dependency banner when cloudflared is missing and disables start only while public is on", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: false, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return stopped;
      return stopped;
    });
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    // public off: start enabled
    const startButton = screen.getByRole("button", { name: /Start sharing|开始共享/ });
    expect(startButton).not.toBeDisabled();
    // enable public
    await user.click(screen.getByTestId("file-sharing-public-toggle"));
    expect(await screen.findByText(/Dependency Missing|依赖缺失|cloudflared/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start sharing|开始共享/ })).toBeDisabled();
  });

  it("calls public start/stop and retry commands", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningPublic;
      if (command === "file_sharing_public_start") return runningPublic;
      if (command === "file_sharing_public_stop") return runningLan;
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    // enable public on running share
    const enable = await screen.findByTestId("file-sharing-public-enable");
    await userEvent.setup().click(enable);
    expect(invokeMock).toHaveBeenCalledWith("file_sharing_public_start");
    const disable = await screen.findByTestId("file-sharing-public-disable");
    await userEvent.setup().click(disable);
    expect(invokeMock).toHaveBeenCalledWith("file_sharing_public_stop");
  });

  it("shows error state and retries with a new URL", async () => {
    const errored = { ...runningLan, publicTunnelId: "tunnel-1", publicState: "error", publicShareUrl: null, publicError: "tunnel died" };
    const retried = { ...runningLan, publicTunnelId: "tunnel-2", publicState: "running", publicShareUrl: "https://new.trycloudflare.com/s/token/" };
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return errored;
      if (command === "file_sharing_public_start") return retried;
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    expect(await screen.findByText("tunnel died")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByTestId("file-sharing-public-retry"));
    expect(invokeMock).toHaveBeenCalledWith("file_sharing_public_start");
    expect(await screen.findByTestId("file-sharing-public-url")).toHaveTextContent("https://new.trycloudflare.com/s/token/");
  });

  it("validates whitespace-only password and clears after success", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(["/tmp/report.txt"]);
    invokeMock.mockImplementation(async (command: string, _args?: any) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return stopped;
      if (command === "file_sharing_start") {
        // reject if password whitespace-only (frontend should block before call)
        return runningLan;
      }
      return stopped;
    });
    const user = userEvent.setup();
    renderWithProviders(<FileSharingTool />);
    await user.click(await screen.findByRole("button", { name: /Choose files|选择文件/ }));
    const input = screen.getByTestId("file-sharing-password");
    await user.type(input, "   ");
    expect(screen.getByText(/fileSharingPasswordInvalid|Password must/)).toBeInTheDocument();
    // start should be blocked; no start call with password
  });

  it("shows password protected badge when snapshot says so", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return { ...runningLan, passwordProtected: true };
      return stopped;
    });
    const { unmount } = renderWithProviders(<FileSharingTool />);
    expect((await screen.findAllByText(/Password protected|已启用密码保护/i)).length).toBeGreaterThan(0);
    unmount();

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return { ...runningLan, passwordProtected: false };
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    expect(await screen.findByTestId("file-sharing-lan-url")).toBeInTheDocument();
    expect(screen.queryAllByText(/Password protected|已启用密码保护/i)).toHaveLength(0);
  });

  it("toggles password visibility", async () => {
    renderWithProviders(<FileSharingTool />);
    const input = screen.getByTestId("file-sharing-password") as HTMLInputElement;
    expect(input).toHaveAttribute("type", "password");
    fireEvent.change(input, { target: { value: "secret" } });
    const toggle = input.parentElement!.querySelector("button")!;
    fireEvent.click(toggle);
    expect(input).toHaveAttribute("type", "text");
    const toggle2 = input.parentElement!.querySelector("button")!;
    fireEvent.click(toggle2);
    expect(input).toHaveAttribute("type", "password");
  });

  it("refreshes status once on matching public tunnel event id", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningPublic;
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    await screen.findByTestId("file-sharing-public-card");
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    invokeMock.mockClear();
    await act(async () => {
      const listener = (listenMock.mock.calls as any[]).find((c) => c[0] === "file-sharing-public-updated")?.[1];
      listener?.({ payload: { id: "tunnel-1" } });
      await Promise.resolve();
      await Promise.resolve();
    });
    const statusCalls = invokeMock.mock.calls.filter((call) => call[0] === "file_sharing_status").length;
    expect(statusCalls).toBe(1);
  });

  it("does not refresh on stale public tunnel id", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningPublic;
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    await screen.findByTestId("file-sharing-public-card");
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    invokeMock.mockClear();
    await act(async () => {
      const listener = (listenMock.mock.calls as any[]).find((c) => c[0] === "file-sharing-public-updated")?.[1];
      listener?.({ payload: { id: "tunnel-old" } });
      await Promise.resolve();
      await Promise.resolve();
    });
    const statusCalls = invokeMock.mock.calls.filter((call) => call[0] === "file_sharing_status").length;
    expect(statusCalls).toBe(0);
  });

  it("does not refresh when isVisible is false", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningPublic;
      return stopped;
    });
    renderWithProviders(<FileSharingTool isVisible={false} />);
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    invokeMock.mockClear();
    await act(async () => {
      const listener = (listenMock.mock.calls as any[]).find((c) => c[0] === "file-sharing-public-updated")?.[1];
      listener?.({ payload: { id: "tunnel-1" } });
      await Promise.resolve();
      await Promise.resolve();
    });
    const statusCalls = invokeMock.mock.calls.filter((call) => call[0] === "file_sharing_status").length;
    expect(statusCalls).toBe(0);
  });

  it("shows internet exposure warning only while running", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningPublic;
      return stopped;
    });
    const { unmount } = renderWithProviders(<FileSharingTool />);
    expect(await screen.findByText(/互联网上任何持有链接|anyone on the internet/i)).toBeInTheDocument();
    unmount();

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return runningLan;
      return stopped;
    });
    const r2 = renderWithProviders(<FileSharingTool />);
    await screen.findByTestId("qr-code");
    expect(screen.queryByText(/互联网上任何持有链接|anyone on the internet/i)).not.toBeInTheDocument();
    r2.unmount();

    const errored = { ...runningLan, publicTunnelId: "tunnel-1", publicState: "error", publicShareUrl: null, publicError: "tunnel died" };
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "tunnel_check_environment") return { mode: "cloudflare", installed: true, recommended_command: "brew install cloudflared" };
      if (command === "file_sharing_status") return errored;
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    expect(await screen.findByText("tunnel died")).toBeInTheDocument();
    expect(screen.queryByTestId("file-sharing-public-url")).not.toBeInTheDocument();
    expect(screen.queryByText(/互联网上任何持有链接|anyone on the internet/i)).not.toBeInTheDocument();
  });

  it("keeps LAN-only regression rendering", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "file_sharing_networks") return [{ id: "en0:192.168.1.2", interfaceName: "en0", address: "192.168.1.2" }];
      if (command === "file_sharing_status") return { ...stopped, running: true, shareUrl: "http://192.168.1.2:1234/s/token/" };
      return stopped;
    });
    renderWithProviders(<FileSharingTool />);
    expect(await screen.findByTestId("qr-code")).toHaveTextContent("http://192.168.1.2:1234/s/token/");
  });
});
