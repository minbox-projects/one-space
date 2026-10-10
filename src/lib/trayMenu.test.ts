import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { SUPPORTED_ROLES, type ProfileMatrix } from "@/lib/aiWorkflowProfiles";
import {
  buildTrayMenuModel,
  isAcceleratorHint,
  type TrayMenuNode,
  type TrayMenuState,
} from "@/lib/trayMenu";
import * as trayMenuModule from "@/lib/trayMenu";
import type { BleLockConfig, BleLockStatus } from "@/lib/bleLock";

type ItemNode = Extract<TrayMenuNode, { kind: "item" }>;

type ApplyTrayMenu = (
  model: TrayMenuNode[],
  onAction: (id: string) => void,
) => Promise<boolean>;

/** `applyTrayMenu` is the delivery contract; it does not exist before the applier lands. */
const applyTrayMenu = (
  trayMenuModule as unknown as { applyTrayMenu: ApplyTrayMenu }
).applyTrayMenu;

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

const nativeMenuMocks = vi.hoisted(() => {
  const menuNew = vi.fn();
  const menuItemNew = vi.fn();
  const checkMenuItemNew = vi.fn();
  const submenuNew = vi.fn();
  const predefinedMenuItemNew = vi.fn();
  const trayGetById = vi.fn();
  const traySetMenu = vi.fn();
  return {
    menuNew,
    menuItemNew,
    checkMenuItemNew,
    submenuNew,
    predefinedMenuItemNew,
    trayGetById,
    traySetMenu,
  };
});

vi.mock("@tauri-apps/api/menu", () => ({
  Menu: { new: nativeMenuMocks.menuNew },
  MenuItem: { new: nativeMenuMocks.menuItemNew },
  CheckMenuItem: { new: nativeMenuMocks.checkMenuItemNew },
  Submenu: { new: nativeMenuMocks.submenuNew },
  PredefinedMenuItem: { new: nativeMenuMocks.predefinedMenuItemNew },
}));

vi.mock("@tauri-apps/api/tray", () => ({
  TrayIcon: { getById: nativeMenuMocks.trayGetById },
}));

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
      "separator",
      "launcher",
      "ai-sessions",
      "ai-environments",
      "ai-gateway",
      "ai-usage",
      "ai-workflow",
      "more-pages",
      "ble-lock-controls",
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

  it("attaches a single-segment shortcut now that bare keys are valid", () => {
    const model = build({ shortcuts: { main: "F5", quick: "Space" } });
    expect(itemById(model, "toggle-window").accelerator).toBe("F5");
    expect(itemById(model, "quick-ai").accelerator).toBe("Space");
  });

  it("omits a Meta combination because Meta is not a modifier", () => {
    const model = build({
      shortcuts: { main: "Meta+A", quick: "Meta+Shift+B" },
    });
    expect(itemById(model, "toggle-window").accelerator).toBeUndefined();
    expect(itemById(model, "quick-ai").accelerator).toBeUndefined();
  });

  it.each([
    [""],
    [null],
    ["NotAKey+Whatever"],
    ["AltPlus"],
    ["Meta+A"],
    ["Alt+"],
  ])("omits an invalid shortcut value %p", (shortcut) => {
    const model = build({ shortcuts: { main: shortcut, quick: shortcut } });
    expect(itemById(model, "toggle-window").accelerator).toBeUndefined();
    expect(itemById(model, "quick-ai").accelerator).toBeUndefined();
  });

  it.each([
    // Modifier tokens followed by a named key.
    "Alt+Space",
    "Alt+Enter",
    "Alt+Shift+A",
    "Cmd+Q",
    "Command+Q",
    "Ctrl+Q",
    "Control+Q",
    "Option+Q",
    "Shift+A",
    "Super+A",
    "CmdOrCtrl+K",
    "CommandOrControl+K",
    "ctrl+shift+p",
    "command+space",
    "Alt+1",
    "Shift+F5",
    "Cmd+F24",
    // A single bare last segment is valid on its own.
    "F5",
    "F24",
    "Space",
    "Enter",
    "Tab",
    "enter",
    "A",
    "1",
    "PageDown",
    "ArrowLeft",
  ])("accepts the valid accelerator hint %s", (value) => {
    expect(isAcceleratorHint(value)).toBe(true);
  });

  it.each([
    [""],
    [null],
    [undefined],
    ["NotAKey+Whatever"],
    ["AltPlus"],
    ["Meta+A"],
    ["Alt+"],
    ["+A"],
    ["Alt+Shift+"],
    ["Alt+Plus"],
    ["Alt+AB"],
    ["F25"],
    ["Alt+Shift"],
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

type NativeItemOptions = {
  id?: string;
  text?: string;
  enabled?: boolean;
  checked?: boolean;
  accelerator?: string;
  action?: (id: string) => void;
  items?: unknown[];
};

/** Every option object handed to a native item constructor, tagged with its native kind. */
function createdNativeItems(): Array<NativeItemOptions & { kind: string }> {
  const created: Array<NativeItemOptions & { kind: string }> = [];
  for (const call of nativeMenuMocks.menuItemNew.mock.calls) {
    created.push({ kind: "MenuItem", ...(call[0] as NativeItemOptions) });
  }
  for (const call of nativeMenuMocks.checkMenuItemNew.mock.calls) {
    created.push({ kind: "CheckMenuItem", ...(call[0] as NativeItemOptions) });
  }
  for (const call of nativeMenuMocks.submenuNew.mock.calls) {
    created.push({ kind: "Submenu", ...(call[0] as NativeItemOptions) });
  }
  return created;
}

function nativeItemById(id: string) {
  return createdNativeItems().find((item) => item.id === id);
}

describe("applyTrayMenu native menu", () => {
  beforeEach(() => {
    nativeMenuMocks.menuNew.mockReset().mockResolvedValue({ native: "menu" });
    nativeMenuMocks.menuItemNew
      .mockReset()
      .mockImplementation(async (options: NativeItemOptions) => ({
        kind: "MenuItem",
        ...options,
      }));
    nativeMenuMocks.checkMenuItemNew
      .mockReset()
      .mockImplementation(async (options: NativeItemOptions) => ({
        kind: "CheckMenuItem",
        ...options,
      }));
    nativeMenuMocks.submenuNew
      .mockReset()
      .mockImplementation(async (options: NativeItemOptions) => ({
        kind: "Submenu",
        ...options,
      }));
    nativeMenuMocks.predefinedMenuItemNew
      .mockReset()
      .mockResolvedValue({ kind: "PredefinedMenuItem" });
    nativeMenuMocks.trayGetById.mockReset().mockResolvedValue({
      id: "main",
      setMenu: nativeMenuMocks.traySetMenu,
    });
    nativeMenuMocks.traySetMenu.mockReset().mockResolvedValue(undefined);
  });

  it("maps model node kinds with ids, labels, enabled and checked", async () => {
    const model = build({
      windowVisible: true,
      gateway: { running: true, port: 17688 },
      router: { running: false, port: 17689 },
      tunnels: { connected: 2, total: 5 },
      sharing: { running: true, fileCount: 3 },
    });

    await expect(applyTrayMenu(model, vi.fn())).resolves.toBe(true);

    const toggle = nativeItemById("toggle-window");
    expect(toggle).toMatchObject({ kind: "MenuItem", enabled: true });
    expect(toggle?.text).toBe(itemById(model, "toggle-window").label);
    expect(toggle?.checked).toBeUndefined();

    expect(nativeItemById("gateway")).toMatchObject({
      kind: "CheckMenuItem",
      checked: true,
    });
    expect(nativeItemById("router")).toMatchObject({
      kind: "CheckMenuItem",
      checked: false,
    });
    expect(nativeItemById("more-pages")).toMatchObject({ kind: "Submenu" });
    expect(nativeItemById("services")).toMatchObject({ kind: "Submenu" });
    expect(nativeItemById("tunnels-status")).toMatchObject({
      kind: "MenuItem",
      enabled: false,
    });

    // Four top-level separators plus three separators inside the Services submenu.
    expect(nativeMenuMocks.predefinedMenuItemNew).toHaveBeenCalledTimes(7);
    for (const call of nativeMenuMocks.predefinedMenuItemNew.mock.calls) {
      expect(call[0]).toMatchObject({ item: "Separator" });
    }
  });

  it("wires every leaf item action to onAction with its own id", async () => {
    const onAction = vi.fn();
    await applyTrayMenu(build(), onAction);

    const leaves = createdNativeItems().filter(
      (item) => item.kind !== "Submenu" && typeof item.action === "function",
    );
    expect(leaves.length).toBeGreaterThan(0);

    for (const leaf of leaves) {
      onAction.mockClear();
      leaf.action?.(leaf.id as string);
      expect(onAction).toHaveBeenCalledTimes(1);
      expect(onAction).toHaveBeenCalledWith(leaf.id);
    }
  });

  it("sets the created menu on the 'main' tray icon", async () => {
    await applyTrayMenu(build(), vi.fn());

    expect(nativeMenuMocks.trayGetById).toHaveBeenCalledWith("main");
    const menu = await nativeMenuMocks.menuNew.mock.results[0].value;
    expect(nativeMenuMocks.traySetMenu).toHaveBeenCalledWith(menu);
  });

  it("returns false without throwing when the main tray icon is missing", async () => {
    nativeMenuMocks.trayGetById.mockResolvedValue(null);

    await expect(applyTrayMenu(build(), vi.fn())).resolves.toBe(false);
    expect(nativeMenuMocks.traySetMenu).not.toHaveBeenCalled();
  });

  it("returns false without throwing when the native tray API is unavailable", async () => {
    nativeMenuMocks.trayGetById.mockRejectedValue(new Error("tauri unavailable"));

    await expect(applyTrayMenu(build(), vi.fn())).resolves.toBe(false);
  });

  it("returns false without throwing when native menu creation fails", async () => {
    nativeMenuMocks.menuNew.mockRejectedValue(new Error("no menu api"));

    await expect(applyTrayMenu(build(), vi.fn())).resolves.toBe(false);
    expect(nativeMenuMocks.traySetMenu).not.toHaveBeenCalled();
  });

  it("retries an item without its accelerator when native creation rejects", async () => {
    let rejectedAccelerator = false;
    nativeMenuMocks.menuItemNew.mockImplementation(
      async (options: NativeItemOptions) => {
        if (!rejectedAccelerator && options.accelerator !== undefined) {
          rejectedAccelerator = true;
          throw new Error("invalid accelerator");
        }
        return { kind: "MenuItem", ...options };
      },
    );

    await expect(applyTrayMenu(build(), vi.fn())).resolves.toBe(true);

    const toggleAttempts = nativeMenuMocks.menuItemNew.mock.calls
      .map((call) => call[0] as NativeItemOptions)
      .filter((options) => options.id === "toggle-window");
    expect(toggleAttempts.length).toBe(2);
    expect(toggleAttempts[0]?.accelerator).toBe("Alt+Space");
    expect(toggleAttempts[1]?.accelerator).toBeUndefined();

    // The rest of the menu is still applied and attached to the tray.
    expect(nativeItemById("quick-ai")).toBeDefined();
    expect(nativeItemById("services")).toMatchObject({ kind: "Submenu" });
    expect(nativeMenuMocks.traySetMenu).toHaveBeenCalled();
  });
});

describe("registry-derived tray entries", () => {
  it("keeps every registry tray tool in the More Pages submenu", async () => {
    const registry = await loadRegistry();
    const trayTools = registry.listToolboxTools("tray");
    expect(trayTools.length).toBeGreaterThan(0);

    const morePages = submenuOf(build(), "more-pages");
    for (const tool of trayTools) {
      expect(itemById(morePages, tool.id)).toBeDefined();
    }
  });

  it("keeps the Notes and Snippets entries present and enabled", async () => {
    const registry = await loadRegistry();
    const trayIds = registry.listToolboxTools("tray").map((tool) => tool.id);
    expect(trayIds).toEqual(expect.arrayContaining(["notes", "snippets"]));

    const morePages = submenuOf(build(), "more-pages");
    for (const id of ["notes", "snippets"]) {
      expect(itemById(morePages, id).enabled).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------
// AI WorkFlow tray submenu: currently installed model + reasoning effort for
// every subagent role across the supported hosts, plus loading / empty /
// unavailable states. The exact user-visible labels and ids are the frozen
// contract for the implementation.
// ---------------------------------------------------------------------------

type AiWorkflowTrayState = {
  profile: ProfileMatrix | null;
  unavailable?: boolean;
};

function fixedT(language: "en" | "zh") {
  return i18n.getFixedT(language) as (
    key: string,
    options?: Record<string, unknown>,
  ) => string;
}

function buildWithAiWorkflow(
  aiWorkflow: AiWorkflowTrayState | undefined,
  t: (key: string, options?: Record<string, unknown>) => string,
): TrayMenuNode[] {
  const state: TrayMenuState & { aiWorkflow?: AiWorkflowTrayState } = {
    ...BASE_STATE,
    aiWorkflow,
  };
  return buildTrayMenuModel(state, t);
}

function activeProfileMatrix(): ProfileMatrix {
  return {
    name: "team-alpha",
    rows: [
      {
        role: "backend",
        codex: { model: "gpt-6-astra", reasoning_effort: "high" },
      },
      {
        role: "frontend",
        claude: { model: "claude-sonnet-4", reasoning_effort: "medium" },
      },
      {
        role: "test",
        opencode: { model: "opencode/deepseek-v4", reasoning_effort: "low" },
      },
      { role: "researcher", codex: { model: "", reasoning_effort: "" } },
    ],
  };
}

describe("AI WorkFlow tray submenu", () => {
  it("renders the active profile header and every role host leaf", () => {
    const model = buildWithAiWorkflow(
      { profile: activeProfileMatrix() },
      fixedT("en"),
    );

    expect(itemById(model, "ai-workflow").label).toBe("AI WorkFlow");
    const header = itemById(model, "ai-workflow-profile");
    expect(header.enabled).toBe(false);
    expect(header.label).toBe("Active profile: team-alpha");

    for (const role of SUPPORTED_ROLES) {
      const roleNode = itemById(model, `ai-workflow-role-${role}`);
      expect(roleNode.kind).toBe("item");
      expect(roleNode.submenu, `role ${role} must expose a submenu`).toBeDefined();
    }

    expect(itemById(model, "ai-workflow-role-backend-codex")).toMatchObject({
      enabled: false,
      label: "Codex: gpt-6-astra · Reasoning: high",
    });
    expect(itemById(model, "ai-workflow-role-frontend-claude").label).toBe(
      "Claude Code: claude-sonnet-4 · Reasoning: medium",
    );
    expect(itemById(model, "ai-workflow-role-test-opencode").label).toBe(
      "OpenCode: opencode/deepseek-v4 · Reasoning: low",
    );
  });

  it("renders Not set for absent and blank host values", () => {
    const model = buildWithAiWorkflow(
      { profile: activeProfileMatrix() },
      fixedT("en"),
    );

    expect(itemById(model, "ai-workflow-role-backend-claude").label).toBe(
      "Claude Code: Not set · Reasoning: Not set",
    );
    expect(itemById(model, "ai-workflow-role-researcher-codex").label).toBe(
      "Codex: Not set · Reasoning: Not set",
    );
    expect(itemById(model, "ai-workflow-role-researcher-opencode").label).toBe(
      "OpenCode: Not set · Reasoning: Not set",
    );
  });

  it("renders the localized Chinese header and host labels", () => {
    const model = buildWithAiWorkflow(
      { profile: activeProfileMatrix() },
      fixedT("zh"),
    );

    expect(itemById(model, "ai-workflow").label).toBe("AI WorkFlow");
    expect(itemById(model, "ai-workflow-profile").label).toBe(
      "当前方案：team-alpha",
    );
    expect(itemById(model, "ai-workflow-role-backend-codex").label).toBe(
      "Codex：gpt-6-astra · 推理强度：high",
    );
    expect(itemById(model, "ai-workflow-role-test-opencode").label).toBe(
      "OpenCode：opencode/deepseek-v4 · 推理强度：low",
    );
    expect(itemById(model, "ai-workflow-role-backend-claude").label).toBe(
      "Claude Code：未设置 · 推理强度：未设置",
    );
  });

  it("renders the loading placeholder while the state is undefined", () => {
    const model = buildWithAiWorkflow(undefined, fixedT("en"));
    expect(findItem(model, "ai-workflow-loading")).toBeDefined();
  });

  it("renders the empty state for a null active profile", () => {
    const enModel = buildWithAiWorkflow({ profile: null }, fixedT("en"));
    expect(itemById(enModel, "ai-workflow-empty").label).toBe("No active profile");
    expect(findItem(enModel, "ai-workflow-role-backend")).toBeUndefined();

    const zhModel = buildWithAiWorkflow({ profile: null }, fixedT("zh"));
    expect(itemById(zhModel, "ai-workflow-empty").label).toBe("未激活方案");
  });

  it("renders unavailable and supersedes stale role rows", () => {
    const enModel = buildWithAiWorkflow(
      { profile: activeProfileMatrix(), unavailable: true },
      fixedT("en"),
    );
    expect(itemById(enModel, "ai-workflow-unavailable").label).toBe(
      "Unable to read active models",
    );
    expect(findItem(enModel, "ai-workflow-role-backend")).toBeUndefined();

    const zhModel = buildWithAiWorkflow(
      { profile: activeProfileMatrix(), unavailable: true },
      fixedT("zh"),
    );
    expect(itemById(zhModel, "ai-workflow-unavailable").label).toBe(
      "无法读取当前模型",
    );
  });
});

// ---------------------------------------------------------------------------
// BLE Lock tray submenu: one shared snapshot drives the status line, the manual
// lock, the Set Device navigation target, two independent RSSI threshold lists
// and five optional behavior check marks. Ids, the threshold range and the
// checked semantics are the frozen contract for the implementation.
// ---------------------------------------------------------------------------

const BLE_LOCK_THRESHOLDS = [
  -95, -90, -85, -80, -75, -70, -65, -60, -55, -50, -45, -40, -35, -30,
] as const;

const BLE_LOCK_CONTROL_IDS = [
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

function bleLockConfig(overrides: Partial<BleLockConfig> = {}): BleLockConfig {
  return {
    schema_version: 1,
    device_id: null,
    device_name: null,
    mode: "active",
    unlock_rssi: null,
    lock_rssi: null,
    lock_delay_secs: 10,
    no_signal_timeout_secs: 30,
    wake_on_proximity: false,
    wake_without_unlocking: false,
    pause_now_playing: false,
    use_screensaver_to_lock: false,
    sleep_display: false,
    ...overrides,
  };
}

function bleLockStatus(overrides: Partial<BleLockStatus> = {}): BleLockStatus {
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
    config: bleLockConfig(),
    last_error: null,
    ...overrides,
  };
}

function itemIds(nodes: TrayMenuNode[]): string[] {
  return nodes
    .filter((node): node is ItemNode => node.kind === "item")
    .map((node) => node.id);
}

function checkedIds(nodes: TrayMenuNode[]): string[] {
  return nodes
    .filter(
      (node): node is ItemNode =>
        node.kind === "item" && node.checked === true,
    )
    .map((node) => node.id);
}

function submenuItemIds(model: TrayMenuNode[], id: string): string[] {
  return itemIds(submenuOf(model, id));
}

describe("BLE Lock tray submenu", () => {
  it("adds the exact ble-lock-controls submenu with the ten frozen entries", () => {
    const model = build({ bleLock: bleLockStatus() });
    expect(submenuItemIds(model, "ble-lock-controls")).toEqual([
      ...BLE_LOCK_CONTROL_IDS,
    ]);
  });

  it("keeps the status line disabled and every actionable entry enabled", () => {
    const model = build({ bleLock: bleLockStatus() });
    expect(itemById(model, "ble-lock-status").enabled).toBe(false);
    for (const id of BLE_LOCK_CONTROL_IDS) {
      if (id === "ble-lock-status") continue;
      expect(itemById(model, id).enabled, `${id} should be enabled`).toBe(true);
    }
  });

  it("renders a status line from the tray.bleLock.status key", () => {
    const model = build({ bleLock: bleLockStatus({ presence: "away" }) }, OPTIONS_TRANSLATOR);
    const status = itemById(model, "ble-lock-status");
    expect(status.enabled).toBe(false);
    expect(status.label).toContain("tray.bleLock.status");
  });

  it("lists Disable plus every 5 dBm threshold in both RSSI submenus", () => {
    const model = build({ bleLock: bleLockStatus() });
    for (const prefix of ["ble-lock-unlock-rssi", "ble-lock-lock-rssi"]) {
      expect(submenuItemIds(model, prefix)).toEqual([
        `${prefix}-disable`,
        ...BLE_LOCK_THRESHOLDS.map((value) => `${prefix}-${value}`),
      ]);
    }
  });

  it("checks only the configured unlock threshold independently of the lock threshold", () => {
    const model = build({
      bleLock: bleLockStatus({
        config: bleLockConfig({ unlock_rssi: -60, lock_rssi: -80 }),
      }),
    });

    expect(checkedIds(submenuOf(model, "ble-lock-unlock-rssi"))).toEqual([
      "ble-lock-unlock-rssi--60",
    ]);
    expect(checkedIds(submenuOf(model, "ble-lock-lock-rssi"))).toEqual([
      "ble-lock-lock-rssi--80",
    ]);
  });

  it("checks Disable in each RSSI submenu when that action is disabled", () => {
    const model = build({
      bleLock: bleLockStatus({
        config: bleLockConfig({ unlock_rssi: -60, lock_rssi: null }),
      }),
    });

    expect(checkedIds(submenuOf(model, "ble-lock-unlock-rssi"))).toEqual([
      "ble-lock-unlock-rssi--60",
    ]);
    expect(checkedIds(submenuOf(model, "ble-lock-lock-rssi"))).toEqual([
      "ble-lock-lock-rssi-disable",
    ]);
  });

  it("checks each optional behavior toggle from the shared snapshot", () => {
    const model = build({
      bleLock: bleLockStatus({
        config: bleLockConfig({
          wake_on_proximity: true,
          wake_without_unlocking: false,
          pause_now_playing: true,
          use_screensaver_to_lock: true,
          sleep_display: false,
        }),
      }),
    });

    expect(itemById(model, "ble-lock-wake-on-proximity").checked).toBe(true);
    expect(itemById(model, "ble-lock-wake-without-unlocking").checked).toBe(false);
    expect(itemById(model, "ble-lock-pause-while-locked").checked).toBe(true);
    expect(itemById(model, "ble-lock-use-screensaver").checked).toBe(true);
    expect(itemById(model, "ble-lock-turn-off-display").checked).toBe(false);
  });

  it("keeps the status line and Lock Now independent of a missing device", () => {
    const model = build({
      bleLock: bleLockStatus({ selected_device: null, monitoring: false }),
    });
    expect(itemById(model, "ble-lock-status").enabled).toBe(false);
    expect(itemById(model, "ble-lock-lock-now").enabled).toBe(true);
  });
});

