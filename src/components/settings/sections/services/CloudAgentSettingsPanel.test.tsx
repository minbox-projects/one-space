import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CloudAgentSettingsPanel } from "./CloudAgentSettingsPanel";
import { DEFAULT_CLOUD_AGENT_SETTINGS } from "@/lib/cloudAgent";

vi.mock("@/lib/sshTunnels", () => ({
  sshHostsList: vi.fn().mockResolvedValue([
    { name: "dmit-server", host_name: "1.2.3.4" },
    { name: "backup-vps", host_name: "5.6.7.8" },
  ]),
}));

describe("CloudAgentSettingsPanel", () => {
  const dummyT = ((key: string, fallback?: string) => fallback ?? key) as any;

  it("renders auth settings and toggle yolo mode", async () => {
    const settings = { ...DEFAULT_CLOUD_AGENT_SETTINGS };
    const setSettings = vi.fn();

    await act(async () => {
      render(
        <CloudAgentSettingsPanel
          t={dummyT}
          settings={settings}
          setSettings={setSettings}
        />
      );
    });

    // Verify title and descriptions
    expect(screen.getByText("OpenCode 权限与运行策略")).toBeTruthy();
    expect(screen.getByText("完全自动授权模式 (--yolo)")).toBeTruthy();
    expect(screen.getByText("本地直连终端软件")).toBeTruthy();

    // Verify switch toggle
    const switches = screen.getAllByRole("switch");
    expect(switches.length).toBeGreaterThan(0);
    fireEvent.click(switches[0]);
    expect(setSettings).toHaveBeenCalled();
  });

  it("updates default extra args input", async () => {
    const settings = { ...DEFAULT_CLOUD_AGENT_SETTINGS, opencodeExtraArgs: "--verbose" };
    const setSettings = vi.fn();

    await act(async () => {
      render(
        <CloudAgentSettingsPanel
          t={dummyT}
          settings={settings}
          setSettings={setSettings}
        />
      );
    });

    const input = screen.getByDisplayValue("--verbose");
    fireEvent.change(input, { target: { value: "--model new-model" } });
    expect(setSettings).toHaveBeenCalled();
  });

  it("updates workspace base path and alert threshold", async () => {
    const settings = { ...DEFAULT_CLOUD_AGENT_SETTINGS };
    const setSettings = vi.fn();

    await act(async () => {
      render(
        <CloudAgentSettingsPanel
          t={dummyT}
          settings={settings}
          setSettings={setSettings}
        />
      );
    });

    const workspaceInput = screen.getByDisplayValue(settings.remoteWorkspaceRoot);
    fireEvent.change(workspaceInput, { target: { value: "/data/repos" } });
    expect(setSettings).toHaveBeenCalled();

    const thresholdInput = screen.getByDisplayValue(String(settings.diskAlertThresholdPercent));
    fireEvent.change(thresholdInput, { target: { value: "90" } });
    expect(setSettings).toHaveBeenCalled();
  });
});
