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

const writeTextMock = vi.fn(async () => undefined);

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

describe("App tray menu integration", () => {
  let eventHandlers: Record<string, Array<(event: { payload?: unknown }) => unknown>>;
  let gatewayRunning: boolean;
  let gatewayStartError: Error | null;
  let sshConnectResult: BatchResult | null;

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
    sshConnectResult = null;

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
        case "api_gateway_status":
          return gatewayStatus();
        case "api_gateway_start":
          if (gatewayStartError) throw gatewayStartError;
          gatewayRunning = true;
          return gatewayStatus();
        case "api_gateway_stop":
          gatewayRunning = false;
          return gatewayStatus();
        case "protocol_router_status":
          return { running: false, enabled: false, port: 17689, route_count: 0 };
        case "protocol_router_start":
          return { running: true, enabled: true, port: 17689, route_count: 0 };
        case "protocol_router_stop":
          return { running: false, enabled: true, port: 17689, route_count: 0 };
        case "ssh_tunnels_snapshot":
          return SSH_SNAPSHOT;
        case "ssh_tunnels_connect_all":
          return sshConnectResult ?? emptyBatch("connect");
        case "ssh_tunnels_disconnect_all":
          return emptyBatch("disconnect");
        case "file_sharing_status":
          return {
            running: false,
            sessionId: null,
            address: null,
            port: null,
            shareUrl: null,
            startedAt: null,
            stoppedAt: null,
            files: [],
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

    await fireActionAndExpectCommand("gateway", "api_gateway_start");

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

    await fireActionAndExpectCommand("gateway", "api_gateway_stop");
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
    await triggerEvent("api-gateway-status-update", undefined);

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
});
