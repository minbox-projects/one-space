import {
  isToolboxInvokeAvailable,
  invokeToolboxCommand,
} from "@/toolbox/invoke";
import { sshHostsList } from "@/lib/sshTunnels";
import type { SshHost } from "@/components/sshTunnels/types";

export const SERVER_MONITOR_UPDATED_EVENT = "server-monitor-updated";

export type ServerMonitorHistoryRange = "twentyFourHours" | "sevenDays";

export type ServerMonitorScheduleMode = "interval" | "daily";

export type ServerMonitorSourceKind = "savedHost" | "custom";

export type ServerMonitorAuthKind = "password" | "key" | "agent";

export type ServerMonitorFailureKind =
  | "nonLinux"
  | "script"
  | "timeout"
  | "noUsableResources"
  | "transport"
  | "auth"
  | "hostKey"
  | "config";

export interface CpuMetric {
  usagePercent: number;
}

export interface MemoryMetric {
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  usedPercent: number;
}

export interface DiskMetric {
  filesystem: string;
  mountPoint: string;
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  usedPercent: number;
}

export interface NetworkMetric {
  interface: string;
  rxBytesPerSec: number;
  txBytesPerSec: number;
  rxDeltaBytes: number;
  txDeltaBytes: number;
  elapsedSeconds: number;
}

export interface MetricSnapshot {
  collectedAtMs: number;
  samplingWindowSeconds: number;
  cpu: CpuMetric | null;
  memory: MemoryMetric | null;
  disks: DiskMetric[] | null;
  network: NetworkMetric | null;
  networks: NetworkMetric[];
  networkInterface: string | null;
}

export type ServerMonitorOutcome =
  | { success: MetricSnapshot }
  | { partial: MetricSnapshot }
  | "failed";

export interface ServerMonitorSample {
  id?: number | null;
  targetId: string;
  collectedAtMs: number;
  outcome: ServerMonitorOutcome;
  samplingWindowSeconds: number;
  sourceMode: ServerMonitorScheduleMode;
  expectedIntervalSeconds?: number | null;
  cpu: CpuMetric | null;
  memory: MemoryMetric | null;
  disks: DiskMetric[] | null;
  networks: NetworkMetric[];
  networkInterface: string | null;
  errorKind?: ServerMonitorFailureKind | null;
  error?: string | null;
}

export interface ServerMonitorSettings {
  autoCollectEnabled: boolean;
  scheduleMode: ServerMonitorScheduleMode;
  intervalMinutes: number;
  dailyTimes: string[];
  retentionDays: number;
  alertEnabled: boolean;
  cpuThresholdPercent: number;
  memoryThresholdPercent: number;
  diskThresholdPercent: number;
  alertCooldownMinutes: number;
}

export const DEFAULT_SERVER_MONITOR_SETTINGS: ServerMonitorSettings = {
  autoCollectEnabled: true,
  scheduleMode: "interval",
  intervalMinutes: 5,
  dailyTimes: ["09:00"],
  retentionDays: 7,
  alertEnabled: true,
  cpuThresholdPercent: 80,
  memoryThresholdPercent: 85,
  diskThresholdPercent: 85,
  alertCooldownMinutes: 60,
};

export interface ServerMonitorTunnelView {
  enabled: boolean;
  sourceKind: ServerMonitorSourceKind;
  savedHostName: string | null;
  host: string | null;
  port: number | null;
  user: string | null;
  authKind: ServerMonitorAuthKind | null;
  keyPath: string | null;
  hasPassword: boolean;
}

export interface ServerMonitorTargetView {
  id: string;
  name: string;
  enabled: boolean;
  sourceKind: ServerMonitorSourceKind;
  savedHostName: string | null;
  host: string | null;
  port: number | null;
  user: string | null;
  authKind: ServerMonitorAuthKind | null;
  keyPath: string | null;
  hasPassword: boolean;
  tunnel?: ServerMonitorTunnelView | null;
  blocked: boolean;
  lastErrorKind: ServerMonitorFailureKind | null;
  lastError: string | null;
  lastSuccessAtMs: number | null;
  hasAlert: boolean;
  alertReasons: string[];
  collecting: boolean;
  nextCollectionAtMs: number | null;
}

export interface ServerMonitorState {
  settings: ServerMonitorSettings;
  targets: ServerMonitorTargetView[];
  nextCollectionAtMs: number | null;
  lastSuccessAtMs: number | null;
  error?: string | null;
}

export interface ServerMonitorCustomDraft {
  host: string;
  port: number;
  user: string;
  authKind: ServerMonitorAuthKind;
  keyPath?: string | null;
  password?: string | null;
}

export interface ServerMonitorTunnelDraft {
  enabled: boolean;
  sourceKind: ServerMonitorSourceKind;
  savedHostName?: string | null;
  host?: string | null;
  port?: number | null;
  user?: string | null;
  authKind?: ServerMonitorAuthKind | null;
  keyPath?: string | null;
  password?: string | null;
}

export interface ServerMonitorTargetDraft {
  id?: string | null;
  name: string;
  enabled?: boolean;
  sourceKind: ServerMonitorSourceKind;
  savedHostName?: string | null;
  custom?: ServerMonitorCustomDraft | null;
  tunnel?: ServerMonitorTunnelDraft | null;
}

export interface ServerMonitorTestResult {
  succeeded: boolean;
  partial: boolean;
  errorKind: ServerMonitorFailureKind | null;
  error: string | null;
  snapshot: MetricSnapshot | null;
}

export function isServerMonitorAvailable(): boolean {
  return isToolboxInvokeAvailable();
}

export async function serverMonitorSnapshot(): Promise<ServerMonitorState> {
  return invokeToolboxCommand<ServerMonitorState>(
    "server_monitor_snapshot",
    undefined,
  );
}

export async function serverMonitorUpsert(
  draft: ServerMonitorTargetDraft,
): Promise<ServerMonitorState> {
  return invokeToolboxCommand<ServerMonitorState>("server_monitor_upsert", {
    draft,
  });
}

export async function serverMonitorDelete(
  targetId: string,
): Promise<ServerMonitorState> {
  return invokeToolboxCommand<ServerMonitorState>("server_monitor_delete", {
    targetId,
  });
}

export async function serverMonitorSetEnabled(
  targetId: string,
  enabled: boolean,
): Promise<ServerMonitorState> {
  return invokeToolboxCommand<ServerMonitorState>("server_monitor_set_enabled", {
    targetId,
    enabled,
  });
}

export async function serverMonitorRefresh(
  targetId: string,
): Promise<ServerMonitorState> {
  return invokeToolboxCommand<ServerMonitorState>("server_monitor_refresh", {
    targetId,
  });
}

export async function serverMonitorTestConnection(
  draft: ServerMonitorTargetDraft,
): Promise<ServerMonitorTestResult> {
  return invokeToolboxCommand<ServerMonitorTestResult>(
    "server_monitor_test_connection",
    { draft },
  );
}

export async function serverMonitorHistory(
  targetId: string,
  range: ServerMonitorHistoryRange,
): Promise<ServerMonitorSample[]> {
  return invokeToolboxCommand<ServerMonitorSample[]>("server_monitor_history", {
    targetId,
    range,
  });
}

export async function serverMonitorGetSettings(): Promise<ServerMonitorSettings> {
  return invokeToolboxCommand<ServerMonitorSettings>(
    "server_monitor_get_settings",
    undefined,
  );
}

export async function serverMonitorSaveSettings(
  settings: ServerMonitorSettings,
): Promise<ServerMonitorState> {
  return invokeToolboxCommand<ServerMonitorState>(
    "server_monitor_save_settings",
    { settings },
  );
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Formats a 0-100 percentage with one decimal; unknown values stay unknown. */
export function formatPercent(value: number | null | undefined): string {
  if (!isFiniteNumber(value)) return "—";
  return `${value.toFixed(1)}%`;
}

/** Formats a byte count using IEC units with one decimal above bytes. */
export function formatBytes(value: number | null | undefined): string {
  if (!isFiniteNumber(value) || value < 0) return "—";
  if (value < 1024) return `${Math.round(value)} B`;
  const units = ["KiB", "MiB", "GiB", "TiB", "PiB"];
  let amount = value / 1024;
  let index = 0;
  while (amount >= 1024 && index < units.length - 1) {
    amount /= 1024;
    index += 1;
  }
  return `${amount.toFixed(1)} ${units[index]}`;
}

export function formatRate(value: number | null | undefined): string {
  const formatted = formatBytes(value);
  return formatted === "—" ? formatted : `${formatted}/s`;
}

export function formatMonitorTimestamp(
  value: number | null | undefined,
): string {
  if (!isFiniteNumber(value)) return "—";
  return new Date(value).toLocaleString();
}

export function sampleOutcome(
  sample: ServerMonitorSample | null | undefined,
): "success" | "partial" | "failed" {
  if (!sample || !sample.outcome) return "failed";
  if (typeof sample.outcome === "object") {
    if ("success" in sample.outcome) return "success";
    if ("partial" in sample.outcome) return "partial";
  }
  return "failed";
}

export function sampleMetrics(
  sample: ServerMonitorSample | null | undefined,
): MetricSnapshot | null {
  if (!sample || !sample.outcome || typeof sample.outcome !== "object") {
    return null;
  }
  if ("success" in sample.outcome) return sample.outcome.success;
  if ("partial" in sample.outcome) return sample.outcome.partial;
  return null;
}

/** Samples are newest first; the newest successful or partial one wins. */
export function latestGoodSample(
  samples: ServerMonitorSample[] | null | undefined,
): ServerMonitorSample | null {
  if (!Array.isArray(samples)) return null;
  return samples.find((sample) => sampleMetrics(sample) !== null) ?? null;
}

type NetworkSource = {
  networks?: NetworkMetric[] | null;
  network?: NetworkMetric | null;
  networkInterface?: string | null;
};

/**
 * Returns exactly one interface metric. Selection never sums interfaces: an
 * explicit preference wins, then the recorded default, then the first listed.
 */
export function selectNetworkInterface(
  sample: NetworkSource | ServerMonitorSample | null | undefined,
  preferred?: string | null,
): NetworkMetric | null {
  if (!sample) return null;
  const network = (sample as NetworkSource).network ?? null;
  const candidates = Array.isArray(sample.networks) && sample.networks.length > 0
    ? sample.networks
    : network
      ? [network]
      : [];
  if (candidates.length === 0) return null;
  if (preferred) {
    const match = candidates.find((item) => item.interface === preferred);
    if (match) return match;
  }
  if (sample.networkInterface) {
    const match = candidates.find(
      (item) => item.interface === sample.networkInterface,
    );
    if (match) return match;
  }
  return candidates[0];
}

function isIntegerInRange(
  value: unknown,
  minimum: number,
  maximum: number,
): boolean {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= minimum &&
    value <= maximum
  );
}

export function isValidDailyTime(value: string): boolean {
  return /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
}

/** Strict settings validation; invalid input is never silently corrected. */
export function validateServerMonitorSettings(
  settings: ServerMonitorSettings,
): boolean {
  if (!isIntegerInRange(settings.intervalMinutes, 1, 1440)) return false;
  if (!isIntegerInRange(settings.retentionDays, 1, 30)) return false;
  if (!isIntegerInRange(settings.cpuThresholdPercent, 1, 100)) return false;
  if (!isIntegerInRange(settings.memoryThresholdPercent, 1, 100)) return false;
  if (!isIntegerInRange(settings.diskThresholdPercent, 1, 100)) return false;
  if (!isIntegerInRange(settings.alertCooldownMinutes, 1, 1440)) return false;
  if (!Array.isArray(settings.dailyTimes) || settings.dailyTimes.length === 0) {
    return false;
  }
  const seen = new Set<string>();
  for (const time of settings.dailyTimes) {
    if (typeof time !== "string" || !isValidDailyTime(time)) return false;
    if (seen.has(time)) return false;
    seen.add(time);
  }
  return true;
}

/**
 * Strict draft validation. Editing an existing password target may submit an
 * empty password, which keeps the stored credential.
 */
export function validateServerMonitorDraft(
  draft: ServerMonitorTargetDraft,
  editingTargetId?: string,
): boolean {
  const name = typeof draft.name === "string" ? draft.name.trim() : "";
  if (name.length < 1 || name.length > 80) return false;

  if (draft.tunnel && draft.tunnel.enabled) {
    const tunnel = draft.tunnel;
    if (tunnel.sourceKind === "savedHost") {
      if (typeof tunnel.savedHostName !== "string" || tunnel.savedHostName.trim().length === 0) {
        return false;
      }
    } else if (tunnel.sourceKind === "custom") {
      if (typeof tunnel.host !== "string" || tunnel.host.trim().length === 0) {
        return false;
      }
      if (!isIntegerInRange(tunnel.port, 1, 65535)) return false;
      if (typeof tunnel.user !== "string" || tunnel.user.trim().length === 0) {
        return false;
      }
      if (tunnel.authKind === "key") {
        if (typeof tunnel.keyPath !== "string" || tunnel.keyPath.trim().length === 0) {
          return false;
        }
      } else if (tunnel.authKind === "password") {
        const hasPassword = typeof tunnel.password === "string" && tunnel.password.length > 0;
        const editing = Boolean(editingTargetId || draft.id);
        if (!hasPassword && !editing) return false;
      } else {
        return false;
      }
    } else {
      return false;
    }
  }

  if (draft.sourceKind === "savedHost") {
    return (
      typeof draft.savedHostName === "string" &&
      draft.savedHostName.trim().length > 0
    );
  }

  if (draft.sourceKind !== "custom" || !draft.custom) return false;
  const custom = draft.custom;
  if (typeof custom.host !== "string" || custom.host.trim().length === 0) {
    return false;
  }
  if (!isIntegerInRange(custom.port, 1, 65535)) return false;
  if (typeof custom.user !== "string" || custom.user.trim().length === 0) {
    return false;
  }
  if (custom.authKind === "key") {
    return typeof custom.keyPath === "string" && custom.keyPath.trim().length > 0;
  }
  if (custom.authKind === "password") {
    const hasPassword =
      typeof custom.password === "string" && custom.password.length > 0;
    const editing = Boolean(editingTargetId || draft.id);
    return hasPassword || editing;
  }
  return custom.authKind === "agent";
}

/** Fills missing backend settings with the documented defaults. */
export function normalizeServerMonitorSettings(
  input?: Partial<ServerMonitorSettings> | null,
): ServerMonitorSettings {
  const dailyTimes =
    Array.isArray(input?.dailyTimes) && input.dailyTimes.length > 0
      ? input.dailyTimes.filter((time): time is string => typeof time === "string")
      : [...DEFAULT_SERVER_MONITOR_SETTINGS.dailyTimes];
  return {
    autoCollectEnabled:
      typeof input?.autoCollectEnabled === "boolean"
        ? input.autoCollectEnabled
        : DEFAULT_SERVER_MONITOR_SETTINGS.autoCollectEnabled,
    scheduleMode:
      input?.scheduleMode === "daily"
        ? "daily"
        : DEFAULT_SERVER_MONITOR_SETTINGS.scheduleMode,
    intervalMinutes: isIntegerInRange(input?.intervalMinutes, 1, 1440)
      ? (input?.intervalMinutes as number)
      : DEFAULT_SERVER_MONITOR_SETTINGS.intervalMinutes,
    dailyTimes:
      dailyTimes.length > 0 ? dailyTimes : [...DEFAULT_SERVER_MONITOR_SETTINGS.dailyTimes],
    retentionDays: isIntegerInRange(input?.retentionDays, 1, 30)
      ? (input?.retentionDays as number)
      : DEFAULT_SERVER_MONITOR_SETTINGS.retentionDays,
    alertEnabled:
      typeof input?.alertEnabled === "boolean"
        ? input.alertEnabled
        : DEFAULT_SERVER_MONITOR_SETTINGS.alertEnabled,
    cpuThresholdPercent: isIntegerInRange(input?.cpuThresholdPercent, 1, 100)
      ? (input?.cpuThresholdPercent as number)
      : DEFAULT_SERVER_MONITOR_SETTINGS.cpuThresholdPercent,
    memoryThresholdPercent: isIntegerInRange(input?.memoryThresholdPercent, 1, 100)
      ? (input?.memoryThresholdPercent as number)
      : DEFAULT_SERVER_MONITOR_SETTINGS.memoryThresholdPercent,
    diskThresholdPercent: isIntegerInRange(input?.diskThresholdPercent, 1, 100)
      ? (input?.diskThresholdPercent as number)
      : DEFAULT_SERVER_MONITOR_SETTINGS.diskThresholdPercent,
    alertCooldownMinutes: isIntegerInRange(input?.alertCooldownMinutes, 1, 1440)
      ? (input?.alertCooldownMinutes as number)
      : DEFAULT_SERVER_MONITOR_SETTINGS.alertCooldownMinutes,
  };
}

export interface AvailableSshServer {
  id: string;
  name: string;
  host: string;
  port: number;
  user: string;
  sourceType: "config" | "history";
  authType?: "password" | "key";
  authVal?: string;
}

export async function loadAvailableSshServers(): Promise<AvailableSshServer[]> {
  const result: AvailableSshServer[] = [];
  try {
    const configHosts = await sshHostsList<SshHost[]>();
    if (Array.isArray(configHosts)) {
      for (const h of configHosts) {
        if (!h.name || h.name === "*") continue;
        result.push({
          id: `config:${h.name}`,
          name: h.name,
          host: h.host_name || h.name,
          port: h.port || 22,
          user: h.user || "root",
          sourceType: "config",
        });
      }
    }
  } catch {
    // Ignore config host loading failures
  }

  try {
    if (isToolboxInvokeAvailable()) {
      const savedHistoryStr = await invokeToolboxCommand<string | null>("get_secret", {
        key: "onespace_ssh_history",
      });
      if (savedHistoryStr) {
        const historyList = JSON.parse(savedHistoryStr);
        if (Array.isArray(historyList)) {
          for (const item of historyList) {
            const alreadyExists = result.some(
              (r) =>
                r.name === item.name ||
                (r.host === item.host_name && r.user === item.user && r.port === item.port),
            );
            if (!alreadyExists && item.host_name) {
              result.push({
                id: `history:${item.id || item.name || item.host_name}`,
                name: item.name || `${item.user || "root"}@${item.host_name}`,
                host: item.host_name,
                port: item.port || 22,
                user: item.user || "root",
                sourceType: "history",
                authType: item.auth_type,
                authVal: item.auth_val,
              });
            }
          }
        }
      }
    }
  } catch {
    // Ignore history loading failures
  }

  return result;
}
