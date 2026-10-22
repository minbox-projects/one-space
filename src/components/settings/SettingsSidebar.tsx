import { useTranslation } from "react-i18next";
import type { MainSettingsTab } from "./types";
import { MAIN_SETTINGS_CATEGORIES } from "./types";

export interface SettingsSidebarProps {
  activeMainTab: MainSettingsTab;
  onSelectMainTab: (tab: MainSettingsTab) => void;
  subTabDirtyMap: Record<string, boolean>;
}

export function SettingsSidebar({
  activeMainTab,
  onSelectMainTab,
  subTabDirtyMap,
}: SettingsSidebarProps) {
  const { t } = useTranslation();

  return (
    <div className="w-60 border-r bg-muted/20 flex flex-col shrink-0 p-3 space-y-1 select-none">
      <div className="px-3 py-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground/70">
        {t("settingsNavigation", "分类导航")}
      </div>

      {MAIN_SETTINGS_CATEGORIES.map((cat) => {
        const Icon = cat.icon;
        const active = activeMainTab === cat.id;
        const hasDirtyChild = cat.subTabs.some((st) => subTabDirtyMap[st.id]);

        return (
          <button
            key={cat.id}
            type="button"
            onClick={() => onSelectMainTab(cat.id)}
            className={`w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-sm transition-all text-left ${
              active
                ? "bg-primary text-primary-foreground font-medium shadow-sm"
                : "hover:bg-muted text-muted-foreground hover:text-foreground"
            }`}
          >
            <Icon className={`w-4 h-4 shrink-0 ${active ? "animate-pulse" : ""}`} />
            <span className="truncate flex-1 font-medium">
              {t(cat.titleKey, cat.fallbackTitle)}
            </span>
            {hasDirtyChild && (
              <span
                aria-label="dirty"
                className="w-2 h-2 rounded-full bg-amber-500 shrink-0"
                title={t(
                  "currentSectionUnsaved",
                  "Unsaved changes in this category",
                )}
              />
            )}
          </button>
        );
      })}
    </div>
  );
}
