import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { X, Download, Upload, FileJson, Key } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface MCPServer {
  id: string;
  name: string;
}

interface MCPImportExportProps {
  servers: MCPServer[];
  onClose: () => void;
  onImported?: (ids: string[]) => void;
}

export function MCPImportExport({ servers, onClose, onImported }: MCPImportExportProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<'export' | 'import'>('export');
  const [selectedServers, setSelectedServers] = useState<string[]>([]);
  const [notes, setNotes] = useState('');
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importPath, setImportPath] = useState<string | null>(null);
  const [linkToAll, setLinkToAll] = useState(false);

  async function handleExport() {
    if (selectedServers.length === 0) {
      alert(t('selectServerToExport'));
      return;
    }
    
    setExporting(true);
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const defaultPath = `onespace-mcp-export-${stamp}.json`;
      
      const outputPath = await save({
        defaultPath,
        filters: [{ name: t('mcpConfig'), extensions: ['json'] }]
      });
      
      if (!outputPath || Array.isArray(outputPath)) {
        setExporting(false);
        return;
      }
      
      const filePath = await invoke('export_mcp_config', {
        serverIds: selectedServers,
        outputPath: outputPath as string,
        notes: notes || undefined
      });
      alert(t('exportedTo', { path: filePath }));
      onClose();
    } catch (e) {
      alert(t('exportFailed', { error: e }));
    } finally {
      setExporting(false);
    }
  }

  async function handleSelectImportFile() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: t('mcpConfig'),
          extensions: ['json']
        }]
      });
      
      if (selected) {
        setImportPath(selected as string);
      }
    } catch (e) {
      alert(t('selectFileFailed', { error: e }));
    }
  }

  async function handleImport() {
    if (!importPath) {
      alert(t('selectFileToImport'));
      return;
    }
    
    setImporting(true);
    try {
      const importedIds = await invoke('import_mcp_config', {
        importPath: importPath,
        linkToProviderIds: linkToAll ? servers.map(s => s.id) : []
      }) as string[];
      
      if (onImported) {
        onImported(importedIds as string[]);
      }
      
      alert(t('importSuccess', { count: importedIds.length }));
      onClose();
    } catch (e) {
      alert(t('importFailed', { error: e }));
    } finally {
      setImporting(false);
    }
  }

  function toggleServer(serverId: string) {
    setSelectedServers(prev => 
      prev.includes(serverId)
        ? prev.filter(id => id !== serverId)
        : [...prev, serverId]
    );
  }

  function selectAll() {
    setSelectedServers(servers.map(s => s.id));
  }

  function deselectAll() {
    setSelectedServers([]);
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-background rounded-lg w-full max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="p-6 border-b flex justify-between items-center">
          <div>
            <h3 className="text-xl font-bold">{t('mcpConfiguration')}</h3>
            <p className="text-sm text-muted-foreground mt-1">
              {t('importExportDesc')}
            </p>
          </div>
          <button 
            onClick={onClose}
            className="p-2 hover:bg-secondary rounded"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b">
          <button
            onClick={() => setActiveTab('export')}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
              activeTab === 'export'
                ? 'bg-primary text-primary-foreground'
                : 'hover:bg-muted'
            }`}
          >
            <div className="flex items-center justify-center gap-2">
              <Download className="w-4 h-4" />
              {t('export')}
            </div>
          </button>
          <button
            onClick={() => setActiveTab('import')}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
              activeTab === 'import'
                ? 'bg-primary text-primary-foreground'
                : 'hover:bg-muted'
            }`}
          >
            <div className="flex items-center justify-center gap-2">
              <Upload className="w-4 h-4" />
              {t('import')}
            </div>
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {activeTab === 'export' ? (
            <div className="space-y-4">
              <div>
                <div className="flex justify-between items-center mb-2">
                  <label className="text-sm font-medium">{t('selectServersExport')}</label>
                  <div className="flex gap-2 text-xs">
                    <button onClick={selectAll} className="text-primary hover:underline">
                      {t('selectAll')}
                    </button>
                    <button onClick={deselectAll} className="text-primary hover:underline">
                      {t('deselectAll')}
                    </button>
                  </div>
                </div>
                
                <div className="border rounded-md max-h-60 overflow-y-auto">
                  {servers.length === 0 ? (
                    <div className="p-8 text-center text-muted-foreground">
                      {t('noMcpServersConfigured')}
                    </div>
                  ) : (
                    <div className="divide-y">
                      {servers.map(server => (
                        <label 
                          key={server.id}
                          className="flex items-center gap-3 p-3 hover:bg-muted/50 cursor-pointer"
                        >
                          <input
                            type="checkbox"
                            checked={selectedServers.includes(server.id)}
                            onChange={() => toggleServer(server.id)}
                            className="w-4 h-4"
                          />
                          <FileJson className="w-4 h-4 text-muted-foreground" />
                          <span className="text-sm font-medium">{server.name}</span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>
                <p className="text-xs text-muted-foreground mt-2">
                  {t('selectedCount', { selected: selectedServers.length, total: servers.length })}
                </p>
              </div>

              <div>
                <label className="text-sm font-medium block mb-2">{t('notesOptional')}</label>
                <textarea
                  value={notes}
                  onChange={e => setNotes(e.target.value)}
                  placeholder={t('addDescriptionPlaceholder')}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm min-h-[80px] focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
                <p className="text-xs text-muted-foreground mt-1">
                  {t('exportNotesIncluded')}
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              <div>
                <label className="text-sm font-medium block mb-2">{t('selectImportFile')}</label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={importPath || ''}
                    readOnly
                    placeholder={t('noFileSelected')}
                    className="flex-1 bg-background border rounded-md px-3 py-2 text-sm"
                  />
                  <button
                    onClick={handleSelectImportFile}
                    className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80"
                  >
                    {t('browse')}
                  </button>
                </div>
              </div>

              {importPath && (
                <div className="bg-muted/30 border rounded-md p-4">
                  <div className="flex items-start gap-3">
                    <FileJson className="w-5 h-5 text-primary mt-0.5" />
                    <div className="flex-1">
                      <p className="text-sm font-medium">{importPath.split('/').pop()}</p>
                      <p className="text-xs text-muted-foreground mt-1">
                        {t('readyToImport')}
                      </p>
                    </div>
                  </div>
                </div>
              )}

              <div className="bg-primary/5 border border-primary/20 rounded-md p-4">
                <div className="flex items-start gap-3">
                  <Key className="w-5 h-5 text-primary mt-0.5" />
                  <div className="flex-1">
                    <p className="text-sm font-medium">{t('securityNotice')}</p>
                    <p className="text-xs text-muted-foreground mt-1">
                      {t('securityNoticeDesc')}
                    </p>
                  </div>
                </div>
              </div>

              <label className="flex items-center gap-3 p-4 border rounded-md cursor-pointer hover:bg-muted/50">
                <input
                  type="checkbox"
                  checked={linkToAll}
                  onChange={e => setLinkToAll(e.target.checked)}
                  className="w-4 h-4"
                />
                <div>
                  <p className="text-sm font-medium">{t('linkToAllEnvironments')}</p>
                  <p className="text-xs text-muted-foreground">
                    {t('linkToAllEnvironmentsDesc')}
                  </p>
                </div>
              </label>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-6 border-t flex justify-end gap-3">
          <button
            onClick={onClose}
            className="px-4 py-2 hover:bg-secondary rounded"
          >
            {t('cancel')}
          </button>
          {activeTab === 'export' ? (
            <button
              onClick={handleExport}
              disabled={exporting || selectedServers.length === 0}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50 flex items-center gap-2"
            >
              <Download className="w-4 h-4" />
              {exporting ? t('exporting') : t('exportBtn')}
            </button>
          ) : (
            <button
              onClick={handleImport}
              disabled={importing || !importPath}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50 flex items-center gap-2"
            >
              <Upload className="w-4 h-4" />
              {importing ? t('importing') : t('importBtn')}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
