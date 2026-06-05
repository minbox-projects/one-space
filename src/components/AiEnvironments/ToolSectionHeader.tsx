import { Upload, Download, Loader2, Search } from 'lucide-react';
import { ToolIcon } from './index';

import type { TFunction } from 'i18next';

interface ToolSectionHeaderProps {
  activeTool: string;
  providerCount: number;
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
  activeTool,
  providerCount,
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
    <div className="space-y-3">
      {/* Title + Actions row */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ToolIcon tool={activeTool} className="w-5 h-5" />
          <h2 className="text-lg font-semibold capitalize">{activeTool}</h2>
          <span className="text-sm text-muted-foreground">
            ({providerCount}{' '}
            {providerCount === 1
              ? t('provider', 'Service Provider')
              : t('providers', 'Service Providers')})
          </span>
        </div>
        <div className="flex items-center gap-1.5">
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

      <div className="flex">
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
      </div>
    </div>
  );
}
