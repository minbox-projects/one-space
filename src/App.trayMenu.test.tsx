import { act, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher" />,
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
  "ai-assistants",
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
    aboutModalHarness.props.length = 0;

    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }) as unknown as typeof window.matchMedia;

    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: writeTextMock },
      configurable: true,
    });
    writeTextMock.mockClear();

    await i18n.changeLanguage("en");

    invokeMock.mockImplementation(async (command: string) => {
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

  it("applies the tray menu on mount", async () => {
    renderApp();

    await waitFor(() => expect(trayHarness.applyTrayMenu).toHaveBeenCalled());
    const model = latestModel();
    expect(findModelItem(model, "toggle-window")).toBeDefined();
    expect(findModelItem(model, "services")).toBeDefined();
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

  it("opens the quick assistant window from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand(
      "quick-assistant",
      "show_quick_assistant_window",
    );
  });

  it("opens the selection assistant window from the tray", async () => {
    renderApp();

    await fireActionAndExpectCommand(
      "selection-assistant",
      "show_selection_assistant_window",
    );
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
});
