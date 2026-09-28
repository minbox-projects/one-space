import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  COPIED_FEEDBACK_RESET_MS,
  useCopyToClipboard,
} from "@/toolbox/useCopyToClipboard";

const writeText = vi.fn<(...args: unknown[]) => Promise<void>>();

function installClipboard() {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

async function runCopy(
  copy: (text: string) => Promise<boolean>,
  text: string,
): Promise<boolean> {
  let result = false;
  await act(async () => {
    result = await copy(text);
  });
  return result;
}

describe("useCopyToClipboard", () => {
  beforeEach(() => {
    writeText.mockReset();
    writeText.mockResolvedValue(undefined);
    installClipboard();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("pins the shared feedback reset constant to 1600 ms", () => {
    expect(COPIED_FEEDBACK_RESET_MS).toBe(1600);
  });

  it("writes through the clipboard, reports success and resets after the timeout", async () => {
    const onSuccess = vi.fn();
    const { result } = renderHook(() =>
      useCopyToClipboard({ onSuccess }),
    );

    const ok = await runCopy(result.current.copy, "hello");
    expect(ok).toBe(true);
    expect(writeText).toHaveBeenCalledWith("hello");
    expect(result.current.copied).toBe(true);
    expect(onSuccess).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(COPIED_FEEDBACK_RESET_MS);
    });
    expect(result.current.copied).toBe(false);
  });

  it("restarts the reset timer on a second successful copy", async () => {
    const { result } = renderHook(() => useCopyToClipboard());

    await runCopy(result.current.copy, "first");
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current.copied).toBe(true);

    await runCopy(result.current.copy, "second");
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    // 2000 ms since the first success, but only 1000 ms since the restart.
    expect(result.current.copied).toBe(true);

    act(() => {
      vi.advanceTimersByTime(600);
    });
    expect(result.current.copied).toBe(false);
  });

  it("honours a custom reset duration", async () => {
    const { result } = renderHook(() =>
      useCopyToClipboard({ resetMs: 200 }),
    );

    await runCopy(result.current.copy, "text");
    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(result.current.copied).toBe(false);
  });

  it("reports failure without setting copied and calls onError", async () => {
    const onSuccess = vi.fn();
    const onError = vi.fn();
    writeText.mockRejectedValueOnce(new Error("clipboard denied"));
    const { result } = renderHook(() =>
      useCopyToClipboard({ onSuccess, onError }),
    );

    const ok = await runCopy(result.current.copy, "text");
    expect(ok).toBe(false);
    expect(result.current.copied).toBe(false);
    expect(onSuccess).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError.mock.calls[0][0]).toBeInstanceOf(Error);
  });

  it("clears the pending reset timer on unmount", async () => {
    const { result, unmount } = renderHook(() => useCopyToClipboard());

    await runCopy(result.current.copy, "text");
    expect(result.current.copied).toBe(true);
    expect(vi.getTimerCount()).toBeGreaterThan(0);

    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
