import { useState } from 'react';
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
  Settings
} from 'lucide-react';

function App() {
  const [activeTab, setActiveTab] = useState('launcher');

  const navigation = [
    { id: 'launcher', name: 'Launcher', icon: Rocket },
    { id: 'ai-sessions', name: 'AI Sessions', icon: Terminal },
    { id: 'ssh', name: 'SSH Servers', icon: Server },
    { id: 'snippets', name: 'Snippets', icon: Code2 },
    { id: 'bookmarks', name: 'Bookmarks', icon: Star },
    { id: 'notes', name: 'Notes', icon: StickyNote },
    { id: 'cloud', name: 'Cloud Drive', icon: Cloud },
    { id: 'mail', name: 'Mail', icon: Mail },
  ];

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

        <div className="p-3 border-t">
          <button className="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors">
            <Settings className="w-4 h-4" />
            Settings
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col overflow-hidden relative">
        <header className="h-14 border-b flex items-center px-6 justify-between bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
          <h1 className="text-lg font-semibold capitalize">
            {navigation.find(n => n.id === activeTab)?.name || 'Dashboard'}
          </h1>
          
          {/* Omni Search Trigger Hint */}
          <button className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground bg-muted/50 hover:bg-muted rounded-md border border-transparent hover:border-border transition-all">
            <Search className="w-4 h-4" />
            <span>Search...</span>
            <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-background px-1.5 font-mono text-[10px] font-medium opacity-100">
              <span className="text-xs">⌘</span>K
            </kbd>
          </button>
        </header>

        <main className="flex-1 overflow-y-auto p-6">
          <div className="rounded-xl border border-dashed border-border/60 bg-muted/10 h-full flex items-center justify-center text-muted-foreground/50">
            {navigation.find(n => n.id === activeTab)?.name} Content Area
          </div>
        </main>
      </div>
    </div>
  );
}

export default App;