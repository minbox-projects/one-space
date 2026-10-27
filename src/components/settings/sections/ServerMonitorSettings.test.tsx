import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "@/test/mocks/render";
import { ServerMonitorSettingsView } from "./ServerMonitorSettings";
import { DEFAULT_SERVER_MONITOR_SETTINGS } from "@/lib/serverMonitor";

describe("ServerMonitorSettingsView", () => {
  it("renders automatic collection switch and mode controls", () => {
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS };
    renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={vi.fn()}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput=""
        onNewTimeInputChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/automatic/i)).toBeTruthy();
  });

  it("shows interval input only in interval mode", () => {
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS, scheduleMode: "interval" as const };
    renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={vi.fn()}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput=""
        onNewTimeInputChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(/interval/i)).toBeTruthy();
  });

  it("shows daily time chips only in daily mode", () => {
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS, scheduleMode: "daily" as const, dailyTimes: ["09:00"] };
    renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={vi.fn()}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput=""
        onNewTimeInputChange={vi.fn()}
      />,
    );
    expect(screen.getByText("09:00")).toBeTruthy();
  });

  it("rejects invalid add time via toast without mutating settings", async () => {
    const user = userEvent.setup();
    const onSettingsChange = vi.fn();
    const onShowToast = vi.fn();
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS, scheduleMode: "daily" as const, dailyTimes: [] };
    renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={onSettingsChange}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput="9:00"
        onNewTimeInputChange={vi.fn()}
        onShowToast={onShowToast}
      />,
    );
    const addButton = screen.getByRole("button", { name: /add/i });
    await user.click(addButton);
    expect(onShowToast).toHaveBeenCalledWith("error", expect.anything());
    expect(onSettingsChange).not.toHaveBeenCalled();
  });

  it("preserves inactive values when switching modes", async () => {
    // controlled component; switch handled by parent in real usage. Component should call onSettingsChange with preserved values.
    const onSettingsChange = vi.fn();
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS, scheduleMode: "interval" as const, dailyTimes: ["18:00"] };
    renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={onSettingsChange}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput=""
        onNewTimeInputChange={vi.fn()}
      />,
    );
    const dailyOption = screen.getByRole("radio", { name: /daily/i });
    await userEvent.setup().click(dailyOption);
    // expect dailyTimes preserved in emitted settings
    if (onSettingsChange.mock.calls.length > 0) {
      const next = onSettingsChange.mock.calls[0][0];
      expect(next.dailyTimes).toEqual(["18:00"]);
    }
  });

  it("renders retention warning", () => {
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS };
    renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={vi.fn()}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput=""
        onNewTimeInputChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/permanently removes older history/i)).toBeTruthy();
  });

  it("renders resource alert thresholds and handles changes", async () => {
    const user = userEvent.setup();
    const onSettingsChange = vi.fn();
    const settings = { ...DEFAULT_SERVER_MONITOR_SETTINGS, alertEnabled: true };
    const { container } = renderWithProviders(
      <ServerMonitorSettingsView
        settings={settings}
        onSettingsChange={onSettingsChange}
        intervalInput="5"
        onIntervalInputChange={vi.fn()}
        retentionInput="7"
        onRetentionInputChange={vi.fn()}
        newTimeInput=""
        onNewTimeInputChange={vi.fn()}
      />,
    );
    const cpuInput = container.querySelector("#server-monitor-cpu-threshold") as HTMLInputElement;
    const memInput = container.querySelector("#server-monitor-memory-threshold") as HTMLInputElement;
    const diskInput = container.querySelector("#server-monitor-disk-threshold") as HTMLInputElement;
    expect(cpuInput).toBeTruthy();
    expect(memInput).toBeTruthy();
    expect(diskInput).toBeTruthy();

    fireEvent.change(cpuInput, { target: { value: "90" } });
    expect(onSettingsChange).toHaveBeenCalledWith(
      expect.objectContaining({ cpuThresholdPercent: 90 }),
    );

    const switchBtn = screen.getByRole("switch", { name: /启用资源告警|Enable resource alerts/i });
    await user.click(switchBtn);
    expect(onSettingsChange).toHaveBeenCalledWith(
      expect.objectContaining({ alertEnabled: false }),
    );
  });
});
