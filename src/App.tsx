import { useState } from 'react';
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
  Mail,
  Settings,
  Languages,
  Moon,
  Sun,
  Monitor
} from 'lucide-react';
import { AiSessions } from './components/AiSessions';

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState('ai-sessions');

  const navigation = [
    { id: 'launcher', name: t('launcher'), icon: Rocket },
    { id: 'ai-sessions', name: t('aiSessions'), icon: Terminal },
    { id: 'ssh', name: t('sshServers'), icon: Server },
    { id: 'snippets', name: t('snippets'), icon: Code2 },
    { id: 'bookmarks', name: t('bookmarks'), icon: Star },
    { id: 'notes', name: t('notes'), icon: StickyNote },
    { id: 'cloud', name: t('cloudDrive'), icon: Cloud },
    { id: 'mail', name: t('mail'), icon: Mail },
  ];

  const renderContent = () => {
    switch (activeTab) {
      case 'ai-sessions':
        return <AiSessions />;
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
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                activeTab === item.id 
                  ? 'bg-primary text-primary-foreground font-medium' 
                  : 'hover:bg-muted text-muted-foreground hover:text-foreground'
              }`}
            >
              <item.icon className="w-4 h-4" />
              {item.name}
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
          <button className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors">
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
          <button className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground bg-muted/50 hover:bg-muted rounded-md border border-transparent hover:border-border transition-all">
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
    </div>
  );
}

export default App;