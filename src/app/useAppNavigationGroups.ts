import { useMemo } from "react";
import type { TFunction } from "i18next";
import {
  Rocket,
  Terminal,
  FolderOpen,
  Sparkles,
  Cpu,
  Bot,
  BarChart3,
  Network,
} from "lucide-react";
import { listToolboxTools, resolveToolboxText } from "@/toolbox/registry";
import { isMoreToolsTab } from "@/lib/navigation";
import { formatCompactCount } from "./helpers";
import { MCPIcon } from "./icons";
import type { AppI18n, AppCounts } from "./types";
import type { GatewayStatus } from "@/lib/aiGateway";

type UseAppNavigationGroupsParams = {
  counts: AppCounts;
  i18n: AppI18n;
  moreToolsLabel: string;
  t: TFunction;
  aiGatewayHeaderStatus: GatewayStatus | null;
  usageTodayTokens: number;
  activeTab: string;
};

export function useAppNavigationGroups({
  counts,
  i18n,
  moreToolsLabel,
  t,
  aiGatewayHeaderStatus,
  usageTodayTokens,
  activeTab,
}: UseAppNavigationGroupsParams) {
  const navigationGroups = useMemo(
    () => [
      {
        id: "core",
        label: i18n.language === "zh" ? "工作台" : "Workbench",
        items: [
          {
            id: "launcher",
            name: t("launcher"),
            icon: Rocket,
            count: counts.launcher,
          },
          {
            id: "workspaces",
            name: t("workspaces", "Workspaces"),
            icon: FolderOpen,
            count: counts.workspaces,
          },
        ],
      },
      {
        id: "capabilities",
        label: i18n.language === "zh" ? "AI 能力" : "AI Capabilities",
        items: [
          {
            id: "ai-environments",
            name: t("cliEnvironments", "AI Terminal Environments"),
            icon: Cpu,
            count: counts.environments,
          },
          {
            id: "ai-sessions",
            name: t("aiSessions"),
            icon: Terminal,
            count: counts.sessions,
          },
          {
            id: "ai-gateway",
            name: t("aiGateway", "AI Gateway"),
            icon: Network,
            count: aiGatewayHeaderStatus?.provider_count ?? 0,
          },
          {
            id: "ai-usage",
            name: t("aiUsageStatsMenu", "AI Usage Stats"),
            icon: BarChart3,
            count: usageTodayTokens,
            countText: formatCompactCount(usageTodayTokens),
          },
        ],
      },
      {
        id: "extensions",
        label: i18n.language === "zh" ? "AI 扩展" : "AI Extensions",
        items: [
          {
            id: "skills",
            name: t("skills", "Skills"),
            icon: Sparkles,
            count: counts.skills,
          },
          {
            id: "mcp-servers",
            name: "MCP Servers",
            icon: MCPIcon,
            count: counts.mcpServers,
          },
          {
            id: "subagents",
            name: t("subagents", "Subagents"),
            icon: Bot,
            count: counts.subagents,
          },
        ],
      },
      {
        id: "tools",
        label: i18n.language === "zh" ? "工具" : "Tools",
        items: [
          ...listToolboxTools("sidebar").map((tool) => ({
            id: tool.id,
            name: resolveToolboxText(tool.labelText, tool.labelKey, t),
            icon: tool.icon,
            count:
              tool.id === "snippets"
                ? counts.snippets
                : tool.id === "notes"
                  ? counts.notes
                  : undefined,
          })),
          {
            id: "more-tools",
            name: moreToolsLabel,
            icon: Rocket,
          },
        ],
      },
    ],
    [counts, i18n.language, moreToolsLabel, t, aiGatewayHeaderStatus, usageTodayTokens],
  );

  const isNavigationItemActive = (itemId: string) => {
    if (itemId === "more-tools") {
      return isMoreToolsTab(activeTab);
    }
    return activeTab === itemId;
  };

  return { navigationGroups, isNavigationItemActive };
}
