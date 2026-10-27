import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_SERVER_MONITOR_SETTINGS,
  SERVER_MONITOR_UPDATED_EVENT,
  formatBytes,
  formatMonitorTimestamp,
  formatPercent,
  formatRate,
  isServerMonitorAvailable,
  latestGoodSample,
  sampleMetrics,
  sampleOutcome,
  selectNetworkInterface,
  serverMonitorDelete,
  serverMonitorGetSettings,
  serverMonitorHistory,
  serverMonitorRefresh,
  serverMonitorSaveSettings,
  serverMonitorSetEnabled,
  serverMonitorSnapshot,
  serverMonitorTestConnection,
  serverMonitorUpsert,
  validateServerMonitorDraft,
  validateServerMonitorSettings,
  type ServerMonitorHistoryRange,
  type ServerMonitorSample,
  type ServerMonitorTargetDraft,
} from "@/lib/serverMonitor";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

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

describe("server monitor command wrappers", () => {
  beforeEach(() => {
    resetTauriMocks();
    setTauri(true);
  });

  afterEach(() => {
    setTauri(false);
  });

  it("invokes snapshot with no args", async () => {
    await serverMonitorSnapshot();
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_snapshot", undefined);
  });

  it("invokes upsert with draft", async () => {
    const draft = { name: "x" } as never;
    await serverMonitorUpsert(draft);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_upsert", { draft });
  });

  it("invokes delete with targetId", async () => {
    await serverMonitorDelete("t1");
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_delete", { targetId: "t1" });
  });

  it("invokes setEnabled", async () => {
    await serverMonitorSetEnabled("t1", false);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_set_enabled", { targetId: "t1", enabled: false });
  });

  it("invokes refresh", async () => {
    await serverMonitorRefresh("t1");
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_refresh", { targetId: "t1" });
  });

  it("invokes testConnection with draft", async () => {
    const draft = { name: "x" } as never;
    await serverMonitorTestConnection(draft);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_test_connection", { draft });
  });

  it("invokes history with targetId and range", async () => {
    const range: ServerMonitorHistoryRange = "twentyFourHours";
    await serverMonitorHistory("t1", range);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_history", { targetId: "t1", range });
  });

  it("invokes getSettings with no args", async () => {
    await serverMonitorGetSettings();
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_get_settings", undefined);
  });

  it("invokes saveSettings with settings", async () => {
    const settings = DEFAULT_SERVER_MONITOR_SETTINGS;
    await serverMonitorSaveSettings(settings);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_save_settings", { settings });
  });
});

describe("server monitor availability", () => {
  beforeEach(() => resetTauriMocks());
  it("is false outside Tauri and wrappers do not invoke", async () => {
    setTauri(false);
    expect(isServerMonitorAvailable()).toBe(false);
    await expect(serverMonitorSnapshot()).rejects.toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("server monitor formatters", () => {
  it("formats percent", () => {
    expect(formatPercent(37.25)).toBe("37.3%");
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(NaN)).toBe("—");
  });
  it("formats bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KiB");
    expect(formatBytes(1500000)).toBe("1.4 MiB");
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(NaN)).toBe("—");
  });
  it("formats rate with /s", () => {
    expect(formatRate(1536)).toContain("/s");
  });
  it("formats timestamp", () => {
    expect(typeof formatMonitorTimestamp(0)).toBe("string");
  });
});

describe("server monitor sample helpers", () => {
  it("returns metrics for success and null for failed", () => {
    const snapshot = { collectedAtMs: 1, cpu: null, memory: null, disks: null, network: null, networks: [], networkInterface: null, samplingWindowSeconds: 1 } as never;
    const successSample = { outcome: { success: snapshot } } as unknown as ServerMonitorSample;
    const failedSample = { outcome: "failed" } as unknown as ServerMonitorSample;
    expect(sampleOutcome(successSample)).toBe("success");
    expect(sampleMetrics(failedSample)).toBeNull();
    expect(sampleMetrics(successSample)).not.toBeNull();
  });
  it("latest good ignores later failures", () => {
    const snapshot = { collectedAtMs: 1, cpu: null, memory: null, disks: null, network: null, networks: [], networkInterface: null, samplingWindowSeconds: 1 } as never;
    const samples = [
      { outcome: { success: snapshot } },
      { outcome: "failed" },
    ] as unknown as ServerMonitorSample[];
    expect(latestGoodSample(samples)).toBe(samples[0]);
  });
  it("selects preferred NIC, falls back to first, never sums", () => {
    const sample = {
      networks: [
        { interface: "eth0", rxBytesPerSec: 1, txBytesPerSec: 2, rxDeltaBytes: 1, txDeltaBytes: 2, elapsedSeconds: 1 },
        { interface: "eth1", rxBytesPerSec: 3, txBytesPerSec: 4, rxDeltaBytes: 3, txDeltaBytes: 4, elapsedSeconds: 1 },
      ],
      network: null,
    } as unknown as ServerMonitorSample;
    const preferred = selectNetworkInterface(sample, "eth1");
    expect((preferred as { interface: string }).interface).toBe("eth1");
    const fallback = selectNetworkInterface(sample);
    expect((fallback as { interface: string }).interface).toBe("eth0");
  });
});

describe("server monitor validation", () => {
  const baseSettings = { ...DEFAULT_SERVER_MONITOR_SETTINGS };
  it("validates settings boundaries", () => {
    expect(validateServerMonitorSettings({ ...baseSettings, intervalMinutes: 1, retentionDays: 1 })).toBe(true);
    expect(validateServerMonitorSettings({ ...baseSettings, intervalMinutes: 1440, retentionDays: 30 })).toBe(true);
    expect(validateServerMonitorSettings({ ...baseSettings, intervalMinutes: 0 })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, intervalMinutes: 1441 })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, intervalMinutes: 1.5 })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, retentionDays: 31 })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, retentionDays: -1 })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, dailyTimes: ["00:00", "23:59"] })).toBe(true);
    expect(validateServerMonitorSettings({ ...baseSettings, dailyTimes: ["9:00"] })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, dailyTimes: ["24:00"] })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, dailyTimes: ["09:00", "09:00"] })).toBe(false);
    expect(validateServerMonitorSettings({ ...baseSettings, dailyTimes: [] })).toBe(false);
  });

  it("validates draft boundaries", () => {
    const validDraft = {
      name: "  host  ",
      sourceKind: "custom",
      custom: { host: "h", port: 22, user: "u", authKind: "password", password: "p" },
    } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(validDraft)).toBe(true);
    const emptyName = { ...validDraft, name: "   " } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(emptyName)).toBe(false);
    const longName = { ...validDraft, name: "a".repeat(81) } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(longName)).toBe(false);
    const badPort = { ...validDraft, custom: { host: "h", port: 0, user: "u", authKind: "password", password: "p" } } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(badPort)).toBe(false);
    const missingHost = { ...validDraft, custom: { host: "", port: 22, user: "u", authKind: "password", password: "p" } } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(missingHost)).toBe(false);
    const missingUser = { ...validDraft, custom: { host: "h", port: 22, user: "", authKind: "password", password: "p" } } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(missingUser)).toBe(false);
    const keyMissing = { ...validDraft, custom: { host: "h", port: 22, user: "u", authKind: "key", keyPath: "" } } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(keyMissing)).toBe(false);
    const newPwdMissing = { name: "h", sourceKind: "custom", custom: { host: "h", port: 22, user: "u", authKind: "password" } } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(newPwdMissing)).toBe(false);
    const editEmptyPwd = { name: "h", sourceKind: "custom", custom: { host: "h", port: 22, user: "u", authKind: "password" } } as ServerMonitorTargetDraft;
    expect(validateServerMonitorDraft(editEmptyPwd, "existing-id")).toBe(true);
  });
});

export { SERVER_MONITOR_UPDATED_EVENT };
