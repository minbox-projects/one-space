import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Bluetooth } from "lucide-react";

import { ToolShell } from "@/components/toolbox/ToolShell";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  BLE_LOCK_DEVICES_EVENT,
  BleLockCommandError,
  bleLockClearDevice,
  bleLockDeletePassword,
  bleLockLockNow,
  bleLockRequestAccessibility,
  bleLockSaveConfig,
  bleLockScanStart,
  bleLockScanStop,
  bleLockSelectDevice,
  bleLockSetPassword,
  bleLockUnavailableStatus,
  isBleLockPlatformAvailable,
  type BleDevice,
  type BleLockConfig,
  type BleLockStatus,
} from "@/lib/bleLock";
import { publishRuntimeStatus, useRuntimeStatus } from "@/lib/runtimeStatus";
import { useTauriEvent } from "@/toolbox/useTauriEvent";
import type { ToolboxNavigationProps } from "@/toolbox/types";

const RSSI_THRESHOLDS: readonly number[] = [
  -95, -90, -85, -80, -75, -70, -65, -60, -55, -50, -45, -40, -35, -30,
];

const MIN_RSSI = -95;
const MAX_RSSI = -30;
const RSSI_STEP = 5;
const MIN_LOCK_DELAY = 1;
const MAX_LOCK_DELAY = 3600;
const MIN_NO_SIGNAL_TIMEOUT = 5;
const MAX_NO_SIGNAL_TIMEOUT = 3600;

const EMPTY_CONFIG: BleLockConfig = {
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
};

const OPTIMISTIC_STATUS: BleLockStatus = {
  platform_available: true,
  bluetooth_state: "unknown",
  monitoring: false,
  scanning: false,
  selected_device: null,
  presence: "unknown",
  averaged_rssi: null,
  screen_locked: false,
  password_configured: false,
  accessibility_trusted: false,
  config: EMPTY_CONFIG,
  last_error: null,
};

function isValidThreshold(value: number): boolean {
  return (
    Number.isInteger(value) &&
    value >= MIN_RSSI &&
    value <= MAX_RSSI &&
    value % RSSI_STEP === 0
  );
}

function isValidLockConfig(config: BleLockConfig): boolean {
  if (config.unlock_rssi !== null && !isValidThreshold(config.unlock_rssi)) return false;
  if (config.lock_rssi !== null && !isValidThreshold(config.lock_rssi)) return false;
  if (
    !Number.isInteger(config.lock_delay_secs) ||
    config.lock_delay_secs < MIN_LOCK_DELAY ||
    config.lock_delay_secs > MAX_LOCK_DELAY
  ) {
    return false;
  }
  if (
    !Number.isInteger(config.no_signal_timeout_secs) ||
    config.no_signal_timeout_secs < MIN_NO_SIGNAL_TIMEOUT ||
    config.no_signal_timeout_secs > MAX_NO_SIGNAL_TIMEOUT
  ) {
    return false;
  }
  if (
    config.unlock_rssi !== null &&
    config.lock_rssi !== null &&
    config.unlock_rssi < config.lock_rssi
  ) {
    return false;
  }
  return true;
}

type NumberFieldProps = {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  onCommit: (value: number) => void;
};

function NumberField({ id, label, value, min, max, onCommit }: NumberFieldProps) {
  // `draft` is only set while the user is editing; otherwise the input follows
  // the canonical value, so an external snapshot update is reflected immediately
  // without an effect and without fighting an in-progress edit.
  const [draft, setDraft] = useState<string | null>(null);

  return (
    <div className="space-y-1">
      <label htmlFor={id} className="text-sm font-medium">
        {label}
      </label>
      <input
        id={id}
        type="number"
        min={min}
        max={max}
        className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
        value={draft ?? String(value)}
        onBlur={() => setDraft(null)}
        onChange={(event) => {
          const next = event.target.value;
          setDraft(next);
          const parsed = Number(next);
          if (next.trim() !== "" && Number.isInteger(parsed) && parsed >= min && parsed <= max) {
            onCommit(parsed);
          }
        }}
      />
    </div>
  );
}

export function BleLockTool({ isVisible = true }: ToolboxNavigationProps) {
  const { t } = useTranslation();
  const platformAvailable = isBleLockPlatformAvailable();
  const runtime = useRuntimeStatus<BleLockStatus>("ble-lock", {
    enabled: platformAvailable,
  });

  const [devices, setDevices] = useState<BleDevice[]>([]);
  const [password, setPassword] = useState("");
  const [actionError, setActionError] = useState<string | null>(null);

  useTauriEvent(
    BLE_LOCK_DEVICES_EVENT,
    (payload) => {
      if (Array.isArray(payload)) setDevices(payload as BleDevice[]);
    },
    platformAvailable && isVisible,
    { respectVisibility: true },
  );

  const status = useMemo<BleLockStatus>(() => {
    if (!platformAvailable) return bleLockUnavailableStatus();
    return runtime.data ?? OPTIMISTIC_STATUS;
  }, [platformAvailable, runtime.data]);
  const config = status.config;
  const nativeEnabled = platformAvailable && status.platform_available;

  const applyStatus = useCallback((result: BleLockStatus | null | undefined) => {
    if (result) publishRuntimeStatus("ble-lock", result);
  }, []);

  const describeError = useCallback(
    (error: unknown): string => {
      if (error instanceof BleLockCommandError && error.code) {
        return String(t(`bleLockError_${error.code}`));
      }
      const message = error instanceof Error ? error.message : String(error);
      return String(t("error", { message }));
    },
    [t],
  );

  const commitConfig = useCallback(
    (next: BleLockConfig) => {
      if (!isValidLockConfig(next)) {
        setActionError(String(t("bleLockError_invalid_config")));
        return;
      }
      setActionError(null);
      bleLockSaveConfig(next)
        .then((result) => applyStatus(result))
        .catch((error) => setActionError(describeError(error)));
    },
    [applyStatus, describeError, t],
  );

  const updateConfig = useCallback(
    (patch: Partial<BleLockConfig>) => {
      commitConfig({ ...config, ...patch });
    },
    [commitConfig, config],
  );

  const toggleConfig = useCallback(
    (key: "wake_on_proximity" | "wake_without_unlocking" | "pause_now_playing" | "use_screensaver_to_lock" | "sleep_display", checked: boolean) => {
      const patch: Partial<BleLockConfig> = { [key]: checked };
      // Screensaver lock and display-off are mutually exclusive.
      if (key === "use_screensaver_to_lock" && checked) patch.sleep_display = false;
      if (key === "sleep_display" && checked) patch.use_screensaver_to_lock = false;
      updateConfig(patch);
    },
    [updateConfig],
  );

  const handleScanToggle = useCallback(() => {
    setActionError(null);
    const request = status.scanning ? bleLockScanStop() : bleLockScanStart();
    request
      .then((result) => applyStatus(result))
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError, status.scanning]);

  const handleSelectDevice = useCallback(
    (device: BleDevice) => {
      setActionError(null);
      bleLockSelectDevice(device.id, device.name || undefined)
        .then((result) => applyStatus(result))
        .catch((error) => setActionError(describeError(error)));
    },
    [applyStatus, describeError],
  );

  const handleClearDevice = useCallback(() => {
    setActionError(null);
    bleLockClearDevice()
      .then((result) => {
        applyStatus(result);
        setDevices([]);
      })
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError]);

  const handleSavePassword = useCallback(() => {
    if (!password) return;
    setActionError(null);
    bleLockSetPassword(password)
      .then((result) => {
        applyStatus(result);
        setPassword("");
      })
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError, password]);

  const handleDeletePassword = useCallback(() => {
    setActionError(null);
    bleLockDeletePassword()
      .then((result) => applyStatus(result))
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError]);

  const handleRequestAccessibility = useCallback(() => {
    setActionError(null);
    bleLockRequestAccessibility()
      .then((result) => applyStatus(result))
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError]);

  const handleLockNow = useCallback(() => {
    setActionError(null);
    bleLockLockNow()
      .then((result) => applyStatus(result))
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError]);

  const statusError = status.last_error
    ? String(t(`bleLockError_${status.last_error.code}`))
    : null;
  const errorMessage = actionError ?? statusError;

  if (!platformAvailable) {
    return (
      <ToolShell
        title={String(t("bleLock"))}
        description={String(t("bleLockDesc"))}
        icon={<Bluetooth className="h-5 w-5 text-sky-600" aria-hidden="true" />}
      >
        <p className="rounded-md border bg-muted/40 p-4 text-sm text-muted-foreground">
          {String(t("bleLockPlatformUnavailable"))}
        </p>
      </ToolShell>
    );
  }

  const toggles: Array<{
    key: "wake_on_proximity" | "wake_without_unlocking" | "pause_now_playing" | "use_screensaver_to_lock" | "sleep_display";
    labelKey: string;
  }> = [
    { key: "wake_on_proximity", labelKey: "bleLockWakeOnProximity" },
    { key: "wake_without_unlocking", labelKey: "bleLockWakeWithoutUnlocking" },
    { key: "pause_now_playing", labelKey: "bleLockPauseWhileLocked" },
    { key: "use_screensaver_to_lock", labelKey: "bleLockUseScreensaver" },
    { key: "sleep_display", labelKey: "bleLockTurnOffDisplay" },
  ];

  return (
    <ToolShell
      title={String(t("bleLock"))}
      description={String(t("bleLockDesc"))}
      error={errorMessage}
      icon={<Bluetooth className="h-5 w-5 text-sky-600" aria-hidden="true" />}
      actions={
        <>
          <Button variant="outline" onClick={handleScanToggle} disabled={!nativeEnabled}>
            {status.scanning ? String(t("bleLockStopScan")) : String(t("bleLockScan"))}
          </Button>
          <Button variant="outline" onClick={handleLockNow} disabled={!nativeEnabled}>
            {String(t("bleLockLockNow"))}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        {/* Device discovery and selection */}
        <section className="space-y-3 rounded-lg border bg-card p-4">
          <h3 className="text-sm font-semibold">{String(t("bleLockDevices"))}</h3>
          {devices.length === 0 ? (
            <p className="text-sm text-muted-foreground">{String(t("bleLockNoDevices"))}</p>
          ) : (
            <ul className="divide-y">
              {devices.map((device) => (
                <li key={device.id} className="flex items-center justify-between gap-3 py-2">
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium">{device.name}</div>
                    <div className="truncate font-mono text-xs text-muted-foreground">{device.id}</div>
                  </div>
                  <div className="flex items-center gap-3">
                    <span className="text-xs text-muted-foreground">{device.rssi} dBm</span>
                    <Button
                      variant="outline"
                      size="sm"
                      aria-label={`${String(t("bleLockSelectDevice"))} ${device.name}`}
                      onClick={() => handleSelectDevice(device)}
                      disabled={!nativeEnabled}
                    >
                      {String(t("bleLockSelectDevice"))}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )}

          {status.selected_device ? (
            <div className="flex items-center justify-between gap-3 rounded-md bg-muted/40 p-3">
              <div className="min-w-0 space-y-0.5">
                <div className="truncate text-sm font-medium">{status.selected_device.name}</div>
                <div className="truncate font-mono text-xs text-muted-foreground">
                  {status.selected_device.id}
                </div>
                <div className="text-xs text-muted-foreground">
                  {status.selected_device.rssi} dBm
                </div>
              </div>
              <Button variant="outline" size="sm" onClick={handleClearDevice}>
                {String(t("bleLockClearDevice"))}
              </Button>
            </div>
          ) : null}
        </section>

        {/* Observed state */}
        <section className="grid grid-cols-2 gap-3 rounded-lg border bg-card p-4 text-sm md:grid-cols-4">
          <div>
            <div className="text-xs text-muted-foreground">
              {String(t("bleLockBluetoothStatus"))}
            </div>
            <div className="font-medium">
              {String(t(`bleLockBluetoothState_${status.bluetooth_state}`))}
            </div>
          </div>
          <div>
            <div className="text-xs text-muted-foreground">RSSI</div>
            <div className="font-medium">{status.averaged_rssi ?? "--"} dBm</div>
          </div>
          <div>
            <div className="text-xs text-muted-foreground">
              {String(t("bleLockPresenceLabel"))}
            </div>
            <div className="font-medium">
              {String(t(`bleLockPresence_${status.presence}`))}
            </div>
          </div>
          <div>
            <div className="text-xs text-muted-foreground">
              {String(t("bleLockScreenStatus"))}
            </div>
            <div className="font-medium">
              {String(
                t(status.screen_locked ? "bleLockScreen_locked" : "bleLockScreen_unlocked"),
              )}
            </div>
          </div>
        </section>

        {/* Configuration */}
        <section className="space-y-4 rounded-lg border bg-card p-4">
          <div className="space-y-1">
            <label htmlFor="ble-lock-mode" className="text-sm font-medium">
              {String(t("bleLockMode"))}
            </label>
            <select
              id="ble-lock-mode"
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              value={config.mode}
              onChange={(event) =>
                updateConfig({ mode: event.target.value === "passive" ? "passive" : "active" })
              }
            >
              <option value="active">{String(t("bleLockModeActive"))}</option>
              <option value="passive">{String(t("bleLockModePassive"))}</option>
            </select>
          </div>

          <div className="grid gap-4 md:grid-cols-2">
            <div className="space-y-1">
              <label htmlFor="ble-lock-unlock-rssi" className="text-sm font-medium">
                {String(t("bleLockUnlockRssi"))}
              </label>
              <select
                id="ble-lock-unlock-rssi"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
                value={config.unlock_rssi === null ? "disable" : String(config.unlock_rssi)}
                onChange={(event) =>
                  updateConfig({
                    unlock_rssi: event.target.value === "disable" ? null : Number(event.target.value),
                  })
                }
              >
                <option value="disable">{String(t("bleLockDisable"))}</option>
                {RSSI_THRESHOLDS.map((value) => (
                  <option key={value} value={String(value)}>
                    {value} dBm
                  </option>
                ))}
              </select>
            </div>

            <div className="space-y-1">
              <label htmlFor="ble-lock-lock-rssi" className="text-sm font-medium">
                {String(t("bleLockLockRssi"))}
              </label>
              <select
                id="ble-lock-lock-rssi"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
                value={config.lock_rssi === null ? "disable" : String(config.lock_rssi)}
                onChange={(event) =>
                  updateConfig({
                    lock_rssi: event.target.value === "disable" ? null : Number(event.target.value),
                  })
                }
              >
                <option value="disable">{String(t("bleLockDisable"))}</option>
                {RSSI_THRESHOLDS.map((value) => (
                  <option key={value} value={String(value)}>
                    {value} dBm
                  </option>
                ))}
              </select>
            </div>

            <NumberField
              id="ble-lock-delay"
              label={String(t("bleLockLockDelay"))}
              value={config.lock_delay_secs}
              min={MIN_LOCK_DELAY}
              max={MAX_LOCK_DELAY}
              onCommit={(value) => updateConfig({ lock_delay_secs: value })}
            />
            <NumberField
              id="ble-lock-no-signal-timeout"
              label={String(t("bleLockNoSignalTimeout"))}
              value={config.no_signal_timeout_secs}
              min={MIN_NO_SIGNAL_TIMEOUT}
              max={MAX_NO_SIGNAL_TIMEOUT}
              onCommit={(value) => updateConfig({ no_signal_timeout_secs: value })}
            />
          </div>

          <div className="space-y-2">
            {toggles.map((toggle) => (
              <div key={toggle.key} className="flex items-center justify-between gap-3">
                <span className="text-sm">{String(t(toggle.labelKey))}</span>
                <Switch
                  aria-label={String(t(toggle.labelKey))}
                  checked={config[toggle.key]}
                  onCheckedChange={(checked) => toggleConfig(toggle.key, checked)}
                />
              </div>
            ))}
          </div>
        </section>

        {/* Password and Accessibility */}
        <section className="space-y-3 rounded-lg border bg-card p-4">
          <div className="space-y-1">
            <label htmlFor="ble-lock-password" className="text-sm font-medium">
              {String(t("bleLockPassword"))}
            </label>
            <input
              id="ble-lock-password"
              type="password"
              autoComplete="off"
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" onClick={handleSavePassword} disabled={!password}>
              {String(t("bleLockSetPassword"))}
            </Button>
            {status.password_configured ? (
              <Button variant="outline" onClick={handleDeletePassword}>
                {String(t("bleLockDeletePassword"))}
              </Button>
            ) : null}
            {status.password_configured ? (
              <span className="text-xs text-muted-foreground">
                {String(t("bleLockPasswordConfigured"))}
              </span>
            ) : null}
          </div>
          <div className="flex items-center gap-2">
            <Button variant="outline" onClick={handleRequestAccessibility}>
              {String(t("bleLockRequestAccessibility"))}
            </Button>
            {!status.accessibility_trusted ? (
              <span className="text-xs text-muted-foreground">
                {String(t("bleLockError_accessibility_required"))}
              </span>
            ) : null}
          </div>
        </section>
      </div>
    </ToolShell>
  );
}
