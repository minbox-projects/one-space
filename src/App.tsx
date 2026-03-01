import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
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
  Languages,
  Moon,
  Sun,
  Monitor,
  Cpu
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

import { getUnreadEmailCount } from './lib/gmail';

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState('ai-sessions');
  const [omniOpen, setOmniOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

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
    
    // Launcher
    const savedLauncher = localStorage.getItem('onespace_launcher_items');
    if (savedLauncher) newCounts.launcher = JSON.parse(savedLauncher).length;
    else newCounts.launcher = 3; // Default items
    
    if (isTauri) {
      try {
        // Parallel execution for better performance
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
      } catch (e) {
        console.error("Failed to load local counts", e);
      }
    }

    // Gmail count (independent of Tauri check, but requires network)
    // We check this even if not in Tauri if we want, but for now only if connected
    try {
      const mailCount = await getUnreadEmailCount();
      newCounts.mail = mailCount;
    } catch (e) {
      // Ignore mail errors to not break other counts
    }
    
    setCounts(newCounts);
  };

  // Initial load and poll every 10 seconds (increased from 5s to reduce API usage)
  useEffect(() => {
    if (isTauri) {
      // Sync git repository on startup
      invoke('sync_git').catch(e => console.error("Git sync failed:", e));
    }
    
    loadCounts();
    const interval = setInterval(loadCounts, 10000);
    return () => clearInterval(interval);
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
        return <AiSessions />;
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

  const toggleLanguage = () => {
    const newLang = i18n.language === 'zh' ? 'en' : 'zh';
    i18n.changeLanguage(newLang);
  };

  const cycleTheme = () => {
    if (theme === 'system') setTheme('dark');
    else if (theme === 'dark') setTheme('light');
    else setTheme('system');
  };

  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;
  const themeLabel = theme === 'system' ? t('themeSystem') : theme === 'dark' ? t('themeDark') : t('themeLight');

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden">
      {/* Sidebar */}
      <div className="w-64 border-r bg-muted/20 flex flex-col">
        <div className="h-14 flex items-center px-4 border-b font-semibold tracking-tight">
          OneSpace
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
            onClick={cycleTheme}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <ThemeIcon className="w-4 h-4" />
            {themeLabel}
          </button>
          <button 
            onClick={toggleLanguage}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Languages className="w-4 h-4" />
            {t('toggleLanguage')}
          </button>
          <button 
            onClick={() => setSettingsOpen(true)}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Settings className="w-4 h-4" />
            {t('settings')}
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col overflow-hidden relative">
        <header className="h-14 border-b flex items-center px-6 justify-between bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
          <h1 className="text-lg font-semibold capitalize">
            {navigation.find(n => n.id === activeTab)?.name || t('dashboard')}
          </h1>
          
          {/* Omni Search Trigger Hint */}
          <button 
            onClick={() => setOmniOpen(true)}
            className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground bg-muted/50 hover:bg-muted rounded-md border border-transparent hover:border-border transition-all"
          >
            <Search className="w-4 h-4" />
            <span>{t('search')}</span>
            <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-background px-1.5 font-mono text-[10px] font-medium opacity-100">
              <span className="text-xs">⌘</span>K
            </kbd>
          </button>
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
    </div>
  );
}

export default App;