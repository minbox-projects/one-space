import { describe, expect, it } from "vitest";
import i18n from "@/i18n";
import {
  buildTrayMenuModel,
  isAcceleratorHint,
  type TrayMenuNode,
  type TrayMenuState,
} from "@/lib/trayMenu";

type ItemNode = Extract<TrayMenuNode, { kind: "item" }>;

/** Wording-independent translator: every label is just its key. */
const KEY_TRANSLATOR = (key: string) => key;

/** Stub that surfaces interpolation options so port/count assertions stay wording-free. */
const OPTIONS_TRANSLATOR = (
  key: string,
  options?: Record<string, unknown>,
) =>
  options && Object.keys(options).length > 0
    ? `${key} ${JSON.stringify(options)}`
    : key;

const BASE_STATE: TrayMenuState = {
  windowVisible: false,
  gateway: { running: false, port: 17688 },
  router: { running: false, port: 17689 },
  tunnels: { connected: 0, total: 0 },
  sharing: { running: false, fileCount: 0 },
  shortcuts: { main: "Alt+Space", quick: "Alt+Shift+A" },
};

function build(
  overrides: Partial<TrayMenuState> = {},
  t: (key: string, options?: Record<string, unknown>) => string = KEY_TRANSLATOR,
): TrayMenuNode[] {
  return buildTrayMenuModel({ ...BASE_STATE, ...overrides }, t);
}

function findItem(nodes: TrayMenuNode[], id: string): ItemNode | undefined {
  for (const node of nodes) {
    if (node.kind !== "item") continue;
    if (node.id === id) return node;
    if (node.submenu) {
      const nested = findItem(node.submenu, id);
      if (nested) return nested;
    }
  }
  return undefined;
}

function itemById(nodes: TrayMenuNode[], id: string): ItemNode {
  const item = findItem(nodes, id);
  if (!item) throw new Error(`tray menu item not found: ${id}`);
  return item;
}

function submenuOf(nodes: TrayMenuNode[], id: string): TrayMenuNode[] {
  const item = itemById(nodes, id);
  if (!item.submenu) throw new Error(`tray menu item has no submenu: ${id}`);
  return item.submenu;
}

/** Map a node list to a stable token list: item ids and the literal `separator`. */
function nodeKeys(nodes: TrayMenuNode[]): string[] {
  return nodes.map((node) => (node.kind === "separator" ? "separator" : node.id));
}

describe("tray menu structure", () => {
  it("builds the exact top-level id and kind order", () => {
    expect(nodeKeys(build())).toEqual([
      "toggle-window",
      "quick-ai",
      "quick-assistant",
      "selection-assistant",
      "separator",
      "launcher",
      "ai-sessions",
      "ai-assistants",
      "ai-environments",
      "api-gateway",
      "ai-usage",
      "more-pages",
      "separator",
      "services",
      "separator",
      "settings",
      "check-for-updates",
      "about",
      "separator",
      "quit",
    ]);
  });

  it("builds the exact More Pages child order and enables every child", () => {
    const children = submenuOf(build(), "more-pages");
    expect(nodeKeys(children)).toEqual([
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
    ]);
    for (const child of children) {
      expect(child.kind).toBe("item");
      if (child.kind === "item") {
        expect(child.enabled, `child ${child.id} should be enabled`).toBe(true);
      }
    }
  });

  it("builds the exact Services child order", () => {
    const children = submenuOf(build(), "services");
    expect(nodeKeys(children)).toEqual([
      "gateway",
      "router",
      "separator",
      "tunnels-status",
      "connect-all",
      "disconnect-all",
      "separator",
      "sharing-status",
      "stop-sharing",
      "separator",
      "sync",
      "copy-address",
    ]);
  });
});

describe("dynamic toggle-window label", () => {
  it("differs between a hidden and a visible window and is never empty", () => {
    const hidden = itemById(
      build({ windowVisible: false }, OPTIONS_TRANSLATOR),
      "toggle-window",
    ).label;
    const visible = itemById(
      build({ windowVisible: true }, OPTIONS_TRANSLATOR),
      "toggle-window",
    ).label;
    expect(hidden).not.toBe(visible);
    expect(hidden.length).toBeGreaterThan(0);
    expect(visible.length).toBeGreaterThan(0);
  });
});

describe("accelerator hints", () => {
  it("attaches valid main and quick shortcuts to the first two items only", () => {
    const model = build({
      shortcuts: { main: "Alt+Space", quick: "Alt+Shift+A" },
    });
    expect(itemById(model, "toggle-window").accelerator).toBe("Alt+Space");
    expect(itemById(model, "quick-ai").accelerator).toBe("Alt+Shift+A");
    expect(itemById(model, "launcher").accelerator).toBeUndefined();
  });

  it.each([
    [""],
    [null],
    ["NotAKey+Whatever"],
    ["AltPlus"],
    ["Alt+"],
  ])("omits an invalid shortcut value %p", (shortcut) => {
    const model = build({ shortcuts: { main: shortcut, quick: shortcut } });
    expect(itemById(model, "toggle-window").accelerator).toBeUndefined();
    expect(itemById(model, "quick-ai").accelerator).toBeUndefined();
  });

  it.each([
    "Alt+Space",
    "Alt+Shift+A",
    "Cmd+Q",
    "Command+Q",
    "Ctrl+Q",
    "Control+Q",
    "Option+Q",
    "Shift+A",
    "Super+A",
    "Meta+A",
    "CmdOrCtrl+K",
    "CommandOrControl+K",
    "ctrl+shift+p",
    "Alt+1",
    "Shift+F5",
  ])("accepts the valid accelerator hint %s", (value) => {
    expect(isAcceleratorHint(value)).toBe(true);
  });

  it.each([
    [""],
    [null],
    [undefined],
    ["NotAKey+Whatever"],
    ["AltPlus"],
    ["Alt+"],
    ["+A"],
    ["Alt+Shift+"],
  ])("rejects the invalid accelerator hint %p", (value) => {
    expect(isAcceleratorHint(value)).toBe(false);
  });
});

describe("Services state rules", () => {
  it("checks gateway and router from their running state", () => {
    const stopped = build({
      gateway: { running: false, port: 17688 },
      router: { running: false, port: 17689 },
    });
    expect(itemById(stopped, "gateway").checked).toBe(false);
    expect(itemById(stopped, "router").checked).toBe(false);

    const running = build({
      gateway: { running: true, port: 17688 },
      router: { running: true, port: 17689 },
    });
    expect(itemById(running, "gateway").checked).toBe(true);
    expect(itemById(running, "router").checked).toBe(true);
  });

  it("shows the service port only while the service runs", () => {
    const running = build(
      { gateway: { running: true, port: 17688 }, router: { running: true, port: 17689 } },
      OPTIONS_TRANSLATOR,
    );
    expect(itemById(running, "gateway").label).toContain("17688");
    expect(itemById(running, "router").label).toContain("17689");

    const stopped = build(
      { gateway: { running: false, port: 17688 }, router: { running: false, port: 17689 } },
      OPTIONS_TRANSLATOR,
    );
    expect(itemById(stopped, "gateway").label).not.toContain("17688");
    expect(itemById(stopped, "router").label).not.toContain("17689");
  });

  it("builds a running gateway without a port without failing", () => {
    const model = build(
      { gateway: { running: true, port: undefined }, router: { running: true } },
      OPTIONS_TRANSLATOR,
    );
    expect(itemById(model, "gateway").checked).toBe(true);
    expect(itemById(model, "gateway").label.length).toBeGreaterThan(0);
    expect(itemById(model, "router").label.length).toBeGreaterThan(0);
  });

  it("disables Connect All and Disconnect All when there are zero tunnels", () => {
    const model = build({ tunnels: { connected: 0, total: 0 } });
    expect(itemById(model, "connect-all").enabled).toBe(false);
    expect(itemById(model, "disconnect-all").enabled).toBe(false);
  });

  it("enables Connect All only while some tunnels remain disconnected", () => {
    const noneConnected = build({ tunnels: { connected: 0, total: 5 } });
    expect(itemById(noneConnected, "connect-all").enabled).toBe(true);
    expect(itemById(noneConnected, "disconnect-all").enabled).toBe(false);

    const partial = build({ tunnels: { connected: 2, total: 5 } });
    expect(itemById(partial, "connect-all").enabled).toBe(true);
    expect(itemById(partial, "disconnect-all").enabled).toBe(true);
  });

  it("disables Connect All and enables Disconnect All when every tunnel is connected", () => {
    const model = build({ tunnels: { connected: 5, total: 5 } });
    expect(itemById(model, "connect-all").enabled).toBe(false);
    expect(itemById(model, "disconnect-all").enabled).toBe(true);
  });

  it("keeps the tunnels status line disabled and carrying connected/total", () => {
    const model = build({ tunnels: { connected: 2, total: 5 } }, OPTIONS_TRANSLATOR);
    const status = itemById(model, "tunnels-status");
    expect(status.enabled).toBe(false);
    expect(status.label).toContain("2");
    expect(status.label).toContain("5");
  });

  it("enables Stop Sharing only while sharing runs and keeps the status line disabled", () => {
    const idle = build({ sharing: { running: false, fileCount: 0 } });
    expect(itemById(idle, "sharing-status").enabled).toBe(false);
    expect(itemById(idle, "stop-sharing").enabled).toBe(false);

    const active = build({ sharing: { running: true, fileCount: 3 } });
    expect(itemById(active, "sharing-status").enabled).toBe(false);
    expect(itemById(active, "stop-sharing").enabled).toBe(true);
  });

  it("keeps Sync Now enabled and Copy API Address gated on the gateway", () => {
    const stopped = build({ gateway: { running: false, port: 17688 } });
    expect(itemById(stopped, "sync").enabled).toBe(true);
    expect(itemById(stopped, "copy-address").enabled).toBe(false);

    const running = build({ gateway: { running: true, port: 17688 } });
    expect(itemById(running, "sync").enabled).toBe(true);
    expect(itemById(running, "copy-address").enabled).toBe(true);
  });
});

describe("bilingual labels", () => {
  it("renders non-empty toggle-window, quit and services labels that differ per language", () => {
    const zhT = i18n.getFixedT("zh") as (
      key: string,
      options?: Record<string, unknown>,
    ) => string;
    const enT = i18n.getFixedT("en") as (
      key: string,
      options?: Record<string, unknown>,
    ) => string;

    const zh = build({ windowVisible: false }, zhT);
    const en = build({ windowVisible: false }, enT);

    for (const id of ["toggle-window", "quit", "services"]) {
      const zhLabel = itemById(zh, id).label;
      const enLabel = itemById(en, id).label;
      expect(zhLabel.length, `${id} zh label should be non-empty`).toBeGreaterThan(0);
      expect(enLabel.length, `${id} en label should be non-empty`).toBeGreaterThan(0);
      expect(zhLabel, `${id} label should differ between zh and en`).not.toBe(enLabel);
    }
  });
});
