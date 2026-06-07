import { useTranslation } from "react-i18next";
import type { WorkspaceCapabilityEntry } from "../workspaceCapabilityContext";
import {
  WorkspaceSubagentsDetailDialogs,
  WorkspaceSubagentsDiscoverySection,
  WorkspaceSubagentsInstallDialog,
  WorkspaceSubagentsInstalledSection,
  WorkspaceSubagentsPanelHeader,
} from "./components/WorkspaceSubagentsPanelSections";
import { useWorkspaceSubagentsPanelState } from "./hooks/useWorkspaceSubagentsPanelState";

export function WorkspaceSubagentsPanel({
  rootPath,
  isVisible = true,
  onNavigateToGlobalPage,
}: {
  rootPath: string;
  isVisible?: boolean;
  onNavigateToGlobalPage?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { t } = useTranslation();
  const state = useWorkspaceSubagentsPanelState({
    rootPath,
    isVisible,
    onNavigateToGlobalPage,
  });

  return (
    <div className="flex h-full flex-col gap-4">
      <WorkspaceSubagentsPanelHeader
        t={t}
        loading={state.loading}
        message={state.message}
        onRefresh={() => {
          void state.handleRefresh();
        }}
        onNavigate={onNavigateToGlobalPage}
      />
      <WorkspaceSubagentsInstalledSection t={t} state={state} />
      <WorkspaceSubagentsDiscoverySection t={t} state={state} />
      <WorkspaceSubagentsDetailDialogs t={t} state={state} />
      <WorkspaceSubagentsInstallDialog t={t} state={state} />
    </div>
  );
}
