import { act, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import {
  invokeMock,
  isVisibleMock,
  listenMock,
  resetTauriMocks,
} from "@/test/mocks/tauri";
import type { TrayMenuNode } from "@/lib/trayMenu";
import * as aiWorkflowProfiles from "@/lib/aiWorkflowProfiles";

/**
 * The native-action fixtures model the macOS Tauri host. jsdom defaults to a
 * non-macOS platform, which the platform guard treats as unavailable; the
 * per-suite `beforeEach` pins macOS for positive native-call coverage and the
 * `afterEach` restores the original value.
 */
const originalPlatform = window.navigator.platform;

function setNavigatorPlatform(platform: string) {
  Object.defineProperty(window.navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

type RegistryToolDescriptor = {
  id: string;
  surfaces: readonly string[];
};

type RegistryModule = {
  listToolboxTools: (surface: string) => RegistryToolDescriptor[];
};

/**
 * Loaded lazily so this suite still runs while the registry module is absent.
 */
async function loadRegistry(): Promise<RegistryModule> {
  const specifier = ["@/toolbox", "registry"].join("/");
  return (await import(/* @vite-ignore */ specifier)) as RegistryModule;
}

type ItemNode = Extract<TrayMenuNode, { kind: "item" }>;

const trayHarness = vi.hoisted(() => ({
  applyTrayMenu: vi.fn<
    (model: TrayMenuNode[], onAction: (id: string) => void) => Promise<boolean>
  >(async () => true),
}));

vi.mock("@/lib/trayMenu", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/trayMenu")>();
  return { ...actual, applyTrayMenu: trayHarness.applyTrayMenu };
});

vi.mock("@tauri-apps/api/menu", () => ({
  Menu: { new: vi.fn(async () => ({})) },
  MenuItem: { new: vi.fn(async () => ({})) },
  CheckMenuItem: { new: vi.fn(async () => ({})) },
  Submenu: { new: vi.fn(async () => ({})) },
  PredefinedMenuItem: { new: vi.fn(async () => ({})) },
}));

vi.mock("@tauri-apps/api/tray", () => ({
  TrayIcon: { getById: vi.fn(async () => null) },
}));

/**
 * Render probe for an unrelated always-mounted view whose props stay identical,
 * so an extra App render caused by an unchanged runtime event is observable.
 */
const launcherRenderHarness = vi.hoisted(() => ({ renders: 0 }));

vi.mock("@/components/Launcher", () => ({
  Launcher: () => {
    launcherRenderHarness.renders += 1;
    return <div data-testid="mock-launcher" />;
  },
}));

const aboutModalHarness = vi.hoisted(() => ({
  props: [] as Array<{ open: boolean; autoCheckOnOpen: boolean }>,
}));

vi.mock("@/components/AboutModal", () => ({
  AboutModal: (props: { open?: boolean; autoCheckOnOpen?: boolean }) => {
    aboutModalHarness.props.push({
      open: props.open ?? false,
      autoCheckOnOpen: props.autoCheckOnOpen ?? false,
    });
    return null;
  },
}));

const writeTextMock = vi.fn(async () => undefined);

/** Top-level tray destinations that must show the window and activate their page. */
const TOP_LEVEL_DESTINATIONS = [
  "launcher",
  "ai-sessions",
  "ai-environments",
  "ai-gateway",
  "ai-usage",
] as const;

/** More Pages submenu destinations that must show the window and activate their page. */
const MORE_PAGES_DESTINATIONS = [
  "workspaces",
  "mcp-servers",
  "skills",
  "subagents",
  "ssh",
  "ssh-tunnels",
  "protocol-router",
  "file-sharing",
  "ai-news",
  "bookmarks",
  "mail",
  "snippets",
  "notes",
  "documentation",
  "more-tools",
] as const;

type NavWindow = Window & { setActiveTab?: (tab: string) => void };

type BatchResult = {
  operation: "connect" | "disconnect";
  group_id: string;
  group_name: string;
  success_count: number;
  failed_count: number;
  skipped_count: number;
  total_count: number;
  failures: Array<{ tunnel_id: string; tunnel_name: string; error: string }>;
};

const SSH_SNAPSHOT = {
  groups: [],
  tunnels: [
    {
      id: "t1",
      name: "One",
      group_id: "default",
      source_kind: "custom",
      forward: { mode: "local" },
      auto_connect: false,
      auto_reconnect: true,
      created_at: 0,
      updated_at: 0,
    },
    {
      id: "t2",
      name: "Two",
      group_id: "default",
      source_kind: "custom",
      forward: { mode: "local" },
      auto_connect: false,
      auto_reconnect: true,
      created_at: 0,
      updated_at: 0,
    },
  ],
  runtime: [
    { id: "t1", status: "connected", active_client_count: 0, mode: "local", summary: "" },
    { id: "t2", status: "disconnected", active_client_count: 0, mode: "local", summary: "" },
  ],
};

function emptyBatch(operation: "connect" | "disconnect"): BatchResult {
  return {
    operation,
    group_id: "all",
    group_name: "All Tunnels",
    success_count: 0,
    failed_count: 0,
    skipped_count: 0,
    total_count: 0,
    failures: [],
  };
}

/** Copy the two-tunnel snapshot with the given runtime statuses for t1 and t2. */
function snapshotWithRuntime(first: string, second: string) {
  return {
    ...SSH_SNAPSHOT,
    runtime: SSH_SNAPSHOT.runtime.map((entry, index) => ({
      ...entry,
      status: index === 0 ? first : second,
    })),
  };
}

describe("App tray menu integration", () => {
  let eventHandlers: Record<string, Array<(event: { payload?: unknown }) => unknown>>;
  let gatewayRunning: boolean;
  let gatewayStartError: Error | null;
  let routerRunning: boolean;
  let sharingRunning: boolean;
  let sharingFileCount: number;
  let sshSnapshot: typeof SSH_SNAPSHOT;
  let sshConnectResult: BatchResult | null;
  let sshDisconnectResult: BatchResult | null;
  let sshSnapshotAfterConnect: typeof SSH_SNAPSHOT | null;
  let sshSnapshotAfterDisconnect: typeof SSH_SNAPSHOT | null;
  let activeModelsResult: unknown;
  let activeModelsError: Error | null;
  let activationReport: {
    active_profile: string;
    hosts: string[];
    installations: unknown[];
  };
  let bleLockStatus: ReturnType<typeof bleLockStatusFixture>;
  let bleLockLockNowError: unknown;

  function appActiveMatrix(name: string, backendModel: string) {
    return {
      name,
      rows: aiWorkflowProfiles.SUPPORTED_ROLES.map((role) => ({
        role,
        ...(role === "backend"
          ? { codex: { model: backendModel, reasoning_effort: "high" } }
          : {}),
      })),
    };
  }

  function gatewayStatus() {
    return {
      running: gatewayRunning,
      enabled: true,
      port: 17688,
      local_base_url: "http://127.0.0.1:17688/v1",
      provider_count: 1,
      auto_disabled_count: 0,
      key_count: 1,
      default_key_id: "k1",
    };
  }

  function bleLockStatusFixture() {
    return {
      platform_available: true,
      bluetooth_state: "powered_on",
      monitoring: true,
      scanning: false,
      selected_device: { id: "uuid-a", name: "Key Fob", rssi: -55 },
      presence: "near",
      averaged_rssi: -55,
      screen_locked: false,
      password_configured: true,
      accessibility_trusted: true,
      config: {
        schema_version: 1,
        device_id: "uuid-a",
        device_name: "Key Fob",
        mode: "active",
        unlock_rssi: -60,
        lock_rssi: -80,
        lock_delay_secs: 10,
        no_signal_timeout_secs: 30,
        wake_on_proximity: false,
        wake_without_unlocking: false,
        pause_now_playing: false,
        use_screensaver_to_lock: false,
        sleep_display: false,
      },
      last_error: null,
    };
  }

  function renderApp() {
    return renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );
  }

  function latestModel(): TrayMenuNode[] {
    const call = trayHarness.applyTrayMenu.mock.calls.at(-1);
    if (!call) throw new Error("applyTrayMenu was not called");
    return call[0] as TrayMenuNode[];
  }

  function findModelItem(nodes: TrayMenuNode[], id: string): ItemNode | undefined {
    for (const node of nodes) {
      if (node.kind !== "item") continue;
      if (node.id === id) return node;
      if (node.submenu) {
        const found = findModelItem(node.submenu, id);
        if (found) return found;
      }
    }
    return undefined;
  }

  function commandCallCount(command: string) {
    return invokeMock.mock.calls.filter(([name]) => name === command).length;
  }

  async function waitForSetActiveTab() {
    await waitFor(() =>
      expect((window as NavWindow).setActiveTab).toBeTypeOf("function"),
    );
  }

  /** Wrap the App's exposed navigation binding so tray actions are observable. */
  function installSetActiveTabSpy() {
    const original = (window as NavWindow).setActiveTab;
    const spy = vi.fn((tab: string) => {
      original?.(tab);
    });
    (window as NavWindow).setActiveTab = spy;
    return spy;
  }

  async function waitForAppliedMenu() {
    await waitFor(() => expect(trayHarness.applyTrayMenu).toHaveBeenCalled());
  }

  async function fireTrayAction(id: string) {
    await waitForAppliedMenu();
    const call = trayHarness.applyTrayMenu.mock.calls.at(-1)!;
    const onAction = call[1] as (actionId: string) => unknown;
    await act(async () => {
      const result = onAction(id);
      if (result instanceof Promise) await result;
    });
  }

  async function fireActionAndExpectCommand(id: string, command: string) {
    const before = commandCallCount(command);
    await fireTrayAction(id);
    await waitFor(() =>
      expect(
        commandCallCount(command),
        `${id} should invoke ${command}`,
      ).toBeGreaterThan(before),
    );
  }

  async function triggerEvent(name: string, payload?: unknown) {
    const handlers = eventHandlers[name] ?? [];
    await act(async () => {
      for (const handler of handlers) {
        const result = handler({ payload });
        if (result instanceof Promise) await result;
      }
    });
  }

  beforeEach(async () => {
    resetTauriMocks();
    trayHarness.applyTrayMenu.mockClear();
    trayHarness.applyTrayMenu.mockImplementation(async () => true);
    eventHandlers = {};
    gatewayRunning = false;
    gatewayStartError = null;
    routerRunning = false;
    sharingRunning = false;
    sharingFileCount = 0;
    sshSnapshot = snapshotWithRuntime("connected", "disconnected");
    sshConnectResult = null;
    sshDisconnectResult = null;
    sshSnapshotAfterConnect = null;
    sshSnapshotAfterDisconnect = null;
    activeModelsResult = null;
    activeModelsError = null;
    activationReport = { active_profile: "team-alpha", hosts: [], installations: [] };
    bleLockStatus = bleLockStatusFixture();
    bleLockLockNowError = null;
    aboutModalHarness.props.length = 0;
    launcherRenderHarness.renders = 0;

    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }) as unknown as typeof window.matchMedia;

    setNavigatorPlatform("MacIntel");

    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: writeTextMock },
      configurable: true,
    });
    writeTextMock.mockClear();

    await i18n.changeLanguage("en");

    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        switch (command) {
        case "should_show_onboarding":
          return false;
        case "get_storage_config":
          return {
            language: "en",
            storage_type: "local",
            main_shortcut: "Alt+Space",
            quick_ai_shortcut: "Alt+Shift+A",
            auto_update_enabled: false,
          };
        case "dashboard_counts":
          return {
            ok: true,
            data: {
              launcher: 0,
              workspaces: 0,
              sessions: 0,
              ssh: 0,
              snippets: 0,
              bookmarks: 0,
              notes: 0,
              ai_news: 0,
              environments: 0,
              skills: 0,
              subagents: 0,
              mcp_servers: 0,
            },
            meta: { schema_version: 1, revision: 1 },
          };
        case "ai_gateway_status":
          return gatewayStatus();
        case "ai_gateway_start":
          if (gatewayStartError) throw gatewayStartError;
          gatewayRunning = true;
          return gatewayStatus();
        case "ai_gateway_stop":
          gatewayRunning = false;
          return gatewayStatus();
        case "protocol_router_status":
          return {
            running: routerRunning,
            enabled: routerRunning,
            port: 17689,
            route_count: 0,
          };
        case "protocol_router_start":
          routerRunning = true;
          return { running: true, enabled: true, port: 17689, route_count: 0 };
        case "protocol_router_stop":
          routerRunning = false;
          return { running: false, enabled: true, port: 17689, route_count: 0 };
        case "ssh_tunnels_snapshot":
          return sshSnapshot;
        case "get_ssh_hosts":
          return [];
        case "ssh_tunnels_connect_all":
          if (sshSnapshotAfterConnect) sshSnapshot = sshSnapshotAfterConnect;
          return sshConnectResult ?? emptyBatch("connect");
        case "ssh_tunnels_disconnect_all":
          if (sshSnapshotAfterDisconnect) sshSnapshot = sshSnapshotAfterDisconnect;
          return sshDisconnectResult ?? emptyBatch("disconnect");
        case "file_sharing_networks":
          return [];
        case "file_sharing_status":
          return {
            running: sharingRunning,
            sessionId: sharingRunning ? "s1" : null,
            address: null,
            port: null,
            shareUrl: null,
            startedAt: null,
            stoppedAt: null,
            files: Array.from({ length: sharingFileCount }, (_, index) => ({
              id: `file-${index}`,
            })),
            transfers: [],
            summary: {
              activeTransfers: 0,
              completedTransfers: 0,
              failedTransfers: 0,
              cancelledTransfers: 0,
              bytesSent: 0,
              droppedTransferRecords: 0,
            },
            lastError: null,
          };
        case "ai_workflow_get_active_models":
          if (activeModelsError) throw activeModelsError;
          return activeModelsResult;
        case "ai_workflow_activate_profile":
          return activationReport;
        case "ble_lock_get_status":
          return bleLockStatus;
        case "ble_lock_lock_now":
          if (bleLockLockNowError) throw bleLockLockNowError;
          bleLockStatus = { ...bleLockStatus, screen_locked: true };
          return bleLockStatus;
        case "ble_lock_save_config": {
          const nextConfig = (args as { config?: unknown } | undefined)?.config;
          bleLockStatus = {
            ...bleLockStatus,
            config: (nextConfig ?? bleLockStatus.config) as typeof bleLockStatus.config,
          };
          return bleLockStatus;
        }
        case "ble_lock_select_device":
          return bleLockStatus;
        case "ble_lock_request_accessibility":
          bleLockStatus = { ...bleLockStatus, accessibility_trusted: true };
          return bleLockStatus;
        default:
          return undefined;
      }
    });

    listenMock.mockImplementation(
      async (
        eventName: string,
        handler: (event: { payload?: unknown }) => unknown,
      ) => {
        if (!eventHandlers[eventName]) eventHandlers[eventName] = [];
        eventHandlers[eventName].push(handler);
        return () => {
          eventHandlers[eventName] = eventHandlers[eventName].filter(
            (registered) => registered !== handler,
          );
        };
      },
    );
  });

  afterEach(() => {
    setNavigatorPlatform(originalPlatform);
  });

  it("applies the tray menu on mount", async () => {
    renderApp();

    await waitFor(() => expect(trayHarness.applyTrayMenu).toHaveBeenCalled());
    const model = latestModel();
    expect(findModelItem(model, "toggle-window")).toBeDefined();
    expect(findModelItem(model, "services")).toBeDefined();
  });

  it("includes every registry tray tool in the applied model", async () => {
    const registry = await loadRegistry();
    const trayTools = registry.listToolboxTools("tray");
    expect(trayTools.length).toBeGreaterThan(0);

    renderApp();
    await waitForAppliedMenu();

    const model = latestModel();
    for (const tool of trayTools) {
      expect(findModelItem(model, tool.id)).toBeDefined();
    }
  });

  it("hides the window from the tray while it is visible", async () => {
    isVisibleMock.mockResolvedValue(true);
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.label).toBe(
        String(i18n.t("tray.toggle.hide")),
      ),
    );

    await fireActionAndExpectCommand("toggle-window", "hide_window");
  });

  it("shows the window from the tray while it is hidden", async () => {
    isVisibleMock.mockResolvedValue(false);
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.label).toBe(
        String(i18n.t("tray.toggle.show")),
      ),
    );

    await fireActionAndExpectCommand("toggle-window", "show_main_window");
  });

  it("opens the quick AI window from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand("quick-ai", "toggle_quick_ai_window");
  });

  it("starts the gateway from the tray and refreshes the check state", async () => {
    gatewayRunning = false;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(false),
    );

    await fireActionAndExpectCommand("gateway", "ai_gateway_start");

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(true),
    );
  });

  it("stops the gateway from the tray", async () => {
    gatewayRunning = true;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(true),
    );

    await fireActionAndExpectCommand("gateway", "ai_gateway_stop");
  });

  it("connects all tunnels from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand("connect-all", "ssh_tunnels_connect_all");
  });

  it("disconnects all tunnels from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand(
      "disconnect-all",
      "ssh_tunnels_disconnect_all",
    );
  });

  it("stops file sharing from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand("stop-sharing", "file_sharing_stop");
  });

  it("runs a sync from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand("sync", "sync_run_now");
  });

  it("copies the gateway local base url from the tray", async () => {
    gatewayRunning = true;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(true),
    );

    await fireTrayAction("copy-address");

    await waitFor(() =>
      expect(writeTextMock).toHaveBeenCalledWith("http://127.0.0.1:17688/v1"),
    );
  });

  it("quits the app from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand("quit", "quit_app");
  });

  it("rebuilds with the opposite toggle label after a visibility change", async () => {
    isVisibleMock.mockResolvedValue(true);
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.label).toBe(
        String(i18n.t("tray.toggle.hide")),
      ),
    );

    isVisibleMock.mockResolvedValue(false);
    await triggerEvent("main-window-visibility-changed", false);

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.label).toBe(
        String(i18n.t("tray.toggle.show")),
      ),
    );
  });

  it("rebuilds the service check state after a gateway status event", async () => {
    gatewayRunning = false;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(false),
    );

    gatewayRunning = true;
    await triggerEvent("ai-gateway-status-update", undefined);

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(true),
    );
  });

  it("shows the gateway start failure toast and keeps the item unchecked", async () => {
    gatewayRunning = false;
    gatewayStartError = new Error("port already in use");
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(false),
    );

    await fireTrayAction("gateway");

    await waitFor(() => {
      const text = document.body.textContent ?? "";
      expect(text).toContain("17688");
      expect(text).toContain("port already in use");
    });
    await waitFor(() =>
      expect(findModelItem(latestModel(), "gateway")?.checked).toBe(false),
    );
  });

  it("shows the partial SSH batch result toast", async () => {
    sshConnectResult = {
      operation: "connect",
      group_id: "all",
      group_name: "All Tunnels",
      success_count: 2,
      failed_count: 1,
      skipped_count: 0,
      total_count: 3,
      failures: [{ tunnel_id: "t3", tunnel_name: "Three", error: "auth failed" }],
    };
    renderApp();

    await fireTrayAction("connect-all");

    const expected = String(
      i18n.t("tray.action.sshBatchPartial", { success: 2, failed: 1 }),
    );
    await waitFor(() =>
      expect(document.body.textContent ?? "").toContain(expected),
    );
  });

  it.each([...TOP_LEVEL_DESTINATIONS, ...MORE_PAGES_DESTINATIONS])(
    "shows the window and activates the %s page from the tray",
    async (id) => {
      renderApp();
      await waitForAppliedMenu();
      await waitForSetActiveTab();

      const spy = installSetActiveTabSpy();
      const beforeShow = commandCallCount("show_main_window");

      await fireTrayAction(id);

      await waitFor(() =>
        expect(commandCallCount("show_main_window")).toBeGreaterThan(beforeShow),
      );
      expect(spy).toHaveBeenCalledWith(id);
    },
  );

  it("activates the settings page from the tray", async () => {
    renderApp();
    await waitForAppliedMenu();
    await waitForSetActiveTab();

    const spy = installSetActiveTabSpy();
    const beforeShow = commandCallCount("show_main_window");

    await fireTrayAction("settings");

    await waitFor(() =>
      expect(commandCallCount("show_main_window")).toBeGreaterThan(beforeShow),
    );
    expect(spy).toHaveBeenCalledWith("settings");
  });

  it("opens the About modal with the auto-check flag for check-for-updates", async () => {
    renderApp();

    await fireTrayAction("check-for-updates");

    await waitFor(() => {
      expect(aboutModalHarness.props.at(-1)).toEqual({
        open: true,
        autoCheckOnOpen: true,
      });
    });
  });

  it("opens the About modal without the auto-check flag for about", async () => {
    renderApp();

    await fireTrayAction("about");

    await waitFor(() => {
      expect(aboutModalHarness.props.at(-1)).toEqual({
        open: true,
        autoCheckOnOpen: false,
      });
    });
  });

  it("starts the stopped router from the tray and re-queries status", async () => {
    routerRunning = false;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "router")?.checked).toBe(false),
    );

    const statusBefore = commandCallCount("protocol_router_status");
    await fireActionAndExpectCommand("router", "protocol_router_start");

    await waitFor(() =>
      expect(commandCallCount("protocol_router_status")).toBeGreaterThan(
        statusBefore,
      ),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "router")?.checked).toBe(true),
    );
  });

  it("stops the running router from the tray and re-queries status", async () => {
    routerRunning = true;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "router")?.checked).toBe(true),
    );

    const statusBefore = commandCallCount("protocol_router_status");
    await fireActionAndExpectCommand("router", "protocol_router_stop");

    await waitFor(() =>
      expect(commandCallCount("protocol_router_status")).toBeGreaterThan(
        statusBefore,
      ),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "router")?.checked).toBe(false),
    );
  });

  it("rebuilds the tunnel count after an ssh-tunnels-updated event", async () => {
    sshSnapshot = snapshotWithRuntime("connected", "disconnected");
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "tunnels-status")?.label).toContain(
        "1/2",
      ),
    );

    sshSnapshot = snapshotWithRuntime("connected", "connected");
    const before = trayHarness.applyTrayMenu.mock.calls.length;
    await triggerEvent("ssh-tunnels-updated");

    await waitFor(() =>
      expect(trayHarness.applyTrayMenu.mock.calls.length).toBeGreaterThan(
        before,
      ),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "tunnels-status")?.label).toContain(
        "2/2",
      ),
    );
  });

  it("rebuilds the sharing state after a file-sharing-updated event", async () => {
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "stop-sharing")?.enabled).toBe(false),
    );

    sharingRunning = true;
    sharingFileCount = 2;
    const before = trayHarness.applyTrayMenu.mock.calls.length;
    await triggerEvent("file-sharing-updated");

    await waitFor(() =>
      expect(trayHarness.applyTrayMenu.mock.calls.length).toBeGreaterThan(
        before,
      ),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "stop-sharing")?.enabled).toBe(true),
    );
    expect(findModelItem(latestModel(), "sharing-status")?.label).toContain("2");
  });

  it("rebuilds the router check state after a protocol-router-status-update event", async () => {
    routerRunning = false;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "router")?.checked).toBe(false),
    );

    routerRunning = true;
    const before = trayHarness.applyTrayMenu.mock.calls.length;
    await triggerEvent("protocol-router-status-update");

    await waitFor(() =>
      expect(trayHarness.applyTrayMenu.mock.calls.length).toBeGreaterThan(
        before,
      ),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "router")?.checked).toBe(true),
    );
  });

  it("rebuilds every label on a language change", async () => {
    isVisibleMock.mockResolvedValue(false);
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.label).toBe(
        String(i18n.t("tray.toggle.show")),
      ),
    );

    const before = trayHarness.applyTrayMenu.mock.calls.length;
    await act(async () => {
      await i18n.changeLanguage("zh");
    });

    await waitFor(() =>
      expect(trayHarness.applyTrayMenu.mock.calls.length).toBeGreaterThan(
        before,
      ),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.label).toBe(
        String(i18n.t("tray.toggle.show")),
      ),
    );
  });

  it("reflects only successful connections after a partially failed connect-all", async () => {
    sshSnapshot = snapshotWithRuntime("disconnected", "disconnected");
    sshConnectResult = {
      ...emptyBatch("connect"),
      success_count: 1,
      failed_count: 1,
      total_count: 2,
      failures: [
        { tunnel_id: "t2", tunnel_name: "Two", error: "auth failed" },
      ],
    };
    sshSnapshotAfterConnect = snapshotWithRuntime("connected", "disconnected");
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "tunnels-status")?.label).toContain(
        "0/2",
      ),
    );

    await fireTrayAction("connect-all");

    await waitFor(() =>
      expect(findModelItem(latestModel(), "tunnels-status")?.label).toContain(
        "1/2",
      ),
    );
    expect(findModelItem(latestModel(), "tunnels-status")?.label).not.toContain(
      "2/2",
    );
  });

  it("reflects only successful disconnections after a partially failed disconnect-all", async () => {
    sshSnapshot = snapshotWithRuntime("connected", "connected");
    sshDisconnectResult = {
      ...emptyBatch("disconnect"),
      success_count: 1,
      failed_count: 1,
      total_count: 2,
      failures: [
        { tunnel_id: "t2", tunnel_name: "Two", error: "stop failed" },
      ],
    };
    sshSnapshotAfterDisconnect = snapshotWithRuntime(
      "disconnected",
      "connected",
    );
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "tunnels-status")?.label).toContain(
        "2/2",
      ),
    );

    await fireTrayAction("disconnect-all");

    await waitFor(() =>
      expect(findModelItem(latestModel(), "tunnels-status")?.label).toContain(
        "1/2",
      ),
    );
  });

  it("rebuilds the model with new accelerator hints from tray-shortcuts-updated", async () => {
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.accelerator).toBe(
        "Alt+Space",
      ),
    );

    await triggerEvent("tray-shortcuts-updated", {
      main: "Cmd+K",
      quick: "Ctrl+Shift+L",
    });

    await waitFor(() =>
      expect(findModelItem(latestModel(), "toggle-window")?.accelerator).toBe(
        "Cmd+K",
      ),
    );
    expect(findModelItem(latestModel(), "quick-ai")?.accelerator).toBe(
      "Ctrl+Shift+L",
    );
  });

  it("does not register a trigger-sync listener", async () => {
    renderApp();

    await waitFor(() => expect(eventHandlers["refresh-counts"]).toBeDefined());

    expect(
      listenMock.mock.calls.some(([name]) => name === "trigger-sync"),
    ).toBe(false);
  });

  it("shows the installed active profile and role hosts on mount", async () => {
    activeModelsResult = appActiveMatrix("team-alpha", "gpt-6-astra");
    renderApp();

    await waitFor(() =>
      expect(
        findModelItem(latestModel(), "ai-workflow-role-backend"),
      ).toBeDefined(),
    );

    const model = latestModel();
    expect(findModelItem(model, "ai-workflow")?.label).toBe("AI WorkFlow");
    expect(findModelItem(model, "ai-workflow-profile")?.label).toBe(
      "Active profile: team-alpha",
    );
    expect(findModelItem(model, "ai-workflow-role-backend-codex")?.label).toBe(
      "Codex: gpt-6-astra · Reasoning: high",
    );
    expect(
      commandCallCount("ai_workflow_get_active_models"),
    ).toBeGreaterThan(0);
  });

  it("shows the empty state when no profile is active", async () => {
    activeModelsResult = null;
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ai-workflow-empty")).toBeDefined(),
    );
    expect(findModelItem(latestModel(), "ai-workflow-empty")?.label).toBe(
      "No active profile",
    );
  });

  it("refetches on a visibility change and surfaces unavailable, clearing stale rows", async () => {
    activeModelsResult = appActiveMatrix("team-alpha", "gpt-6-astra");
    renderApp();

    await waitFor(() =>
      expect(
        findModelItem(latestModel(), "ai-workflow-role-backend"),
      ).toBeDefined(),
    );

    activeModelsError = new Error("active models read failed");
    await triggerEvent("main-window-visibility-changed", false);

    await waitFor(() =>
      expect(
        findModelItem(latestModel(), "ai-workflow-unavailable"),
      ).toBeDefined(),
    );
    expect(
      findModelItem(latestModel(), "ai-workflow-unavailable")?.label,
    ).toBe("Unable to read active models");
    expect(
      findModelItem(latestModel(), "ai-workflow-role-backend"),
    ).toBeUndefined();
  });

  it("refetches after a successful profile activation event", async () => {
    activeModelsResult = appActiveMatrix("team-alpha", "gpt-6-astra");
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ai-workflow-profile")?.label).toBe(
        "Active profile: team-alpha",
      ),
    );

    activationReport = {
      active_profile: "team-beta",
      hosts: [],
      installations: [],
    };
    activeModelsResult = appActiveMatrix("team-beta", "gpt-6-beta");

    await act(async () => {
      await aiWorkflowProfiles.activateProfile("team-beta");
    });

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ai-workflow-profile")?.label).toBe(
        "Active profile: team-beta",
      ),
    );
    expect(
      findModelItem(latestModel(), "ai-workflow-role-backend-codex")?.label,
    ).toBe("Codex: gpt-6-beta · Reasoning: high");
  });

  it("polls active models every 60000ms even while hidden and stops on unmount", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      activeModelsResult = appActiveMatrix("team-alpha", "gpt-6-astra");
      isVisibleMock.mockResolvedValue(false);

      const view = renderApp();

      await waitFor(() =>
        expect(
          commandCallCount("ai_workflow_get_active_models"),
        ).toBeGreaterThan(0),
      );
      const afterMount = commandCallCount("ai_workflow_get_active_models");

      await act(async () => {
        vi.advanceTimersByTime(60000);
      });
      await waitFor(() =>
        expect(
          commandCallCount("ai_workflow_get_active_models"),
        ).toBeGreaterThan(afterMount),
      );

      view.unmount();
      const afterUnmount = commandCallCount("ai_workflow_get_active_models");
      await act(async () => {
        vi.advanceTimersByTime(180000);
      });
      expect(commandCallCount("ai_workflow_get_active_models")).toBe(
        afterUnmount,
      );
    } finally {
      vi.useRealTimers();
    }
  });

  // -------------------------------------------------------------------------
  // BLE Lock tray controls: one shared snapshot drives the submenu, every
  // action calls the same typed wrapper as the tool page, and failures are
  // localized instead of leaking the raw error.
  // -------------------------------------------------------------------------

  const BLE_LOCK_TRAY_IDS = [
    "ble-lock-status",
    "ble-lock-lock-now",
    "ble-lock-set-device",
    "ble-lock-unlock-rssi",
    "ble-lock-lock-rssi",
    "ble-lock-wake-on-proximity",
    "ble-lock-wake-without-unlocking",
    "ble-lock-pause-while-locked",
    "ble-lock-use-screensaver",
    "ble-lock-turn-off-display",
  ] as const;

  it("includes the ble-lock controls submenu from the shared runtime snapshot", async () => {
    renderApp();
    await waitForAppliedMenu();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-controls")).toBeDefined(),
    );
    const model = latestModel();
    for (const id of BLE_LOCK_TRAY_IDS) {
      expect(findModelItem(model, id), `${id} should exist`).toBeDefined();
    }
    expect(findModelItem(model, "ble-lock-status")?.enabled).toBe(false);
    expect(findModelItem(model, "ble-lock-wake-on-proximity")?.checked).toBe(
      false,
    );
  });

  it("locks now from the tray through the shared wrapper", async () => {
    renderApp();

    await fireActionAndExpectCommand("ble-lock-lock-now", "ble_lock_lock_now");
  });

  it("opens the ble-lock tool for Set Device", async () => {
    renderApp();
    await waitForAppliedMenu();
    await waitForSetActiveTab();

    const spy = installSetActiveTabSpy();
    const beforeShow = commandCallCount("show_main_window");

    await fireTrayAction("ble-lock-set-device");

    await waitFor(() =>
      expect(commandCallCount("show_main_window")).toBeGreaterThan(beforeShow),
    );
    expect(spy).toHaveBeenCalledWith("ble-lock");
  });

  it("saves a flipped toggle from the tray and refreshes the check mark", async () => {
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-wake-on-proximity")?.checked).toBe(
        false,
      ),
    );

    await fireTrayAction("ble-lock-wake-on-proximity");

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-wake-on-proximity")?.checked).toBe(
        true,
      ),
    );
    expect(commandCallCount("ble_lock_save_config")).toBeGreaterThan(0);
  });

  it("reflects a ble-lock-status-updated event in the tray check marks", async () => {
    renderApp();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-wake-on-proximity")?.checked).toBe(
        false,
      ),
    );

    await triggerEvent("ble-lock-status-updated", {
      ...bleLockStatus,
      config: { ...bleLockStatus.config, wake_on_proximity: true },
    });

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-wake-on-proximity")?.checked).toBe(
        true,
      ),
    );
  });

  it("keeps the latest BLE configuration when a tray toggle follows a status event changing non-menu fields", async () => {
    renderApp();
    await waitForAppliedMenu();

    // Wait until the shared runtime snapshot has reached the tray projection.
    const nearLabel = String(
      i18n.t("tray.bleLock.status", { status: i18n.t("bleLockPresence_near") }),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-status")?.label).toBe(
        nearLabel,
      ),
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    // A tool-page status event changes the persisted device, mode and timing
    // fields that the tray menu never displays. A subsequent tray toggle must
    // still save the full latest configuration, not a stale pre-change snapshot.
    const latestConfig = {
      ...bleLockStatus.config,
      device_id: "uuid-latest",
      device_name: "Latest Fob",
      mode: "passive",
      lock_delay_secs: 25,
      no_signal_timeout_secs: 45,
    };
    await triggerEvent("ble-lock-status-updated", {
      ...bleLockStatus,
      config: latestConfig,
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    await fireTrayAction("ble-lock-wake-on-proximity");

    const saveCalls = invokeMock.mock.calls.filter(
      ([name]) => name === "ble_lock_save_config",
    );
    expect(saveCalls.length).toBeGreaterThan(0);
    const savedConfig = (
      saveCalls.at(-1)![1] as { config: typeof bleLockStatus.config }
    ).config;
    expect(savedConfig).toEqual({
      ...latestConfig,
      wake_on_proximity: true,
    });
  });

  it("surfaces a localized ble-lock failure instead of the raw error", async () => {
    bleLockLockNowError = { code: "native_action_unavailable" };
    renderApp();
    await waitForAppliedMenu();

    await fireTrayAction("ble-lock-lock-now");

    const expected = String(i18n.t("bleLockError_native_action_unavailable"));
    await waitFor(() =>
      expect(document.body.textContent ?? "").toContain(expected),
    );
  });

  it("keeps BLE Lock native tray actions disabled and invokes no command on a Windows Tauri host", async () => {
    setNavigatorPlatform("Win32");
    renderApp();
    await waitForAppliedMenu();

    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-controls")).toBeDefined(),
    );

    // The unavailable platform is surfaced through a localized label, not a
    // raw enum or the generic running-status line.
    const unavailableLabel = String(i18n.t("bleLockPlatformUnavailable"));
    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-status")?.label).toBe(
        unavailableLabel,
      ),
    );

    // Every native action is disabled; navigation stays available.
    for (const id of [
      "ble-lock-lock-now",
      "ble-lock-unlock-rssi",
      "ble-lock-lock-rssi",
      "ble-lock-wake-on-proximity",
      "ble-lock-wake-without-unlocking",
      "ble-lock-pause-while-locked",
      "ble-lock-use-screensaver",
      "ble-lock-turn-off-display",
    ]) {
      expect(findModelItem(latestModel(), id)?.enabled, `${id} disabled`).toBe(
        false,
      );
    }
    expect(findModelItem(latestModel(), "ble-lock-set-device")?.enabled).toBe(
      true,
    );

    // No native invoke happens on the unsupported host.
    expect(
      invokeMock.mock.calls.filter(([name]) =>
        String(name).startsWith("ble_lock_"),
      ),
    ).toHaveLength(0);
  });

  it("does not reapply the native menu or re-render unrelated views for an unchanged ble-lock snapshot, yet still reflects a real presence change", async () => {
    renderApp();
    await waitForAppliedMenu();
    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-status")).toBeDefined(),
    );

    // Let the remaining mount-time queries (visibility, storage config) settle
    // so their work cannot be mistaken for work triggered by the events below.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    // A genuine presence change must still reach the native menu.
    const nearLabel = String(
      i18n.t("tray.bleLock.status", { status: i18n.t("bleLockPresence_near") }),
    );
    expect(findModelItem(latestModel(), "ble-lock-status")?.label).toBe(nearLabel);

    await triggerEvent("ble-lock-status-updated", {
      ...bleLockStatus,
      presence: "away",
    });

    const awayLabel = String(
      i18n.t("tray.bleLock.status", { status: i18n.t("bleLockPresence_away") }),
    );
    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-status")?.label).toBe(
        awayLabel,
      ),
    );

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    const menuApplies = trayHarness.applyTrayMenu.mock.calls.length;
    const launcherRenders = launcherRenderHarness.renders;

    // Re-publishing the same snapshot must not rebuild the native menu (which
    // creates unmanaged native resources) nor re-render an unrelated view.
    await triggerEvent("ble-lock-status-updated", {
      ...bleLockStatus,
      presence: "away",
    });
    await triggerEvent("ble-lock-status-updated", {
      ...bleLockStatus,
      presence: "away",
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(trayHarness.applyTrayMenu.mock.calls.length).toBe(menuApplies);
    expect(launcherRenderHarness.renders).toBe(launcherRenders);
  });

  it("keeps the applied menu and unrelated views stable across consecutive RSSI-only ble-lock updates", async () => {
    renderApp();
    await waitForAppliedMenu();
    await waitFor(() =>
      expect(findModelItem(latestModel(), "ble-lock-status")).toBeDefined(),
    );

    // Let the remaining mount-time queries (visibility, storage config) settle
    // so their work cannot be mistaken for work triggered by the events below.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    // A moving RSSI with unchanged presence and config is normal background
    // churn: only the reading changes, so neither the native menu nor an
    // unrelated view may be rebuilt for it.
    const menuApplies = trayHarness.applyTrayMenu.mock.calls.length;
    const launcherRenders = launcherRenderHarness.renders;

    const device = bleLockStatus.selected_device;
    for (const rssi of [-58, -61, -63, -59, -60]) {
      await triggerEvent("ble-lock-status-updated", {
        ...bleLockStatus,
        selected_device: { ...device, rssi },
        averaged_rssi: rssi,
      });
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
    }

    expect(trayHarness.applyTrayMenu.mock.calls.length).toBe(menuApplies);
    expect(launcherRenderHarness.renders).toBe(launcherRenders);
  });
});
