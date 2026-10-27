export const SKILLS_AUTO_UPDATED_EVENT = "onespace:skills-auto-updated";
export const USAGE_TODAY_TOKENS_REFRESH_MS = 5 * 60 * 1000;

export const BLE_LOCK_TRAY_TOGGLES: Record<
  string,
  | "wake_on_proximity"
  | "wake_without_unlocking"
  | "pause_now_playing"
  | "use_screensaver_to_lock"
  | "sleep_display"
> = {
  "ble-lock-wake-on-proximity": "wake_on_proximity",
  "ble-lock-wake-without-unlocking": "wake_without_unlocking",
  "ble-lock-pause-while-locked": "pause_now_playing",
  "ble-lock-use-screensaver": "use_screensaver_to_lock",
  "ble-lock-turn-off-display": "sleep_display",
};

export const TRAY_NAVIGATION_IDS = new Set([
  "launcher",
  "workspaces",
  "ai-sessions",
  "ai-environments",
  "ai-usage",
  "ai-gateway",
  "ai-news",
  "more-tools",
  "skills",
  "subagents",
  "mcp-servers",
  "ssh",
  "ssh-tunnels",
  "protocol-router",
  "file-sharing",
  "snippets",
  "bookmarks",
  "notes",
  "mail",
  "documentation",
]);
