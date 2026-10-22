import { useTranslation } from "react-i18next";
import { Filter, Search, X } from "lucide-react";
import { getSourceLabel } from "./types";

interface MessageCenterToolbarProps {
  searchQuery: string;
  selectedSource: string;
  availableSources: string[];
  hasActiveFilters: boolean;
  onSearchChange: (query: string) => void;
  onSourceChange: (source: string) => void;
  onClearFilters: () => void;
}

export function MessageCenterToolbar({
  searchQuery,
  selectedSource,
  availableSources,
  hasActiveFilters,
  onSearchChange,
  onSourceChange,
  onClearFilters,
}: MessageCenterToolbarProps) {
  const { t } = useTranslation();

  return (
    <div className="flex shrink-0 items-center gap-2 border-b bg-card/20 px-5 py-2">
      {/* 搜索框 */}
      <div className="relative min-w-0 flex-1">
        <Search className="absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t(
            "messageCenterSearchPlaceholder",
            "Search messages, stack traces, sources...",
          )}
          className="h-8 w-full rounded-lg border border-border/60 bg-muted/20 pr-7 pl-8 text-xs text-foreground placeholder:text-muted-foreground/60 transition-colors focus:border-foreground/30 focus:bg-background focus:outline-none focus:ring-1 focus:ring-foreground/10"
        />
        {searchQuery ? (
          <button
            type="button"
            onClick={() => onSearchChange("")}
            className="absolute top-1/2 right-2 -translate-y-1/2 rounded-md p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <X className="h-3 w-3" />
          </button>
        ) : null}
      </div>

      {/* 来源筛选下拉 */}
      {availableSources.length > 0 ? (
        <div className="relative shrink-0">
          <select
            value={selectedSource}
            onChange={(e) => onSourceChange(e.target.value)}
            className="h-8 rounded-lg border border-border/60 bg-muted/20 px-2.5 text-xs text-muted-foreground transition-colors hover:text-foreground focus:border-foreground/30 focus:bg-background focus:text-foreground focus:outline-none focus:ring-1 focus:ring-foreground/10"
          >
            <option value="all">
              {t("messageCenterFilterSourceAll", "All Sources")}
            </option>
            {availableSources.map((source) => (
              <option key={source} value={source}>
                {t(`messageSource_${source}`, getSourceLabel(source))}
              </option>
            ))}
          </select>
        </div>
      ) : null}

      {/* 清除筛选条件 */}
      {hasActiveFilters ? (
        <button
          type="button"
          onClick={onClearFilters}
          className="inline-flex h-8 shrink-0 items-center gap-1 rounded-lg border border-border/60 px-2 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          title={t("messageCenterClearFilters", "Clear filters")}
        >
          <Filter className="h-3 w-3" />
          <span>{t("messageCenterClearFilters", "Clear filters")}</span>
        </button>
      ) : null}
    </div>
  );
}
