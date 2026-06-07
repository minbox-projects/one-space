import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceCapabilityEntry } from "../workspaceCapabilityContext";
import {
  WorkspaceSkillsDetailDialogs,
  WorkspaceSkillsDiscoverySection,
  WorkspaceSkillsInstallDialog,
  WorkspaceSkillsInstalledSection,
  WorkspaceSkillsPanelHeader,
} from "./components/WorkspaceSkillsPanelSections";
import { useWorkspaceSkillsPanelState } from "./hooks/useWorkspaceSkillsPanelState";

export function WorkspaceSkillsPanel({
  rootPath,
  isVisible = true,
  onNavigateToGlobalPage,
}: {
  rootPath: string;
  isVisible?: boolean;
  onNavigateToGlobalPage?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { t } = useTranslation();
  const discoverySectionRef = useRef<HTMLDivElement | null>(null);
  const state = useWorkspaceSkillsPanelState({
    rootPath,
    isVisible,
    onNavigateToGlobalPage,
  });

  return (
    <div className="flex h-full flex-col gap-4">
      <WorkspaceSkillsPanelHeader
        t={t}
        loading={state.loading}
        message={state.message}
        onRefresh={() => {
          void state.handleRefresh();
        }}
        onNavigate={onNavigateToGlobalPage}
      />
      <WorkspaceSkillsInstalledSection t={t} state={state} />
      <div
        ref={discoverySectionRef}
        onTransitionEnd={() => {
          if (state.discoveryMode) {
            discoverySectionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
          }
        }}
      >
        <WorkspaceSkillsDiscoverySection t={t} state={state} />
      </div>
      <WorkspaceSkillsDetailDialogs t={t} state={state} />
      <WorkspaceSkillsInstallDialog t={t} state={state} />
    </div>
  );
}
