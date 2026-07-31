import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileSharingTool } from "@/components/FileSharingTool";
import type { FileSharingSnapshot } from "@/lib/fileSharing";
import { getMoreToolPresentation } from "@/lib/moreToolPresentation";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("qrcode.react", () => ({ QRCodeSVG: ({ value }: { value: string }) => <div data-testid="qr-code">{value}</div> }));

const stopped: FileSharingSnapshot = { running: false, sessionId: null, address: null, port: null, shareUrl: null, startedAt: null, stoppedAt: null, files: [], transfers: [], summary: { activeTransfers: 0, completedTransfers: 0, failedTransfers: 0, cancelledTransfers: 0, bytesSent: 0, droppedTransferRecords: 0 }, lastError: null };

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
    const { iconClassName } = getMoreToolPresentation("file-sharing");

    expect(iconContainer).toHaveClass(...iconClassName.split(" "));
    expect(iconContainer?.querySelector("svg")).toHaveClass("lucide-share-2");
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
      stoppedAt: 1,
      transfers: [{ id: "transfer-1", fileId: "file-1", fileName: "report.txt", clientAddress: "127.0.0.1", state: "completed", startedAt: 0, finishedAt: 1, bytesSent: 12, responseBytes: 12, error: null }],
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
});
