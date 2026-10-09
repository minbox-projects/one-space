import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import {
  isAppVisible,
  publishRuntimeStatus,
  refreshRuntimeStatus,
  setNativeWindowVisible,
  useAppVisibility,
  useRuntimeStatus,
} from "@/lib/runtimeStatus";
import { useVisibleInterval } from "@/toolbox/useVisibleInterval";

function invokeCountFor(command: string): number {
  return invokeMock.mock.calls.filter((call) => call[0] === command).length;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const SSH_SNAPSHOT = { tunnels: [{ id: "tunnel-1", name: "Local tunnel" }] };

beforeEach(() => {
  resetTauriMocks();
});

afterEach(() => {
  act(() => {
    setNativeWindowVisible(true);
  });
  delete (document as unknown as { visibilityState?: unknown }).visibilityState;
});

describe("runtimeStatus store", () => {
  it("stores a valid ssh-tunnels snapshot for two subscribers with zero fallback invokes", async () => {
    const first = renderHook(() =>
      useRuntimeStatus<typeof SSH_SNAPSHOT>("ssh-tunnels", { enabled: false }),
    );
    const second = renderHook(() =>
      useRuntimeStatus<typeof SSH_SNAPSHOT>("ssh-tunnels", { enabled: false }),
    );
    invokeMock.mockClear();

    act(() => {
      publishRuntimeStatus("ssh-tunnels", SSH_SNAPSHOT);
    });

    expect(first.result.current.data).toEqual(SSH_SNAPSHOT);
    expect(second.result.current.data).toEqual(SSH_SNAPSHOT);
    // A valid payload is consumed directly: no follow-up query for either subscriber.
    expect(invokeCountFor("ssh_tunnels_snapshot")).toBe(0);
  });

  it("coalesces missing/invalid ssh-tunnels payloads into exactly one fallback invoke", async () => {
    renderHook(() => useRuntimeStatus("ssh-tunnels", { enabled: false }));
    renderHook(() => useRuntimeStatus("ssh-tunnels", { enabled: false }));
    invokeMock.mockClear();

    act(() => {
      publishRuntimeStatus("ssh-tunnels", undefined);
      publishRuntimeStatus("ssh-tunnels", { unexpected: true });
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(invokeCountFor("ssh_tunnels_snapshot")).toBe(1);
  });

  it("single-flights two overlapping router refreshes into one invoke", async () => {
    const pending = deferred<unknown>();
    invokeMock.mockImplementation((command: string) => {
      if (command === "protocol_router_status") return pending.promise;
      return Promise.resolve(undefined);
    });
    invokeMock.mockClear();

    const first = refreshRuntimeStatus("router");
    const second = refreshRuntimeStatus("router");

    expect(first).toBe(second);
    await act(async () => {
      await Promise.resolve();
    });
    expect(invokeCountFor("protocol_router_status")).toBe(1);

    await act(async () => {
      pending.resolve({ running: true, enabled: true });
      await first;
    });
    expect(invokeCountFor("protocol_router_status")).toBe(1);
  });

  it("keeps the newer router snapshot when a late older response resolves", async () => {
    const older = deferred<unknown>();
    const newer = deferred<unknown>();
    const pending = [older, newer];
    invokeMock.mockImplementation((command: string) => {
      if (command === "protocol_router_status") {
        return (pending.shift() ?? deferred<unknown>()).promise;
      }
      return Promise.resolve(undefined);
    });

    const subscriber = renderHook(() =>
      useRuntimeStatus<{ marker?: string }>("router", { enabled: false }),
    );
    invokeMock.mockClear();

    let olderRequest!: Promise<{ marker?: string } | null>;
    let newerRequest!: Promise<{ marker?: string } | null>;
    await act(async () => {
      olderRequest = refreshRuntimeStatus<{ marker?: string }>("router", { force: true });
      newerRequest = refreshRuntimeStatus<{ marker?: string }>("router", { force: true });
      await Promise.resolve();
    });

    await act(async () => {
      newer.resolve({ marker: "new" });
      await newerRequest;
    });

    const olderResult = await act(async () => {
      older.resolve({ marker: "old" });
      return olderRequest;
    });

    expect(olderResult?.marker).toBe("new");
    expect(subscriber.result.current.data?.marker).toBe("new");
  });

  it("reflects native window hide/show through useAppVisibility", () => {
    const { result } = renderHook(() => useAppVisibility());
    expect(result.current).toBe(true);
    expect(isAppVisible()).toBe(true);

    act(() => {
      setNativeWindowVisible(false);
    });
    expect(result.current).toBe(false);
    expect(isAppVisible()).toBe(false);

    act(() => {
      setNativeWindowVisible(true);
    });
    expect(result.current).toBe(true);
  });

  it("reflects document hidden/visible through useAppVisibility", () => {
    const { result } = renderHook(() => useAppVisibility());

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "hidden",
    });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(result.current).toBe(false);
    expect(isAppVisible()).toBe(false);

    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "visible",
    });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(result.current).toBe(true);
  });

  it("pauses useVisibleInterval while hidden and catches up exactly once on show", () => {
    vi.useFakeTimers();
    try {
      const callback = vi.fn();
      renderHook(() => useVisibleInterval(callback, 1000, true));

      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(callback).toHaveBeenCalledTimes(1);

      act(() => {
        setNativeWindowVisible(false);
      });
      act(() => {
        vi.advanceTimersByTime(3000);
      });
      expect(callback).toHaveBeenCalledTimes(1);

      act(() => {
        setNativeWindowVisible(true);
      });
      expect(callback).toHaveBeenCalledTimes(2);

      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(callback).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });
});
