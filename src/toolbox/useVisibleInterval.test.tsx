import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useVisibleInterval } from "@/toolbox/useVisibleInterval";

describe("useVisibleInterval", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("runs the callback on every interval while enabled", () => {
    const callback = vi.fn();
    renderHook(() => useVisibleInterval(callback, 1000));

    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(callback).toHaveBeenCalledTimes(3);
  });

  it("does not run while disabled and resumes when re-enabled", () => {
    const callback = vi.fn();
    const { rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useVisibleInterval(callback, 1000, enabled),
      { initialProps: { enabled: false } },
    );

    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(callback).not.toHaveBeenCalled();

    rerender({ enabled: true });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("stops ticking when disabled and resumes afterwards", () => {
    const callback = vi.fn();
    const { rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useVisibleInterval(callback, 1000, enabled),
      { initialProps: { enabled: true } },
    );

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(callback).toHaveBeenCalledTimes(1);

    rerender({ enabled: false });
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(callback).toHaveBeenCalledTimes(1);

    rerender({ enabled: true });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(callback).toHaveBeenCalledTimes(2);
  });

  it("clears the interval on unmount", () => {
    const callback = vi.fn();
    const { unmount } = renderHook(() => useVisibleInterval(callback, 1000));

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(callback).toHaveBeenCalledTimes(1);

    unmount();
    expect(vi.getTimerCount()).toBe(0);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("uses the latest callback without resetting the interval", () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(
      ({ callback }: { callback: () => void }) =>
        useVisibleInterval(callback, 1000),
      { initialProps: { callback: first } },
    );

    act(() => {
      vi.advanceTimersByTime(600);
    });
    expect(first).not.toHaveBeenCalled();

    rerender({ callback: second });
    act(() => {
      vi.advanceTimersByTime(400);
    });
    // The pending tick from the original schedule still fires at 1000 ms.
    expect(second).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(second).toHaveBeenCalledTimes(2);
    expect(first).not.toHaveBeenCalled();
  });
});
