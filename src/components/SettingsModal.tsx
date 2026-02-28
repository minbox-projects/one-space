import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { X, Save, RefreshCw } from 'lucide-react';

interface StorageConfig {
  storage_type: 'local' | 'git';
  git_url?: string;
  auth_method?: 'http' | 'ssh';
  http_username?: string;
  http_token?: string;
  ssh_key_path?: string;
}

export function SettingsModal({ open, onClose }: { open: boolean, onClose: () => void }) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });

  useEffect(() => {
    if (open) {
      loadConfig();
    }
  }, [open]);

  const loadConfig = async () => {
    try {
      const cfg = await invoke<StorageConfig>('get_storage_config');
      setConfig({
        ...cfg,
        storage_type: cfg.storage_type || 'local',
        auth_method: cfg.auth_method || 'http'
      });
    } catch (e) {
      console.error(e);
    }
  };

  const saveConfig = async () => {
    setLoading(true);
    setMessage({ type: '', text: '' });
    try {
      await invoke('save_storage_config', { config });
      setMessage({ type: 'success', text: t('settingsSaved', 'Settings saved successfully') });
      setTimeout(() => onClose(), 1500);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleSync = async () => {
    setSyncing(true);
    setMessage({ type: '', text: '' });
    try {
      await invoke('sync_git');
      setMessage({ type: 'success', text: t('syncSuccess', 'Sync successful') });
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setSyncing(false);
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="bg-card w-full max-w-lg rounded-xl border shadow-lg flex flex-col max-h-[90vh]">
        <div className="flex items-center justify-between p-4 border-b">
          <h2 className="font-semibold text-lg">{t('settings')}</h2>
          <button onClick={onClose} className="p-1 rounded-md hover:bg-muted text-muted-foreground transition-colors">
            <X className="w-5 h-5" />
          </button>
        </div>
        
        <div className="p-6 overflow-y-auto flex-1 space-y-6">
          {message.text && (
            <div className={`p-3 rounded-md text-sm ${message.type === 'error' ? 'bg-destructive/10 text-destructive border border-destructive/20' : 'bg-primary/10 text-primary border border-primary/20'}`}>
              {message.text}
            </div>
          )}

          <div className="space-y-4">
            <h3 className="font-medium">{t('dataStorage', 'Data Storage Location')}</h3>
            
            <div className="space-y-2">
              <label className="text-sm font-medium text-muted-foreground">{t('storageType', 'Storage Type')}</label>
              <select 
                className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                value={config.storage_type}
                onChange={e => setConfig({...config, storage_type: e.target.value as 'local' | 'git'})}
              >
                <option value="local">{t('local', 'Local (~/.config/onespace/data)')}</option>
                <option value="git">{t('gitRepo', 'Git Repository')}</option>
              </select>
            </div>

            {config.storage_type === 'git' && (
              <div className="space-y-4 pt-2 border-t mt-4">
                <div className="space-y-2">
                  <label className="text-sm font-medium text-muted-foreground">{t('remoteUrl', 'Remote URL')}</label>
                  <input 
                    type="text" 
                    placeholder="https://github.com/user/repo.git or git@github.com:user/repo.git"
                    className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                    value={config.git_url || ''}
                    onChange={e => setConfig({...config, git_url: e.target.value})}
                  />
                </div>

                <div className="space-y-2">
                  <label className="text-sm font-medium text-muted-foreground">{t('authMethod', 'Authentication Method')}</label>
                  <div className="flex gap-4">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input 
                        type="radio" 
                        name="auth_method" 
                        checked={config.auth_method === 'http'}
                        onChange={() => setConfig({...config, auth_method: 'http'})}
                        className="accent-primary"
                      />
                      <span className="text-sm">HTTP/HTTPS</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input 
                        type="radio" 
                        name="auth_method" 
                        checked={config.auth_method === 'ssh'}
                        onChange={() => setConfig({...config, auth_method: 'ssh'})}
                        className="accent-primary"
                      />
                      <span className="text-sm">SSH</span>
                    </label>
                  </div>
                </div>

                {config.auth_method === 'http' && (
                  <>
                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('username', 'Username')}</label>
                      <input 
                        type="text" 
                        className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                        value={config.http_username || ''}
                        onChange={e => setConfig({...config, http_username: e.target.value})}
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('token', 'Password / Personal Access Token')}</label>
                      <input 
                        type="password" 
                        className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                        value={config.http_token || ''}
                        onChange={e => setConfig({...config, http_token: e.target.value})}
                      />
                    </div>
                  </>
                )}

                {config.auth_method === 'ssh' && (
                  <div className="space-y-2">
                    <label className="text-sm font-medium text-muted-foreground">{t('sshKeyPath', 'Private Key Path')}</label>
                    <input 
                      type="text" 
                      placeholder="e.g. /Users/name/.ssh/id_rsa"
                      className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                      value={config.ssh_key_path || ''}
                      onChange={e => setConfig({...config, ssh_key_path: e.target.value})}
                    />
                    <p className="text-xs text-muted-foreground mt-1">
                      Absolute path is recommended (e.g. /Users/name/.ssh/id_rsa)
                    </p>
                  </div>
                )}
                
                <div className="pt-2">
                  <button 
                    onClick={handleSync}
                    disabled={syncing}
                    className="flex items-center gap-2 px-4 py-2 border rounded-md text-sm hover:bg-muted disabled:opacity-50 transition-colors"
                  >
                    <RefreshCw className={`w-4 h-4 ${syncing ? 'animate-spin' : ''}`} />
                    {syncing ? t('syncing', 'Syncing...') : t('syncNow', 'Sync Now')}
                  </button>
                  <p className="text-xs text-muted-foreground mt-2">
                    {t('syncHint', 'Data is auto-synced on save. Use this to force pull/push.')}
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="p-4 border-t bg-muted/20 flex justify-end gap-3 rounded-b-xl">
          <button 
            onClick={onClose}
            className="px-4 py-2 text-sm border bg-background hover:bg-muted rounded-md transition-colors"
          >
            {t('cancel')}
          </button>
          <button 
            onClick={saveConfig}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors"
          >
            {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            {t('save')}
          </button>
        </div>
      </div>
    </div>
  );
}