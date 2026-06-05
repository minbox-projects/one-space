import { Upload, Download, Loader2, Search } from 'lucide-react';

import type { TFunction } from 'i18next';

interface ToolSectionHeaderProps {
  searchQuery: string;
  onSearchChange: (value: string) => void;
  onImport: () => void;
  onExport: () => void;
  loading: boolean;
  previewingImport: boolean;
  applyingImport: boolean;
  exportingProviders: boolean;
  t: TFunction;
}

export function ToolSectionHeader({
  searchQuery,
  onSearchChange,
  onImport,
  onExport,
  loading,
  previewingImport,
  applyingImport,
  exportingProviders,
  t,
}: ToolSectionHeaderProps) {
  const isImportDisabled = loading || previewingImport || applyingImport;

  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="relative w-full sm:max-w-lg">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('searchProviders', 'Search Service Providers...')}
          className="w-full bg-background border rounded-lg pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
        />
      </div>

      <div className="flex items-center justify-end gap-1.5">
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
  );
}
