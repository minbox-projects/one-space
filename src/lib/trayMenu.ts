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
  "alt",
  "shift",
  "cmd",
  "command",
  "ctrl",
  "control",
  "option",
  "super",
  "meta",
  "cmdorctrl",
  "commandorcontrol",
]);

export function isAcceleratorHint(value: string | null | undefined): boolean {
  if (typeof value !== "string") return false;
  const segments = value.split("+");
  if (segments.length < 2) return false;
  const key = segments[segments.length - 1];
  if (key === "" || MODIFIER_KEYS.has(key.toLowerCase())) return false;
  for (let index = 0; index < segments.length - 1; index += 1) {
    const modifier = segments[index];
    if (modifier === "" || !MODIFIER_KEYS.has(modifier.toLowerCase())) return false;
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
