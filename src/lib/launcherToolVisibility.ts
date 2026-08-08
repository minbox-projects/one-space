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
  | "md5Encryption"
  | "short-link"
  | "file-sharing"
  | "ai-work-flow";

export type LauncherToolVisibility = Record<LauncherToolId, boolean>;

const DEFAULT_VISIBILITY: LauncherToolVisibility = {
  bookmarks: true,
  cloud: true,
  ssh: true,
  "ssh-tunnels": true,
  "protocol-router": true,
  "random-password": true,
  "json-parser": true,
  md5Encryption: true,
  "short-link": true,
  "file-sharing": true,
  "ai-work-flow": true,
};

export function readLauncherToolVisibility(): LauncherToolVisibility {
  try {
    const raw = localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY);
    if (!raw) return { ...DEFAULT_VISIBILITY };
    const parsed: unknown = JSON.parse(raw);
    const visibility = { ...DEFAULT_VISIBILITY };
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return visibility;
    }

    for (const toolId of Object.keys(DEFAULT_VISIBILITY) as LauncherToolId[]) {
      const value = (parsed as Record<string, unknown>)[toolId];
      if (typeof value === "boolean") {
        visibility[toolId] = value;
      }
    }
    return visibility;
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
