import type { TFunction } from "i18next";
import { describe, expect, it, vi } from "vitest";
import {
  buildMessageInput,
  confirmSensitiveAction,
  notifySystemEvent,
  notifyActionResult,
  runUserAction,
} from "@/lib/userActions";
import { createMockActionContext } from "@/test/mocks/actionContext";

function createContext() {
  return createMockActionContext() as ReturnType<typeof createMockActionContext> & {
    t: TFunction;
  };
}

describe("userActions", () => {
  it("builds message input with metadata and dedupe key", () => {
    const input = buildMessageInput(
      {
        source: "skills",
        category: "sync",
        action: "manual-sync",
        metadata: { scope: "global" },
        target: { tab: "skills" },
      },
      "success",
      {
        title: "Done",
        summary: "ok",
        metadata: { count: 2 },
      },
    );

    expect(input).toMatchObject({
      source: "skills",
      category: "sync",
      severity: "success",
      title: "Done",
      summary: "ok",
      dedupe_key: "skills:sync:manual-sync:success",
      target: { tab: "skills" },
      metadata: { action: "manual-sync", scope: "global", count: 2 },
    });
  });

  it("returns null when confirmation is cancelled", async () => {
    const context = createContext();
    context.confirm.mockResolvedValue(false);

    const result = await runUserAction(
      context,
      {
        source: "settings",
        category: "save",
        action: "save-config",
        confirm: { message: "confirm?" },
      },
      async () => "never",
    );

    expect(result).toBeNull();
    expect(context.recordMessage).not.toHaveBeenCalled();
    expect(context.pushToast).not.toHaveBeenCalled();
  });

  it("records and toasts success", async () => {
    const context = createContext();

    const result = await runUserAction(
      context,
      {
        source: "settings",
        category: "save",
        action: "save-config",
        success: { title: "Saved", summary: "Done" },
      },
      async () => 42,
    );

    expect(result).toBe(42);
    expect(context.recordMessage).toHaveBeenCalledTimes(1);
    expect(context.pushToast).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Saved", description: "Done", kind: "success" }),
    );
  });

  it("records and toasts failure then rethrows", async () => {
    const context = createContext();

    await expect(
      runUserAction(
        context,
        {
          source: "settings",
          category: "save",
          action: "save-config",
        },
        async () => {
          throw new Error("boom");
        },
      ),
    ).rejects.toThrow("boom");

    expect(context.recordMessage).toHaveBeenCalledTimes(1);
    expect(context.pushToast).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "error" }),
    );
  });

  it("uses sensitive action presets with overrides", async () => {
    const confirm = vi.fn(async () => true);
    await confirmSensitiveAction(
      { confirm },
      "delete",
      { message: "Delete this provider?" },
    );

    expect(confirm).toHaveBeenCalledWith(
      "Delete this provider?",
      expect.objectContaining({ okLabel: "Delete", kind: "error" }),
    );
  });

  it("notifies system events without toast when disabled", async () => {
    const context = createContext();
    await notifySystemEvent(context, {
      source: "skills",
      category: "auto_update",
      severity: "success",
      message: { title: "Auto update complete" },
      toast: false,
    });

    expect(context.recordMessage).toHaveBeenCalledTimes(1);
    expect(context.pushToast).not.toHaveBeenCalled();
  });

  it("closes a loading toast before pushing final result", async () => {
    const context = createContext();

    await notifyActionResult(
      context,
      {
        source: "skills",
        category: "sync",
        action: "manual-sync",
      },
      "warning",
      {
        title: "Partial success",
        summary: "1 failed",
      },
      {
        closeToastId: "loading-1",
        toastKind: "warning",
      },
    );

    expect(context.dismissToast).toHaveBeenCalledWith("loading-1");
    expect(context.pushToast).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "warning" }),
    );
  });

  it("preserves explicit dedupe key and target combination", async () => {
    const input = buildMessageInput(
      {
        source: "workspaces",
        category: "copy",
        action: "copy-workspace",
        dedupeKey: "workspaces:copy:stable",
        target: { tab: "workspaces", entity_id: "ws-1" },
      },
      "warning",
      {
        title: "Partial copy",
      },
    );

    expect(input.dedupe_key).toBe("workspaces:copy:stable");
    expect(input.target).toEqual({ tab: "workspaces", entity_id: "ws-1" });
  });
});
