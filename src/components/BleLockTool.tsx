import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  Bluetooth,
  BluetoothConnected,
  BluetoothSearching,
  CheckCircle2,
  ExternalLink,
  Eye,
  EyeOff,
  KeyRound,
  Lock,
  LockOpen,
  Monitor,
  Moon,
  Radio,
  RefreshCw,
  Shield,
  ShieldAlert,
  ShieldCheck,
  Signal,
  SignalHigh,
  SignalLow,
  SignalMedium,
  SignalZero,
  Sliders,
  Trash2,
  VolumeX,
  Zap,
} from "lucide-react";

import { ToolShell } from "@/components/toolbox/ToolShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
  helperText?: string;
};

function NumberField({ id, label, value, min, max, onCommit, helperText }: NumberFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between">
        <label htmlFor={id} className="text-sm font-medium text-foreground">
          {label}
        </label>
        {helperText ? (
          <span className="text-xs text-muted-foreground">{helperText}</span>
        ) : null}
      </div>
      <div className="relative">
        <input
          id={id}
          type="number"
          min={min}
          max={max}
          className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
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
    </div>
  );
}

function SignalIndicator({ rssi }: { rssi: number | null | undefined }) {
  if (rssi == null) {
    return <SignalZero className="h-4 w-4 text-muted-foreground" aria-hidden="true" />;
  }
  if (rssi >= -60) {
    return <SignalHigh className="h-4 w-4 text-emerald-500" aria-hidden="true" />;
  }
  if (rssi >= -75) {
    return <SignalMedium className="h-4 w-4 text-sky-500" aria-hidden="true" />;
  }
  if (rssi >= -85) {
    return <SignalLow className="h-4 w-4 text-amber-500" aria-hidden="true" />;
  }
  return <SignalZero className="h-4 w-4 text-rose-500" aria-hidden="true" />;
}

export function BleLockTool({ isVisible = true }: ToolboxNavigationProps) {
  const { t } = useTranslation();
  const platformAvailable = isBleLockPlatformAvailable();
  const runtime = useRuntimeStatus<BleLockStatus>("ble-lock", {
    enabled: platformAvailable,
  });

  const [devices, setDevices] = useState<BleDevice[]>([]);
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  // Dialog visibility states
  const [scanDialogOpen, setScanDialogOpen] = useState(false);
  const [passwordDialogOpen, setPasswordDialogOpen] = useState(false);
  // User-initiated discovery intent, kept separate from the passive
  // `status.scanning` flag the native monitor may already hold.
  const [userScanning, setUserScanning] = useState(false);

  useTauriEvent(
    BLE_LOCK_DEVICES_EVENT,
    (payload) => {
      if (Array.isArray(payload)) {
        setDevices(payload as BleDevice[]);
      }
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
      if (key === "use_screensaver_to_lock" && checked) patch.sleep_display = false;
      if (key === "sleep_display" && checked) patch.use_screensaver_to_lock = false;
      updateConfig(patch);
    },
    [updateConfig],
  );

  const beginUserDiscovery = useCallback(() => {
    setActionError(null);
    setScanDialogOpen(true);
    setUserScanning(true);
    bleLockScanStart()
      .then((result) => applyStatus(result))
      .catch((error) => {
        setUserScanning(false);
        setActionError(describeError(error));
      });
  }, [applyStatus, describeError]);

  const stopUserScan = useCallback(() => {
    if (!userScanning) return;
    setUserScanning(false);
    bleLockScanStop()
      .then((result) => applyStatus(result))
      .catch((error) => setActionError(describeError(error)));
  }, [applyStatus, describeError, userScanning]);

  const closeUserDiscovery = useCallback(() => {
    setScanDialogOpen(false);
    stopUserScan();
  }, [stopUserScan]);

  const handleScanToggle = useCallback(() => {
    if (userScanning) {
      stopUserScan();
    } else {
      beginUserDiscovery();
    }
  }, [beginUserDiscovery, stopUserScan, userScanning]);

  const handleSelectDevice = useCallback(
    (device: BleDevice) => {
      setActionError(null);
      bleLockSelectDevice(device.id, device.name || undefined)
        .then((result) => {
          applyStatus(result);
          closeUserDiscovery();
        })
        .catch((error) => setActionError(describeError(error)));
    },
    [applyStatus, describeError, closeUserDiscovery],
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
        setPasswordDialogOpen(false);
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
    icon: typeof Zap;
    category: "wake" | "lock";
  }> = [
    { key: "wake_on_proximity", labelKey: "bleLockWakeOnProximity", icon: Zap, category: "wake" },
    { key: "wake_without_unlocking", labelKey: "bleLockWakeWithoutUnlocking", icon: Radio, category: "wake" },
    { key: "pause_now_playing", labelKey: "bleLockPauseWhileLocked", icon: VolumeX, category: "lock" },
    { key: "use_screensaver_to_lock", labelKey: "bleLockUseScreensaver", icon: Monitor, category: "lock" },
    { key: "sleep_display", labelKey: "bleLockTurnOffDisplay", icon: Moon, category: "lock" },
  ];

  return (
    <ToolShell
      title={String(t("bleLock"))}
      description={String(t("bleLockDesc"))}
      error={errorMessage}
      icon={<Bluetooth className="h-5 w-5 text-sky-600" aria-hidden="true" />}
      actions={
        <>
          <Button
            variant={userScanning ? "default" : "outline"}
            className={userScanning ? "bg-sky-600 hover:bg-sky-700 text-white" : ""}
            onClick={handleScanToggle}
            disabled={!nativeEnabled}
          >
            {userScanning ? (
              <>
                <RefreshCw className="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />
                {String(t("bleLockStopScan"))}
              </>
            ) : (
              <>
                <BluetoothSearching className="mr-2 h-4 w-4 text-sky-600" aria-hidden="true" />
                {String(t("bleLockScan"))}
              </>
            )}
          </Button>
          <Button variant="outline" onClick={handleLockNow} disabled={!nativeEnabled}>
            <Lock className="mr-2 h-4 w-4 text-amber-600" aria-hidden="true" />
            {String(t("bleLockLockNow"))}
          </Button>
        </>
      }
    >
      <div className="space-y-6">
        {/* 1. Hero Status Dashboard */}
        <div className="grid gap-4 md:grid-cols-12">
          {/* Active Device Card */}
          <div className="md:col-span-5 flex flex-col justify-between rounded-xl border bg-card/60 p-4 shadow-sm backdrop-blur-sm transition-all hover:shadow-md">
            <div>
              <div className="flex items-center justify-between pb-3 border-b">
                <div className="flex items-center gap-2">
                  <div className={`p-1.5 rounded-lg ${status.selected_device ? "bg-sky-500/10 text-sky-600" : "bg-muted text-muted-foreground"}`}>
                    <BluetoothConnected className="h-4 w-4" aria-hidden="true" />
                  </div>
                  <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    {String(t("bleLockMode"))}
                  </span>
                </div>
                {status.selected_device ? (
                  <Badge variant="outline" className="bg-sky-500/10 text-sky-600 border-sky-200/50">
                    {String(t(config.mode === "active" ? "bleLockModeActive" : "bleLockModePassive"))}
                  </Badge>
                ) : null}
              </div>

              {status.selected_device ? (
                <div className="pt-4 space-y-3">
                  <div>
                    <h4 className="text-base font-semibold tracking-tight text-foreground truncate">
                      {status.selected_device.name}
                    </h4>
                    <p className="font-mono text-xs text-muted-foreground truncate select-all">
                      {status.selected_device.id}
                    </p>
                  </div>
                  <div className="flex items-center gap-2 pt-1">
                    <SignalIndicator rssi={status.selected_device.rssi} />
                    <span className="font-mono text-xs font-medium text-foreground">
                      {status.selected_device.rssi} dBm
                    </span>
                    <span className="text-xs text-muted-foreground">
                      ({String(t("bleLockPresenceLabel"))}: {String(t(`bleLockPresence_${status.presence}`))})
                    </span>
                  </div>
                </div>
              ) : (
                <div className="pt-6 pb-4 text-center space-y-3">
                  <Radio className="mx-auto h-8 w-8 text-muted-foreground/50 animate-pulse" aria-hidden="true" />
                  <div>
                    <p className="text-sm font-medium text-foreground">
                      {String(t("bleLockNoDevices"))}
                    </p>
                    <p className="text-xs text-muted-foreground mt-0.5">
                      选择手环、手表等蓝牙设备以启用自动感应锁屏
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    className="border-sky-300/60 text-sky-600 hover:bg-sky-500/10"
                    onClick={beginUserDiscovery}
                    disabled={!nativeEnabled}
                  >
                    <BluetoothSearching className="mr-1.5 h-3.5 w-3.5" />
                    查找附近设备
                  </Button>
                </div>
              )}
            </div>

            {status.selected_device ? (
              <div className="pt-4 flex items-center justify-between border-t mt-3">
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 text-xs text-sky-600 hover:text-sky-700 hover:bg-sky-500/10"
                  onClick={beginUserDiscovery}
                  disabled={!nativeEnabled}
                >
                  <RefreshCw className="mr-1.5 h-3.5 w-3.5" />
                  更换设备
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 text-xs text-destructive hover:text-destructive hover:bg-destructive/10"
                  onClick={handleClearDevice}
                >
                  <Trash2 className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                  {String(t("bleLockClearDevice"))}
                </Button>
              </div>
            ) : null}
          </div>

          {/* Observed 4 Metric Cards */}
          <div className="md:col-span-7 grid grid-cols-2 gap-3">
            {/* Bluetooth Status */}
            <div className="rounded-xl border bg-card/60 p-3.5 shadow-sm backdrop-blur-sm flex flex-col justify-between">
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted-foreground">
                  {String(t("bleLockBluetoothStatus"))}
                </div>
                <div className={`h-2 w-2 rounded-full ${status.bluetooth_state === "powered_on" ? "bg-emerald-500 shadow-sm shadow-emerald-500/50" : "bg-muted-foreground"}`} />
              </div>
              <div className="pt-2 font-medium text-foreground text-sm flex items-center gap-1.5">
                <Bluetooth className="h-4 w-4 text-sky-500" aria-hidden="true" />
                <span>{String(t(`bleLockBluetoothState_${status.bluetooth_state}`))}</span>
              </div>
            </div>

            {/* RSSI */}
            <div className="rounded-xl border bg-card/60 p-3.5 shadow-sm backdrop-blur-sm flex flex-col justify-between">
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted-foreground">RSSI</div>
                <SignalIndicator rssi={status.averaged_rssi} />
              </div>
              <div className="pt-2 font-medium text-foreground text-sm flex items-center gap-1.5">
                <Signal className="h-4 w-4 text-emerald-500" aria-hidden="true" />
                <span>{status.averaged_rssi ?? "--"} dBm</span>
              </div>
            </div>

            {/* Presence */}
            <div className="rounded-xl border bg-card/60 p-3.5 shadow-sm backdrop-blur-sm flex flex-col justify-between">
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted-foreground">
                  {String(t("bleLockPresenceLabel"))}
                </div>
                <Radio className={`h-3.5 w-3.5 ${status.presence === "near" ? "text-emerald-500" : status.presence === "away" ? "text-amber-500" : "text-muted-foreground"}`} aria-hidden="true" />
              </div>
              <div className="pt-2 font-medium text-foreground text-sm flex items-center gap-1.5">
                <span className={`inline-block h-2 w-2 rounded-full ${status.presence === "near" ? "bg-emerald-500" : status.presence === "away" ? "bg-amber-500" : "bg-zinc-400"}`} />
                <span>{String(t(`bleLockPresence_${status.presence}`))}</span>
              </div>
            </div>

            {/* Screen Lock Status */}
            <div className="rounded-xl border bg-card/60 p-3.5 shadow-sm backdrop-blur-sm flex flex-col justify-between">
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted-foreground">
                  {String(t("bleLockScreenStatus"))}
                </div>
                {status.screen_locked ? (
                  <Lock className="h-3.5 w-3.5 text-amber-500" aria-hidden="true" />
                ) : (
                  <LockOpen className="h-3.5 w-3.5 text-emerald-500" aria-hidden="true" />
                )}
              </div>
              <div className="pt-2 font-medium text-foreground text-sm flex items-center gap-1.5">
                {status.screen_locked ? (
                  <Lock className="h-4 w-4 text-amber-500" aria-hidden="true" />
                ) : (
                  <LockOpen className="h-4 w-4 text-emerald-500" aria-hidden="true" />
                )}
                <span>
                  {String(t(status.screen_locked ? "bleLockScreen_locked" : "bleLockScreen_unlocked"))}
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* 2. Distance & Threshold Configuration */}
        <section className="space-y-5 rounded-xl border bg-card p-5 shadow-sm">
          <div className="flex items-center justify-between border-b pb-3">
            <div className="flex items-center gap-2">
              <Sliders className="h-4 w-4 text-sky-600" aria-hidden="true" />
              <h3 className="text-sm font-semibold text-foreground">
                {String(t("bleLockMode"))} & 阈值参数
              </h3>
            </div>
          </div>

          {/* Mode Selection */}
          <div className="space-y-1.5">
            <label htmlFor="ble-lock-mode" className="text-sm font-medium text-foreground">
              {String(t("bleLockMode"))}
            </label>
            <select
              id="ble-lock-mode"
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              value={config.mode}
              onChange={(event) =>
                updateConfig({ mode: event.target.value === "passive" ? "passive" : "active" })
              }
            >
              <option value="active">{String(t("bleLockModeActive"))}</option>
              <option value="passive">{String(t("bleLockModePassive"))}</option>
            </select>
          </div>

          {/* RSSI Thresholds & Timers */}
          <div className="grid gap-4 md:grid-cols-2">
            <div className="space-y-1.5">
              <label htmlFor="ble-lock-unlock-rssi" className="text-sm font-medium text-foreground">
                {String(t("bleLockUnlockRssi"))}
              </label>
              <select
                id="ble-lock-unlock-rssi"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
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

            <div className="space-y-1.5">
              <label htmlFor="ble-lock-lock-rssi" className="text-sm font-medium text-foreground">
                {String(t("bleLockLockRssi"))}
              </label>
              <select
                id="ble-lock-lock-rssi"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
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
              helperText={`${MIN_LOCK_DELAY} ~ ${MAX_LOCK_DELAY}s`}
            />
            <NumberField
              id="ble-lock-no-signal-timeout"
              label={String(t("bleLockNoSignalTimeout"))}
              value={config.no_signal_timeout_secs}
              min={MIN_NO_SIGNAL_TIMEOUT}
              max={MAX_NO_SIGNAL_TIMEOUT}
              onCommit={(value) => updateConfig({ no_signal_timeout_secs: value })}
              helperText={`${MIN_NO_SIGNAL_TIMEOUT} ~ ${MAX_NO_SIGNAL_TIMEOUT}s`}
            />
          </div>

          {/* Automation Switches */}
          <div className="pt-2 border-t space-y-2">
            <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-3">
              自动化策略
            </h4>
            <div className="grid gap-2.5 sm:grid-cols-2">
              {toggles.map((toggle) => {
                const Icon = toggle.icon;
                return (
                  <div
                    key={toggle.key}
                    className="flex items-center justify-between gap-3 p-3 rounded-lg border bg-background/50 hover:bg-muted/30 transition-colors"
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <Icon className="h-4 w-4 text-sky-600 shrink-0" aria-hidden="true" />
                      <span className="text-sm font-medium text-foreground truncate">
                        {String(t(toggle.labelKey))}
                      </span>
                    </div>
                    <Switch
                      aria-label={String(t(toggle.labelKey))}
                      checked={config[toggle.key]}
                      onCheckedChange={(checked) => toggleConfig(toggle.key, checked)}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        </section>

        {/* 3. Credentials & System Permissions */}
        <section className="grid gap-4 md:grid-cols-2">
          {/* Password Section */}
          <div className="space-y-4 rounded-xl border bg-card p-4 shadow-sm flex flex-col justify-between">
            <div className="space-y-3">
              <div className="flex items-center justify-between border-b pb-2">
                <div className="flex items-center gap-2">
                  <KeyRound className="h-4 w-4 text-sky-600" aria-hidden="true" />
                  <h3 className="text-sm font-semibold text-foreground">
                    {String(t("bleLockPassword"))}
                  </h3>
                </div>
                {status.password_configured ? (
                  <Badge variant="outline" className="bg-emerald-500/10 text-emerald-600 border-emerald-300 gap-1 text-[11px]">
                    <ShieldCheck className="h-3 w-3" />
                    {String(t("bleLockPasswordConfigured"))}
                  </Badge>
                ) : (
                  <Badge variant="outline" className="text-muted-foreground text-[11px]">
                    未保存
                  </Badge>
                )}
              </div>

              <div className="space-y-2">
                <p className="text-xs text-muted-foreground leading-relaxed">
                  {status.password_configured
                    ? "您的 Mac 登录密码已安全保存在本地系统钥匙串（Keychain）中。在低功耗蓝牙设备靠近时，将自动为您唤醒并解锁屏幕。"
                    : "尚未设置登录密码。保存密码后，OneSpace 可在检测到设备靠近时自动安全解锁屏幕。"}
                </p>
                {status.password_configured ? (
                  <div className="flex items-center gap-2 px-3 py-2 rounded-md bg-muted/40 border text-xs font-mono text-muted-foreground">
                    <Lock className="h-3.5 w-3.5 text-emerald-600" />
                    <span>••••••••••••</span>
                    <span className="ml-auto text-[10px] text-emerald-600 font-sans font-medium">Keychain 已保护</span>
                  </div>
                ) : null}
              </div>
            </div>

            <div className="pt-3 border-t flex flex-wrap items-center gap-2">
              <Button
                variant={status.password_configured ? "outline" : "default"}
                size="sm"
                className={!status.password_configured ? "bg-sky-600 hover:bg-sky-700 text-white" : ""}
                onClick={() => setPasswordDialogOpen(true)}
              >
                <KeyRound className="mr-1.5 h-3.5 w-3.5" />
                {status.password_configured ? "修改密码" : String(t("bleLockPassword"))}
              </Button>
              {status.password_configured ? (
                <Button variant="outline" size="sm" onClick={handleDeletePassword}>
                  <Trash2 className="mr-1.5 h-3.5 w-3.5 text-destructive" />
                  {String(t("bleLockDeletePassword"))}
                </Button>
              ) : null}
            </div>
          </div>

          {/* Accessibility Section */}
          <div className="space-y-4 rounded-xl border bg-card p-4 shadow-sm flex flex-col justify-between">
            <div className="space-y-3">
              <div className="flex items-center justify-between border-b pb-2">
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-sky-600" aria-hidden="true" />
                  <h3 className="text-sm font-semibold text-foreground">
                    系统辅助功能权限
                  </h3>
                </div>
                {status.accessibility_trusted ? (
                  <Badge variant="outline" className="bg-emerald-500/10 text-emerald-600 border-emerald-300 gap-1 text-[11px]">
                    <CheckCircle2 className="h-3 w-3" />
                    已授权
                  </Badge>
                ) : (
                  <Badge variant="outline" className="bg-amber-500/10 text-amber-600 border-amber-300 gap-1 text-[11px]">
                    <AlertCircle className="h-3 w-3" />
                    未授权
                  </Badge>
                )}
              </div>

              <div>
                {!status.accessibility_trusted ? (
                  <div className="p-3 rounded-lg bg-amber-500/10 border border-amber-200/50 text-xs text-amber-800 dark:text-amber-300 space-y-2">
                    <div className="font-medium flex items-center gap-1.5">
                      <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
                      <span>{String(t("bleLockError_accessibility_required"))}</span>
                    </div>
                    <div className="text-muted-foreground text-[11px] space-y-1">
                      <p>操作指引：</p>
                      <ol className="list-decimal pl-4 space-y-0.5">
                        <li>点击下方按钮请求权限；</li>
                        <li>系统将打开“系统设置 &gt; 隐私与安全性 &gt; 辅助功能”；</li>
                        <li>在列表中勾选允许 <strong>OneSpace</strong> 即可。</li>
                      </ol>
                    </div>
                  </div>
                ) : (
                  <div className="p-3 rounded-lg bg-emerald-500/10 border border-emerald-200/50 text-xs text-emerald-800 dark:text-emerald-300 space-y-1">
                    <div className="font-medium flex items-center gap-1.5">
                      <ShieldCheck className="h-3.5 w-3.5 shrink-0" />
                      <span>辅助功能权限就绪</span>
                    </div>
                    <p className="text-muted-foreground text-[11px]">
                      OneSpace 已获得必要的系统自动化控制权限，可顺畅执行距离靠近唤醒与锁屏操作。
                    </p>
                  </div>
                )}
              </div>
            </div>

            <div className="pt-3 border-t">
              <Button
                variant={status.accessibility_trusted ? "outline" : "default"}
                size="sm"
                className={!status.accessibility_trusted ? "bg-amber-600 hover:bg-amber-700 text-white" : ""}
                onClick={handleRequestAccessibility}
              >
                <ExternalLink className="mr-1.5 h-3.5 w-3.5" />
                {String(t("bleLockRequestAccessibility"))}
              </Button>
            </div>
          </div>
        </section>

        {/* 4. Scan Devices Dialog */}
        <Dialog
          open={scanDialogOpen}
          onOpenChange={(open) => {
            if (!open) closeUserDiscovery();
          }}
        >
          <DialogContent className="sm:max-w-md max-h-[85vh] flex flex-col p-6">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2 text-base">
                <BluetoothSearching className="h-5 w-5 text-sky-600" />
                {String(t("bleLockDevices"))}
              </DialogTitle>
              <DialogDescription>
                {userScanning
                  ? "正在搜索附近的低功耗蓝牙设备，请靠近您的设备..."
                  : "蓝牙扫描已就绪。点击下方重新扫描以发现新设备。"}
              </DialogDescription>
            </DialogHeader>

            <div className="flex-1 overflow-y-auto py-2 min-h-[200px] max-h-[350px] space-y-2">
              {devices.length === 0 ? (
                <div className="h-44 flex flex-col items-center justify-center text-center p-4 rounded-lg border border-dashed">
                  {userScanning ? (
                    <>
                      <RefreshCw className="h-8 w-8 text-sky-500 animate-spin mb-3" />
                      <p className="text-sm font-medium">{String(t("bleLockScan"))}中...</p>
                      <p className="text-xs text-muted-foreground mt-1">正在监听周围广播信号，请稍候</p>
                    </>
                  ) : (
                    <>
                      <Radio className="h-8 w-8 text-muted-foreground/40 mb-3" />
                      <p className="text-sm text-muted-foreground">{String(t("bleLockNoDevices"))}</p>
                    </>
                  )}
                </div>
              ) : (
                <div className="divide-y rounded-lg border bg-background/50">
                  {devices.map((device) => {
                    const isSelected = status.selected_device?.id === device.id;
                    return (
                      <div
                        key={device.id}
                        className={`flex items-center justify-between gap-3 p-3 transition-colors ${isSelected ? "bg-sky-500/5" : "hover:bg-muted/40"}`}
                      >
                        <div className="min-w-0 flex items-center gap-2.5">
                          <SignalIndicator rssi={device.rssi} />
                          <div className="min-w-0">
                            <div className="flex items-center gap-2">
                              <span className="truncate text-sm font-medium text-foreground">
                                {device.name}
                              </span>
                              {isSelected ? (
                                <Badge variant="outline" className="text-[10px] text-sky-600 border-sky-300">
                                  当前
                                </Badge>
                              ) : null}
                            </div>
                            <div className="truncate font-mono text-xs text-muted-foreground">
                              {device.id}
                            </div>
                          </div>
                        </div>
                        <div className="flex items-center gap-2 shrink-0">
                          <span className="font-mono text-xs text-muted-foreground">
                            {device.rssi} dBm
                          </span>
                          <Button
                            variant={isSelected ? "secondary" : "outline"}
                            size="sm"
                            aria-label={`${String(t("bleLockSelectDevice"))} ${device.name}`}
                            onClick={() => handleSelectDevice(device)}
                            disabled={!nativeEnabled}
                          >
                            {String(t("bleLockSelectDevice"))}
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            <DialogFooter className="flex-row sm:justify-between items-center gap-2 border-t pt-3 mt-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleScanToggle}
                disabled={!nativeEnabled}
              >
                {userScanning ? (
                  <>
                    <RefreshCw className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                    {String(t("bleLockStopScan"))}
                  </>
                ) : (
                  <>
                    <BluetoothSearching className="mr-1.5 h-3.5 w-3.5" />
                    {String(t("bleLockScan"))}
                  </>
                )}
              </Button>
              <Button variant="ghost" size="sm" onClick={closeUserDiscovery}>
                关闭
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* 5. Password Maintenance Dialog */}
        <Dialog open={passwordDialogOpen} onOpenChange={setPasswordDialogOpen}>
          <DialogContent className="sm:max-w-md p-6">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2 text-base">
                <KeyRound className="h-5 w-5 text-sky-600" />
                {status.password_configured ? "修改 Mac 登录密码" : "设置 Mac 登录密码"}
              </DialogTitle>
              <DialogDescription>
                登录密码用于在检测到蓝牙设备靠近时自动唤醒并解锁屏幕。
              </DialogDescription>
            </DialogHeader>

            {/* Keychain Security Promise Banner */}
            <div className="rounded-lg border border-sky-200/60 bg-sky-500/10 p-3.5 text-xs text-sky-950 dark:text-sky-200 space-y-1.5">
              <div className="flex items-center gap-1.5 font-semibold text-sky-700 dark:text-sky-300">
                <ShieldCheck className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
                <span>系统钥匙串（Keychain）安全保护</span>
              </div>
              <p className="leading-relaxed text-muted-foreground text-[11px]">
                您的密码将安全保存在 macOS 系统的钥匙串（Keychain）中。OneSpace 仅在检测到蓝牙设备靠近时通过系统安全接口唤醒并解锁屏幕，绝不会明文存储在本地磁盘文件或进行任何网络上传。
              </p>
            </div>

            {/* Password Input */}
            <div className="space-y-2 py-2">
              <label htmlFor="ble-lock-password" className="text-sm font-medium text-foreground">
                {String(t("bleLockPassword"))}
              </label>
              <div className="relative">
                <input
                  id="ble-lock-password"
                  type={showPassword ? "text" : "password"}
                  autoComplete="off"
                  placeholder="输入您的 Mac 登录密码"
                  className="h-9 w-full rounded-md border border-input bg-background pl-3 pr-9 text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
                <button
                  type="button"
                  className="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                  onClick={() => setShowPassword((prev) => !prev)}
                  tabIndex={-1}
                  aria-label={showPassword ? "Hide password" : "Show password"}
                >
                  {showPassword ? (
                    <EyeOff className="h-4 w-4" aria-hidden="true" />
                  ) : (
                    <Eye className="h-4 w-4" aria-hidden="true" />
                  )}
                </button>
              </div>
            </div>

            <DialogFooter className="gap-2 sm:gap-0">
              <Button variant="ghost" size="sm" onClick={() => setPasswordDialogOpen(false)}>
                取消
              </Button>
              <Button
                size="sm"
                className="bg-sky-600 hover:bg-sky-700 text-white"
                onClick={handleSavePassword}
                disabled={!password}
              >
                {String(t("bleLockSetPassword"))}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>
    </ToolShell>
  );
}
