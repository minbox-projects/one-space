import { describe, expect, it } from "vitest";
import {
  FILE_SHARING_PUBLIC_UPDATED_EVENT,
  fileSharingNetworks,
  fileSharingPublicStart,
  fileSharingPublicStop,
  fileSharingStart,
  fileSharingStatus,
  fileSharingStop,
  getFileSharingPublicUrl,
} from "@/lib/fileSharing";
import type { FileSharingPublicState } from "@/lib/fileSharing";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

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

  it("omits unset optional start fields", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await fileSharingStart({ networkId: "en0:192.168.1.2", paths: ["/tmp/a.txt"] });
    expect(invokeMock).toHaveBeenLastCalledWith("file_sharing_start", {
      input: { networkId: "en0:192.168.1.2", paths: ["/tmp/a.txt"] },
    });
  });

  it("sends publicAccess only when enabled", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await fileSharingStart({ networkId: "en0", paths: ["/tmp/a.txt"], publicAccess: true });
    expect(invokeMock).toHaveBeenLastCalledWith("file_sharing_start", {
      input: { networkId: "en0", paths: ["/tmp/a.txt"], publicAccess: true },
    });
  });

  it("sends password only when provided", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await fileSharingStart({ networkId: "en0", paths: ["/tmp/a.txt"], password: "secret" });
    expect(invokeMock).toHaveBeenLastCalledWith("file_sharing_start", {
      input: { networkId: "en0", paths: ["/tmp/a.txt"], password: "secret" },
    });
  });

  it("sends both optional fields when both are set", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await fileSharingStart({ networkId: "en0", paths: ["/tmp/a.txt"], publicAccess: true, password: "secret" });
    expect(invokeMock).toHaveBeenLastCalledWith("file_sharing_start", {
      input: { networkId: "en0", paths: ["/tmp/a.txt"], publicAccess: true, password: "secret" },
    });
  });

  it("calls public start/stop commands without args", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await fileSharingPublicStart();
    await fileSharingPublicStop();
    expect(invokeMock.mock.calls).toContainEqual(["file_sharing_public_start"]);
    expect(invokeMock.mock.calls).toContainEqual(["file_sharing_public_stop"]);
  });

  it("exposes the public updated event name", () => {
    expect(FILE_SHARING_PUBLIC_UPDATED_EVENT).toBe("file-sharing-public-updated");
  });

  it("chooses the public URL only when running", () => {
    const runningPublic = {
      running: true,
      sessionId: "s",
      shareUrl: "http://lan/s/t/",
      startedAt: 1,
      files: [],
      transfers: [],
      summary: { activeTransfers: 0, completedTransfers: 0, failedTransfers: 0, cancelledTransfers: 0, bytesSent: 0, droppedTransferRecords: 0 },
      passwordProtected: false,
      publicTunnelId: "t1",
      publicState: "running" as FileSharingPublicState,
      publicShareUrl: "https://abc.trycloudflare.com/s/t/",
      publicError: null,
    };
    const errored = { ...runningPublic, publicState: "error" as FileSharingPublicState, publicShareUrl: "https://stale/" };
    const disabled = { ...runningPublic, publicState: "disabled" as FileSharingPublicState, publicShareUrl: null };
    expect(getFileSharingPublicUrl(runningPublic)).toBe("https://abc.trycloudflare.com/s/t/");
    expect(getFileSharingPublicUrl(errored)).toBe("http://lan/s/t/");
    expect(getFileSharingPublicUrl(disabled)).toBe("http://lan/s/t/");
    const emptyPublic = { ...runningPublic, publicShareUrl: "" };
    expect(getFileSharingPublicUrl(emptyPublic)).toBe("http://lan/s/t/");
  });

  it("normalizes errors", async () => {
    resetTauriMocks();
    invokeMock.mockRejectedValue("runtime unavailable");
    await expect(fileSharingStatus()).rejects.toMatchObject({ name: "FileSharingError", message: "runtime unavailable" });
  });
});
