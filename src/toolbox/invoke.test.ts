import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ToolboxInvokeError,
  invokeToolboxCommand,
  isToolboxInvokeAvailable,
} from "@/toolbox/invoke";

const coreInvoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: coreInvoke }));

function setTauriRuntime(present: boolean): void {
  if (present) {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  } else {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
}

async function captureRejection(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Error("expected the promise to reject");
}

describe("isToolboxInvokeAvailable", () => {
  afterEach(() => setTauriRuntime(true));

  it("is true when __TAURI_INTERNALS__ is present", () => {
    setTauriRuntime(true);
    expect(isToolboxInvokeAvailable()).toBe(true);
  });

  it("is false when __TAURI_INTERNALS__ is absent", () => {
    setTauriRuntime(false);
    expect(isToolboxInvokeAvailable()).toBe(false);
  });
});

describe("invokeToolboxCommand", () => {
  beforeEach(() => {
    setTauriRuntime(true);
    coreInvoke.mockReset();
  });

  afterEach(() => {
    setTauriRuntime(true);
  });

  it("resolves the raw result unchanged on success", async () => {
    const payload = { id: "1", nested: { ok: true } };
    coreInvoke.mockResolvedValueOnce(payload);

    await expect(
      invokeToolboxCommand<typeof payload>("toolbox_cmd", { value: 3 }),
    ).resolves.toBe(payload);
    expect(coreInvoke).toHaveBeenCalledTimes(1);
    expect(coreInvoke).toHaveBeenCalledWith("toolbox_cmd", { value: 3 });
  });

  it("passes an omitted args record through as undefined", async () => {
    coreInvoke.mockResolvedValueOnce("done");

    await expect(invokeToolboxCommand<string>("toolbox_cmd")).resolves.toBe(
      "done",
    );
    expect(coreInvoke.mock.calls[0][0]).toBe("toolbox_cmd");
    expect(coreInvoke.mock.calls[0][1]).toBeUndefined();
  });

  it("rejects with ToolboxInvokeError outside Tauri without calling invoke", async () => {
    setTauriRuntime(false);

    const error = await captureRejection(
      invokeToolboxCommand("toolbox_cmd", { value: 1 }),
    );
    expect(error).toBeInstanceOf(ToolboxInvokeError);
    expect((error as ToolboxInvokeError).command).toBe("toolbox_cmd");
    expect(coreInvoke).not.toHaveBeenCalled();
  });

  it("rethrows an Error reason as a ToolboxInvokeError with its message", async () => {
    coreInvoke.mockRejectedValueOnce(new Error("backend exploded"));

    const error = await captureRejection(invokeToolboxCommand("toolbox_cmd"));
    expect(error).toBeInstanceOf(ToolboxInvokeError);
    expect(error).toBeInstanceOf(Error);
    expect((error as ToolboxInvokeError).command).toBe("toolbox_cmd");
    expect((error as Error).message).toContain("backend exploded");
  });

  it("normalizes a string reason to a non-empty message", async () => {
    coreInvoke.mockRejectedValueOnce("plain string failure");

    const error = (await captureRejection(
      invokeToolboxCommand("toolbox_cmd"),
    )) as Error;
    expect(error).toBeInstanceOf(ToolboxInvokeError);
    expect(error.message).toBe("plain string failure");
  });

  it("normalizes an object reason with a message field", async () => {
    coreInvoke.mockRejectedValueOnce({ message: "object message" });

    const error = (await captureRejection(
      invokeToolboxCommand("toolbox_cmd"),
    )) as Error;
    expect(error).toBeInstanceOf(ToolboxInvokeError);
    expect(error.message).toBe("object message");
  });

  it.each([
    ["an empty Error", new Error("")],
    ["an object without a message", { code: 500 }],
    ["null", null],
    ["undefined", undefined],
  ])("falls back to a fixed non-empty message for %s", (_label, reason) => {
    coreInvoke.mockRejectedValueOnce(reason);

    return captureRejection(invokeToolboxCommand("toolbox_cmd")).then((error) => {
      expect(error).toBeInstanceOf(ToolboxInvokeError);
      const message = (error as Error).message;
      expect(typeof message).toBe("string");
      expect(message.trim().length).toBeGreaterThan(0);
      expect(message).not.toBe("[object Object]");
      expect((error as ToolboxInvokeError).command).toBe("toolbox_cmd");
    });
  });

  it("never surfaces an unhandled raw rejection", async () => {
    const raw = { reason: "raw" };
    coreInvoke.mockRejectedValueOnce(raw);

    const error = await captureRejection(invokeToolboxCommand("toolbox_cmd"));
    expect(error).not.toBe(raw);
    expect(error).toBeInstanceOf(ToolboxInvokeError);
  });
});
