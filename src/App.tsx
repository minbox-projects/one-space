import { useState, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { useTheme } from './components/ThemeProvider';
import { 
   Rocket, 
   Terminal, 
   Server, 
   Code2, 
   Star, 
   StickyNote, 
   Search, 
   Cloud, 
   Mail as MailIcon,
   Settings,
   Moon,
   Sun,
   Monitor,
   Cpu,
   BookOpen,
   Info,
   Loader2,
   CheckCircle2,
   AlertCircle
} from 'lucide-react';
import { AiSessions } from './components/AiSessions';
import { AiEnvironments } from './components/AiEnvironments';
import { MCPServers } from './components/MCPServers';
import { SshServers } from './components/SshServers';
import { Snippets } from './components/Snippets';
import { Bookmarks } from './components/Bookmarks';
import { Notes } from './components/Notes';
import { CloudDrive } from './components/CloudDrive';
import { Mail } from './components/Mail';
import { OmniSearch } from './components/OmniSearch';
import { Launcher } from './components/Launcher';
import { SettingsView } from './components/SettingsView';
import { AboutModal } from './components/AboutModal';
import { QuickAiSessionBar } from './components/QuickAiSessionBar';
import { Documentation } from './components/Documentation';
import { OnboardingWizard } from './components/OnboardingWizard';

import { getUnreadEmailCount } from './lib/gmail';
import logoWhite from './assets/onespace_logo_white.png';
import logoBlack from './assets/onespace_logo_black.png';

type ApiResp<T> = { ok: boolean; data: T; meta: { schema_version: number; revision: number } };

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  
  // URL View Routing
  const queryParams = new URLSearchParams(window.location.search);
  const view = queryParams.get('view');

  const [activeTab, setActiveTab] = useState('ai-sessions');
  const [previousTab, setPreviousTab] = useState('ai-sessions');
  const [omniOpen, setOmniOpen] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState('storage');
  const [aboutOpen, setAboutOpen] = useState(false);
  const [storageType, setStorageType] = useState<'local' | 'git' | 'icloud'>('local');
  const [onboardingStatus, setOnboardingStatus] = useState<'checking' | 'required' | 'done'>('checking');

  // Git Sync Status
  const [syncStatus, setSyncStatus] = useState<'idle' | 'pulling' | 'pushing' | 'success' | 'error'>('idle');
  const [syncError, setSyncError] = useState<string | null>(null);

  // Global counts for sidebar
  const [counts, setCounts] = useState({
    launcher: 0,
    sessions: 0,
    ssh: 0,
    snippets: 0,
    bookmarks: 0,
    notes: 0,
    mail: 0,
    environments: 0
  });

  const isTauri = '__TAURI_INTERNALS__' in window;
  const handleDragMouseDown = (e: React.MouseEvent<HTMLElement>) => {
    const target = e.target as HTMLElement;
    if (target.closest('button,input,select,textarea,a,[role="button"],[data-no-drag]')) {
      return;
    }
    if (!isTauri) return;
    getCurrentWindow().startDragging().catch(() => {});
  };

  const loadCounts = async () => {
    const newCounts = { ...counts };
    
    // 1. First load: Local data (Very Fast)
    const savedLauncher = localStorage.getItem('onespace_launcher_items');
    if (savedLauncher) newCounts.launcher = JSON.parse(savedLauncher).length;
    else newCounts.launcher = 3; 
    
    if (isTauri) {
      try {
        const [aiSessions, sshHosts, snippetsStr, bookmarksStr, notesStr, aiProvidersState, storageCfg] = await Promise.all([
          invoke<ApiResp<any[]>>('sessions_list').catch(() => ({
            ok: true,
            data: [],
            meta: { schema_version: 0, revision: 0 }
          } as ApiResp<any[]>),
          ),
          invoke('get_ssh_hosts').catch(() => []),
          invoke('read_snippets').catch(() => "[]"),
          invoke('read_bookmarks').catch(() => "[]"),
          invoke('read_notes').catch(() => "[]"),
          invoke<ApiResp<{ providers: any[] }>>('providers_list').catch(
            () => ({
              ok: true,
              data: { providers: [] },
              meta: { schema_version: 0, revision: 0 }
            } as ApiResp<{ providers: any[] }>),
          ),
          invoke<any>('get_storage_config').catch(() => ({}))
        ]);

        newCounts.sessions = (aiSessions as any).data?.length || 0;
        newCounts.ssh = (sshHosts as any[]).length;
        newCounts.snippets = JSON.parse(snippetsStr as string).length;
        newCounts.bookmarks = JSON.parse(bookmarksStr as string).length;
        newCounts.notes = JSON.parse(notesStr as string).length;
        newCounts.environments = (aiProvidersState as any).data?.providers?.length || 0;
        
        if (storageCfg.storage_type) {
          setStorageType(storageCfg.storage_type);
        }

        // Immediately update UI with local data
        setCounts({ ...newCounts });
      } catch (e) {
        console.error("Failed to load local counts", e);
      }
    }

    // 2. Second load: Remote network data (Slow, Async)
    getUnreadEmailCount().then(mailCount => {
      setCounts(prev => ({ ...prev, mail: mailCount }));
    }).catch(() => {});
  };

  // Expose global navigation for components
  useEffect(() => {
    (window as any).setActiveTab = setActiveTab;
    (window as any).setSettingsOpen = (open: boolean) => {
      if (open) {
        setPreviousTab(activeTab);
        setActiveTab('settings');
      } else {
        setActiveTab(previousTab);
      }
    };
    (window as any).setSettingsTab = setSettingsInitialTab;
  }, [activeTab, previousTab]);

  useEffect(() => {
    if (!isTauri) {
      setOnboardingStatus('done');
      return;
    }
    invoke<boolean>('should_show_onboarding')
      .then((shouldShow) => {
        setOnboardingStatus(shouldShow ? 'required' : 'done');
      })
      .catch(() => setOnboardingStatus('done'));
  }, []);

  // Initial load and poll
  useEffect(() => {
    if (onboardingStatus !== 'done') {
      return;
    }

    let unlisten: (() => void) | undefined;
    let unlistenSync: (() => void) | undefined;

    if (isTauri) {
      invoke('show_main_window').catch(console.error);
      setTimeout(() => {
        loadCounts();
      }, 500);

      setTimeout(() => {
        invoke('sync_run_now').catch(e => console.error("Sync failed:", e));
      }, 3000);

      listen('trigger-sync', () => {
        invoke('sync_run_now').catch(e => console.error("Tray Sync failed:", e));
      });

      listen('refresh-counts', () => {
        loadCounts();
      });

      listen('refresh-mail-count', () => {
        loadCounts();
      }).then(fn => {
        unlisten = fn;
      });

       listen('git-sync-status', (event: any) => {
         const payload = event.payload as { status: string, message?: string };
         const status = payload.status as 'pulling' | 'pushing' | 'success' | 'error';
         setSyncStatus(status);
         if (status === 'error') {
           setSyncError(payload.message || 'Unknown sync error');
         } else {
           setSyncError(null);
         }

         if (status === 'success') {
           loadCounts();
           setTimeout(() => setSyncStatus('idle'), 3000);
         }
       }).then(fn => {
        unlistenSync = fn;
      });

      invoke<any>('get_storage_config').then(cfg => {
        if (cfg.language) {
          i18n.changeLanguage(cfg.language);
        }
        if (cfg.storage_type) {
          setStorageType(cfg.storage_type);
        }
      }).catch(e => console.error("Failed to load language", e));
    }
    
    let timeoutId: any;
    const pollCounts = async () => {
      await loadCounts();
      timeoutId = setTimeout(pollCounts, 15000);
    };
    pollCounts();

    return () => {
      if (timeoutId) clearTimeout(timeoutId);
      if (unlisten) unlisten();
      if (unlistenSync) unlistenSync();
    };
  }, [onboardingStatus]);

  const navigation = useMemo(() => [
    { id: 'launcher', name: t('launcher'), icon: Rocket, count: counts.launcher },
    { id: 'ai-sessions', name: t('aiSessions'), icon: Terminal, count: counts.sessions },
    { id: 'ai-environments', name: t('aiEnvironments'), icon: Cpu, count: counts.environments },
    { id: 'mcp-servers', name: 'MCP Servers', icon: Server, count: undefined },
    { id: 'ssh', name: t('sshServers'), icon: Server, count: counts.ssh },
    { id: 'snippets', name: t('snippets'), icon: Code2, count: counts.snippets },
    { id: 'bookmarks', name: t('bookmarks'), icon: Star, count: counts.bookmarks },
    { id: 'notes', name: t('notes'), icon: StickyNote, count: counts.notes },
    { id: 'cloud', name: t('cloudDrive'), icon: Cloud },
    { id: 'mail', name: t('mail'), icon: MailIcon, count: counts.mail > 0 ? counts.mail : undefined },
  ], [t, counts]);

  const toggleLanguage = async () => {
    const newLang = i18n.language === 'zh' ? 'en' : 'zh';
    await i18n.changeLanguage(newLang);
    
    if (isTauri) {
      try {
        const cfg = await invoke<any>('get_storage_config');
        await invoke('save_storage_config', { config: { ...cfg, language: newLang } });
        await invoke('update_tray_menu', { lang: newLang });
      } catch (e) {
        console.error('Failed to save language preference:', e);
      }
    }
  };

  const cycleTheme = () => {
    if (theme === 'system') setTheme('dark');
    else if (theme === 'dark') setTheme('light');
    else setTheme('system');
  };

  const copySyncError = () => {
    if (syncError) {
      navigator.clipboard.writeText(syncError);
      const originalError = syncError;
      setSyncError(t('copied', 'Copied!'));
      setTimeout(() => setSyncError(originalError), 2000);
    }
  };

  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;
  const themeLabel = theme === 'system' ? t('themeSystem') : theme === 'dark' ? t('themeDark') : t('themeLight');

  const resolvedTheme = useMemo(() => theme === 'system' 
    ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
    : theme, [theme]);

  // If we are in quick-ai view, render only that component
  if (view === 'quick-ai') {
    return <QuickAiSessionBar />;
  }

  if (onboardingStatus === 'checking') {
    return (
      <div className="h-screen w-screen bg-background text-foreground flex items-center justify-center">
        <div className="text-sm text-muted-foreground">{t('loading', 'Loading...')}</div>
      </div>
    );
  }

  if (onboardingStatus === 'required') {
    return (
      <OnboardingWizard
        onComplete={(nextStorageType) => {
          setStorageType(nextStorageType);
          setOnboardingStatus('done');
        }}
      />
    );
  }

  const renderContent = () => {
    return (
      <div className="h-full relative">
        <div className={activeTab === 'launcher' ? 'h-full' : 'hidden'}><Launcher /></div>
        <div className={activeTab === 'ai-sessions' ? 'h-full' : 'hidden'}>
          <AiSessions onNavigate={(tab, hash) => {
            setActiveTab(tab);
            if (hash) window.location.hash = hash;
          }} />
        </div>
        <div className={activeTab === 'ai-environments' ? 'h-full' : 'hidden'}>
          <AiEnvironments isVisible={activeTab === 'ai-environments'} />
        </div>
        <div className={activeTab === 'mcp-servers' ? 'h-full' : 'hidden'}><MCPServers /></div>
        <div className={activeTab === 'ssh' ? 'h-full' : 'hidden'}><SshServers /></div>
        <div className={activeTab === 'snippets' ? 'h-full' : 'hidden'}><Snippets /></div>
        <div className={activeTab === 'bookmarks' ? 'h-full' : 'hidden'}><Bookmarks /></div>
        <div className={activeTab === 'notes' ? 'h-full' : 'hidden'}><Notes /></div>
        <div className={activeTab === 'documentation' ? 'h-full' : 'hidden'}><Documentation /></div>
        <div className={activeTab === 'cloud' ? 'h-full' : 'hidden'}><CloudDrive /></div>
        <div className={activeTab === 'mail' ? 'h-full' : 'hidden'}><Mail /></div>
        <div className={activeTab === 'settings' ? 'h-full' : 'hidden'}>
          <SettingsView 
            initialTab={settingsInitialTab} 
            onBack={() => {
              setActiveTab(previousTab);
              setSettingsInitialTab('storage');
              loadCounts();
            }} 
          />
        </div>
      </div>
    );
  };

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden select-none">
      <div className="w-64 border-r bg-muted/20 flex flex-col">
        <div 
          className="h-16 flex items-end pl-5 pr-4 pb-1.5 border-b font-semibold tracking-tight cursor-default select-none relative"
          data-tauri-drag-region
          onMouseDown={handleDragMouseDown}
        >
          <div className="flex items-center gap-2 pointer-events-none">
            <img 
              src={resolvedTheme === 'dark' ? logoWhite : logoBlack} 
              alt="OneSpace" 
              className="w-5 h-5"
            />
            <span className="text-lg">OneSpace</span>
          </div>
        </div>
        
        <div className="flex-1 overflow-y-auto py-4 px-3 space-y-1">
          {navigation.map((item: any) => (
            <button
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={`w-full flex items-center justify-between px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === item.id 
                  ? 'bg-primary text-primary-foreground font-medium shadow-sm' 
                  : 'hover:bg-muted text-muted-foreground hover:text-foreground'
              }`}
            >
              <div className="flex items-center gap-3">
                <item.icon className="w-4 h-4" />
                <span>{item.name}</span>
              </div>
              {item.count !== undefined && (
                <span className={`text-[10px] px-1.5 py-0.5 rounded-full font-mono ${
                  activeTab === item.id 
                    ? 'bg-primary-foreground/20 text-primary-foreground' 
                    : 'bg-muted-foreground/10 text-muted-foreground'
                }`}>
                  {item.count}
                </span>
              )}
            </button>
          ))}
        </div>

        <div className="p-3 border-t space-y-1">
          <button 
            onClick={() => {
              setPreviousTab(activeTab);
              setActiveTab('settings');
            }}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
              activeTab === 'settings' 
                ? 'bg-primary/10 text-primary font-medium' 
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'
            }`}
          >
            <Settings className="w-4 h-4" />
            {t('settings')}
          </button>
          <button 
            onClick={() => setActiveTab('documentation')}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
              activeTab === 'documentation' 
                ? 'bg-primary/10 text-primary font-medium' 
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'
            }`}
          >
            <BookOpen className="w-4 h-4" />
            {t('usageDocs')}
          </button>
          <button 
            onClick={() => setAboutOpen(true)}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Info className="w-4 h-4" />
            {t('about')}
          </button>
        </div>
      </div>

      <div className="flex-1 flex flex-col overflow-hidden relative bg-background">
        {activeTab !== 'settings' && (
          <header 
            className="h-16 border-b flex items-end px-6 pb-1.5 justify-between bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 relative"
            data-tauri-drag-region
            onMouseDown={handleDragMouseDown}
          >
            <div className="flex-1 flex items-center gap-4">
              <button 
                onClick={() => setOmniOpen(true)}
                className="flex items-center justify-between w-full max-w-[320px] px-3 py-1.5 text-sm text-muted-foreground bg-muted/40 hover:bg-muted/60 rounded-lg border border-border/50 transition-all shadow-sm group"
              >
                <div className="flex items-center gap-2.5">
                  <Search className="w-4 h-4 text-muted-foreground/70 group-hover:text-foreground transition-colors" />
                  <span className="group-hover:text-foreground transition-colors">{t('search')}...</span>
                </div>
                <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-background/50 px-1.5 font-mono text-[10px] font-medium opacity-60">
                  <span className="text-xs">⌘</span>K
                </kbd>
              </button>

              {syncStatus !== 'idle' && (
                <div className="flex items-center gap-2">
                  {syncStatus === 'pulling' && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                      <Loader2 className="w-3 h-3 text-primary animate-spin" />
                      <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">
                        {storageType === 'git' ? t('syncingToGit', 'Syncing to Git') : storageType === 'icloud' ? t('savingToICloud', 'Syncing to iCloud') : t('savingLocally')}
                      </span>
                    </div>
                  )}
                  {syncStatus === 'pushing' && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                      <Loader2 className="w-3 h-3 text-primary animate-spin" />
                      <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">
                        {storageType === 'git' ? t('syncingToGit', 'Syncing to Git') : storageType === 'icloud' ? t('savingToICloud', 'Syncing to iCloud') : t('savingLocally')}
                      </span>
                    </div>
                  )}
                  {syncStatus === 'success' && (
                    <div className="flex items-center gap-2 px-2.5 py-1 bg-green-500/5 rounded-full border border-green-500/20">
                      <CheckCircle2 className="w-3 h-3 text-green-500" />
                      <span className="text-[10px] font-semibold text-green-500/80 uppercase tracking-wider">
                        {storageType === 'git' ? t('syncedToGit') : storageType === 'icloud' ? t('savedToICloud', 'Saved to iCloud') : t('savedLocally')}
                      </span>
                    </div>
                  )}
                  {syncStatus === 'error' && (
                    <div 
                      className="group relative flex items-center gap-2 px-2.5 py-1 bg-destructive/5 rounded-full border border-destructive/20 cursor-pointer transition-colors hover:bg-destructive/10"
                      onClick={copySyncError}
                    >
                      <AlertCircle className="w-3 h-3 text-destructive" />
                      <span className="text-[10px] font-semibold text-destructive/80 uppercase tracking-wider">{t('syncError', 'Sync Error')}</span>
                      <div className="absolute left-0 top-full mt-2 w-64 p-2 bg-destructive text-destructive-foreground text-[10px] rounded-md shadow-xl opacity-0 group-hover:opacity-100 transition-opacity z-50 select-text pointer-events-auto border border-destructive/20">
                        <div className="flex flex-col gap-1">
                          <span className="font-bold border-b border-destructive-foreground/20 pb-1 mb-1 flex justify-between items-center">
                            {t('syncErrorInfo', 'Error Details')}
                            <span className="text-[8px] opacity-70 uppercase tracking-widest bg-destructive-foreground/10 px-1 rounded">{t('clickToCopy', 'Click to copy')}</span>
                          </span>
                          <span className="break-words line-clamp-4 leading-relaxed">{syncError}</span>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
            
            <div className="flex items-center gap-1">
              <button 
                onClick={toggleLanguage}
                className="p-2 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
                title={t('toggleLanguage')}
              >
                {i18n.language === 'zh' ? (
                  <span className="text-xs font-bold font-mono">EN</span>
                ) : (
                  <span className="text-xs font-bold">中</span>
                )}
              </button>

              <button 
                onClick={cycleTheme}
                className="p-2 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
                title={themeLabel}
              >
                <ThemeIcon className="w-4 h-4" />
              </button>
            </div>
          </header>
        )}

        <main className={`flex-1 overflow-y-auto ${activeTab === 'settings' ? 'p-0' : 'p-6'}`}>
          {renderContent()}
        </main>
      </div>

      <OmniSearch open={omniOpen} setOpen={setOmniOpen} />
      <AboutModal open={aboutOpen} onClose={() => setAboutOpen(false)} />
    </div>
  );
}

export default App;
