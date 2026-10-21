import { resolveToolboxNavigationAlias } from "@/toolbox/registry";

export type JttParserTab = "jt808" | "jt809" | "jt1078" | "hex";

export type MoreToolsSection =
  | "bookmarks"
  | "ssh"
  | "ssh-tunnels"
  | "protocol-router"
  | "random-password"
  | "json-parser"
  | "jwt-inspector"
  | "curl-playground"
  | "md5-encryption"
  | "short-link"
  | "file-sharing"
  | "jtt-data-parser"
  | "ai-workflow-model-switcher"
  | "ble-lock"
  | "port-tunnel";

export type ResolvedNavigationTarget = {
  tab: string;
  moreToolsSection?: MoreToolsSection;
  jttParserTab?: JttParserTab;
  sshTunnelTab?: string;
};

export function resolveNavigationTarget(target: string): ResolvedNavigationTarget {
  const toolboxTarget = resolveToolboxNavigationAlias(target);
  if (toolboxTarget) {
    if (toolboxTarget.moreToolsSection) {
      return {
        tab: "more-tools",
        moreToolsSection: toolboxTarget.moreToolsSection as MoreToolsSection,
        ...(toolboxTarget.jttParserTab
          ? { jttParserTab: toolboxTarget.jttParserTab }
          : {}),
        ...(toolboxTarget.sshTunnelTab
          ? { sshTunnelTab: toolboxTarget.sshTunnelTab }
          : {}),
      };
    }
    return { tab: toolboxTarget.tab };
  }

  return { tab: target };
}

export function isMoreToolsTab(tab: string) {
  if (tab === "more-tools") return true;
  return resolveToolboxNavigationAlias(tab)?.tab === "more-tools";
}
