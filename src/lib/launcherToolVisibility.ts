import { listToolboxTools } from "@/toolbox/registry";

export const LAUNCHER_TOOL_VISIBILITY_KEY = "onespace_launcher_tool_visibility";
export const LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT =
  "onespace:launcher-tool-visibility-updated";

export type LauncherToolId = string;

export type LauncherToolVisibility = Record<string, boolean>;

const DEFAULT_VISIBILITY: LauncherToolVisibility = Object.fromEntries(
  listToolboxTools("launcher-quick").map((tool) => [tool.id, tool.defaultVisible]),
);

export function readLauncherToolVisibility(): LauncherToolVisibility {
  const visibility: LauncherToolVisibility = { ...DEFAULT_VISIBILITY };
  try {
    const raw = localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY);
    if (!raw) return visibility;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return visibility;
    }

    for (const toolId of Object.keys(DEFAULT_VISIBILITY)) {
      const value = (parsed as Record<string, unknown>)[toolId];
      if (typeof value === "boolean") {
        visibility[toolId] = value;
      }
    }
    return visibility;
  } catch {
    return visibility;
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
