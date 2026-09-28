import { resolveToolboxNavigationAlias } from "@/toolbox/registry";

export type SmartWorkspaceSection =
  | "conversations"
  | "assistants"
  | "automations"
  | "models";

export type JttParserTab = "jt808" | "jt809" | "jt1078" | "hex";

export type MoreToolsSection =
  | "bookmarks"
  | "ssh"
  | "ssh-tunnels"
  | "protocol-router"
  | "random-password"
  | "json-parser"
  | "md5-encryption"
  | "short-link"
  | "file-sharing"
  | "jtt-data-parser"
  | "ai-workflow-model-switcher";

export type ResolvedNavigationTarget = {
  tab: string;
  smartWorkspaceSection?: SmartWorkspaceSection;
  moreToolsSection?: MoreToolsSection;
  jttParserTab?: JttParserTab;
};

const SMART_WORKSPACE_ALIAS_MAP: Record<string, SmartWorkspaceSection> = {
  "ai-assistants": "conversations",
  "ai-assistants-library": "assistants",
  "ai-automations": "automations",
  "ai-model-center": "models",
};

export function resolveNavigationTarget(target: string): ResolvedNavigationTarget {
  if (target in SMART_WORKSPACE_ALIAS_MAP) {
    return {
      tab: "ai-assistants",
      smartWorkspaceSection:
        SMART_WORKSPACE_ALIAS_MAP[target as keyof typeof SMART_WORKSPACE_ALIAS_MAP],
    };
  }

  const toolboxTarget = resolveToolboxNavigationAlias(target);
  if (toolboxTarget) {
    if (toolboxTarget.moreToolsSection) {
      return {
        tab: "more-tools",
        moreToolsSection: toolboxTarget.moreToolsSection as MoreToolsSection,
        ...(toolboxTarget.jttParserTab
          ? { jttParserTab: toolboxTarget.jttParserTab }
          : {}),
      };
    }
    return { tab: toolboxTarget.tab };
  }

  return { tab: target };
}

export function isSmartWorkspaceTab(tab: string) {
  return tab in SMART_WORKSPACE_ALIAS_MAP;
}

export function isMoreToolsTab(tab: string) {
  if (tab === "more-tools") return true;
  return resolveToolboxNavigationAlias(tab)?.tab === "more-tools";
}
