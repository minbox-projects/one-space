import { listToolboxTools } from "@/toolbox/registry";
import type { InternalTarget, LauncherItem, LauncherItemInput } from "./types";

export const SEEDED_MARKER = "onespace_launcher_seeded_v1";

export const DEFAULT_LAUNCHER_ITEMS: LauncherItemInput[] = [
  { name: "VS Code", type: "app", target: 'open -a "Visual Studio Code"' },
  { name: "Google Chrome", type: "app", target: 'open -a "Google Chrome"' },
  { name: "System Settings", type: "app", target: 'open -a "System Settings"' },
];

/** Launcher-internal toolbox entries are owned by the registry descriptors. */
export const INTERNAL_TOOLBOX_TARGETS = new Map<string, InternalTarget>(
  listToolboxTools("launcher-internal").map(
    (tool): [string, InternalTarget] => [
      tool.id,
      { id: tool.id, labelKey: tool.labelKey, fallback: tool.id },
    ],
  ),
);

export function toolboxInternalTarget(id: string): InternalTarget {
  const target = INTERNAL_TOOLBOX_TARGETS.get(id);
  if (!target) {
    throw new Error(
      `launcher-internal toolbox target is not registered: ${id}`,
    );
  }
  return target;
}

export const INTERNAL_TARGETS: InternalTarget[] = [
  { id: "launcher", labelKey: "launcher", fallback: "Launcher" },
  {
    id: "ai-sessions",
    labelKey: "aiSessions",
    fallback: "AI Terminal Sessions",
  },
  {
    id: "ai-environments",
    labelKey: "aiEnvironments",
    fallback: "AI Environments",
  },
  { id: "skills", labelKey: "skills", fallback: "Skills" },
  { id: "mcp-servers", labelKey: "mcpServers", fallback: "MCP Servers" },
  { id: "ssh", labelKey: "sshServers", fallback: "SSH Servers" },
  { id: "ssh-tunnels", labelKey: "sshTunnels", fallback: "SSH Tunnels" },
  { id: "protocol-router", labelKey: "protocolRouter", fallback: "Protocol Router" },
  { id: "ai-gateway", labelKey: "aiGateway", fallback: "AI Gateway" },
  { id: "file-sharing", labelKey: "fileSharing", fallback: "File Sharing" },
  toolboxInternalTarget("snippets"),
  { id: "bookmarks", labelKey: "bookmarks", fallback: "Bookmarks" },
  toolboxInternalTarget("notes"),
  { id: "mail", labelKey: "mail", fallback: "Mail" },
  { id: "settings", labelKey: "settings", fallback: "Settings" },
  { id: "documentation", labelKey: "usageDocs", fallback: "Documentation" },
  {
    id: "ai-workflow-model-switcher",
    labelKey: "aiWorkflowModelSwitcher",
    fallback: "AI Workflow Model Switcher",
  },
];

export const LAUNCHER_TYPE_ORDER: LauncherItem["type"][] = [
  "app",
  "script",
  "url",
  "folder",
  "internal",
];
