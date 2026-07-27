export const LAUNCHER_TOOL_VISIBILITY_KEY = "onespace_launcher_tool_visibility";
export const LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT =
  "onespace:launcher-tool-visibility-updated";

export type LauncherToolId =
  | "bookmarks"
  | "cloud"
  | "ssh"
  | "ssh-tunnels"
  | "protocol-router"
  | "random-password"
  | "json-parser"
  | "ai-request-capture";

export type LauncherToolVisibility = Record<LauncherToolId, boolean>;

const DEFAULT_VISIBILITY: LauncherToolVisibility = {
  bookmarks: true,
  cloud: true,
  ssh: true,
  "ssh-tunnels": true,
  "protocol-router": true,
  "random-password": true,
  "json-parser": true,
  "ai-request-capture": true,
};

export function readLauncherToolVisibility(): LauncherToolVisibility {
  try {
    const raw = localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY);
    if (!raw) return { ...DEFAULT_VISIBILITY };
    const parsed = JSON.parse(raw);
    return {
      bookmarks:
        typeof parsed.bookmarks === "boolean"
          ? parsed.bookmarks
          : DEFAULT_VISIBILITY.bookmarks,
      cloud:
        typeof parsed.cloud === "boolean"
          ? parsed.cloud
          : DEFAULT_VISIBILITY.cloud,
      ssh: typeof parsed.ssh === "boolean" ? parsed.ssh : DEFAULT_VISIBILITY.ssh,
      "ssh-tunnels":
        typeof parsed["ssh-tunnels"] === "boolean"
          ? parsed["ssh-tunnels"]
          : DEFAULT_VISIBILITY["ssh-tunnels"],
      "protocol-router":
        typeof parsed["protocol-router"] === "boolean"
          ? parsed["protocol-router"]
          : DEFAULT_VISIBILITY["protocol-router"],
      "random-password":
        typeof parsed["random-password"] === "boolean"
          ? parsed["random-password"]
          : DEFAULT_VISIBILITY["random-password"],
      "json-parser":
        typeof parsed["json-parser"] === "boolean"
          ? parsed["json-parser"]
          : DEFAULT_VISIBILITY["json-parser"],
      "ai-request-capture":
        typeof parsed["ai-request-capture"] === "boolean"
          ? parsed["ai-request-capture"]
          : DEFAULT_VISIBILITY["ai-request-capture"],
    };
  } catch {
    return { ...DEFAULT_VISIBILITY };
  }
}

export function writeLauncherToolVisibility(
  visibility: LauncherToolVisibility,
): void {
  localStorage.setItem(
    LAUNCHER_TOOL_VISIBILITY_KEY,
    JSON.stringify(visibility),
  );
  window.dispatchEvent(new Event(LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT));
}

export function setLauncherToolVisible(
  toolId: LauncherToolId,
  visible: boolean,
): void {
  const current = readLauncherToolVisibility();
  current[toolId] = visible;
  writeLauncherToolVisibility(current);
}

export function isLauncherToolVisible(toolId: LauncherToolId): boolean {
  const visibility = readLauncherToolVisibility();
  return visibility[toolId] ?? DEFAULT_VISIBILITY[toolId];
}
