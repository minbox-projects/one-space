import { invoke } from "@tauri-apps/api/core";

import { isToolboxInvokeAvailable } from "@/toolbox/invoke";

/** Backend event that publishes the shared status snapshot. */
export const BLE_LOCK_STATUS_EVENT = "ble-lock-status-updated";
/** Backend event that publishes the discovered device list. */
export const BLE_LOCK_DEVICES_EVENT = "ble-lock-devices-updated";

export type BleBluetoothState =
  | "unknown"
  | "resetting"
  | "unsupported"
  | "unauthorized"
  | "powered_off"
  | "powered_on";

export type BleLockMode = "active" | "passive";
export type BlePresence = "unknown" | "near" | "away" | "lost";

export type BleDevice = {
  id: string;
  name: string;
  rssi: number;
};

export type BleLockConfig = {
  schema_version: number;
  device_id: string | null;
  device_name: string | null;
  mode: BleLockMode;
  unlock_rssi: number | null;
  lock_rssi: number | null;
  lock_delay_secs: number;
  no_signal_timeout_secs: number;
  wake_on_proximity: boolean;
  wake_without_unlocking: boolean;
  pause_now_playing: boolean;
  use_screensaver_to_lock: boolean;
  sleep_display: boolean;
};

/** Stable, non-secret error carried by the backend status snapshot. */
export type BleLockError = {
  code: string;
  detail?: string | null;
};

export type BleLockStatus = {
  platform_available: boolean;
  bluetooth_state: BleBluetoothState;
  monitoring: boolean;
  scanning: boolean;
  selected_device: BleDevice | null;
  presence: BlePresence;
  averaged_rssi: number | null;
  screen_locked: boolean;
  password_configured: boolean;
  accessibility_trusted: boolean;
  config: BleLockConfig;
  last_error: BleLockError | null;
};

/**
 * A failed `ble-lock` command. The stable backend error `code` is preserved so
 * callers can localize it instead of rendering `[object Object]`.
 */
export class BleLockCommandError extends Error {
  readonly command: string;
  readonly code?: string;
  readonly detail?: string;

  constructor(command: string, message: string, code?: string, detail?: string) {
    super(message);
    this.name = "BleLockCommandError";
    this.command = command;
    this.code = code;
    this.detail = detail;
  }
}

function normalizeBleLockError(command: string, reason: unknown): BleLockCommandError {
  if (reason instanceof BleLockCommandError) return reason;
  if (reason && typeof reason === "object") {
    const code = (reason as { code?: unknown }).code;
    const detail = (reason as { detail?: unknown }).detail;
    const normalizedCode = typeof code === "string" && code ? code : undefined;
    const normalizedDetail =
      typeof detail === "string" && detail ? detail : undefined;
    if (normalizedCode) return new BleLockCommandError(command, normalizedCode, normalizedCode, normalizedDetail);
  }
  if (typeof reason === "string" && reason.trim()) {
    return new BleLockCommandError(command, reason);
  }
  if (reason instanceof Error && reason.message.trim()) {
    return new BleLockCommandError(command, reason.message);
  }
  return new BleLockCommandError(command, "ble-lock command failed");
}

function isMacOsPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Mac/i.test(navigator.platform ?? "");
}

/**
 * The tool provides native behavior only in the macOS Tauri runtime. Other
 * hosts and browser previews keep the entry visible but unavailable.
 */
export function isBleLockPlatformAvailable(): boolean {
  return isToolboxInvokeAvailable() && isMacOsPlatform();
}

/** A native-safe unavailable snapshot used without ever invoking a command. */
export function bleLockUnavailableStatus(): BleLockStatus {
  return {
    platform_available: false,
    bluetooth_state: "unsupported",
    monitoring: false,
    scanning: false,
    selected_device: null,
    presence: "unknown",
    averaged_rssi: null,
    screen_locked: false,
    password_configured: false,
    accessibility_trusted: false,
    config: {
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
    },
    last_error: { code: "platform_unavailable" },
  };
}

async function bleLockInvoke(
  command: string,
  args?: Record<string, unknown>,
): Promise<BleLockStatus> {
  // Guard before any invoke: outside the macOS Tauri runtime every command must
  // resolve the safe unavailable snapshot without touching the native bridge.
  if (!isBleLockPlatformAvailable()) {
    return bleLockUnavailableStatus();
  }
  try {
    return args
      ? await invoke<BleLockStatus>(command, args)
      : await invoke<BleLockStatus>(command);
  } catch (reason) {
    throw normalizeBleLockError(command, reason);
  }
}

export function bleLockGetStatus(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_get_status");
}

export function bleLockSaveConfig(config: BleLockConfig): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_save_config", { config });
}

export function bleLockScanStart(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_scan_start");
}

export function bleLockScanStop(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_scan_stop");
}

export function bleLockSelectDevice(
  deviceId: string,
  deviceName?: string,
): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_select_device", {
    deviceId,
    ...(deviceName ? { deviceName } : {}),
  });
}

export function bleLockClearDevice(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_clear_device");
}

export function bleLockSetPassword(password: string): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_set_password", { password });
}

export function bleLockDeletePassword(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_delete_password");
}

export function bleLockRequestAccessibility(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_request_accessibility");
}

export function bleLockLockNow(): Promise<BleLockStatus> {
  return bleLockInvoke("ble_lock_lock_now");
}
