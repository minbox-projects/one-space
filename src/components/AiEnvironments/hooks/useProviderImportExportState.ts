import { useState } from 'react';
import type { ProvidersImportPreview } from '@/lib/serviceProviders';

export function useProviderImportExportState() {
  const [exportingProviders, setExportingProviders] = useState(false);
  const [previewingImport, setPreviewingImport] = useState(false);
  const [applyingImport, setApplyingImport] = useState(false);
  const [importPreview, setImportPreview] = useState<ProvidersImportPreview | null>(null);
  const [importPath, setImportPath] = useState<string | null>(null);
  const [importDecisions, setImportDecisions] = useState<Record<string, 'overwrite' | 'new'>>({});

  return {
    exportingProviders,
    setExportingProviders,
    previewingImport,
    setPreviewingImport,
    applyingImport,
    setApplyingImport,
    importPreview,
    setImportPreview,
    importPath,
    setImportPath,
    importDecisions,
    setImportDecisions,
  };
}

export type ProviderImportExportStateBindings = ReturnType<typeof useProviderImportExportState>;
