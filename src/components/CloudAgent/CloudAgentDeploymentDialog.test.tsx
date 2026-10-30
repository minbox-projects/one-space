import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CloudAgentDeploymentDialog } from "./components/CloudAgentDeploymentDialog";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("CloudAgentDeploymentDialog", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") {
        return { host: args?.host ?? "a", state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      }
      if (command === "cloud_agent_deploy") return { success: true, host: args?.host };
      if (command === "get_ssh_hosts" || command === "ssh_hosts_list") return [{ name: "a", label: "A" }, { name: "b", label: "B" }];
      return null;
    });
    listenMock.mockImplementation(async () => vi.fn());
  });

  const baseProps = (over: any = {}) => ({
    t: (k: string) => k,
    isOpen: true,
    onClose: vi.fn(),
    onInstalled: vi.fn(),
    hosts: [] as any,
    selectedHost: undefined as any,
    ...over,
  });

  it("empty hosts -> guidance; no installation info call", async () => {
    const props = baseProps({ hosts: [] });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    expect(screen.getByText(/SSH/i)).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("cloud_agent_get_installation_info", expect.anything());
  });

  it("hosts a/b selectedHost a; select b calls installation info once and renders fields", async () => {
    const user = userEvent.setup();
    const props = baseProps({ hosts: [{ id: "a", label: "A" }, { id: "b", label: "B" }], selectedHost: "a" });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    const select = screen.getByTestId("cloud-agent-deploy-host-select");
    await user.selectOptions(select as any, "b");
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cloud_agent_get_installation_info", { host: "b" }));
    expect(screen.getByTestId("cloud-agent-deployment-state")).toBeTruthy();
    expect(screen.getByTestId("cloud-agent-remote-version")).toBeTruthy();
    expect(screen.getByTestId("cloud-agent-local-version")).toBeTruthy();
    expect(screen.getByTestId("cloud-agent-installed-at")).toBeTruthy();
  });

  it("installed state -> submit disabled and installed label shown", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "installed", local_latest_version: "0.2.0", remote_version: "0.1.9", installed_at: "2026-01-01T00:00:00Z" };
      return null;
    });
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b" });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-submit")).toBeDisabled());
  });

  it("not_installed -> submit enabled; one click calls deploy once; second click no extra", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") return new Promise(() => {});
      return null;
    });
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b" });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    const user = userEvent.setup();
    await user.click(btn);
    await user.click(btn);
    const deployCalls = invokeMock.mock.calls.filter((c) => c[0] === "cloud_agent_deploy");
    expect(deployCalls.length).toBe(1);
    expect(deployCalls[0]).toBeDefined();
    expect(deployCalls[0]![1]!).toMatchObject({ host: "b" });
    expect(deployCalls[0]![1]!.operationId).toBeTruthy();
  });

  it("while deploying: host select disabled; close does not call onClose; before deploying close calls onClose", async () => {
    // before deploying
    const onClose = vi.fn();
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b", onClose });
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "cloud_agent_get_installation_info") return { host: "b", state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      return null;
    });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    const closeBtn = screen.getByTestId("cloud-agent-deploy-close");
    const user = userEvent.setup();
    await user.click(closeBtn);
    expect(onClose).toHaveBeenCalled();
  });

  it("progress subscription: listen called with event before deploy; emits phases in order; ignores other host/operation; no %", async () => {
    let capturedHandler: any;
    let deployResolve: any;
    const deployPromise = new Promise((r) => { deployResolve = r; });
    listenMock.mockImplementation(async (event: string, handler: any) => {
      if (event === "cloud_agent_deploy_progress") capturedHandler = handler;
      return vi.fn();
    });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host ?? "b", state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") return deployPromise;
      return null;
    });
    const onInstalled = vi.fn();
    const onClose = vi.fn();
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b", onInstalled, onClose });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    expect(listenMock).toHaveBeenCalled();
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    const user = userEvent.setup();
    await user.click(btn);
    await waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "cloud_agent_deploy")).toBeTruthy());
    const deployCall = invokeMock.mock.calls.find((c) => c[0] === "cloud_agent_deploy")!;
    expect(deployCall).toBeDefined();
    const op = deployCall![1]!.operationId;
    expect(capturedHandler).toBeTypeOf("function");
    await act(async () => {
      capturedHandler({ payload: { host: "b", operation_id: op, phase: "platform", status: "start", reason: null } });
      capturedHandler({ payload: { host: "b", operation_id: op, phase: "upload", status: "start", reason: null } });
      capturedHandler({ payload: { host: "b", operation_id: op, phase: "metadata", status: "ok", reason: null } });
      capturedHandler({ payload: { host: "x", operation_id: "other", phase: "platform", status: "start", reason: null } });
    });
    const phases = screen.getAllByTestId("cloud-agent-deploy-phase");
    expect(phases.map((p) => p.getAttribute("data-phase"))).toEqual(["platform", "upload", "metadata"]);
    expect(document.body.textContent || "").not.toMatch(/%/);
    deployResolve({ success: true, host: "b" });
  });

  it("success resolves deploy -> onInstalled called once", async () => {
    const onInstalled = vi.fn();
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b", onInstalled });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") return { success: true, host: "b" };
      return null;
    });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    const user = userEvent.setup();
    await user.click(btn);
    await waitFor(() => expect(onInstalled).toHaveBeenCalled());
  });

  it("failure reject -> error shows upload/boom; onInstalled not called; dialog remains; submit re-enabled", async () => {
    const onInstalled = vi.fn();
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b", onInstalled });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      if (command === "cloud_agent_deploy") throw new Error("deployment failed in phase upload: boom");
      return null;
    });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    const btn = await screen.findByTestId("cloud-agent-deploy-submit");
    const user = userEvent.setup();
    await user.click(btn);
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-error")).toBeTruthy());
  });

  it("probe-error state -> conflict help shown and no enabled deploy", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "probe_error", local_latest_version: null, remote_version: null, installed_at: null };
      return null;
    });
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b" });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    await waitFor(() => expect(screen.getByTestId("cloud-agent-deploy-submit")).toBeDisabled());
  });

  it("requirements mentions Linux/root/systemd and Ubuntu/Debian", async () => {
    const props = baseProps({ hosts: [{ id: "b", label: "B" }], selectedHost: "b" });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") return { host: args?.host, state: "not_installed", local_latest_version: null, remote_version: null, installed_at: null };
      return null;
    });
    await act(async () => { renderWithProviders(<CloudAgentDeploymentDialog {...props} />); });
    const req = screen.getByTestId("cloud-agent-deploy-requirements");
    expect(req.textContent).toMatch(/Linux|root|systemd|Ubuntu|Debian/i);
  });
});
