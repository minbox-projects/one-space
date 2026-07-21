export const LAUNCHER_TOOL_VISIBILITY_KEY = "onespace_launcher_tool_visibility";

export type LauncherToolId = "ssh" | "ssh-tunnels" | "protocol-router";

export type LauncherToolVisibility = Record<LauncherToolId, boolean>;

const DEFAULT_VISIBILITY: LauncherToolVisibility = {
  ssh: true,
  "ssh-tunnels": true,
  "protocol-router": true,
};

export function readLauncherToolVisibility(): LauncherToolVisibility {
  try {
    const raw = localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY);
    if (!raw) return { ...DEFAULT_VISIBILITY };
    const parsed = JSON.parse(raw);
    return {
      ssh: typeof parsed.ssh === "boolean" ? parsed.ssh : DEFAULT_VISIBILITY.ssh,
      "ssh-tunnels":
        typeof parsed["ssh-tunnels"] === "boolean"
          ? parsed["ssh-tunnels"]
          : DEFAULT_VISIBILITY["ssh-tunnels"],
      "protocol-router":
        typeof parsed["protocol-router"] === "boolean"
          ? parsed["protocol-router"]
          : DEFAULT_VISIBILITY["protocol-router"],
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