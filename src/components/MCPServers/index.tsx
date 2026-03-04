import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, Edit, Server, X, Key, Link as LinkIcon, ChevronRight, ChevronDown, History, Download } from 'lucide-react';
import { BackupManager } from '../BackupManager';
import { MCPImportExport } from '../MCPImportExport';

interface MCPServer {
  id: string;
  name: string;
  description?: string;
  transport: 'stdio' | 'http' | 'sse';
  command?: string;
  args?: string[];
  cwd?: string;
  url?: string;
  http_url?: string;
  env?: Record<string, string>;
  headers?: Record<string, string>;
  timeout?: number;
  trust?: boolean;
  linked_provider_ids: string[];
}

interface MCPTemplate {
  id: string;
  name: string;
  description: string;
  transport: string;
  command?: string;
  args?: string[];
  url?: string;
  env_placeholders: string[];
  headers_placeholders: string[];
}

interface MCPServersProps {
  providers?: any[];
  onLinkedProvidersChange?: (serverId: string, providerIds: string[]) => void;
}

export function MCPServers({ providers = [], onLinkedProvidersChange }: MCPServersProps) {
  const { t } = useTranslation();
  const [servers, setServers] = useState<MCPServer[]>([]);
  const [templates, setTemplates] = useState<MCPTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [showTemplates, setShowTemplates] = useState(false);
  const [editingServer, setEditingServer] = useState<MCPServer | null>(null);
  const [expandedServers, setExpandedServers] = useState<Set<string>>(new Set());
  const [showBackupManager, setShowBackupManager] = useState(false);
  const [showImportExport, setShowImportExport] = useState(false);

  useEffect(() => {
    loadServers();
    loadTemplates();
  }, []);

  async function loadServers() {
    setLoading(true);
    try {
      const state = await invoke('get_mcp_servers');
      setServers((state as any).servers || []);
    } catch (e) {
      console.error('Failed to load MCP servers:', e);
    } finally {
      setLoading(false);
    }
  }

  async function loadTemplates() {
    try {
      const result = await invoke('list_mcp_templates');
      setTemplates(result as MCPTemplate[]);
    } catch (e) {
      console.error('Failed to load MCP templates:', e);
    }
  }

  async function handleSave(server: MCPServer) {
    try {
      await invoke('save_mcp_server', { server });
      await loadServers();
      setShowAddModal(false);
      setEditingServer(null);
    } catch (e) {
      alert(t('saveFailed', { error: e }));
    }
  }

  async function handleDelete(id: string) {
    if (!confirm(t('confirmDeleteMcp'))) return;
    try {
      await invoke('delete_mcp_server', { serverId: id });
      await loadServers();
    } catch (e) {
      alert(t('deleteFailed', { error: e }));
    }
  }

  async function handleCreateFromTemplate(template: MCPTemplate) {
    try {
      const server = await invoke('get_mcp_template', { templateId: template.id });
      setEditingServer(server as MCPServer);
      setShowAddModal(true);
      setShowTemplates(false);
    } catch (e) {
      alert(t('loadTemplateFailed', { error: e }));
    }
  }

  async function handleLinkProviders(serverId: string, providerIds: string[]) {
    try {
      await invoke('link_mcp_to_providers', { serverId, providerIds });
      await loadServers();
      if (onLinkedProvidersChange) {
        onLinkedProvidersChange(serverId, providerIds);
      }
    } catch (e) {
      alert(t('linkProvidersFailed', { error: e }));
    }
  }

  function toggleExpand(serverId: string) {
    const newExpanded = new Set(expandedServers);
    if (newExpanded.has(serverId)) {
      newExpanded.delete(serverId);
    } else {
      newExpanded.add(serverId);
    }
    setExpandedServers(newExpanded);
  }

  return (
    <div className="p-6">
      <div className="flex justify-between items-center mb-6">
        <div>
          <h2 className="text-2xl font-bold">{t('mcpServers')}</h2>
          <p className="text-sm text-muted-foreground mt-1">
            {t('mcpServersDesc')}
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => setShowBackupManager(true)}
            className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 flex items-center gap-2"
          >
            <History className="w-4 h-4" />
            {t('backups')}
          </button>
          <button
            onClick={() => setShowImportExport(true)}
            className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 flex items-center gap-2"
          >
            <Download className="w-4 h-4" />
            {t('importExport')}
          </button>
          <button
            onClick={() => setShowTemplates(true)}
            className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80"
          >
            {t('useTemplate')}
          </button>
          <button
            onClick={() => {
              setEditingServer(null);
              setShowAddModal(true);
            }}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm flex items-center gap-2 hover:bg-primary/90"
          >
            <Plus className="w-4 h-4" /> {t('addServer')}
          </button>
        </div>
      </div>

      {loading ? (
        <div className="text-center py-12 text-muted-foreground">{t('loading')}</div>
      ) : servers.length === 0 ? (
        <div className="text-center py-12">
          <Server className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">{t('mcpServerListEmpty')}</h3>
          <p className="text-muted-foreground mb-4">
            {t('mcpServerListEmptyDesc')}
          </p>
          <button
            onClick={() => setShowTemplates(true)}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
          >
            {t('browseTemplates')}
          </button>
        </div>
      ) : (
        <div className="grid gap-4">
          {servers.map(server => (
            <div key={server.id} className="border rounded-lg bg-card overflow-hidden">
              <div 
                className="p-4 cursor-pointer hover:bg-accent/50 flex items-center justify-between"
                onClick={() => toggleExpand(server.id)}
              >
                <div className="flex items-center gap-3">
                  {expandedServers.has(server.id) ? (
                    <ChevronDown className="w-5 h-5 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="w-5 h-5 text-muted-foreground" />
                  )}
                  <div className="flex items-center gap-2">
                    <div className={`w-2 h-2 rounded-full ${
                      server.transport === 'stdio' ? 'bg-green-500' :
                      server.transport === 'http' ? 'bg-blue-500' : 'bg-purple-500'
                    }`} />
                    <h3 className="font-semibold text-lg">{server.name}</h3>
                  </div>
                  <span className="text-xs px-2 py-1 bg-secondary rounded uppercase">
                    {server.transport}
                  </span>
                </div>
                <div className="flex items-center gap-4">
                  {server.linked_provider_ids.length > 0 && (
                    <div className="text-sm text-muted-foreground flex items-center gap-1">
                      <LinkIcon className="w-3 h-3" />
                      {t('environmentCount', { count: server.linked_provider_ids.length })}
                    </div>
                  )}
                  <div className="flex gap-2">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setEditingServer(server);
                        setShowAddModal(true);
                      }}
                      className="p-2 hover:bg-secondary rounded"
                      title={t('edit')}
                    >
                      <Edit className="w-4 h-4" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(server.id);
                      }}
                      className="p-2 hover:bg-destructive/10 text-destructive rounded"
                      title={t('delete')}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </div>
              
              {expandedServers.has(server.id) && (
                <div className="px-4 pb-4 border-t bg-muted/30">
                  {server.description && (
                    <p className="text-sm text-muted-foreground mt-3">{server.description}</p>
                  )}
                  
                  <div className="grid grid-cols-2 gap-4 mt-4">
                    {server.command && (
                      <div>
                        <label className="text-xs font-medium text-muted-foreground">{t('command')}</label>
                        <code className="block text-sm bg-background rounded p-2 mt-1 font-mono">
                          {server.command} {server.args?.join(' ')}
                        </code>
                      </div>
                    )}
                    {server.url || server.http_url ? (
                      <div>
                        <label className="text-xs font-medium text-muted-foreground">{server.transport === 'http' ? t('httpUrl') : t('sseUrl')}</label>
                        <code className="block text-sm bg-background rounded p-2 mt-1 font-mono">
                          {server.http_url || server.url}
                        </code>
                      </div>
                    ) : null}
                    {server.timeout && (
                      <div>
                        <label className="text-xs font-medium text-muted-foreground">{t('timeout')}</label>
                        <div className="text-sm mt-1">{server.timeout}ms</div>
                      </div>
                    )}
                    {server.trust && (
                      <div>
                        <label className="text-xs font-medium text-muted-foreground">{t('trust')}</label>
                        <div className="text-sm mt-1 text-green-600">{t('trustAutoApprove')}</div>
                      </div>
                    )}
                  </div>
                  
                  {server.env && Object.keys(server.env).length > 0 && (
                    <div className="mt-4">
                      <label className="text-xs font-medium text-muted-foreground flex items-center gap-1">
                        <Key className="w-3 h-3" /> {t('envVars')}
                      </label>
                      <div className="grid grid-cols-2 gap-2 mt-2">
                        {Object.entries(server.env).map(([key, value]) => (
                          <div key={key} className="text-sm bg-background rounded p-2 font-mono">
                            <span className="text-primary">{key}</span>=
                            <span className="text-muted-foreground">{value}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                  
                  <div className="mt-4">
                    <label className="text-xs font-medium text-muted-foreground flex items-center gap-1 mb-2">
                      <LinkIcon className="w-3 h-3" /> {t('linkToEnvironments')}
                    </label>
                    <div className="flex flex-wrap gap-2">
                      {providers?.map(provider => (
                        <button
                          key={provider.id}
                          onClick={() => {
                            const isLinked = server.linked_provider_ids.includes(provider.id);
                            const newIds = isLinked
                              ? server.linked_provider_ids.filter(id => id !== provider.id)
                              : [...server.linked_provider_ids, provider.id];
                            handleLinkProviders(server.id, newIds);
                          }}
                          className={`px-3 py-1.5 rounded text-xs border ${
                            server.linked_provider_ids.includes(provider.id)
                              ? 'bg-primary text-primary-foreground border-primary'
                              : 'bg-background hover:bg-accent'
                          }`}
                        >
                          {provider.name}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {showAddModal && (
        <MCPServerForm
          server={editingServer}
          onClose={() => {
            setShowAddModal(false);
            setEditingServer(null);
          }}
          onSave={handleSave}
        />
      )}

      {showTemplates && (
        <MCPTemplatesModal
          templates={templates}
          onClose={() => setShowTemplates(false)}
          onSelect={handleCreateFromTemplate}
        />
      )}
      
      {showBackupManager && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-background rounded-lg w-full max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
            <div className="h-14 border-b px-4 flex items-center justify-end">
              <button
                onClick={() => setShowBackupManager(false)}
                className="p-2 hover:bg-secondary rounded"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto">
              <BackupManager activeTool={undefined} />
            </div>
          </div>
        </div>
      )}
      
      {showImportExport && (
        <MCPImportExport
          servers={servers.map(s => ({ id: s.id, name: s.name }))}
          onClose={() => setShowImportExport(false)}
          onImported={loadServers}
        />
      )}
    </div>
  );
}

interface MCPServerFormProps {
  server: MCPServer | null;
  onClose: () => void;
  onSave: (server: MCPServer) => void;
}

function MCPServerForm({ server, onClose, onSave }: MCPServerFormProps) {
  const { t } = useTranslation();
  const [formData, setFormData] = useState<MCPServer>(
    server || {
      id: '',
      name: '',
      description: '',
      transport: 'stdio',
      command: '',
      args: [],
      cwd: '',
      url: '',
      http_url: '',
      env: {},
      headers: {},
      timeout: 60000,
      trust: false,
      linked_provider_ids: [],
    } as any
  );

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    onSave(formData);
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-background rounded-lg w-full max-w-2xl max-h-[90vh] overflow-y-auto">
        <div className="p-6 border-b flex justify-between items-center sticky top-0 bg-background z-10">
          <h3 className="text-xl font-bold">
            {server ? t('saveChanges') : t('addServerBtn')}
          </h3>
          <button onClick={onClose} className="p-2 hover:bg-secondary rounded">
            <X className="w-5 h-5" />
          </button>
        </div>
        
        <form onSubmit={handleSubmit} className="p-6 space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium mb-1">{t('mcpServerName')} *</label>
              <input
                type="text"
                required
                value={formData.name}
                onChange={e => setFormData({ ...formData, name: e.target.value })}
                className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                placeholder={t('mcpNamePlaceholder')}
              />
            </div>
            
            <div>
              <label className="block text-sm font-medium mb-1">{t('transport')} *</label>
              <select
                value={formData.transport}
                onChange={e => setFormData({ ...formData, transport: e.target.value as any })}
                className="w-full bg-background border rounded-md px-3 py-2 text-sm"
              >
                <option value="stdio">{t('stdioTransport')}</option>
                <option value="http">{t('httpTransport')}</option>
                <option value="sse">{t('sseTransport')}</option>
              </select>
            </div>
          </div>
          
          <div>
            <label className="block text-sm font-medium mb-1">{t('mcpServerDescription')}</label>
            <input
              type="text"
              value={formData.description || ''}
              onChange={e => setFormData({ ...formData, description: e.target.value })}
              className="w-full bg-background border rounded-md px-3 py-2 text-sm"
              placeholder={t('descriptionPlaceholder')}
            />
          </div>
          
          {formData.transport === 'stdio' && (
            <>
              <div>
                <label className="block text-sm font-medium mb-1">{t('command')} *</label>
                <input
                  type="text"
                  required={formData.transport === 'stdio'}
                  value={formData.command || ''}
                  onChange={e => setFormData({ ...formData, command: e.target.value })}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm font-mono"
                  placeholder={t('command')}
                />
              </div>
              
              <div>
                <label className="block text-sm font-medium mb-1">{t('arguments')}</label>
                <input
                  type="text"
                  value={(formData.args || []).join(' ')}
                  onChange={e => setFormData({ 
                    ...formData, 
                    args: e.target.value.split(' ').filter(Boolean)
                  })}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm font-mono"
                  placeholder={t('mcpArgsPlaceholder')}
                />
              </div>
              
              <div>
                <label className="block text-sm font-medium mb-1">{t('workingDirectory')}</label>
                <input
                  type="text"
                  value={formData.cwd || ''}
                  onChange={e => setFormData({ ...formData, cwd: e.target.value })}
                  className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                  placeholder={t('optional')}
                />
              </div>
            </>
          )}
          
          {(formData.transport === 'http' || formData.transport === 'sse') && (
            <div>
              <label className="block text-sm font-medium mb-1">
                {formData.transport === 'http' ? t('httpUrl') : t('sseUrl')} *
              </label>
              <input
                type="url"
                required={formData.transport === 'http' || formData.transport === 'sse'}
                value={formData.transport === 'http' ? (formData.http_url || '') : (formData.url || '')}
                onChange={e => setFormData({ 
                  ...formData, 
                  [formData.transport === 'http' ? 'http_url' : 'url']: e.target.value 
                })}
                className="w-full bg-background border rounded-md px-3 py-2 text-sm font-mono"
                placeholder="https://..."
              />
            </div>
          )}
          
          <div>
            <label className="block text-sm font-medium mb-2">
              {t('envVars')}
            </label>
            <div className="space-y-2">
              {Object.entries(formData.env || {}).map(([key, value], idx) => (
                <div key={idx} className="flex gap-2">
                  <input
                    type="text"
                    value={key}
                    onChange={e => {
                      const newEnv = { ...formData.env };
                      delete newEnv[key];
                      newEnv[e.target.value] = value;
                      setFormData({ ...formData, env: newEnv });
                    }}
                    placeholder="VAR_NAME"
                    className="flex-1 bg-background border rounded-md px-3 py-2 text-sm font-mono"
                  />
                  <input
                    type="text"
                    value={value}
                    onChange={e => {
                      const newEnv = { ...formData.env };
                      newEnv[key] = e.target.value;
                      setFormData({ ...formData, env: newEnv });
                    }}
                    placeholder="$VALUE"
                    className="flex-1 bg-background border rounded-md px-3 py-2 text-sm font-mono"
                  />
                  <button
                    type="button"
                    onClick={() => {
                      const newEnv = { ...formData.env };
                      delete newEnv[key];
                      setFormData({ ...formData, env: newEnv });
                    }}
                    className="p-2 hover:bg-destructive/10 text-destructive rounded"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              ))}
              <button
                type="button"
                onClick={() => setFormData({ 
                  ...formData, 
                  env: { ...formData.env, '': '' }
                })}
                className="text-sm text-primary hover:underline flex items-center gap-1"
              >
                <Plus className="w-3 h-3" /> {t('addVariable')}
              </button>
            </div>
          </div>
          
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium mb-1">{t('timeout')}</label>
              <input
                type="number"
                value={formData.timeout || 60000}
                onChange={e => setFormData({ ...formData, timeout: parseInt(e.target.value) || 60000 })}
                className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                placeholder="60000"
              />
            </div>
            
            <div className="flex items-end">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={formData.trust || false}
                  onChange={e => setFormData({ ...formData, trust: e.target.checked })}
                  className="w-4 h-4"
                />
                <span className="text-sm">{t('trustAutoApprove')}</span>
              </label>
            </div>
          </div>
          
          <div className="flex justify-end gap-3 pt-4 border-t">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 hover:bg-secondary rounded"
            >
              {t('cancel')}
            </button>
            <button
              type="submit"
              className="px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90"
            >
              {server ? t('saveChanges') : t('addServerBtn')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

interface MCPTemplatesModalProps {
  templates: MCPTemplate[];
  onClose: () => void;
  onSelect: (template: MCPTemplate) => void;
}

function MCPTemplatesModal({ templates, onClose, onSelect }: MCPTemplatesModalProps) {
  const { t } = useTranslation();
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-background rounded-lg w-full max-w-3xl max-h-[90vh] overflow-y-auto">
        <div className="p-6 border-b flex justify-between items-center sticky top-0 bg-background z-10">
          <div>
            <h3 className="text-xl font-bold">{t('chooseTemplate')}</h3>
            <p className="text-sm text-muted-foreground mt-1">
              {t('chooseTemplateDesc')}
            </p>
          </div>
          <button onClick={onClose} className="p-2 hover:bg-secondary rounded">
            <X className="w-5 h-5" />
          </button>
        </div>
        
        <div className="p-6 grid gap-4">
          {templates.map(template => (
            <div 
              key={template.id}
              className="border rounded-lg p-4 hover:bg-accent/50 cursor-pointer transition-colors"
              onClick={() => onSelect(template)}
            >
              <div className="flex justify-between items-start">
                <div>
                  <h4 className="font-semibold text-lg">{template.name}</h4>
                  <p className="text-sm text-muted-foreground mt-1">{template.description}</p>
                  
                  <div className="flex items-center gap-3 mt-3">
                    <span className="text-xs px-2 py-1 bg-secondary rounded uppercase">
                      {template.transport}
                    </span>
                    {template.command && (
                      <code className="text-xs bg-muted px-2 py-1 rounded font-mono">
                        {template.command} {template.args?.join(' ')}
                      </code>
                    )}
                    {template.url && (
                      <code className="text-xs bg-muted px-2 py-1 rounded font-mono">
                        {template.url}
                      </code>
                    )}
                  </div>
                  
                  {template.env_placeholders.length > 0 && (
                    <div className="mt-2 text-xs text-muted-foreground flex items-center gap-1">
                      <Key className="w-3 h-3" />
                      {t('requires')}: {template.env_placeholders.join(', ')}
                    </div>
                  )}
                </div>
                <ChevronRight className="w-5 h-5 text-muted-foreground" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
