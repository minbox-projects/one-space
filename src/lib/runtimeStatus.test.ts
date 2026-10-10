import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { invokeMock, bleLockGetStatusMock, resetTauriMocks } from "@/test/mocks/tauri";
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

/**
 * The ble-lock fallback pulls a native command, which the platform guard only
 * permits on macOS. jsdom defaults to a non-macOS platform, so pin macOS for
 * the suite and restore the original afterwards.
 */
const originalPlatform = window.navigator.platform;

function setNavigatorPlatform(platform: string) {
  Object.defineProperty(window.navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

beforeEach(() => {
  resetTauriMocks();
  setNavigatorPlatform("MacIntel");
});

afterEach(() => {
  setNavigatorPlatform(originalPlatform);
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

// ---------------------------------------------------------------------------
// ble-lock service: the shared store must consume a full backend snapshot with
// zero follow-up query and coalesce invalid payloads into one fallback pull.
// The snapshot literal is an independent DTO fixture from the frozen contract.
// ---------------------------------------------------------------------------

const BLE_LOCK_STATUS = {
  platform_available: true,
  bluetooth_state: "powered_on",
  monitoring: true,
  scanning: false,
  selected_device: { id: "uuid-a", name: "Key Fob", rssi: -55 },
  presence: "near",
  averaged_rssi: -55,
  screen_locked: false,
  password_configured: false,
  accessibility_trusted: true,
  config: {
    schema_version: 1,
    device_id: "uuid-a",
    device_name: "Key Fob",
    mode: "active",
    unlock_rssi: -60,
    lock_rssi: null,
    lock_delay_secs: 10,
    no_signal_timeout_secs: 30,
    wake_on_proximity: false,
    wake_without_unlocking: false,
    pause_now_playing: false,
    use_screensaver_to_lock: false,
    sleep_display: false,
  },
  last_error: null,
};

describe("ble-lock runtime status", () => {
  it("stores a valid ble-lock snapshot for two subscribers with zero fallback invokes", async () => {
    const first = renderHook(() =>
      useRuntimeStatus<typeof BLE_LOCK_STATUS>("ble-lock", { enabled: false }),
    );
    const second = renderHook(() =>
      useRuntimeStatus<typeof BLE_LOCK_STATUS>("ble-lock", { enabled: false }),
    );
    invokeMock.mockClear();

    act(() => {
      publishRuntimeStatus("ble-lock", BLE_LOCK_STATUS);
    });

    expect(first.result.current.data).toEqual(BLE_LOCK_STATUS);
    expect(second.result.current.data).toEqual(BLE_LOCK_STATUS);
    expect(invokeCountFor("ble_lock_get_status")).toBe(0);
  });

  it("coalesces invalid ble-lock payloads into exactly one ble_lock_get_status fallback", async () => {
    bleLockGetStatusMock.mockResolvedValue(BLE_LOCK_STATUS);
    renderHook(() => useRuntimeStatus("ble-lock", { enabled: false }));
    renderHook(() => useRuntimeStatus("ble-lock", { enabled: false }));
    invokeMock.mockClear();

    act(() => {
      publishRuntimeStatus("ble-lock", undefined);
      publishRuntimeStatus("ble-lock", { unexpected: true });
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(invokeCountFor("ble_lock_get_status")).toBe(1);
  });
});
