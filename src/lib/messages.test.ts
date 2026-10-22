import { beforeEach, describe, expect, it, vi } from "vitest";

vi.unmock("@/lib/messages");

import {
  errorToMessage,
  notifyMessage,
  recordMessage,
  safeRecordMessage,
} from "./messages";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe("src/lib/messages.ts", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("formats unknown errors with errorToMessage", () => {
    const err = new Error("something went wrong");
    expect(errorToMessage(err)).toContain("something went wrong");
    expect(errorToMessage("string error")).toBe("string error");
    expect(errorToMessage({ code: 500 })).toContain('"code": 500');
  });

  it("automatically extracts summary, detail and derives dedupeKey from Error", async () => {
    invokeMock.mockResolvedValueOnce({ id: "msg_123" });

    const err = new Error("First line error\nStack trace line 2\nStack trace line 3");
    await notifyMessage.error({
      source: "content",
      category: "notes",
      title: "Save failed",
      error: err,
      target: { tab: "notes" },
    });

    expect(invokeMock).toHaveBeenCalledTimes(1);
    const [command, args] = invokeMock.mock.calls[0];
    expect(command).toBe("messages_create");
    expect(args.input).toMatchObject({
      source: "content",
      category: "notes",
      severity: "error",
      title: "Save failed",
      summary: "First line error",
      dedupe_key: "content:notes:error",
      target: { tab: "notes" },
    });
    expect(args.input.detail).toContain("Stack trace line 2");
  });

  it("preserves explicit summary, detail and dedupeKey when supplied", async () => {
    invokeMock.mockResolvedValueOnce({ id: "msg_456" });

    await notifyMessage.warn({
      source: "ssh_tunnels",
      category: "supervisor",
      title: "Tunnel reconnecting",
      summary: "Custom summary",
      detail: "Full trace log",
      dedupeKey: "ssh:custom:key",
    });

    expect(invokeMock).toHaveBeenCalledWith("messages_create", {
      input: {
        source: "ssh_tunnels",
        category: "supervisor",
        severity: "warning",
        title: "Tunnel reconnecting",
        summary: "Custom summary",
        detail: "Full trace log",
        dedupe_key: "ssh:custom:key",
        target: null,
        metadata: undefined,
      },
    });
  });

  it("supports info and success helpers", async () => {
    invokeMock.mockResolvedValue({ id: "msg_789" });

    await notifyMessage.info({
      source: "skills",
      category: "sync",
      title: "Syncing skills",
    });

    await notifyMessage.success({
      source: "ai_environments",
      category: "save",
      title: "Provider saved",
    });

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock.mock.calls[0][1].input.severity).toBe("info");
    expect(invokeMock.mock.calls[1][1].input.severity).toBe("success");
  });

  it("backward-compatible recordMessage and safeRecordMessage work without throwing", async () => {
    invokeMock.mockRejectedValueOnce(new Error("network failure"));

    await expect(
      recordMessage({
        source: "system",
        category: "test",
        severity: "error",
        title: "Legacy failure",
      }),
    ).resolves.toBeUndefined();

    await expect(
      safeRecordMessage({
        source: "system",
        category: "test",
        severity: "info",
        title: "Legacy info",
      }),
    ).resolves.toBeUndefined();
  });
});
