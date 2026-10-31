import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CloudAgent } from "./index";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const flushEffects = async () => {
  await act(async () => {});
};

const advanceFakeTimers = (ms: number) => {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
};

describe("CloudAgent deployment console", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "installed", local_latest_version: "0.2.0", remote_version: "0.1.9", installed_at: "2026-01-01T00:00:00Z" };
      return null;
    });
  });

  it("header deploy trigger opens dialog and loads SSH host list", async () => {
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getAllByText(/A/).length).toBeGreaterThan(0));
    const btn = screen.getByTestId("cloud-agent-deploy-open");
    await user.click(btn);
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy());
  });

  it("installed host summary shows remote 0.1.9, local 0.2.0 and formatted time; selecting host never invokes deploy", async () => {
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-installation-summary")).toBeTruthy());
    const summary = screen.getByTestId("cloud-agent-installation-summary");
    expect(summary.textContent).toContain("0.1.9");
    expect(summary.textContent).toContain("0.2.0");
    const deployCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_deploy");
    expect(deployCalls.length).toBe(0);
  });

  it("stale isolation: delayed A late resolve does not overwrite B", async () => {
    let resolveAStatus: any, resolveASessions: any, resolveAWorktrees: any, resolveATasks: any, resolveAInstall: any;
    const aStatus = new Promise((r) => { resolveAStatus = r; });
    const aSessions = new Promise((r) => { resolveASessions = r; });
    const aWorktrees = new Promise((r) => { resolveAWorktrees = r; });
    const aTasks = new Promise((r) => { resolveATasks = r; });
    const aInstall = new Promise((r) => { resolveAInstall = r; });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      const host = args?.host;
      if (host === "A") {
        if (command === "cloud_agent_get_status") return aStatus;
        if (command === "cloud_agent_list_sessions") return aSessions;
        if (command === "cloud_agent_list_worktrees") return aWorktrees;
        if (command === "cloud_agent_list_tasks") return aTasks;
        if (command === "cloud_agent_get_installation_info") return aInstall;
      }
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return [{ session_name: "ses-beta-only" }];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: "B", state: "installed", local_latest_version: "0.2.0", remote_version: "0.2.1", installed_at: "2026-01-01T00:00:00Z" };
      return null;
    });
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getAllByText(/A/).length).toBeGreaterThan(0));
    const select = screen.getByTestId("cloud-agent-host-select");
    await user.selectOptions(select as any, "B");
    await waitFor(() => expect(screen.getByText(/ses-beta-only/)).toBeTruthy());
    await act(async () => {
      resolveAStatus({ service_active: false });
      resolveASessions([{ session_name: "ses-alpha-only" }]);
      resolveAWorktrees([]);
      resolveATasks([]);
      resolveAInstall({ host: "A", state: "installed", local_latest_version: "0.0.1", remote_version: "0.0.1", installed_at: "2026-01-01T00:00:00Z" });
    });
    await waitFor(() => expect(screen.getByText(/ses-beta-only/)).toBeTruthy());
    expect(screen.queryByText(/ses-alpha-only/)).toBeNull();
    const summary = screen.getByTestId("cloud-agent-installation-summary");
    expect(summary.textContent).toContain("0.2.1");
    expect(summary.textContent).not.toContain("0.0.1");
  });

  it("handoff: after deploy success selected host is B and status called with host B once", async () => {
    let deployResolve: any;
    const deployPromise = new Promise((r) => { deployResolve = r; });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return args?.host === "A" ? [{ session_name: "alpha-only" }] : [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: args?.host === "A" ? "installed" : "not_installed", local_latest_version: args?.host === "A" ? "0.2.0" : null, remote_version: args?.host === "A" ? "0.1.9" : null, installed_at: args?.host === "A" ? "2026-01-01T00:00:00Z" : null };
      if (command === "cloud_agent_deploy") return deployPromise;
      return null;
    });
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy());
    // open dialog
    await user.click(screen.getByTestId("cloud-agent-deploy-open"));
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy());
    // select target B in dialog
    const dialogSelect = screen.getByTestId("cloud-agent-deploy-host-select");
    await user.selectOptions(dialogSelect as any, "B");
    // deploy
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    await user.click(btn);
    // clear mock history before resolving deploy
    invokeMock.mockClear();
    // resolve deploy
    await act(async () => { deployResolve({ host: "B", operation_id: "op", success: true, phase: "metadata", remote_version: "0.2.0", installed_at: "2026-01-02T00:00:00Z" }); });
    await waitFor(() => expect(screen.queryByTestId("cloud-agent-deployment-dialog")).toBeNull());
    const select = screen.getByTestId("cloud-agent-host-select");
    expect((select as any).value).toBe("B");
    const statusCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status" && c[1]?.host === "B");
    expect(statusCalls.length).toBe(1);
    expect(screen.queryByText(/alpha-only/)).toBeNull();
  });

  it("handoff same-host variant calls status once for A", async () => {
    let deployResolve: any;
    const deployPromise = new Promise((r) => { deployResolve = r; });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") return deployPromise;
      return null;
    });
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy());
    await user.click(screen.getByTestId("cloud-agent-deploy-open"));
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy());
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    await user.click(btn);
    invokeMock.mockClear();
    await act(async () => { deployResolve({ host: "A", operation_id: "op2", success: true, phase: "metadata", remote_version: "0.2.0", installed_at: "2026-01-02T00:00:00Z" }); });
    await waitFor(() => expect(screen.queryByTestId("cloud-agent-deployment-dialog")).toBeNull());
    const statusCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status" && c[1]?.host === "A");
    expect(statusCalls.length).toBe(1);
  });

  it("deployment failure keeps host A selected and dialog open with error", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") throw new Error("deployment failed in phase upload: boom");
      return null;
    });
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy());
    await user.click(screen.getByTestId("cloud-agent-deploy-open"));
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy());
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    await user.click(btn);
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-error")).toBeTruthy());
    const select = screen.getByTestId("cloud-agent-host-select");
    expect((select as any).value).toBe("A");
    expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy();
  });

  it("online-test failure keeps installed target and retest button calls status", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") {
        if (args?.host === "B") return { service_active: false };
        return { service_active: true };
      }
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "installed", local_latest_version: "0.2.0", remote_version: "0.1.9", installed_at: "2026-01-01T00:00:00Z" };
      return null;
    });
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy());
    // simulate handoff to B by selecting B
    const select = screen.getByTestId("cloud-agent-host-select");
    await user.selectOptions(select as any, "B");
    await waitFor(() => expect(screen.getByTestId("cloud-agent-retest")).toBeTruthy());
    const summary = screen.getByTestId("cloud-agent-installation-summary");
    expect(summary).toBeTruthy();
    const before = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status").length;
    await user.click(screen.getByTestId("cloud-agent-retest"));
    await waitFor(() => expect(invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status").length).toBeGreaterThan(before));
    const deployCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_deploy");
    expect(deployCalls.length).toBe(0);
  });

  it("selecting hosts and polling never invokes deploy; advance one interval", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval", "clearTimeout", "clearInterval"] });
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await flushEffects();
    expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy();
    const deployCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_deploy");
    expect(deployCalls.length).toBe(0);
    advanceFakeTimers(10000);
    await flushEffects();
    const deployCalls2 = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_deploy");
    expect(deployCalls2.length).toBe(0);
  });

  it("polling suppression and resume during deployment", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval", "clearTimeout", "clearInterval"] });
    let deployResolve: any;
    const deployPromise = new Promise((r) => { deployResolve = r; });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") return deployPromise;
      return null;
    });
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await flushEffects();
    expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy();
    // open dialog, select B, click deploy
    fireEvent.click(screen.getByTestId("cloud-agent-deploy-open"));
    await flushEffects();
    expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy();
    const dialogSelect = screen.getByTestId("cloud-agent-deploy-host-select");
    fireEvent.change(dialogSelect, { target: { value: "B" } });
    await flushEffects();
    // run the dialog's deferred installation probe so the target is settled
    advanceFakeTimers(50);
    await flushEffects();
    const btn = screen.getByTestId("cloud-agent-deploy-submit");
    fireEvent.click(btn);
    await flushEffects();
    invokeMock.mockClear();
    // advance 30s -> no status call for A
    advanceFakeTimers(30000);
    await flushEffects();
    const aCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status" && c[1]?.host === "A");
    expect(aCalls.length).toBe(0);
    // resolve success
    await act(async () => { deployResolve({ host: "B", operation_id: "op", success: true, phase: "metadata", remote_version: "0.2.0", installed_at: "2026-01-02T00:00:00Z" }); });
    await flushEffects();
    const bCallsAfterHandoff = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status" && c[1]?.host === "B");
    expect(bCallsAfterHandoff.length).toBe(1);
    // advance one polling interval
    advanceFakeTimers(10000);
    await flushEffects();
    const bCallsAfterPoll = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_get_status" && c[1]?.host === "B");
    expect(bCallsAfterPoll.length).toBeGreaterThanOrEqual(2);
  });

  it("installed target can invoke cloud_agent_deploy for redeployment", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_settings") return { default_host: "A", remote_workspace_root: "/r", remote_agent_dir: "/a", auto_refresh_interval_seconds: 10 };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "A", host_name: "1.1.1.1" }, { name: "B", host_name: "2.2.2.2" }];
      if (command === "cloud_agent_get_status") return { service_active: true };
      if (command === "cloud_agent_list_sessions") return [];
      if (command === "cloud_agent_list_worktrees") return [];
      if (command === "cloud_agent_list_tasks") return [];
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "installed", local_latest_version: "0.2.0", remote_version: "0.1.9", installed_at: "2026-01-01T00:00:00Z" };
      if (command === "cloud_agent_deploy") return new Promise(() => {});
      return null;
    });
    const user = userEvent.setup();
    await act(async () => { renderWithProviders(<CloudAgent isVisible={true} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-open")).toBeTruthy());
    await user.click(screen.getByTestId("cloud-agent-deploy-open"));
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deployment-dialog")).toBeTruthy());
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    await waitFor(() => expect(btn).toBeEnabled());
    await user.click(btn);
    const deployCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_deploy");
    expect(deployCalls.length).toBe(1);
    expect(deployCalls[0]![1]!).toMatchObject({ host: "A" });
  });
});

describe("CloudAgent New Task dialog modes", () => {
  it("ordinary mode renders and local-plan mode mounts plan form", async () => {
    renderWithProviders(<CloudAgent />);
    await waitFor(() => expect(screen.getByText(/Cloud Agent|云代理/i)).toBeTruthy());
  });
});
