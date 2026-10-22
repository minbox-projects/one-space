import type { MessageSeverity, MessageTarget } from "@/lib/messages";

export type FilterTab = "all" | "unread" | "alerts";

export interface MessageFilterState {
  tab: FilterTab;
  searchQuery: string;
  source: string;
  severity: MessageSeverity | "all";
}

export interface MessageCenterProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onNavigate: (target: MessageTarget) => void;
}

export const SOURCE_LABELS: Record<string, string> = {
  ai_news: "AI News",
  ssh_tunnels: "SSH Tunnels",
  sync: "Sync",
  system: "System",
  updater: "Updates",
  launcher: "Launcher",
  ai_environments: "AI Environments",
  skills: "Skills",
  subagents: "Subagents",
  mcp_servers: "MCP",
  settings: "Settings",
  mail: "Mail",
  backup: "Backup",
  workspaces: "Workspaces",
  content: "Content",
  ai_gateway: "AI Gateway",
};

export function getSourceLabel(source: string): string {
  return SOURCE_LABELS[source] || source || "System";
}
