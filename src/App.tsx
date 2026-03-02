import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
import { SshServers } from './components/SshServers';
import { Snippets } from './components/Snippets';
import { Bookmarks } from './components/Bookmarks';
import { Notes } from './components/Notes';
import { CloudDrive } from './components/CloudDrive';
import { Mail } from './components/Mail';
import { OmniSearch } from './components/OmniSearch';
import { Launcher } from './components/Launcher';
import { SettingsModal } from './components/SettingsModal';
import { AboutModal } from './components/AboutModal';
import { QuickAiSessionBar } from './components/QuickAiSessionBar';
import { Documentation } from './components/Documentation';

import { getUnreadEmailCount } from './lib/gmail';
import logoWhite from './assets/onespace_logo_white.png';
import logoBlack from './assets/onespace_logo_black.png';

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  
  // URL View Routing
  const queryParams = new URLSearchParams(window.location.search);
  const view = queryParams.get('view');

  const [activeTab, setActiveTab] = useState('ai-sessions');
  const [omniOpen, setOmniOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);

  // Git Sync Status
  const [syncStatus, setSyncStatus] = useState<'idle' | 'syncing' | 'success' | 'error'>('idle');
  const [syncError, setSyncError] = useState<string | null>(null);

  // If we are in quick-ai view, render only that component
  if (view === 'quick-ai') {
    return <QuickAiSessionBar />;
  }

  // Global counts for sidebar
  const [counts, setCounts] = useState({
    launcher: 0,
    sessions: 0,
    ssh: 0,
    snippets: 0,
    bookmarks: 0,
    notes: 0,
    mail: 0
  });

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadCounts = async () => {
    let newCounts = { ...counts };
    
    // 1. First load: Local data (Very Fast)
    const savedLauncher = localStorage.getItem('onespace_launcher_items');
    if (savedLauncher) newCounts.launcher = JSON.parse(savedLauncher).length;
    else newCounts.launcher = 3; 
    
    if (isTauri) {
      try {
        const [sessions, sshHosts, snippetsStr, bookmarksStr, notesStr] = await Promise.all([
          invoke('get_tmux_sessions').catch(() => []),
          invoke('get_ssh_hosts').catch(() => []),
          invoke('read_snippets').catch(() => "[]"),
          invoke('read_bookmarks').catch(() => "[]"),
          invoke('read_notes').catch(() => "[]")
        ]);

        newCounts.sessions = (sessions as any[]).length;
        newCounts.ssh = (sshHosts as any[]).length;
        newCounts.snippets = JSON.parse(snippetsStr as string).length;
        newCounts.bookmarks = JSON.parse(bookmarksStr as string).length;
        newCounts.notes = JSON.parse(notesStr as string).length;
        
        // Immediately update UI with local data
        setCounts({ ...newCounts });
      } catch (e) {
        console.error("Failed to load local counts", e);
      }
    }

    // 2. Second load: Remote network data (Slow, Async)
    // Don't 'await' this so we don't block the function return
    getUnreadEmailCount().then(mailCount => {
      setCounts(prev => ({ ...prev, mail: mailCount }));
    }).catch(() => {});
  };

  // Initial load and poll
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenSync: (() => void) | undefined;

    if (isTauri) {
      // 1. SHOW WINDOW IMMEDIATELY (Highest Priority)
      invoke('show_main_window').catch(console.error);

      // 2. Load Local Data slightly after UI renders (Medium Priority)
      setTimeout(() => {
        loadCounts();
      }, 500);

      // 3. DELAY GIT SYNC (Lowest Priority)
      // Wait 3 seconds to ensure everything is smooth before hitting the network/Git
      setTimeout(() => {
        invoke('sync_git').catch(e => console.error("Git sync failed:", e));
      }, 3000);

      listen('trigger-sync', () => {
        invoke('sync_git').catch(e => console.error("Tray Sync failed:", e));
      });

      listen('refresh-mail-count', () => {
        loadCounts();
      }).then(fn => {
        unlisten = fn;
      });

      // Listen for Git Sync Completion
      listen('git-sync-status', (event: any) => {
        const payload = event.payload as { status: string, message?: string };
        setSyncStatus(payload.status as any);
        if (payload.status === 'error') {
          setSyncError(payload.message || 'Unknown sync error');
        } else {
          setSyncError(null);
        }

        if (payload.status === 'success') {
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
      }).catch(e => console.error("Failed to load language", e));
    }
    
    // Start polling after initial delay
    const interval = setInterval(loadCounts, 15000); // Increased poll interval to reduce load

    return () => {
      clearInterval(interval);
      if (unlisten) unlisten();
      if (unlistenSync) unlistenSync();
    };
  }, []);

  const navigation = [
    { id: 'launcher', name: t('launcher'), icon: Rocket, count: counts.launcher },
    { id: 'ai-sessions', name: t('aiSessions'), icon: Terminal, count: counts.sessions },
    { id: 'ai-environments', name: t('aiEnvironments'), icon: Cpu },
    { id: 'ssh', name: t('sshServers'), icon: Server, count: counts.ssh },
    { id: 'snippets', name: t('snippets'), icon: Code2, count: counts.snippets },
    { id: 'bookmarks', name: t('bookmarks'), icon: Star, count: counts.bookmarks },
    { id: 'notes', name: t('notes'), icon: StickyNote, count: counts.notes },
    { id: 'cloud', name: t('cloudDrive'), icon: Cloud },
    { id: 'mail', name: t('mail'), icon: MailIcon, count: counts.mail > 0 ? counts.mail : undefined },
  ];

  const renderContent = () => {
    switch (activeTab) {
      case 'launcher':
        return <Launcher />;
      case 'ai-sessions':
        return <AiSessions onNavigate={(tab, hash) => {
          setActiveTab(tab);
          if (hash) window.location.hash = hash;
        }} />;
      case 'ai-environments':
        return <AiEnvironments />;
      case 'ssh':
        return <SshServers />;
      case 'snippets':
        return <Snippets />;
      case 'bookmarks':
        return <Bookmarks />;
      case 'notes':
        return <Notes />;
      case 'documentation':
        return <Documentation />;
      case 'cloud':
        return <CloudDrive />;
      case 'mail':
        return <Mail />;
      default:
        return (
          <div className="rounded-xl border border-dashed border-border/60 bg-muted/10 h-full flex items-center justify-center text-muted-foreground/50">
            {navigation.find(n => n.id === activeTab)?.name} {t('contentArea')}
          </div>
        );
    }
  };

  const toggleLanguage = async () => {
    const newLang = i18n.language === 'zh' ? 'en' : 'zh';
    i18n.changeLanguage(newLang);
    
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

  const resolvedTheme = theme === 'system' 
    ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
    : theme;

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden select-none">
      {/* Sidebar */}
      <div className="w-64 border-r bg-muted/20 flex flex-col" data-tauri-drag-region>
        <div className="h-14 flex items-center pl-20 pr-4 border-b font-semibold tracking-tight gap-2">
          <div className="flex items-center gap-2 pointer-events-none">
            <img 
              src={resolvedTheme === 'dark' ? logoWhite : logoBlack} 
              alt="OneSpace" 
              className="w-5 h-5"
            />
            <span>OneSpace</span>
          </div>

          {/* Git Sync Status Indicator */}
          {syncStatus !== 'idle' && (
            <div className="flex items-center ml-auto">
              {syncStatus === 'syncing' && (
                <Loader2 className="w-3.5 h-3.5 text-primary animate-spin" />
              )}
              {syncStatus === 'success' && (
                <CheckCircle2 className="w-3.5 h-3.5 text-green-500" />
              )}
              {syncStatus === 'error' && (
                <div className="group relative cursor-pointer pointer-events-auto" onClick={copySyncError}>
                  <AlertCircle className="w-3.5 h-3.5 text-destructive" />
                  <div className="absolute left-0 top-full mt-2 w-64 p-2 bg-destructive text-destructive-foreground text-[10px] rounded shadow-lg opacity-0 group-hover:opacity-100 transition-opacity z-50 select-text pointer-events-auto">
                    <div className="flex flex-col gap-1">
                      <span className="font-bold border-b border-destructive-foreground/20 pb-1 mb-1 flex justify-between">
                        {t('syncError', 'Sync Error')}
                        <span className="text-[8px] opacity-70 uppercase tracking-widest">{t('clickToCopy', 'Click icon to copy')}</span>
                      </span>
                      <span className="break-words line-clamp-4">{syncError}</span>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
        
        <div className="flex-1 overflow-y-auto py-4 px-3 space-y-1">
          {navigation.map((item) => (
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
            onClick={() => setSettingsOpen(true)}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
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

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col overflow-hidden relative bg-background">
        <header 
          className="h-14 border-b flex items-center px-6 justify-between bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60"
          data-tauri-drag-region
        >
          <div className="flex items-center gap-4">
            <h1 className="text-lg font-semibold capitalize pointer-events-none">
              {navigation.find(n => n.id === activeTab)?.name || t('dashboard')}
            </h1>
            
            {/* Minimal Sync Status in Header */}
            {syncStatus === 'syncing' && (
              <div className="flex items-center gap-2 px-2 py-1 bg-primary/10 rounded-full animate-pulse">
                <Loader2 className="w-3 h-3 text-primary animate-spin" />
                <span className="text-[10px] font-medium text-primary uppercase tracking-wider">{t('syncing', 'Syncing...')}</span>
              </div>
            )}
          </div>
          
          <div className="flex items-center gap-2">
            {/* Omni Search Trigger Hint */}
            <button 
              onClick={() => setOmniOpen(true)}
              className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground bg-muted/50 hover:bg-muted rounded-md border border-transparent hover:border-border transition-all mr-2"
            >
              <Search className="w-4 h-4" />
              <span>{t('search')}</span>
              <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-background px-1.5 font-mono text-[10px] font-medium opacity-100">
                <span className="text-xs">⌘</span>K
              </kbd>
            </button>

            {/* Language Toggle */}
            <button 
              onClick={toggleLanguage}
              className="p-2 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
              title={t('toggleLanguage')}
            >
              {i18n.language === 'zh' ? (
                <span className="text-xs font-bold">EN</span>
              ) : (
                <span className="text-xs font-bold">中</span>
              )}
            </button>

            {/* Theme Toggle */}
            <button 
              onClick={cycleTheme}
              className="p-2 text-muted-foreground hover:bg-muted hover:text-foreground rounded-md transition-colors"
              title={themeLabel}
            >
              <ThemeIcon className="w-4 h-4" />
            </button>
          </div>
        </header>

        <main className="flex-1 overflow-y-auto p-6">
          {renderContent()}
        </main>
      </div>

      <OmniSearch open={omniOpen} setOpen={setOmniOpen} />
      <SettingsModal open={settingsOpen} onClose={() => {
        setSettingsOpen(false);
        loadCounts(); // Reload counts since data might have changed
      }} />
      <AboutModal open={aboutOpen} onClose={() => setAboutOpen(false)} />
    </div>
  );
}

export default App;