export type SmartWorkspaceSection =
  | "conversations"
  | "assistants"
  | "automations"
  | "models";

export type MoreToolsSection =
  | "bookmarks"
  | "cloud"
  | "backup"
  | "notes"
  | "snippets"
  | "ssh"
  | "ssh-tunnels"
  | "protocol-router"
  | "ai-routing-gateway"
  | "random-password"
  | "json-parser"
  | "md5-encryption"
  | "short-link"
  | "file-sharing";

export type ResolvedNavigationTarget = {
  tab: string;
  smartWorkspaceSection?: SmartWorkspaceSection;
  moreToolsSection?: MoreToolsSection;
};

const SMART_WORKSPACE_ALIAS_MAP: Record<string, SmartWorkspaceSection> = {
  "ai-assistants": "conversations",
  "ai-assistants-library": "assistants",
  "ai-automations": "automations",
  "ai-model-center": "models",
};

const MORE_TOOLS_ALIAS_MAP: Record<string, MoreToolsSection> = {
  bookmarks: "bookmarks",
  cloud: "cloud",
  backup: "backup",
  notes: "notes",
  snippets: "snippets",
  ssh: "ssh",
  ["ssh-tunnels"]: "ssh-tunnels",
  ["protocol-router"]: "protocol-router",
  ["ai-routing-gateway"]: "ai-routing-gateway",
  ["random-password"]: "random-password",
  ["json-parser"]: "json-parser",
  ["md5-encryption"]: "md5-encryption",
  ["short-link"]: "short-link",
  ["file-sharing"]: "file-sharing",
};

export function normalizeLegacyTabTarget(target: string) {
  if (
    target === "agents" ||
    target === "schedules" ||
    target === "ai-assistant"
  ) {
    return "ai-assistants";
  }
  return target;
}

export function resolveNavigationTarget(target: string): ResolvedNavigationTarget {
  const normalizedTarget = normalizeLegacyTabTarget(target);

  if (normalizedTarget in SMART_WORKSPACE_ALIAS_MAP) {
    return {
      tab: "ai-assistants",
      smartWorkspaceSection:
        SMART_WORKSPACE_ALIAS_MAP[normalizedTarget as keyof typeof SMART_WORKSPACE_ALIAS_MAP],
    };
  }

  if (normalizedTarget in MORE_TOOLS_ALIAS_MAP) {
    return {
      tab: "more-tools",
      moreToolsSection:
        MORE_TOOLS_ALIAS_MAP[normalizedTarget as keyof typeof MORE_TOOLS_ALIAS_MAP],
    };
  }

  return { tab: normalizedTarget };
}

export function isSmartWorkspaceTab(tab: string) {
  return tab in SMART_WORKSPACE_ALIAS_MAP;
}

export function isMoreToolsTab(tab: string) {
  return tab === "more-tools" || tab in MORE_TOOLS_ALIAS_MAP;
}
