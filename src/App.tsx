import { useState, useEffect, useMemo } from 'react';
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
    
    // Start polling with recursive timeout to avoid piling up calls
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
  }, []);

  const navigation = useMemo(() => [
    { id: 'launcher', name: t('launcher'), icon: Rocket, count: counts.launcher },
    { id: 'ai-sessions', name: t('aiSessions'), icon: Terminal, count: counts.sessions },
    { id: 'ai-environments', name: t('aiEnvironments'), icon: Cpu },
    { id: 'ssh', name: t('sshServers'), icon: Server, count: counts.ssh },
    { id: 'snippets', name: t('snippets'), icon: Code2, count: counts.snippets },
    { id: 'bookmarks', name: t('bookmarks'), icon: Star, count: counts.bookmarks },
    { id: 'notes', name: t('notes'), icon: StickyNote, count: counts.notes },
    { id: 'cloud', name: t('cloudDrive'), icon: Cloud },
    { id: 'mail', name: t('mail'), icon: MailIcon, count: counts.mail > 0 ? counts.mail : undefined },
  ], [t, counts]);

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
        <div className={activeTab === 'ai-environments' ? 'h-full' : 'hidden'}><AiEnvironments /></div>
        <div className={activeTab === 'ssh' ? 'h-full' : 'hidden'}><SshServers /></div>
        <div className={activeTab === 'snippets' ? 'h-full' : 'hidden'}><Snippets /></div>
        <div className={activeTab === 'bookmarks' ? 'h-full' : 'hidden'}><Bookmarks /></div>
        <div className={activeTab === 'notes' ? 'h-full' : 'hidden'}><Notes /></div>
        <div className={activeTab === 'documentation' ? 'h-full' : 'hidden'}><Documentation /></div>
        <div className={activeTab === 'cloud' ? 'h-full' : 'hidden'}><CloudDrive /></div>
        <div className={activeTab === 'mail' ? 'h-full' : 'hidden'}><Mail /></div>
      </div>
    );
  };

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

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden select-none">
      {/* Sidebar */}
      <div className="w-64 border-r bg-muted/20 flex flex-col" data-tauri-drag-region>
        <div className="h-16 flex items-end pl-5 pr-4 pb-1.5 border-b font-semibold tracking-tight">
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
          className="h-16 border-b flex items-end px-6 pb-1.5 justify-between bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60"
          data-tauri-drag-region
        >
          <div className="flex-1 flex items-center gap-4">
            {/* Omni Search Bar - Moved to left to balance the header */}
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

            {/* Sync Status - Shown next to search when active */}
            {syncStatus !== 'idle' && (
              <div className="flex items-center gap-2">
                {syncStatus === 'syncing' && (
                  <div className="flex items-center gap-2 px-2.5 py-1 bg-primary/5 rounded-full border border-primary/10 animate-pulse">
                    <Loader2 className="w-3 h-3 text-primary animate-spin" />
                    <span className="text-[10px] font-semibold text-primary/80 uppercase tracking-wider">{t('syncing', 'Syncing')}</span>
                  </div>
                )}
                {syncStatus === 'success' && (
                  <div className="flex items-center gap-2 px-2.5 py-1 bg-green-500/5 rounded-full border border-green-500/20">
                    <CheckCircle2 className="w-3 h-3 text-green-500" />
                    <span className="text-[10px] font-semibold text-green-500/80 uppercase tracking-wider">{t('synced', 'Synced')}</span>
                  </div>
                )}
                {syncStatus === 'error' && (
                  <div 
                    className="group relative flex items-center gap-2 px-2.5 py-1 bg-destructive/5 rounded-full border border-destructive/20 cursor-pointer transition-colors hover:bg-destructive/10"
                    onClick={copySyncError}
                  >
                    <AlertCircle className="w-3 h-3 text-destructive" />
                    <span className="text-[10px] font-semibold text-destructive/80 uppercase tracking-wider">{t('syncError', 'Sync Error')}</span>
                    
                    {/* Error Tooltip */}
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
            {/* Language Toggle */}
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