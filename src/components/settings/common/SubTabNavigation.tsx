import type { LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { LegacySettingsSection } from "../types";

export interface SubTabItem {
  id: LegacySettingsSection;
  titleKey: string;
  fallbackTitle: string;
  icon?: LucideIcon;
  dirty?: boolean;
}

export interface SubTabNavigationProps {
  items: SubTabItem[];
  activeSubTab: LegacySettingsSection;
  onChange: (id: LegacySettingsSection) => void;
}

export function SubTabNavigation({
  items,
  activeSubTab,
  onChange,
}: SubTabNavigationProps) {
  const { t } = useTranslation();

  if (items.length <= 1) return null;

  return (
    <div className="flex items-center gap-1.5 p-1 bg-muted/60 rounded-xl border border-border/50 max-w-fit mb-6 overflow-x-auto">
      {items.map((item) => {
        const active = activeSubTab === item.id;
        const Icon = item.icon;
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onChange(item.id)}
            className={`flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs sm:text-sm font-medium transition-all shrink-0 ${
              active
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground hover:bg-background/40"
            }`}
          >
            {Icon && <Icon className="w-3.5 h-3.5" />}
            <span>{t(item.titleKey, item.fallbackTitle)}</span>
            {item.dirty && (
              <span
                className="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse"
                title={t("currentSectionUnsaved", "Unsaved changes")}
              />
            )}
          </button>
        );
      })}
    </div>
  );
}
