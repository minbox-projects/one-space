import { useEffect, useRef } from "react";
import { useRuntimeStatus } from "@/lib/runtimeStatus";
import type { BleLockStatus } from "@/lib/bleLock";

/**
 * The BLE Lock status fields the shell forwards to the tray controller:
 * platform availability, presence and the full behavior config. The config is
 * taken in full rather than only the fields the `ble-lock-controls` submenu
 * paints, because a tray toggle saves the whole latest config back through
 * `ble_lock_save_config`; a signature that ignored persisted fields would let
 * the shell hold a stale snapshot and write outdated device, mode or timing
 * values. Live RSSI, signal averages and device metadata stay excluded because
 * the menu never reads them and they churn on every reading.
 */
function trayBleLockSignature(status: BleLockStatus): string {
  const { config } = status;
  // Explicit field list keeps the signature independent of object key order.
  return JSON.stringify([
    status.platform_available,
    status.presence,
    config.schema_version,
    config.device_id,
    config.device_name,
    config.mode,
    config.unlock_rssi,
    config.lock_rssi,
    config.lock_delay_secs,
    config.no_signal_timeout_secs,
    config.wake_on_proximity,
    config.wake_without_unlocking,
    config.pause_now_playing,
    config.use_screensaver_to_lock,
    config.sleep_display,
  ]);
}

/**
 * Null-render bridge that owns the App's BLE Lock runtime subscription. It
 * re-renders on every shared-store emission (including per-reading RSSI churn)
 * but forwards a status to the shell only when a tray-relevant field changes,
 * keeping an idle Bluetooth stream from re-rendering the whole window.
 */
export function TrayBleLockBridge({
  enabled,
  onStatusChange,
}: {
  enabled: boolean;
  onStatusChange: (status: BleLockStatus) => void;
}) {
  const { data } = useRuntimeStatus<BleLockStatus>("ble-lock", { enabled });
  const appliedSignatureRef = useRef<string | null>(null);

  useEffect(() => {
    if (!enabled || !data) return;
    const signature = trayBleLockSignature(data);
    if (appliedSignatureRef.current === signature) return;
    appliedSignatureRef.current = signature;
    onStatusChange(data);
  }, [data, enabled, onStatusChange]);

  return null;
}
