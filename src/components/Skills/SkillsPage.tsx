import { useSkillsActions } from './hooks/useSkillsActions';
import { useSkillsData } from './hooks/useSkillsData';
import {
  SkillsCatalogDetailDialog,
  SkillsInstallDialog,
  SkillsInstalledDetailDialog,
  SkillsReloadDialog,
} from './components/SkillsDialogs';
import {
  SkillsHeader,
  SkillsInstalledSection,
  SkillsModelTabs,
  SkillsModeTabs,
  SkillsRecommendedSection,
  SkillsRepositorySection,
} from './components/SkillsSections';

export function Skills({
  isVisible = true,
  initialEntry,
  onConsumeInitialEntry,
}: {
  isVisible?: boolean;
  initialEntry?: 'installed' | 'recommended' | 'repository';
  onConsumeInitialEntry?: () => void;
}) {
  const data = useSkillsData({ isVisible, initialEntry, onConsumeInitialEntry });
  const actions = useSkillsActions(data);
  const view = { ...data, ...actions };
  const { activeMode } = view;

  return (
    <div className="flex h-full flex-col gap-4">
      <SkillsHeader view={view} />
      <SkillsModeTabs view={view} />

      {activeMode !== 'repository' && <SkillsModelTabs view={view} />}

      {activeMode === 'installed' && <SkillsInstalledSection view={view} />}

      {activeMode === 'repository' && <SkillsRepositorySection view={view} />}

      {activeMode === 'recommended' && <SkillsRecommendedSection view={view} />}

      <SkillsCatalogDetailDialog view={view} />
      <SkillsInstallDialog view={view} />
      <SkillsInstalledDetailDialog view={view} />
      <SkillsReloadDialog view={view} />
    </div>
  );
}
