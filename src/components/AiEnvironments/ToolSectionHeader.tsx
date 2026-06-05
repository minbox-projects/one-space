import { Plus, Upload, Download, Loader2, Search } from 'lucide-react';
import { ToolIcon } from './index';

import type { TFunction } from 'i18next';

interface ToolSectionHeaderProps {
  activeTool: string;
  providerCount: number;
  searchQuery: string;
  onSearchChange: (value: string) => void;
  onImport: () => void;
  onExport: () => void;
  onAdd: () => void;
  activeFilters: Set<string>;
  onFilterChange: (filter: string) => void;
  loading: boolean;
  previewingImport: boolean;
  applyingImport: boolean;
  exportingProviders: boolean;
  t: TFunction;
}

const FILTER_OPTIONS = [
  { key: 'all', labelKey: 'filterAll', defaultLabel: 'All' },
  { key: 'active', labelKey: 'filterActive', defaultLabel: 'Active' },
  { key: 'inactive', labelKey: 'filterInactive', defaultLabel: 'Inactive' },
];

export function ToolSectionHeader({
  activeTool,
  providerCount,
  searchQuery,
  onSearchChange,
  onImport,
  onExport,
  onAdd,
  activeFilters,
  onFilterChange,
  loading,
  previewingImport,
  applyingImport,
  exportingProviders,
  t,
}: ToolSectionHeaderProps) {
  const isImportDisabled = loading || previewingImport || applyingImport;

  return (
    <div className="space-y-3">
      {/* Title + Actions row */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ToolIcon tool={activeTool} className="w-5 h-5" />
          <h2 className="text-lg font-semibold capitalize">{activeTool}</h2>
          <span className="text-sm text-muted-foreground">
            ({providerCount} {providerCount === 1 ? t('provider', 'Service Provider') : t('providers', 'Service Providers')})
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={onAdd}
            className="acc-panel-btn primary h-8 px-3"
            title={t('addProvider', 'Add Service Provider')}
            aria-label={t('addProvider', 'Add Service Provider')}
          >
            <Plus className="w-4 h-4" />
            {t('addProvider', 'Add Service Provider')}
          </button>
          <button
            type="button"
            onClick={onImport}
            disabled={isImportDisabled}
            className="p-1.5 hover:bg-muted rounded-md transition-colors text-muted-foreground disabled:opacity-50"
            title={t('providersImportTitle', 'Import Service Providers')}
            aria-label={t('providersImportTitle', 'Import Service Providers')}
          >
            {previewingImport ? <Loader2 className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />}
          </button>
          <button
            type="button"
            onClick={onExport}
            disabled={loading || exportingProviders}
            className="p-1.5 hover:bg-muted rounded-md transition-colors text-muted-foreground disabled:opacity-50"
            title={t('providersExportTitle', 'Export Service Providers')}
            aria-label={t('providersExportTitle', 'Export Service Providers')}
          >
            {exportingProviders ? <Loader2 className="w-4 h-4 animate-spin" /> : <Download className="w-4 h-4" />}
          </button>
        </div>
      </div>

      {/* Search + Filter row */}
      <div className="flex flex-col sm:flex-row gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={t('searchProviders', 'Search Service Providers...')}
            className="w-full bg-background border rounded-lg pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
          />
        </div>
        <div className="flex items-center gap-1.5">
          {FILTER_OPTIONS.map(f => {
            const isActive = activeFilters.size === 0 || activeFilters.has(f.key);
            return (
              <button
                key={f.key}
                type="button"
                onClick={() => onFilterChange(f.key)}
                className={`px-3 py-1.5 rounded-full text-xs font-medium transition-colors border ${
                  isActive
                    ? 'bg-primary text-primary-foreground border-primary'
                    : 'bg-background text-muted-foreground border-border hover:bg-muted'
                }`}
              >
                {t(f.labelKey, f.defaultLabel)}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
