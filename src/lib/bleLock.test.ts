import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  BLE_LOCK_DEVICES_EVENT,
  BLE_LOCK_STATUS_EVENT,
  bleLockClearDevice,
  bleLockDeletePassword,
  bleLockGetStatus,
  bleLockLockNow,
  bleLockRequestAccessibility,
  bleLockSaveConfig,
  bleLockScanStart,
  bleLockScanStop,
  bleLockSelectDevice,
  bleLockSetPassword,
  bleLockUnavailableStatus,
  isBleLockPlatformAvailable,
  type BleLockConfig,
  type BleLockStatus,
} from "@/lib/bleLock";
import {
  bleLockClearDeviceMock,
  bleLockDeletePasswordMock,
  bleLockGetStatusMock,
  bleLockLockNowMock,
  bleLockRequestAccessibilityMock,
  bleLockSaveConfigMock,
  bleLockScanStartMock,
  bleLockScanStopMock,
  bleLockSelectDeviceMock,
  bleLockSetPasswordMock,
  invokeMock,
  resetTauriMocks,
} from "@/test/mocks/tauri";

/**
 * Independent DTO fixtures written from the frozen public contract in
 * `.ai-workflow/plans/20261010-ble-lock-tool/plan.md` (`BleLockConfig`,
 * `BleLockStatus`, `BleDevice`). They are plain literals so no expected value is
 * derived from production code.
 */
function emptyConfig(overrides: Partial<BleLockConfig> = {}): BleLockConfig {
  return {
    schema_version: 1,
    device_id: null,
    device_name: null,
    mode: "active",
    unlock_rssi: null,
    lock_rssi: null,
    lock_delay_secs: 10,
    no_signal_timeout_secs: 30,
    wake_on_proximity: false,
    wake_without_unlocking: false,
    pause_now_playing: false,
    use_screensaver_to_lock: false,
    sleep_display: false,
    ...overrides,
  };
}

function statusFixture(overrides: Partial<BleLockStatus> = {}): BleLockStatus {
  return {
    platform_available: true,
    bluetooth_state: "powered_on",
    monitoring: false,
    scanning: false,
    selected_device: null,
    presence: "unknown",
    averaged_rssi: null,
    screen_locked: false,
    password_configured: false,
    accessibility_trusted: false,
    config: emptyConfig(),
    last_error: null,
    ...overrides,
  };
}

const originalPlatform = window.navigator.platform;

function setTauri(present: boolean) {
  if (present) {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  } else {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
}

function setPlatform(platform: string) {
  Object.defineProperty(window.navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

describe("ble-lock command wrappers", () => {
  beforeEach(() => {
    resetTauriMocks();
    setTauri(true);
    setPlatform("MacIntel");
  });

  afterEach(() => {
    setTauri(true);
    setPlatform(originalPlatform);
  });

  it("publishes the frozen event names", () => {
    expect(BLE_LOCK_STATUS_EVENT).toBe("ble-lock-status-updated");
    expect(BLE_LOCK_DEVICES_EVENT).toBe("ble-lock-devices-updated");
  });

  it("calls ble_lock_get_status with no arguments and returns the snapshot", async () => {
    const snapshot = statusFixture();
    bleLockGetStatusMock.mockResolvedValue(snapshot);

    await expect(bleLockGetStatus()).resolves.toEqual(snapshot);
    expect(invokeMock).toHaveBeenCalledWith("ble_lock_get_status");
  });

  it("forwards the full config object to ble_lock_save_config", async () => {
    const config = emptyConfig({ unlock_rssi: -60, lock_rssi: -70 });
    bleLockSaveConfigMock.mockResolvedValue(statusFixture({ config }));

    await bleLockSaveConfig(config);

    expect(invokeMock).toHaveBeenCalledWith("ble_lock_save_config", { config });
    expect(bleLockSaveConfigMock).toHaveBeenCalledWith(config);
  });

  it("calls scan start and scan stop with no arguments", async () => {
    bleLockScanStartMock.mockResolvedValue(statusFixture());
    bleLockScanStopMock.mockResolvedValue(statusFixture());

    await bleLockScanStart();
    await bleLockScanStop();

    expect(invokeMock).toHaveBeenCalledWith("ble_lock_scan_start");
    expect(invokeMock).toHaveBeenCalledWith("ble_lock_scan_stop");
  });

  it("sends deviceId and deviceName on selection", async () => {
    bleLockSelectDeviceMock.mockResolvedValue(statusFixture());

    await bleLockSelectDevice("uuid-a", "Key Fob");

    const [command, args] = invokeMock.mock.calls.at(-1) as [
      string,
      Record<string, unknown>,
    ];
    expect(command).toBe("ble_lock_select_device");
    expect(args.deviceId).toBe("uuid-a");
    expect(args.deviceName).toBe("Key Fob");
  });

  it("omits the optional device name when selection has no display name", async () => {
    bleLockSelectDeviceMock.mockResolvedValue(statusFixture());

    await bleLockSelectDevice("uuid-a");

    const [, args] = invokeMock.mock.calls.at(-1) as [
      string,
      Record<string, unknown>,
    ];
    expect(args.deviceId).toBe("uuid-a");
    expect(args.deviceName ?? null).toBeNull();
  });

  it("calls clear, delete-password, request-accessibility and lock-now with no arguments", async () => {
    bleLockClearDeviceMock.mockResolvedValue(statusFixture());
    bleLockDeletePasswordMock.mockResolvedValue(statusFixture());
    bleLockRequestAccessibilityMock.mockResolvedValue(statusFixture());
    bleLockLockNowMock.mockResolvedValue(statusFixture());

    await bleLockClearDevice();
    await bleLockDeletePassword();
    await bleLockRequestAccessibility();
    await bleLockLockNow();

    expect(invokeMock).toHaveBeenCalledWith("ble_lock_clear_device");
    expect(invokeMock).toHaveBeenCalledWith("ble_lock_delete_password");
    expect(invokeMock).toHaveBeenCalledWith("ble_lock_request_accessibility");
    expect(invokeMock).toHaveBeenCalledWith("ble_lock_lock_now");
  });

  it("sends the password only through ble_lock_set_password", async () => {
    bleLockSetPasswordMock.mockResolvedValue(
      statusFixture({ password_configured: true }),
    );

    await bleLockSetPassword("correct horse battery staple");

    expect(invokeMock).toHaveBeenCalledWith("ble_lock_set_password", {
      password: "correct horse battery staple",
    });
  });
});

describe("ble-lock platform guard", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  afterEach(() => {
    setTauri(true);
    setPlatform(originalPlatform);
  });

  it("reports unavailable outside the Tauri runtime even on macOS", () => {
    setTauri(false);
    setPlatform("MacIntel");
    expect(isBleLockPlatformAvailable()).toBe(false);
  });

  it("reports unavailable on a non-macOS Tauri host", () => {
    setTauri(true);
    setPlatform("Win32");
    expect(isBleLockPlatformAvailable()).toBe(false);
  });

  it("reports available on a macOS Tauri host", () => {
    setTauri(true);
    setPlatform("MacIntel");
    expect(isBleLockPlatformAvailable()).toBe(true);
  });

  it("exposes an explicit platform_unavailable snapshot without native surfaces", () => {
    const status = bleLockUnavailableStatus();

    expect(status.platform_available).toBe(false);
    expect(status.monitoring).toBe(false);
    expect(status.selected_device).toBeNull();
    expect(status.last_error?.code).toBe("platform_unavailable");
  });
});

describe("ble-lock wrappers on unavailable platforms", () => {
  afterEach(() => {
    setTauri(true);
    setPlatform(originalPlatform);
  });

  /**
   * Invoke all ten public wrappers with inert inputs and return their resolved
   * snapshots. This is the shared-store seam the App depends on: on an
   * unavailable platform every wrapper must resolve the safe unavailable
   * snapshot without a single native command.
   */
  async function callAllWrappers(): Promise<BleLockStatus[]> {
    return Promise.all([
      bleLockGetStatus(),
      bleLockSaveConfig(emptyConfig()),
      bleLockScanStart(),
      bleLockScanStop(),
      bleLockSelectDevice("uuid-a", "Key Fob"),
      bleLockClearDevice(),
      bleLockSetPassword("inert-password"),
      bleLockDeletePassword(),
      bleLockRequestAccessibility(),
      bleLockLockNow(),
    ]);
  }

  it("resolves the unavailable snapshot from all ten wrappers on a non-Tauri macOS host without invoking", async () => {
    resetTauriMocks();
    setTauri(false);
    setPlatform("MacIntel");

    const results = await callAllWrappers();

    expect(results).toHaveLength(10);
    for (const status of results) {
      expect(status.platform_available).toBe(false);
      expect(status.last_error?.code).toBe("platform_unavailable");
    }
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("resolves the unavailable snapshot from all ten wrappers on a Windows Tauri host without invoking", async () => {
    resetTauriMocks();
    setTauri(true);
    setPlatform("Win32");

    const results = await callAllWrappers();

    expect(results).toHaveLength(10);
    for (const status of results) {
      expect(status.platform_available).toBe(false);
      expect(status.last_error?.code).toBe("platform_unavailable");
    }
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
