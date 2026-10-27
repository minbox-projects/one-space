import { useSubagentsActions } from './hooks/useSubagentsActions';
import { useSubagentsData } from './hooks/useSubagentsData';
import {
  SubagentsCatalogDetailDialog,
  SubagentsInstallDialog,
  SubagentsInstalledDetailDialog,
  SubagentsReloadDialog,
  SubagentsUpdateDiffDialog,
} from './components/SubagentsDialogs';
import {
  SubagentsHeader,
  SubagentsInstalledSection,
  SubagentsModelTabs,
  SubagentsModeTabs,
  SubagentsRecommendedSection,
  SubagentsRepositorySection,
} from './components/SubagentsSections';

export function Subagents({
  isVisible = true,
  initialEntry,
  onConsumeInitialEntry,
}: {
  isVisible?: boolean;
  initialEntry?: 'installed' | 'recommended' | 'repository';
  onConsumeInitialEntry?: () => void;
}) {
  const data = useSubagentsData({ isVisible, initialEntry, onConsumeInitialEntry });
  const actions = useSubagentsActions(data);
  const view = { ...data, ...actions };
  const { activeMode } = view;

  return (
    <div className="flex h-full flex-col gap-4">
      <SubagentsHeader view={view} />
      <SubagentsModeTabs view={view} />

      {activeMode !== 'repository' && <SubagentsModelTabs view={view} />}

      {activeMode === 'installed' && <SubagentsInstalledSection view={view} />}

      {activeMode === 'repository' && <SubagentsRepositorySection view={view} />}

      {activeMode === 'recommended' && <SubagentsRecommendedSection view={view} />}

      <SubagentsCatalogDetailDialog view={view} />
      <SubagentsInstallDialog view={view} />
      <SubagentsInstalledDetailDialog view={view} />
      <SubagentsReloadDialog view={view} />
      <SubagentsUpdateDiffDialog view={view} />
    </div>
  );
}
