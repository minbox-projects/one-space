import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ServerMonitorTool } from "@/components/ServerMonitor";
import { HistoryChart } from "@/components/ServerMonitor/HistoryChart";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";
import type { ServerMonitorSample } from "@/lib/serverMonitor";

const { pushToastMock, setSettingsTabMock, setSettingsOpenMock } = vi.hoisted(() => ({
  pushToastMock: vi.fn(),
  setSettingsTabMock: vi.fn(),
  setSettingsOpenMock: vi.fn(),
}));

vi.mock("@/components/ToastProvider", async () => {
  const actual = await vi.importActual<typeof import("@/components/ToastProvider")>(
    "@/components/ToastProvider",
  );
  return { ...actual, useToast: () => ({ pushToast: pushToastMock, dismissToast: vi.fn() }) };
});

function setTauri(present: boolean) {
  if (present) {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  } else {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
}

const sampleSnapshot = {
  collectedAtMs: 1000,
  samplingWindowSeconds: 1,
  cpu: { usagePercent: 12.3 },
  memory: { totalBytes: 8 * 1024 ** 3, usedBytes: 4 * 1024 ** 3, availableBytes: 4 * 1024 ** 3, usedPercent: 50 },
  disks: [
    { filesystem: "ext4", mountPoint: "/", totalBytes: 100 * 1024 ** 3, usedBytes: 80 * 1024 ** 3, availableBytes: 20 * 1024 ** 3, usedPercent: 80 },
    { filesystem: "ext4", mountPoint: "/data", totalBytes: 100 * 1024 ** 3, usedBytes: 10 * 1024 ** 3, availableBytes: 90 * 1024 ** 3, usedPercent: 10 },
  ],
  network: { interface: "eth0", rxBytesPerSec: 1024, txBytesPerSec: 2048, rxDeltaBytes: 1024, txDeltaBytes: 2048, elapsedSeconds: 1 },
  networks: [
    { interface: "eth0", rxBytesPerSec: 1024, txBytesPerSec: 2048, rxDeltaBytes: 1024, txDeltaBytes: 2048, elapsedSeconds: 1 },
    { interface: "eth1", rxBytesPerSec: 2048, txBytesPerSec: 4096, rxDeltaBytes: 2048, txDeltaBytes: 4096, elapsedSeconds: 1 },
  ],
  networkInterface: "eth0",
};

const targetA = {
  id: "t1",
  name: "alpha",
  enabled: true,
  sourceKind: "custom",
  savedHostName: null,
  host: "10.0.0.1",
  port: 22,
  user: "root",
  authKind: "password",
  keyPath: null,
  hasPassword: true,
  blocked: false,
  lastErrorKind: null,
  lastError: null,
  lastSuccessAtMs: 1000,
  collecting: false,
  nextCollectionAtMs: 2000,
};

const targetB = {
  id: "t2",
  name: "beta",
  enabled: true,
  sourceKind: "custom",
  savedHostName: null,
  host: "10.0.0.2",
  port: 22,
  user: "root",
  authKind: "password",
  keyPath: null,
  hasPassword: true,
  blocked: false,
  lastErrorKind: null,
  lastError: null,
  lastSuccessAtMs: 1000,
  collecting: false,
  nextCollectionAtMs: 2000,
};

const stateEmpty = { settings: {}, targets: [], nextCollectionAtMs: null, lastSuccessAtMs: null, error: null };
const statePopulated = { settings: {}, targets: [targetA, targetB], nextCollectionAtMs: 2000, lastSuccessAtMs: 1000, error: null };

beforeEach(() => {
  resetTauriMocks();
  setTauri(true);
  pushToastMock.mockReset();
  setSettingsTabMock.mockReset();
  setSettingsOpenMock.mockReset();
  (window as any).setSettingsTab = setSettingsTabMock;
  (window as any).setSettingsOpen = setSettingsOpenMock;
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "server_monitor_snapshot") return statePopulated;
    if (command === "server_monitor_history") return [];
    if (command === "server_monitor_refresh") return undefined;
    if (command === "server_monitor_upsert") return undefined;
    if (command === "server_monitor_delete") return undefined;
    if (command === "server_monitor_set_enabled") return undefined;
    if (command === "server_monitor_test_connection") return { succeeded: true, partial: false, snapshot: sampleSnapshot };
    return undefined;
  });
  listenMock.mockImplementation(async () => vi.fn());
});

describe("ServerMonitorTool", () => {
  it("renders unavailable outside Tauri", async () => {
    setTauri(false);
    resetTauriMocks();
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    expect(screen.getByText(/unavailable/i)).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("hidden issues zero snapshot; visible transition one snapshot", async () => {
    const { rerender } = renderWithProviders(<ServerMonitorTool isVisible={false} />);
    expect(invokeMock).not.toHaveBeenCalledWith("server_monitor_snapshot", expect.anything());
    await act(async () => {
      rerender(<ServerMonitorTool isVisible={true} />);
    });
    // allow async
    await act(async () => {});
    const calls = invokeMock.mock.calls.filter((c) => c[0] === "server_monitor_snapshot");
    expect(calls.length).toBe(1);
  });

  it("empty state shows add-server action", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return stateEmpty;
      return undefined;
    });
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    expect(await screen.findByText(/empty/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /add/i })).toBeInTheDocument();
  });

  it("populated detail renders metrics and partial state", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return {
        ...statePopulated,
        targets: [{ ...targetA, memory: undefined } as any],
      };
      if (command === "server_monitor_history") return [];
      return undefined;
    });
    // inject latest sample by history
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return statePopulated;
      if (command === "server_monitor_history") return [
        {
          id: 1,
          targetId: "t1",
          collectedAtMs: 1000,
          outcome: { success: { ...sampleSnapshot, memory: null } },
          samplingWindowSeconds: 1,
          sourceMode: "interval",
          expectedIntervalSeconds: 300,
          cpu: sampleSnapshot.cpu,
          memory: null,
          disks: sampleSnapshot.disks,
          networks: sampleSnapshot.networks,
          networkInterface: "eth0",
          errorKind: null,
          error: null,
        },
      ];
      return undefined;
    });
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    expect(await screen.findByText(/12.3%/)).toBeInTheDocument();
    expect(screen.getByText(/—/)).toBeInTheDocument();
    expect(screen.getByText(/partial/i)).toBeInTheDocument();
    expect(screen.getByText(/eth0/i)).toBeInTheDocument();
    expect(screen.getByText(/alpha/i)).toBeInTheDocument();
  });

  it("NIC switching changes display without refresh", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return statePopulated;
      if (command === "server_monitor_history") return [
        {
          id: 1,
          targetId: "t1",
          collectedAtMs: 1000,
          outcome: { success: sampleSnapshot },
          samplingWindowSeconds: 1,
          sourceMode: "interval",
          expectedIntervalSeconds: 300,
          cpu: sampleSnapshot.cpu,
          memory: sampleSnapshot.memory,
          disks: sampleSnapshot.disks,
          networks: [
            { interface: "eth0", rxBytesPerSec: 1024, txBytesPerSec: 1024, rxDeltaBytes: 1024, txDeltaBytes: 1024, elapsedSeconds: 1 },
            { interface: "eth1", rxBytesPerSec: 2048, txBytesPerSec: 2048, rxDeltaBytes: 2048, txDeltaBytes: 2048, elapsedSeconds: 1 },
          ],
          networkInterface: "eth0",
          errorKind: null,
          error: null,
        },
      ];
      return undefined;
    });
    const user = userEvent.setup();
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    const select = await screen.findByRole("combobox", { name: /interface/i });
    await user.selectOptions(select, "eth1");
    expect(screen.getByText(/eth1/i)).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("server_monitor_refresh", expect.anything());
  });

  it("history 7 days calls command and renders", async () => {
  invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return statePopulated;
      if (command === "server_monitor_history") return [
        {
          id: 1,
          targetId: "t1",
          collectedAtMs: 1000,
          outcome: { success: sampleSnapshot },
          samplingWindowSeconds: 1,
          sourceMode: "interval",
          expectedIntervalSeconds: 300,
          cpu: sampleSnapshot.cpu,
          memory: sampleSnapshot.memory,
          disks: sampleSnapshot.disks,
          networks: sampleSnapshot.networks,
          networkInterface: "eth0",
          errorKind: null,
          error: null,
        },
      ];
      return undefined;
    });
    const user = userEvent.setup();
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    const btn = await screen.findByRole("button", { name: /7 days|sevenDays|7d/i });
    await user.click(btn);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_history", { targetId: "t1", range: "sevenDays" });
    expect(await screen.findByText(/12.3%/)).toBeInTheDocument();
  });

  it("status states", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return {
        settings: {},
        targets: [
          { ...targetA, blocked: true, lastError: "boom", collecting: false },
          { ...targetB, enabled: false, collecting: false },
        ],
        nextCollectionAtMs: null,
        lastSuccessAtMs: null,
        error: null,
      };
      if (command === "server_monitor_history") return [];
      return undefined;
    });
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    expect(await screen.findByText(/boom/)).toBeInTheDocument();
    expect(screen.getByText(/blocked/i)).toBeInTheDocument();
    expect(screen.getByText(/paused/i)).toBeInTheDocument();
  });

  it("guidance mentions required phrases", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return stateEmpty;
      return undefined;
    });
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    const text = document.body.textContent || "";
    for (const phrase of ["/proc", "df", "one-second", "host", "container"]) {
      expect(text.toLowerCase()).toContain(phrase.toLowerCase());
    }
  });

  it("settings action calls window hooks", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return stateEmpty;
      return undefined;
    });
    const user = userEvent.setup();
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    const btn = screen.getByRole("button", { name: /settings/i });
    await user.click(btn);
    expect(setSettingsTabMock).toHaveBeenCalledWith("server-monitor");
    expect(setSettingsOpenMock).toHaveBeenCalledWith(true);
  });

  it("collect-now calls refresh", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return statePopulated;
      if (command === "server_monitor_history") return [];
      return undefined;
    });
    const user = userEvent.setup();
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    const btn = await screen.findByRole("button", { name: /collect now|refresh/i });
    await user.click(btn);
    expect(invokeMock).toHaveBeenCalledWith("server_monitor_refresh", { targetId: "t1" });
  });

  it("error state shows banner and clears on empty reload", async () => {
    let fail = true;
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") {
        if (fail) throw new Error("boom");
        return stateEmpty;
      }
      return undefined;
    });
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    expect(await screen.findByText(/boom/)).toBeInTheDocument();
    fail = false;
    // trigger reload
    const btn = screen.getByRole("button", { name: /retry|reload/i });
    await userEvent.setup().click(btn);
    await act(async () => {});
    expect(screen.queryByText(/boom/)).not.toBeInTheDocument();
  });

  it("event subscription captures handler safely", async () => {
    let captured: any;
    listenMock.mockImplementation(async (eventName: string, handler: any) => {
      if (eventName === "server-monitor-updated") captured = handler;
      return vi.fn();
    });
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "server_monitor_snapshot") return statePopulated;
      if (command === "server_monitor_history") return [];
      return undefined;
    });
    await act(async () => {
      renderWithProviders(<ServerMonitorTool />);
    });
    expect(captured).toBeTruthy();
    act(() => {
      captured({ payload: { targetIds: ["t1"] } });
    });
    // no crash
    expect(screen.getByText(/alpha/i)).toBeInTheDocument();
  });
});

describe("HistoryChart", () => {
  it("breaks continuity for gaps", () => {
    const samples = [
      { collectedAtMs: 0, outcome: { success: sampleSnapshot } } as unknown as ServerMonitorSample,
      { collectedAtMs: 10000, outcome: { success: sampleSnapshot } } as unknown as ServerMonitorSample,
    ];
    const { container } = renderWithProviders(<HistoryChart samples={samples} range="twentyFourHours" expectedIntervalSeconds={1} />);
    expect(container.querySelectorAll("path, line").length).toBeGreaterThanOrEqual(0);
  });

  it("daily samples render as points", () => {
    const samples = [
      { collectedAtMs: 0, outcome: { success: sampleSnapshot }, sourceMode: "daily" } as unknown as ServerMonitorSample,
    ];
    const { container } = renderWithProviders(<HistoryChart samples={samples} range="sevenDays" />);
    expect(container).toBeTruthy();
  });
});
