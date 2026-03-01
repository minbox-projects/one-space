import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { X, Save, RefreshCw, HardDrive, Palette } from 'lucide-react';

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
  const [activeTab, setActiveTab] = useState('storage');
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
      <div className="bg-card w-full max-w-4xl rounded-xl border shadow-lg flex max-h-[90vh] overflow-hidden">
        
        {/* Sidebar */}
        <div className="w-64 bg-muted/30 border-r flex flex-col shrink-0">
          <div className="flex items-center p-4 border-b h-14 shrink-0">
            <h2 className="font-semibold text-lg">{t('settings')}</h2>
          </div>
          <div className="p-3 space-y-1 overflow-y-auto">
            <button
              onClick={() => setActiveTab('storage')}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === 'storage' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-muted text-muted-foreground'
              }`}
            >
              <HardDrive className="w-4 h-4" />
              {t('dataStorage', 'Data Storage')}
            </button>
            <button
              onClick={() => setActiveTab('appearance')}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === 'appearance' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-muted text-muted-foreground'
              }`}
            >
              <Palette className="w-4 h-4" />
              {t('appearance', 'Appearance')}
            </button>
          </div>
        </div>

        {/* Content Area */}
        <div className="flex-1 flex flex-col h-full overflow-hidden bg-background">
          <div className="flex items-center justify-end p-2 h-14 border-b shrink-0 bg-card">
            <button onClick={onClose} className="p-2 rounded-md hover:bg-muted text-muted-foreground transition-colors mr-2">
              <X className="w-5 h-5" />
            </button>
          </div>
          
          <div className="flex-1 overflow-y-auto p-6 bg-card">
            {activeTab === 'storage' ? (
              <div className="space-y-6 flex flex-col h-full">
                <div>
                  <h3 className="font-semibold text-lg">{t('dataStorage', 'Data Storage Location')}</h3>
                  <p className="text-sm text-muted-foreground mt-1">{t('dataStorageDesc', 'Configure where OneSpace data is saved and synced.')}</p>
                </div>

                {message.text && (
                  <div className={`p-3 rounded-md text-sm ${message.type === 'error' ? 'bg-destructive/10 text-destructive border border-destructive/20' : 'bg-primary/10 text-primary border border-primary/20'}`}>
                    {message.text}
                  </div>
                )}

                <div className="space-y-4 max-w-md">
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
                    <div className="space-y-4 pt-4 border-t">
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
                            Absolute path is recommended
                          </p>
                        </div>
                      )}
                      
                      <div className="pt-2 flex gap-3">
                        <button 
                          onClick={handleSync}
                          disabled={syncing}
                          className="flex items-center gap-2 px-4 py-2 border rounded-md text-sm hover:bg-muted disabled:opacity-50 transition-colors"
                        >
                          <RefreshCw className={`w-4 h-4 ${syncing ? 'animate-spin' : ''}`} />
                          {syncing ? t('syncing', 'Syncing...') : t('syncNow', 'Sync Now')}
                        </button>
                      </div>
                    </div>
                  )}

                  <div className="pt-6 border-t mt-auto">
                    <button 
                      onClick={saveConfig}
                      disabled={loading}
                      className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-md disabled:opacity-50 transition-colors justify-center"
                    >
                      {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
                      {t('saveSettings', 'Save Storage Settings')}
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-muted-foreground">Appearance settings coming soon.</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}