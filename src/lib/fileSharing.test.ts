import { describe, expect, it, vi } from "vitest";
import {
  fileSharingNetworks,
  fileSharingStart,
  fileSharingStatus,
  fileSharingStop,
  subscribeFileSharingUpdates,
} from "@/lib/fileSharing";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("file sharing IPC", () => {
  it("uses the Rust command names and camelCase input", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await fileSharingNetworks();
    await fileSharingStart({ networkId: "en0:192.168.1.2", paths: ["/tmp/a.txt"] });
    await fileSharingStatus();
    await fileSharingStop();
    expect(invokeMock.mock.calls).toEqual([
      ["file_sharing_networks"],
      ["file_sharing_start", { input: { networkId: "en0:192.168.1.2", paths: ["/tmp/a.txt"] } }],
      ["file_sharing_status"],
      ["file_sharing_stop"],
    ]);
  });

  it("normalizes errors and tears down event listeners", async () => {
    resetTauriMocks();
    invokeMock.mockRejectedValue("runtime unavailable");
    await expect(fileSharingStatus()).rejects.toMatchObject({ name: "FileSharingError", message: "runtime unavailable" });
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    await expect(subscribeFileSharingUpdates(vi.fn())).resolves.toBe(unlisten);
    expect(listenMock).toHaveBeenCalledWith("file-sharing-updated", expect.any(Function));
  });
});
