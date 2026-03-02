import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { X, Save, RefreshCw, HardDrive, Palette, Keyboard, Terminal, FolderOpen, Zap, CircleDot } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';

interface StorageConfig {
  storage_type: 'local' | 'git';
  git_url?: string;
  auth_method?: 'http' | 'ssh';
  http_username?: string;
  http_token?: string;
  ssh_key_path?: string;
  main_shortcut?: string;
  quick_ai_shortcut?: string;
  default_ai_dir?: string;
}

export function SettingsModal({ open: isOpen, onClose }: { open: boolean, onClose: () => void }) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState('storage');
  const [config, setConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  
  // Shortcut Recording States
  const [recordingField, setRecordingField] = useState<'main' | 'quick' | null>(null);

  useEffect(() => {
    if (isOpen) {
      loadConfig();
    }
  }, [isOpen]);

  // Handle keyboard events while recording
  useEffect(() => {
    if (!recordingField) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      // Only stop on actual keys, not just modifiers
      const modifiers = [];
      if (e.ctrlKey) modifiers.push('Control');
      if (e.altKey) modifiers.push('Alt');
      if (e.shiftKey) modifiers.push('Shift');
      if (e.metaKey) modifiers.push('Command');

      const key = e.key === ' ' ? 'Space' : e.key;
      
      // Ignore if it's only a modifier key being pressed alone
      const isModifierOnly = ['Control', 'Alt', 'Shift', 'Meta'].includes(e.key);
      
      if (!isModifierOnly) {
        let finalShortcut = '';
        if (modifiers.length > 0) {
          finalShortcut = modifiers.join('+') + '+' + key.charAt(0).toUpperCase() + key.slice(1);
        } else {
          finalShortcut = key.charAt(0).toUpperCase() + key.slice(1);
        }

        if (recordingField === 'main') {
          setConfig(prev => ({ ...prev, main_shortcut: finalShortcut }));
        } else {
          setConfig(prev => ({ ...prev, quick_ai_shortcut: finalShortcut }));
        }
        setRecordingField(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [recordingField]);

  const loadConfig = async () => {
    try {
      const cfg = await invoke<StorageConfig>('get_storage_config');
      setConfig({
        ...cfg,
        storage_type: cfg.storage_type || 'local',
        auth_method: cfg.auth_method || 'http',
        main_shortcut: cfg.main_shortcut || 'Alt+Space',
        quick_ai_shortcut: cfg.quick_ai_shortcut || 'Alt+Shift+A'
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
      
      // Notify backend to hot-reload shortcuts
      await invoke('update_shortcuts', { 
        main: config.main_shortcut, 
        quick: config.quick_ai_shortcut 
      });

      setMessage({ type: 'success', text: t('settingsSavedHotReload', 'Settings saved! Shortcuts updated immediately.') });
      setTimeout(() => {
        setMessage({ type: '', text: '' });
        onClose();
      }, 2000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleSelectDefaultDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setConfig({...config, default_ai_dir: selected});
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  if (!isOpen) return null;

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
              onClick={() => setActiveTab('shortcuts')}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === 'shortcuts' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-muted text-muted-foreground'
              }`}
            >
              <Keyboard className="w-4 h-4" />
              {t('shortcuts', 'Shortcuts')}
            </button>
            <button
              onClick={() => setActiveTab('ai')}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === 'ai' ? 'bg-primary/10 text-primary font-medium' : 'hover:bg-muted text-muted-foreground'
              }`}
            >
              <Terminal className="w-4 h-4" />
              {t('aiSessions', 'AI Terminal Sessions')}
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
            {message.text && (
              <div className={`mb-6 p-3 rounded-md text-sm flex items-center gap-2 ${message.type === 'error' ? 'bg-destructive/10 text-destructive border border-destructive/20' : 'bg-green-500/10 text-green-600 border border-green-500/20'}`}>
                <Zap className="w-4 h-4" />
                {message.text}
              </div>
            )}

            {activeTab === 'storage' && (
              <div className="space-y-6">
                <div>
                  <h3 className="font-semibold text-lg">{t('dataStorage', 'Data Storage Location')}</h3>
                  <p className="text-sm text-muted-foreground mt-1">{t('dataStorageDesc', 'Configure where OneSpace data is saved and synced.')}</p>
                </div>

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
                          placeholder="https://github.com/user/repo.git"
                          className="w-full bg-background border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                          value={config.git_url || ''}
                          onChange={e => setConfig({...config, git_url: e.target.value})}
                        />
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {activeTab === 'shortcuts' && (
              <div className="space-y-6">
                <div>
                  <h3 className="font-semibold text-lg">{t('shortcuts', 'Global Shortcuts')}</h3>
                  <p className="text-sm text-muted-foreground mt-1">{t('shortcutsDesc', 'Hotkeys to trigger OneSpace from anywhere.')}</p>
                </div>

                <div className="space-y-4 max-w-md">
                  <div className="space-y-2">
                    <label className="text-sm font-medium text-muted-foreground">{t('toggleMainWindow', 'Toggle Main Window')}</label>
                    <div className="flex gap-2">
                      <div className={`flex-1 flex items-center bg-background border rounded-md px-3 py-2 text-sm transition-all ${recordingField === 'main' ? 'ring-2 ring-primary border-primary' : ''}`}>
                        {recordingField === 'main' ? (
                          <span className="flex items-center gap-2 text-primary font-medium animate-pulse">
                            <CircleDot className="w-3.5 h-3.5" />
                            {t('recordingPlaceholder', 'Press keys...')}
                          </span>
                        ) : (
                          <span className="font-mono">{config.main_shortcut || 'Not Set'}</span>
                        )}
                      </div>
                      <button 
                        onClick={() => setRecordingField(recordingField === 'main' ? null : 'main')}
                        className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                          recordingField === 'main' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90' : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                        }`}
                      >
                        {recordingField === 'main' ? t('stopRecording', 'Stop') : t('record', 'Record')}
                      </button>
                    </div>
                  </div>

                  <div className="space-y-2">
                    <label className="text-sm font-medium text-muted-foreground">{t('toggleQuickAi', 'Quick AI Session Bar')}</label>
                    <div className="flex gap-2">
                      <div className={`flex-1 flex items-center bg-background border rounded-md px-3 py-2 text-sm transition-all ${recordingField === 'quick' ? 'ring-2 ring-primary border-primary' : ''}`}>
                        {recordingField === 'quick' ? (
                          <span className="flex items-center gap-2 text-primary font-medium animate-pulse">
                            <CircleDot className="w-3.5 h-3.5" />
                            {t('recordingPlaceholder', 'Press keys...')}
                          </span>
                        ) : (
                          <span className="font-mono">{config.quick_ai_shortcut || 'Not Set'}</span>
                        )}
                      </div>
                      <button 
                        onClick={() => setRecordingField(recordingField === 'quick' ? null : 'quick')}
                        className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                          recordingField === 'quick' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90' : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                        }`}
                      >
                        {recordingField === 'quick' ? t('stopRecording', 'Stop') : t('record', 'Record')}
                      </button>
                    </div>
                  </div>
                  
                  <p className="text-xs text-muted-foreground bg-muted/50 p-2 rounded border border-dashed italic">
                    {t('shortcutsNote')}
                  </p>
                </div>
              </div>
            )}

            {activeTab === 'ai' && (
              <div className="space-y-6">
                <div>
                  <h3 className="font-semibold text-lg">{t('aiSessions', 'AI Terminal Sessions Defaults')}</h3>
                  <p className="text-sm text-muted-foreground mt-1">{t('aiSessionsDesc', 'Default configuration for quick sessions.')}</p>
                </div>

                <div className="space-y-4 max-w-md">
                  <div className="space-y-2">
                    <label className="text-sm font-medium text-muted-foreground">{t('defaultAiPath', 'Default Project Directory')}</label>
                    <div className="flex gap-2">
                      <input 
                        type="text" 
                        readOnly
                        placeholder={t('chooseDefaultPath', 'Choose default path...')}
                        className="flex-1 bg-muted/30 border rounded-md px-3 py-2 text-sm text-muted-foreground font-mono truncate cursor-not-allowed"
                        value={config.default_ai_dir || ''}
                      />
                      <button 
                        onClick={handleSelectDefaultDir}
                        className="px-3 py-2 bg-secondary text-secondary-foreground rounded-md text-sm font-medium hover:bg-secondary/80 transition-colors"
                      >
                        <FolderOpen className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="text-muted-foreground">{t('appearanceSoon')}</div>
            )}
          </div>

          <div className="p-6 border-t bg-card shrink-0">
            <button 
              onClick={saveConfig}
              disabled={loading}
              className="w-full flex items-center gap-2 px-4 py-2.5 text-sm bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg disabled:opacity-50 transition-all justify-center font-semibold shadow-sm"
            >
              {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-5 h-5" />}
              {t('saveAllSettings', 'Save All Settings')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
