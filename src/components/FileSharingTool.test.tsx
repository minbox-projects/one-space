import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileSharingTool } from "@/components/FileSharingTool";
import type { FileSharingSnapshot } from "@/lib/fileSharing";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

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
});
