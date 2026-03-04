import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { 
  Save, 
  RefreshCw, 
  HardDrive, 
  Palette, 
  Keyboard as KeyboardIcon, 
  Terminal, 
  FolderOpen, 
  Zap, 
  CircleDot, 
  User, 
  Lock, 
  Key, 
  ShieldCheck, 
  Eye, 
  EyeOff, 
  ChevronLeft,
  Settings as SettingsIcon,
  CheckCircle2,
  AlertCircle,
  Command,
  Monitor,
  Moon,
  Sun,
  Globe,
  PlugZap
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { useTheme } from './ThemeProvider';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './AiEnvironments/icons';

interface StorageConfig {
  storage_type: 'local' | 'git' | 'icloud';
  git_url?: string;
  auth_method?: 'http' | 'ssh';
  http_username?: string;
  http_token?: string;
  ssh_key_path?: string;
  main_shortcut?: string;
  quick_ai_shortcut?: string;
  default_ai_dir?: string;
  default_ai_model?: 'claude' | 'gemini' | 'codex' | 'opencode';
  language?: string;
  local_storage_path?: string;
  icloud_storage_path?: string;
  proxy?: ProxyConfig;
}

interface ProxyConfig {
  proxy_enabled: boolean;
  proxy_type: 'http' | 'https' | 'socks5';
  proxy_host: string;
  proxy_port: number;
  proxy_username?: string;
  proxy_password?: string;
  check_interval: number;
}

interface ProxyStatus {
  is_available: boolean;
  latency_ms: number;
  message: string;
  proxy_type: string;
  proxy_host: string;
}

const modelOptions = [
  { id: 'claude', label: 'Claude Code', Icon: ClaudeIcon },
  { id: 'gemini', label: 'Gemini', Icon: GeminiIcon },
  { id: 'codex', label: 'Codex', Icon: OpenAIIcon },
  { id: 'opencode', label: 'OpenCode', Icon: OpenCodeIcon },
] as const;

export function SettingsView({ initialTab = 'storage', onBack }: { initialTab?: string, onBack: () => void }) {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState(initialTab);
  const [config, setConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  
  // Shortcut Recording States
  const [recordingField, setRecordingField] = useState<'main' | 'quick' | null>(null);

  // Security States
  const [masterPassword, setMasterPassword] = useState('');
  const [showPass, setShowPass] = useState(false);
  const [newPass, setNewPass] = useState('');
  const [oldPassInput, setOldPassInput] = useState('');
  const [changingPass, setChangingPass] = useState(false);

  // Proxy States
  const [proxyConfig, setProxyConfig] = useState<ProxyConfig>({
    proxy_enabled: false,
    proxy_type: 'socks5',
    proxy_host: '',
    proxy_port: 1080,
    proxy_username: '',
    proxy_password: '',
    check_interval: 15,
  });
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [testingProxy, setTestingProxy] = useState(false);
  const [authEnabled, setAuthEnabled] = useState(false);

  useEffect(() => {
    loadConfig();
    if (activeTab === 'security') {
      loadMasterPassword();
    }
  }, [activeTab]);

  const loadMasterPassword = async () => {
    try {
      const pass = await invoke<string>('get_master_password');
      setMasterPassword(pass);
    } catch (e) {
      console.error(e);
    }
  };

  const handleChangeMasterPassword = async () => {
    if (!newPass) return;
    setLoading(true);
    try {
      await invoke('change_master_password', { oldPass: oldPassInput, newPass });
      setMasterPassword(newPass);
      setNewPass('');
      setOldPassInput('');
      setChangingPass(false);
      setMessage({ type: 'success', text: t('passwordChanged', 'Master password changed successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  // Handle keyboard events while recording
  useEffect(() => {
    if (!recordingField) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const modifiers = [];
      if (e.ctrlKey) modifiers.push('Control');
      if (e.altKey) modifiers.push('Alt');
      if (e.shiftKey) modifiers.push('Shift');
      if (e.metaKey) modifiers.push('Command');

      const key = e.key === ' ' ? 'Space' : e.key;
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
        quick_ai_shortcut: cfg.quick_ai_shortcut || 'Alt+Shift+A',
        default_ai_model: cfg.default_ai_model || 'claude'
      });
      
      if (cfg.proxy) {
        setProxyConfig({
          proxy_enabled: cfg.proxy.proxy_enabled,
          proxy_type: cfg.proxy.proxy_type || 'socks5',
          proxy_host: cfg.proxy.proxy_host || '',
          proxy_port: cfg.proxy.proxy_port || 1080,
          proxy_username: cfg.proxy.proxy_username || '',
          proxy_password: cfg.proxy.proxy_password || '',
          check_interval: cfg.proxy.check_interval || 15,
        });
        // Enable auth switch if username or password is set
        setAuthEnabled(!!(cfg.proxy.proxy_username || cfg.proxy.proxy_password));
      }
    } catch (e) {
      console.error(e);
    }
  };

  const saveConfig = async () => {
    setLoading(true);
    setMessage({ type: '', text: '' });
    try {
      const fullConfig = { ...config, proxy: proxyConfig };
      await invoke('save_storage_config', { config: fullConfig });
      
      await invoke('update_shortcuts', { 
        main: config.main_shortcut, 
        quick: config.quick_ai_shortcut 
      });

      if (config.language) {
        await invoke('update_tray_menu', { lang: config.language });
      }

      setMessage({ type: 'success', text: t('settingsSavedHotReload', 'Settings saved! Shortcuts updated immediately.') });
      setTimeout(() => {
        setMessage({ type: '', text: '' });
      }, 3000);
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

  const handleSelectSshKey = async () => {
    try {
      const selected = await open({
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setConfig({...config, ssh_key_path: selected});
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  const handleSelectLocalStoragePath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setConfig({...config, local_storage_path: selected});
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSelectICloudPath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        if (selected.includes('com~apple~CloudDocs')) {
          setConfig({...config, icloud_storage_path: selected});
        } else {
          setMessage({ type: 'error', text: t('invalidIcloudPath', 'Selected folder must be inside iCloud Drive (com~apple~CloudDocs).') });
        }
      }
    } catch (err) {
      console.error(err);
    }
  };

  const toggleLanguage = async () => {
    const newLang = i18n.language === 'zh' ? 'en' : 'zh';
    await i18n.changeLanguage(newLang);
    setConfig(prev => ({ ...prev, language: newLang }));
  };

  const cycleTheme = () => {
    if (theme === 'system') setTheme('dark');
    else if (theme === 'dark') setTheme('light');
    else setTheme('system');
  };

  const sidebarItems = [
    { id: 'storage', name: t('dataStorage', 'Data Storage'), icon: HardDrive },
    { id: 'proxy', name: t('proxy', 'Network Proxy'), icon: Globe },
    { id: 'shortcuts', name: t('shortcuts', 'Shortcuts'), icon: KeyboardIcon },
    { id: 'ai', name: t('aiSessions', 'AI Terminal'), icon: Terminal },
    { id: 'appearance', name: t('appearance', 'Appearance'), icon: Palette },
    { id: 'security', name: t('security', 'Security'), icon: ShieldCheck },
  ];

  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;

  return (
    <div className="flex h-full flex-col bg-background animate-in fade-in slide-in-from-right-4 duration-300">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b shrink-0 bg-card/30 backdrop-blur-sm sticky top-0 z-10">
        <div className="flex items-center gap-4">
          <button 
            onClick={onBack}
            className="p-2 rounded-full hover:bg-muted text-muted-foreground transition-all active:scale-95"
          >
            <ChevronLeft className="w-5 h-5" />
          </button>
          <div className="flex items-center gap-2">
            <SettingsIcon className="w-5 h-5 text-primary" />
            <h1 className="text-xl font-bold tracking-tight">{t('settings')}</h1>
          </div>
        </div>
        
        <div className="flex items-center gap-2">
          {message.text && (
            <div className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium animate-in zoom-in-95 ${
              message.type === 'error' ? 'bg-destructive/10 text-destructive border border-destructive/20' : 'bg-green-500/10 text-green-600 border border-green-500/20'
            }`}>
              {message.type === 'error' ? <AlertCircle className="w-3.5 h-3.5" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
              {message.text}
            </div>
          )}
          <button 
            onClick={saveConfig}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg disabled:opacity-50 transition-all font-semibold shadow-sm active:scale-95"
          >
            {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            {t('saveAllSettings', 'Save')}
          </button>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <div className="w-64 border-r bg-muted/20 flex flex-col shrink-0 p-4 space-y-1">
          {sidebarItems.map(item => (
            <button
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={`w-full flex items-center gap-3 px-4 py-2.5 rounded-xl text-sm transition-all ${
                activeTab === item.id 
                  ? 'bg-primary text-primary-foreground font-medium shadow-md' 
                  : 'hover:bg-muted text-muted-foreground hover:text-foreground'
              }`}
            >
              <item.icon className={`w-4 h-4 ${activeTab === item.id ? 'animate-pulse' : ''}`} />
              {item.name}
            </button>
          ))}
        </div>

        {/* Content Area */}
        <div className="flex-1 overflow-y-auto p-8 bg-background/50">
          <div className="max-w-3xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-500">
            
            {activeTab === 'storage' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('dataStorage', 'Data Storage Location')}</h2>
                    <p className="text-sm text-muted-foreground">{t('dataStorageDesc', 'Configure where OneSpace data is saved and synced.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('storageType', 'Storage Type')}</label>
                      <div className="grid grid-cols-3 gap-2 p-1 bg-muted rounded-xl border">
                        <button 
                          onClick={() => setConfig({...config, storage_type: 'local'})}
                          className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${config.storage_type === 'local' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        >
                          {t('local', 'Local')}
                        </button>
                        <button 
                          onClick={() => setConfig({...config, storage_type: 'icloud'})}
                          className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${config.storage_type === 'icloud' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        >
                          {t('icloud', 'iCloud Drive')}
                        </button>
                        <button 
                          onClick={() => setConfig({...config, storage_type: 'git'})}
                          className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${config.storage_type === 'git' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        >
                          {t('gitRepo', 'Git Repository')}
                        </button>
                      </div>
                    </div>

                    {config.storage_type === 'icloud' && (
                      <div className="space-y-4 pt-4 animate-in fade-in zoom-in-95">
                        <div className="p-4 bg-primary/5 rounded-xl border border-primary/10">
                          <p className="text-sm text-primary/80">
                            {t('icloudDesc', 'Your data will be stored securely in iCloud Drive and synced automatically across your devices.')}
                          </p>
                        </div>
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('icloudStoragePath', 'iCloud Storage Path')}</label>
                          <div className="flex gap-2">
                            <input 
                              type="text" 
                              placeholder="~/Library/Mobile Documents/com~apple~CloudDocs/onespace"
                              className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all font-mono"
                              value={config.icloud_storage_path || ''}
                              onChange={e => setConfig({...config, icloud_storage_path: e.target.value})}
                            />
                            <button 
                              onClick={handleSelectICloudPath}
                              className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                            >
                              <FolderOpen className="w-4 h-4" />
                            </button>
                          </div>
                          <p className="text-[10px] text-muted-foreground leading-relaxed text-yellow-600 dark:text-yellow-500">
                            {t('icloudStoragePathNote', 'Path must be inside iCloud Drive (com~apple~CloudDocs). Changing this will migrate existing data.')}
                          </p>
                        </div>
                      </div>
                    )}

                    {config.storage_type === 'local' && (
                      <div className="space-y-4 pt-4 animate-in fade-in zoom-in-95">
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('localStoragePath', 'Local Storage Path')}</label>
                          <div className="flex gap-2">
                            <input 
                              type="text" 
                              placeholder="~/.config/onespace/data"
                              className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all font-mono"
                              value={config.local_storage_path || ''}
                              onChange={e => setConfig({...config, local_storage_path: e.target.value})}
                            />
                            <button 
                              onClick={handleSelectLocalStoragePath}
                              className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                            >
                              <FolderOpen className="w-4 h-4" />
                            </button>
                          </div>
                          <p className="text-[10px] text-muted-foreground leading-relaxed">
                            {t('localStoragePathNote', 'Default: ~/.config/onespace/data. Changing this will migrate existing local data.')}
                          </p>
                        </div>
                      </div>
                    )}

                    {config.storage_type === 'git' && (
                      <div className="space-y-4 pt-4 animate-in fade-in zoom-in-95">
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('remoteUrl', 'Remote URL')}</label>
                          <input 
                            type="text" 
                            placeholder="https://github.com/user/repo.git"
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all"
                            value={config.git_url || ''}
                            onChange={e => setConfig({...config, git_url: e.target.value})}
                          />
                        </div>

                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('authMethod', 'Authentication Method')}</label>
                          <select 
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            value={config.auth_method || 'http'}
                            onChange={e => setConfig({...config, auth_method: e.target.value as 'http' | 'ssh'})}
                          >
                            <option value="http">{t('httpToken', 'HTTP Token')}</option>
                            <option value="ssh">{t('sshKey', 'SSH Key')}</option>
                          </select>
                        </div>

                        {config.auth_method === 'http' && (
                          <div className="grid grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-2">
                            <div className="space-y-2">
                              <label className="text-sm font-medium text-muted-foreground">{t('username', 'Username')}</label>
                              <div className="relative">
                                <User className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                                <input 
                                  type="text"
                                  className="w-full bg-background border rounded-xl pl-10 pr-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={config.http_username || ''}
                                  onChange={e => setConfig({...config, http_username: e.target.value})}
                                />
                              </div>
                            </div>
                            <div className="space-y-2">
                              <label className="text-sm font-medium text-muted-foreground">{t('token', 'Token / Password')}</label>
                              <div className="relative">
                                <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                                <input 
                                  type="password"
                                  className="w-full bg-background border rounded-xl pl-10 pr-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={config.http_token || ''}
                                  onChange={e => setConfig({...config, http_token: e.target.value})}
                                />
                              </div>
                            </div>
                          </div>
                        )}

                        {config.auth_method === 'ssh' && (
                          <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                            <label className="text-sm font-medium text-muted-foreground">{t('sshKeyPath', 'SSH Private Key Path')}</label>
                            <div className="flex gap-2">
                              <div className="relative flex-1">
                                <Key className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                                <input 
                                  type="text"
                                  placeholder={t('chooseSshKey', 'Choose SSH key file...')}
                                  className="w-full bg-background border rounded-xl pl-10 pr-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 font-mono"
                                  value={config.ssh_key_path || ''}
                                  onChange={e => setConfig({...config, ssh_key_path: e.target.value})}
                                />
                              </div>
                              <button 
                                onClick={handleSelectSshKey}
                                className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                              >
                                <FolderOpen className="w-4 h-4" />
                              </button>
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'shortcuts' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('shortcuts', 'Global Shortcuts')}</h2>
                    <p className="text-sm text-muted-foreground">{t('shortcutsDesc', 'Hotkeys to trigger OneSpace from anywhere.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="space-y-3">
                      <label className="text-sm font-medium text-muted-foreground">{t('toggleMainWindow', 'Toggle Main Window')}</label>
                      <div className="flex items-center gap-4">
                        <div className={`flex-1 flex items-center bg-muted/30 border rounded-xl px-4 py-4 text-sm transition-all h-14 ${recordingField === 'main' ? 'ring-2 ring-primary border-primary bg-primary/5' : ''}`}>
                          {recordingField === 'main' ? (
                            <span className="flex items-center gap-3 text-primary font-bold animate-pulse">
                              <CircleDot className="w-4 h-4" />
                              {t('recordingPlaceholder', 'Press keys...')}
                            </span>
                          ) : (
                            <div className="flex gap-1.5">
                              {config.main_shortcut?.split('+').map((key, i) => (
                                <kbd key={i} className="px-2.5 py-1 bg-background border-b-2 border-x border-t rounded-md font-mono text-sm shadow-sm">
                                  {key === 'Control' ? <Command className="w-3 h-3 inline mr-1" /> : null}
                                  {key}
                                </kbd>
                              ))}
                            </div>
                          )}
                        </div>
                        <button 
                          onClick={() => setRecordingField(recordingField === 'main' ? null : 'main')}
                          className={`px-6 h-14 rounded-xl text-sm font-semibold transition-all active:scale-95 ${
                            recordingField === 'main' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-lg shadow-destructive/20' : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                          }`}
                        >
                          {recordingField === 'main' ? t('stopRecording', 'Stop') : t('record', 'Record')}
                        </button>
                      </div>
                    </div>

                    <div className="space-y-3">
                      <label className="text-sm font-medium text-muted-foreground">{t('toggleQuickAi', 'Quick AI Session Bar')}</label>
                      <div className="flex items-center gap-4">
                        <div className={`flex-1 flex items-center bg-muted/30 border rounded-xl px-4 py-4 text-sm transition-all h-14 ${recordingField === 'quick' ? 'ring-2 ring-primary border-primary bg-primary/5' : ''}`}>
                          {recordingField === 'quick' ? (
                            <span className="flex items-center gap-3 text-primary font-bold animate-pulse">
                              <CircleDot className="w-4 h-4" />
                              {t('recordingPlaceholder', 'Press keys...')}
                            </span>
                          ) : (
                            <div className="flex gap-1.5">
                              {config.quick_ai_shortcut?.split('+').map((key, i) => (
                                <kbd key={i} className="px-2.5 py-1 bg-background border-b-2 border-x border-t rounded-md font-mono text-sm shadow-sm">
                                  {key === 'Control' ? <Command className="w-3 h-3 inline mr-1" /> : null}
                                  {key}
                                </kbd>
                              ))}
                            </div>
                          )}
                        </div>
                        <button 
                          onClick={() => setRecordingField(recordingField === 'quick' ? null : 'quick')}
                          className={`px-6 h-14 rounded-xl text-sm font-semibold transition-all active:scale-95 ${
                            recordingField === 'quick' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-lg shadow-destructive/20' : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                          }`}
                        >
                          {recordingField === 'quick' ? t('stopRecording', 'Stop') : t('record', 'Record')}
                        </button>
                      </div>
                    </div>

                    <div className="p-4 bg-primary/5 rounded-xl border border-primary/10 flex gap-3">
                      <Zap className="w-5 h-5 text-primary shrink-0 mt-0.5" />
                      <p className="text-xs text-primary/80 leading-relaxed italic">
                        {t('shortcutsNote', 'Tip: You can use combinations like Command+Shift+K or Alt+Space.')}
                      </p>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'ai' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('aiSessions', 'AI Terminal Sessions')}</h2>
                    <p className="text-sm text-muted-foreground">{t('aiSessionsDesc', 'Default configuration for quick AI terminal sessions.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-4">
                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('defaultAiModel', 'Default Model')}</label>
                      <div className="grid grid-cols-2 gap-2">
                        {modelOptions.map(({ id, label, Icon }) => {
                          const active = (config.default_ai_model || 'claude') === id;
                          return (
                            <button
                              key={id}
                              type="button"
                              onClick={() => setConfig({...config, default_ai_model: id as StorageConfig['default_ai_model']})}
                              className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                                active
                                  ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                                  : 'bg-background hover:bg-muted/50 text-foreground border-border'
                              }`}
                            >
                              <Icon className="w-4 h-4 shrink-0" />
                              <span className="truncate">{label}</span>
                            </button>
                          );
                        })}
                      </div>
                      <p className="text-xs text-muted-foreground">{t('defaultAiModelDesc', 'Preselected model for the Quick AI Session Bar.')}</p>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('defaultAiPath', 'Default Project Directory')}</label>
                      <div className="flex gap-2">
                        <input 
                          type="text" 
                          readOnly
                          className="flex-1 bg-muted/50 border rounded-xl px-4 py-2.5 text-sm text-muted-foreground font-mono truncate cursor-default"
                          value={config.default_ai_dir || t('notSet', 'Not Set')}
                        />
                        <button 
                          onClick={handleSelectDefaultDir}
                          className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                        >
                          <FolderOpen className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('appearance', 'Appearance & Locale')}</h2>
                    <p className="text-sm text-muted-foreground">{t('appearanceDesc', 'Customize how OneSpace looks and feels.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-8">
                    {/* Theme */}
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('theme', 'App Theme')}</h3>
                        <p className="text-xs text-muted-foreground">{t('themeDesc', 'Select your preferred visual theme.')}</p>
                      </div>
                      <button 
                        onClick={cycleTheme}
                        className="flex items-center gap-2 px-4 py-2 bg-muted hover:bg-muted/80 rounded-xl transition-all"
                      >
                        <ThemeIcon className="w-4 h-4" />
                        <span className="text-sm capitalize">{theme}</span>
                      </button>
                    </div>

                    <hr className="border-border/50" />

                    {/* Language */}
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('language', 'Language')}</h3>
                        <p className="text-xs text-muted-foreground">{t('languageDesc', 'Choose the language for the user interface.')}</p>
                      </div>
                      <button 
                        onClick={toggleLanguage}
                        className="px-4 py-2 bg-muted hover:bg-muted/80 rounded-xl transition-all text-sm font-medium"
                      >
                        {i18n.language === 'zh' ? '简体中文' : 'English'}
                      </button>
                    </div>
                  </div>
                </section>
              </div>
            )}

                        {activeTab === 'proxy' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('proxySettings', 'Network Proxy Settings')}</h2>
                    <p className="text-sm text-muted-foreground">{t('proxySettingsDesc', 'Configure proxy for backend network requests')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    {/* Enable Proxy Switch */}
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('proxyEnabled', 'Enable Proxy')}</h3>
                        <p className="text-xs text-muted-foreground">{t('proxyEnabledDesc', 'All backend requests will use the proxy')}</p>
                      </div>
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          className="sr-only peer"
                          checked={proxyConfig.proxy_enabled}
                          onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_enabled: e.target.checked })}
                        />
                        <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                      </label>
                    </div>

                    {/* Conditional Content - Only show when proxy is enabled */}
                    {proxyConfig.proxy_enabled && (
                      <>
                        <hr className="border-border/50 animate-in fade-in" />

                        {/* Proxy Type - Tab Style */}
                        <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                          <label className="text-sm font-medium">{t('proxyType', 'Proxy Type')}</label>
                          <div className="grid grid-cols-3 gap-2 p-1 bg-muted rounded-xl border">
                            {(['http', 'https', 'socks5'] as const).map((type) => (
                              <button
                                key={type}
                                onClick={() => setProxyConfig({ ...proxyConfig, proxy_type: type })}
                                className={`py-2.5 px-4 rounded-lg text-sm font-medium transition-all ${
                                  proxyConfig.proxy_type === type
                                    ? 'bg-background shadow-sm text-foreground'
                                    : 'text-muted-foreground hover:text-foreground'
                                }`}
                              >
                                {type.toUpperCase()}
                              </button>
                            ))}
                          </div>
                        </div>

                        {/* Host and Port */}
                        <div className="grid grid-cols-3 gap-4 animate-in fade-in slide-in-from-top-2">
                          <div className="col-span-2 space-y-2">
                            <label className="text-sm font-medium">{t('proxyHost', 'Proxy Host')}</label>
                            <input
                              type="text"
                              placeholder="127.0.0.1"
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={proxyConfig.proxy_host}
                              onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_host: e.target.value })}
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-medium">{t('proxyPort', 'Port')}</label>
                            <input
                              type="number"
                              placeholder="1080"
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={proxyConfig.proxy_port}
                              onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_port: parseInt(e.target.value) || 0 })}
                            />
                          </div>
                        </div>

                        {/* Authentication Switch */}
                        <div className="space-y-3 animate-in fade-in slide-in-from-top-2">
                          <div className="flex items-center justify-between">
                            <div className="space-y-0.5">
                              <h3 className="text-sm font-medium">{t('proxyAuth', 'Authentication')}</h3>
                              <p className="text-xs text-muted-foreground">{t('proxyAuthDesc', 'Enable if your proxy requires credentials')}</p>
                            </div>
                            <label className="relative inline-flex items-center cursor-pointer">
                              <input
                                type="checkbox"
                                className="sr-only peer"
                                checked={authEnabled}
                                onChange={(e) => {
                                  setAuthEnabled(e.target.checked);
                                  if (!e.target.checked) {
                                    setProxyConfig({ ...proxyConfig, proxy_username: '', proxy_password: '' });
                                  }
                                }}
                              />
                              <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                            </label>
                          </div>

                          {authEnabled && (
                            <div className="grid grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-2">
                              <div className="space-y-2">
                                <label className="text-sm font-medium">{t('proxyUsername', 'Username')}</label>
                                <input
                                  type="text"
                                  className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={proxyConfig.proxy_username}
                                  onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_username: e.target.value })}
                                />
                              </div>
                              <div className="space-y-2">
                                <label className="text-sm font-medium">{t('proxyPassword', 'Password')}</label>
                                <input
                                  type="password"
                                  className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={proxyConfig.proxy_password}
                                  onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_password: e.target.value })}
                                />
                              </div>
                            </div>
                          )}
                        </div>

                        {/* Check Interval - Quick Select */}
                        <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                          <label className="text-sm font-medium">{t('checkInterval', 'Check Interval')}</label>
                          <div className="grid grid-cols-4 gap-2">
                            {[
                              { value: 5, label: t('interval5min', '5 min') },
                              { value: 15, label: t('interval15min', '15 min') },
                              { value: 30, label: t('interval30min', '30 min') },
                              { value: 60, label: t('interval1h', '1 hour') },
                            ].map((item) => (
                              <button
                                key={item.value}
                                onClick={() => setProxyConfig({ ...proxyConfig, check_interval: item.value })}
                                className={`py-2.5 px-3 rounded-lg text-sm font-medium transition-all ${
                                  proxyConfig.check_interval === item.value
                                    ? 'bg-primary text-primary-foreground shadow-sm'
                                    : 'bg-muted text-muted-foreground hover:bg-muted/80'
                                }`}
                              >
                                {item.label}
                              </button>
                            ))}
                          </div>
                        </div>

                        {/* Test Button */}
                        <div className="flex items-center gap-4 pt-4 border-t animate-in fade-in slide-in-from-top-2">
                          <button
                            onClick={async () => {
                              setTestingProxy(true);
                              try {
                                // Test with current form config (even if not saved yet)
                                const status = await invoke<ProxyStatus>('test_proxy_connection', {
                                  config: proxyConfig
                                });
                                setProxyStatus(status);
                              } catch (e: any) {
                                setProxyStatus({
                                  is_available: false,
                                  latency_ms: 0,
                                  message: e.toString(),
                                  proxy_type: proxyConfig.proxy_type,
                                  proxy_host: proxyConfig.proxy_host,
                                });
                              } finally {
                                setTestingProxy(false);
                              }
                            }}
                            disabled={testingProxy || !proxyConfig.proxy_host}
                            className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-xl text-sm font-medium hover:bg-primary/90 disabled:opacity-50"
                          >
                            {testingProxy ? (
                              <RefreshCw className="w-4 h-4 animate-spin" />
                            ) : (
                              <PlugZap className="w-4 h-4" />
                            )}
                            {t('testProxy', 'Test Connection')}
                          </button>

                          {proxyStatus && (
                            <div className={`flex items-center gap-2 text-sm ${
                              proxyStatus.is_available ? 'text-green-600' : 'text-red-600'
                            }`}>
                              {proxyStatus.is_available ? (
                                <CheckCircle2 className="w-4 h-4" />
                              ) : (
                                <AlertCircle className="w-4 h-4" />
                              )}
                              {proxyStatus.message} {proxyStatus.latency_ms > 0 && `(${proxyStatus.latency_ms}ms)`}
                            </div>
                          )}
                        </div>
                      </>
                    )}
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'security' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('security', 'Data Security')}</h2>
                    <p className="text-sm text-muted-foreground">{t('securityDesc', 'Manage your master password used for encrypting sensitive data.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="bg-muted/30 p-5 rounded-2xl border border-dashed space-y-4">
                      <div className="flex items-center justify-between">
                        <label className="text-sm font-medium text-muted-foreground">{t('currentMasterPassword', 'Current Master Password')}</label>
                        <ShieldCheck className="w-5 h-5 text-primary opacity-50" />
                      </div>
                      
                      <div className="relative">
                        <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                        <input 
                          type={showPass ? 'text' : 'password'}
                          readOnly
                          className="w-full bg-background border rounded-xl pl-10 pr-12 py-3 text-sm font-mono tracking-widest"
                          value={masterPassword}
                        />
                        <button 
                          onClick={() => setShowPass(!showPass)}
                          className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
                        >
                          {showPass ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                        </button>
                      </div>
                      <p className="text-[10px] text-muted-foreground leading-relaxed">
                        {t('defaultPassNotice', 'Note: This key is used for encrypting Git credentials and other sensitive data locally.')}
                      </p>
                    </div>

                    {!changingPass ? (
                      <button 
                        onClick={() => setChangingPass(true)}
                        className="w-full py-3 border border-primary/20 bg-primary/5 text-primary rounded-xl text-sm font-semibold hover:bg-primary/10 transition-all"
                      >
                        {t('changeMasterPassword', 'Change Master Password')}
                      </button>
                    ) : (
                      <div className="space-y-4 pt-4 border-t animate-in fade-in slide-in-from-top-2">
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('oldPassword', 'Old Password')}</label>
                          <input 
                            type="password"
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            value={oldPassInput}
                            onChange={e => setOldPassInput(e.target.value)}
                          />
                        </div>
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('newPassword', 'New Password')}</label>
                          <input 
                            type="password"
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            value={newPass}
                            onChange={e => setNewPass(e.target.value)}
                          />
                        </div>
                        <div className="flex gap-2 pt-2">
                          <button 
                            onClick={handleChangeMasterPassword}
                            disabled={!newPass || !oldPassInput || loading}
                            className="flex-1 bg-primary text-primary-foreground py-2.5 rounded-xl text-sm font-semibold hover:bg-primary/90 disabled:opacity-50"
                          >
                            {loading ? <RefreshCw className="w-4 h-4 animate-spin mx-auto" /> : t('confirmChange', 'Update Password')}
                          </button>
                          <button 
                            onClick={() => {
                              setChangingPass(false);
                              setNewPass('');
                              setOldPassInput('');
                            }}
                            className="px-6 py-2.5 border rounded-xl text-sm font-medium hover:bg-muted transition-all"
                          >
                            {t('cancel', 'Cancel')}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </section>
              </div>
            )}
            
            {/* Bottom Spacing */}
            <div className="h-20" />
          </div>
        </div>
      </div>
    </div>
  );
}
