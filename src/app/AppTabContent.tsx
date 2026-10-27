import { AiSessions } from "@/components/AiSessions";
import { Workspaces } from "@/components/Workspaces";
import { AiEnvironments } from "@/components/AiEnvironments";
import { AiUsageStats } from "@/components/AiUsageStats";
import { AiGateway } from "@/components/AiGateway";
import { Skills } from "@/components/Skills";
import { Subagents } from "@/components/Subagents";
import { MCPServers } from "@/components/MCPServers";
import { Mail } from "@/components/Mail";
import { AiNews } from "@/components/AiNews";
import { Launcher } from "@/components/Launcher";
import { MoreToolsHub } from "@/components/MoreToolsHub";
import { Notes } from "@/components/Notes";
import { SettingsView } from "@/components/SettingsView";
import { Snippets } from "@/components/Snippets";
import { Documentation } from "@/components/Documentation";
import { FishPond } from "@/components/FishPond";
import { AppErrorBoundary } from "@/components/AppErrorBoundary";
import type {
  CapabilityTargetTab,
  WorkspaceCapabilityContext,
} from "@/components/workspaceCapabilityContext";
import type { JttParserTab, MoreToolsSection } from "@/lib/navigation";

type AppTabContentProps = {
  activeTab: string;
  mountedTabs: Set<string>;
  navigateToTab: (tab: string) => void;
  handleNavigateToCapability: (
    targetTab: CapabilityTargetTab,
    context: WorkspaceCapabilityContext,
  ) => void;
  workspaceCapabilityNavigation: {
    targetTab: CapabilityTargetTab;
    context: WorkspaceCapabilityContext;
  } | null;
  clearWorkspaceCapabilityNavigation: () => void;
  handleSelectMoreTool: (tool: MoreToolsSection) => void;
  handleMoreToolsBack: () => void;
  moreToolsSection: MoreToolsSection | null;
  moreToolsReturnTab: "launcher" | "more-tools";
  jttParserTab: JttParserTab | null;
  sshTunnelTab: string | null;
  sshTunnelNavigationNonce: number;
  settingsInitialTab: string;
  previousTab: string;
  setActiveTab: (tab: string) => void;
  setSettingsInitialTab: (tab: string) => void;
  scheduleLoadCounts: (delayMs?: number) => void;
};

export function AppTabContent({
  activeTab,
  mountedTabs,
  navigateToTab,
  handleNavigateToCapability,
  workspaceCapabilityNavigation,
  clearWorkspaceCapabilityNavigation,
  handleSelectMoreTool,
  handleMoreToolsBack,
  moreToolsSection,
  moreToolsReturnTab,
  jttParserTab,
  sshTunnelTab,
  sshTunnelNavigationNonce,
  settingsInitialTab,
  previousTab,
  setActiveTab,
  setSettingsInitialTab,
  scheduleLoadCounts,
}: AppTabContentProps) {
  const shouldRenderTab = (tabId: string) =>
    activeTab === tabId || mountedTabs.has(tabId);
  return (
    <div className="h-full relative">
      {shouldRenderTab("launcher") && (
        <div className={activeTab === "launcher" ? "h-full" : "hidden"}>
          <Launcher isVisible={activeTab === "launcher"} />
        </div>
      )}
      {shouldRenderTab("workspaces") && (
        <div className={activeTab === "workspaces" ? "h-full" : "hidden"}>
          <AppErrorBoundary label="工作空间" resetKey={activeTab}>
            <Workspaces
              isVisible={activeTab === "workspaces"}
              onNavigateToCapability={handleNavigateToCapability}
            />
          </AppErrorBoundary>
        </div>
      )}
      {shouldRenderTab("ai-sessions") && (
        <div className={activeTab === "ai-sessions" ? "h-full" : "hidden"}>
          <AiSessions
            isVisible={activeTab === "ai-sessions"}
            onNavigate={(tab, hash) => {
              navigateToTab(tab);
              if (hash) window.location.hash = hash;
            }}
          />
        </div>
      )}
      {shouldRenderTab("ai-environments") && (
        <div
          className={activeTab === "ai-environments" ? "h-full" : "hidden"}
        >
          <AiEnvironments isVisible={activeTab === "ai-environments"} />
        </div>
      )}
      {shouldRenderTab("ai-gateway") && (
        <div className={activeTab === "ai-gateway" ? "h-full" : "hidden"}>
          <AppErrorBoundary label="AI 网关" resetKey={activeTab}>
            <AiGateway isVisible={activeTab === "ai-gateway"} />
          </AppErrorBoundary>
        </div>
      )}
      {shouldRenderTab("ai-usage") && (
        <div className={activeTab === "ai-usage" ? "h-full" : "hidden"}>
          <AiUsageStats isVisible={activeTab === "ai-usage"} />
        </div>
      )}
      {shouldRenderTab("ai-news") && (
        <div className={activeTab === "ai-news" ? "h-full" : "hidden"}>
          <AiNews isVisible={activeTab === "ai-news"} />
        </div>
      )}
      {shouldRenderTab("skills") && (
        <div className={activeTab === "skills" ? "h-full" : "hidden"}>
          <Skills
            isVisible={activeTab === "skills"}
            initialEntry={
              workspaceCapabilityNavigation?.targetTab === "skills"
                ? workspaceCapabilityNavigation.context.entry
                : undefined
            }
            onConsumeInitialEntry={clearWorkspaceCapabilityNavigation}
          />
        </div>
      )}
      {shouldRenderTab("subagents") && (
        <div className={activeTab === "subagents" ? "h-full" : "hidden"}>
          <Subagents
            isVisible={activeTab === "subagents"}
            initialEntry={
              workspaceCapabilityNavigation?.targetTab === "subagents"
                ? workspaceCapabilityNavigation.context.entry
                : undefined
            }
            onConsumeInitialEntry={clearWorkspaceCapabilityNavigation}
          />
        </div>
      )}
      {shouldRenderTab("mcp-servers") && (
        <div className={activeTab === "mcp-servers" ? "h-full" : "hidden"}>
          <MCPServers
            isVisible={activeTab === "mcp-servers"}
            workspaceContext={
              workspaceCapabilityNavigation?.targetTab === "mcp-servers"
                ? workspaceCapabilityNavigation.context
                : undefined
            }
            onDismissWorkspaceContext={clearWorkspaceCapabilityNavigation}
          />
        </div>
      )}
      {shouldRenderTab("snippets") && (
        <div className={activeTab === "snippets" ? "h-full" : "hidden"}>
          <Snippets />
        </div>
      )}
      {shouldRenderTab("notes") && (
        <div className={activeTab === "notes" ? "h-full" : "hidden"}>
          <Notes />
        </div>
      )}
      {shouldRenderTab("more-tools") && (
        <div className={activeTab === "more-tools" ? "h-full" : "hidden"}>
          <MoreToolsHub
            activeTool={moreToolsSection}
            onSelectTool={handleSelectMoreTool}
            onBack={handleMoreToolsBack}
            backToLauncher={moreToolsReturnTab === "launcher"}
            jttParserTab={jttParserTab ?? undefined}
            sshTunnelTab={sshTunnelTab ?? undefined}
            sshTunnelNavigationNonce={sshTunnelNavigationNonce}
            isVisible={activeTab === "more-tools"}
          />
        </div>
      )}
      {shouldRenderTab("documentation") && (
        <div className={activeTab === "documentation" ? "h-full" : "hidden"}>
          <Documentation />
        </div>
      )}
      {shouldRenderTab("mail") && (
        <div className={activeTab === "mail" ? "h-full" : "hidden"}>
          <Mail isVisible={activeTab === "mail"} />
        </div>
      )}
      {shouldRenderTab("fish-pond") && (
        <div className={activeTab === "fish-pond" ? "h-full" : "hidden"}>
          <FishPond />
        </div>
      )}
      {shouldRenderTab("settings") && (
        <div className={activeTab === "settings" ? "h-full" : "hidden"}>
          <SettingsView
            initialTab={settingsInitialTab}
            onBack={() => {
              setActiveTab(previousTab);
              setSettingsInitialTab("storage");
              scheduleLoadCounts(0);
            }}
          />
        </div>
      )}
    </div>
  );
}
