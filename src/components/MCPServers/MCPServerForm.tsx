import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, X } from 'lucide-react';
import type { MCPServer } from './types';

interface MCPServerFormProps {
  server: MCPServer | null;
  onClose: () => void;
  onSave: (server: MCPServer) => void;
}

export function MCPServerForm({ server, onClose, onSave }: MCPServerFormProps) {
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
