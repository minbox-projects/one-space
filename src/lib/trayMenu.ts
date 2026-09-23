import {
  CheckMenuItem,
  Menu,
  MenuItem,
  PredefinedMenuItem,
  Submenu,
} from "@tauri-apps/api/menu";
import { TrayIcon } from "@tauri-apps/api/tray";

export type TrayMenuSeparator = { kind: "separator" };

export interface TrayMenuItem {
  kind: "item";
  id: string;
  label: string;
  enabled: boolean;
  checked?: boolean;
  accelerator?: string;
  submenu?: TrayMenuNode[];
}

export type TrayMenuNode = TrayMenuSeparator | TrayMenuItem;

export interface TrayServiceState {
  running: boolean;
  port?: number;
}

export interface TrayMenuState {
  windowVisible: boolean;
  gateway: TrayServiceState;
  router: TrayServiceState;
  tunnels: { connected: number; total: number };
  sharing: { running: boolean; fileCount: number };
  shortcuts: { main: string | null | undefined; quick: string | null | undefined };
}

export type TrayTranslate = (key: string, options?: Record<string, unknown>) => string;

const MODIFIER_KEYS = new Set([
  "command",
  "cmd",
  "control",
  "ctrl",
  "alt",
  "option",
  "shift",
  "super",
  "cmdorctrl",
  "commandorcontrol",
]);

const NAMED_KEYS = new Set([
  "space",
  "enter",
  "tab",
  "escape",
  "backspace",
  "delete",
  "insert",
  "home",
  "end",
  "pageup",
  "pagedown",
  "arrowup",
  "arrowdown",
  "arrowleft",
  "arrowright",
]);

function isFunctionKey(segment: string): boolean {
  const match = /^f(\d{1,2})$/i.exec(segment);
  if (!match) return false;
  const index = Number(match[1]);
  return index >= 1 && index <= 24;
}

function isValidLastSegment(segment: string): boolean {
  if (/^[A-Za-z0-9]$/.test(segment)) return true;
  const lower = segment.toLowerCase();
  return NAMED_KEYS.has(lower) || isFunctionKey(lower);
}

export function isAcceleratorHint(value: string | null | undefined): boolean {
  if (typeof value !== "string") return false;
  const segments = value.split("+");
  const last = segments[segments.length - 1];
  if (!isValidLastSegment(last)) return false;
  for (let index = 0; index < segments.length - 1; index += 1) {
    if (!MODIFIER_KEYS.has(segments[index].toLowerCase())) return false;
  }
  return true;
}

function acceleratorHint(value: string | null | undefined): string | undefined {
  return isAcceleratorHint(value) && typeof value === "string" ? value : undefined;
}

function separator(): TrayMenuSeparator {
  return { kind: "separator" };
}

function menuItem(
  id: string,
  label: string,
  extra: {
    enabled?: boolean;
    checked?: boolean;
    accelerator?: string;
    submenu?: TrayMenuNode[];
  } = {},
): TrayMenuItem {
  const node: TrayMenuItem = {
    kind: "item",
    id,
    label,
    enabled: extra.enabled ?? true,
  };
  if (extra.checked !== undefined) node.checked = extra.checked;
  if (extra.accelerator !== undefined) node.accelerator = extra.accelerator;
  if (extra.submenu !== undefined) node.submenu = extra.submenu;
  return node;
}

function buildMorePagesMenu(t: TrayTranslate): TrayMenuNode[] {
  return [
    menuItem("workspaces", t("tray.workspaces")),
    menuItem("mcp-servers", t("tray.mcpServers")),
    menuItem("skills", t("tray.skills")),
    menuItem("subagents", t("tray.subagents")),
    menuItem("ssh", t("tray.sshServers")),
    menuItem("ssh-tunnels", t("tray.sshTunnels")),
    menuItem("protocol-router", t("tray.protocolRouter")),
    menuItem("file-sharing", t("tray.fileSharing")),
    menuItem("ai-news", t("tray.aiNews")),
    menuItem("bookmarks", t("tray.bookmarks")),
    menuItem("mail", t("tray.mail")),
    menuItem("snippets", t("tray.snippets")),
    menuItem("notes", t("tray.notes")),
    menuItem("documentation", t("tray.documentation")),
    menuItem("more-tools", t("tray.moreTools")),
  ];
}

function serviceLabel(
  service: TrayServiceState,
  runningKey: string,
  stoppedKey: string,
  t: TrayTranslate,
): string {
  if (!service.running) return t(stoppedKey);
  return service.port !== undefined
    ? t(runningKey, { port: service.port })
    : t(runningKey);
}

function buildServicesMenu(state: TrayMenuState, t: TrayTranslate): TrayMenuNode[] {
  const sharingLabel = state.sharing.running
    ? t("tray.services.sharing.running", { files: state.sharing.fileCount })
    : t("tray.services.sharing.stopped");

  return [
    menuItem(
      "gateway",
      serviceLabel(
        state.gateway,
        "tray.services.gateway.running",
        "tray.services.gateway.stopped",
        t,
      ),
      { checked: state.gateway.running },
    ),
    menuItem(
      "router",
      serviceLabel(
        state.router,
        "tray.services.router.running",
        "tray.services.router.stopped",
        t,
      ),
      { checked: state.router.running },
    ),
    separator(),
    menuItem(
      "tunnels-status",
      t("tray.services.tunnels.status", {
        connected: state.tunnels.connected,
        total: state.tunnels.total,
      }),
      { enabled: false },
    ),
    menuItem("connect-all", t("tray.services.tunnels.connectAll"), {
      enabled: state.tunnels.connected < state.tunnels.total,
    }),
    menuItem("disconnect-all", t("tray.services.tunnels.disconnectAll"), {
      enabled: state.tunnels.connected > 0,
    }),
    separator(),
    menuItem("sharing-status", sharingLabel, { enabled: false }),
    menuItem("stop-sharing", t("tray.services.sharing.stop"), {
      enabled: state.sharing.running,
    }),
    separator(),
    menuItem("sync", t("tray.services.sync")),
    menuItem("copy-address", t("tray.services.copyAddress"), {
      enabled: state.gateway.running,
    }),
  ];
}

export function buildTrayMenuModel(state: TrayMenuState, t: TrayTranslate): TrayMenuNode[] {
  const mainAccelerator = acceleratorHint(state.shortcuts.main);
  const quickAccelerator = acceleratorHint(state.shortcuts.quick);

  return [
    menuItem(
      "toggle-window",
      state.windowVisible ? t("tray.toggle.hide") : t("tray.toggle.show"),
      { accelerator: mainAccelerator },
    ),
    menuItem("quick-ai", t("tray.quickAi"), { accelerator: quickAccelerator }),
    menuItem("quick-assistant", t("tray.quickAssistant")),
    menuItem("selection-assistant", t("tray.selectionAssistant")),
    separator(),
    menuItem("launcher", t("tray.launcher")),
    menuItem("ai-sessions", t("tray.aiSessions")),
    menuItem("ai-assistants", t("tray.aiAssistants")),
    menuItem("ai-environments", t("tray.aiEnvironments")),
    menuItem("api-gateway", t("tray.apiGateway")),
    menuItem("ai-usage", t("tray.aiUsage")),
    menuItem("more-pages", t("tray.morePages"), {
      submenu: buildMorePagesMenu(t),
    }),
    separator(),
    menuItem("services", t("tray.services"), {
      submenu: buildServicesMenu(state, t),
    }),
    separator(),
    menuItem("settings", t("tray.settings")),
    menuItem("check-for-updates", t("tray.checkForUpdates")),
    menuItem("about", t("tray.about")),
    separator(),
    menuItem("quit", t("tray.quit")),
  ];
}

export type TrayMenuActionHandler = (id: string) => void;

type NativeTrayMenuItem =
  | Awaited<ReturnType<typeof MenuItem.new>>
  | Awaited<ReturnType<typeof CheckMenuItem.new>>
  | Awaited<ReturnType<typeof PredefinedMenuItem.new>>
  | Awaited<ReturnType<typeof Submenu.new>>;

async function createLeafItem(
  node: TrayMenuItem,
  onAction: TrayMenuActionHandler,
): Promise<NativeTrayMenuItem | null> {
  const action = () => onAction(node.id);
  const build = (accelerator?: string) => {
    const options = {
      id: node.id,
      text: node.label,
      enabled: node.enabled,
      ...(accelerator !== undefined ? { accelerator } : {}),
      action,
    };
    return node.checked !== undefined
      ? CheckMenuItem.new({ ...options, checked: node.checked })
      : MenuItem.new(options);
  };
  try {
    return await build(node.accelerator);
  } catch {
    if (node.accelerator === undefined) return null;
    try {
      return await build(undefined);
    } catch {
      return null;
    }
  }
}

async function buildNativeItems(
  nodes: TrayMenuNode[],
  onAction: TrayMenuActionHandler,
): Promise<NativeTrayMenuItem[]> {
  const items: NativeTrayMenuItem[] = [];
  for (const node of nodes) {
    if (node.kind === "separator") {
      items.push(await PredefinedMenuItem.new({ item: "Separator" }));
      continue;
    }
    if (node.submenu) {
      items.push(
        await Submenu.new({
          id: node.id,
          text: node.label,
          enabled: node.enabled,
          items: await buildNativeItems(node.submenu, onAction),
        }),
      );
      continue;
    }
    const item = await createLeafItem(node, onAction);
    if (item) items.push(item);
  }
  return items;
}

/**
 * Build the native tray menu from the pure model and attach it to the `main`
 * tray icon. Fails soft (`false`) when the Tauri menu/tray APIs are unavailable
 * so the app never crashes outside a Tauri runtime.
 */
export async function applyTrayMenu(
  model: TrayMenuNode[],
  onAction: TrayMenuActionHandler,
): Promise<boolean> {
  try {
    const items = await buildNativeItems(model, onAction);
    const menu = await Menu.new({ items });
    const tray = await TrayIcon.getById("main");
    if (!tray) return false;
    await tray.setMenu(menu);
    return true;
  } catch {
    return false;
  }
}
