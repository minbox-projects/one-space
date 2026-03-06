import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Rocket, Plus, Trash2, Command, Globe, FolderOpen } from 'lucide-react';
import { open as shellOpen, Command as ShellCommand } from '@tauri-apps/plugin-shell';
import { v4 as uuidv4 } from 'uuid';

interface LauncherItem {
  id: string;
  name: string;
  command: string;
  type: 'app' | 'script' | 'url' | 'folder';
}

export function Launcher() {
  const { t } = useTranslation();
  const [items, setItems] = useState<LauncherItem[]>([]);
  const [isCreating, setIsCreating] = useState(false);
  
  // Form state
  const [newName, setNewName] = useState('');
  const [newCommand, setNewCommand] = useState('');
  const [newType, setNewType] = useState<'app' | 'script' | 'url' | 'folder'>('app');

  const isTauri = '__TAURI_INTERNALS__' in window;

  useEffect(() => {
    const saved = localStorage.getItem('onespace_launcher_items');
    if (saved) {
      setItems(JSON.parse(saved));
    } else {
      // Default items
      setItems([
        { id: '1', name: 'VS Code', command: 'open -a "Visual Studio Code"', type: 'app' },
        { id: '2', name: 'Google Chrome', command: 'open -a "Google Chrome"', type: 'app' },
        { id: '3', name: 'System Settings', command: 'open -a "System Settings"', type: 'app' }
      ]);
    }
  }, []);

  const saveItems = (newItems: LauncherItem[]) => {
    setItems(newItems);
    localStorage.setItem('onespace_launcher_items', JSON.stringify(newItems));
  };

  const handleLaunch = async (item: LauncherItem) => {
    if (!isTauri) return;
    try {
      if (item.type === 'url' || item.type === 'folder') {
        await shellOpen(item.command);
      } else {
        await ShellCommand.create('sh', ['-c', item.command]).spawn();
      }
    } catch (err) {
      console.error(err);
      alert(t('failedToLaunch', 'Failed to launch. Check console.'));
    }
  };

  const handleAdd = () => {
    if (!newName || !newCommand) return;
    
    const newItem: LauncherItem = {
      id: uuidv4(),
      name: newName,
      command: newCommand,
      type: newType
    };
    
    saveItems([...items, newItem]);
    setIsCreating(false);
    setNewName('');
    setNewCommand('');
  };

  const handleDelete = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    saveItems(items.filter(i => i.id !== id));
  };

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('launcher') || 'Launcher'}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t("launcherDesc", "Quick launch your favorite apps and workflows")}</p>
        </div>
        <button
          onClick={() => setIsCreating(true)}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm"
        >
          <Plus className="w-4 h-4" />
          Add Shortcut
        </button>
      </div>

      {isCreating && (
        <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Rocket className="w-4 h-4 text-primary" />
            New Shortcut
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("name")}</label>
              <input 
                type="text" 
                placeholder={t('appNamePlaceholder', 'e.g. My App')} 
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("type", "Type")}</label>
              <select 
                value={newType}
                onChange={(e) => setNewType(e.target.value as any)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background"
              >
                <option value="app">{t("macApp", "Mac Application (open -a)")}</option>
                <option value="script">{t("shellCommand", "Shell Command")}</option>
                <option value="url">{t("websiteUrl", "Website URL")}</option>
                <option value="folder">{t("localFolder", "Local Folder")}</option>
              </select>
            </div>
            <div className="space-y-2 md:col-span-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("commandOrPath", "Command / Path")}</label>
              <input 
                type="text" 
                placeholder={newType === 'app' ? t('openAppPlaceholder', 'open -a "App Name"') : t('pathOrUrlPlaceholder', 'Path or URL...')} 
                value={newCommand}
                onChange={(e) => setNewCommand(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background font-mono"
              />
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button 
              onClick={() => setIsCreating(false)}
              className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
            >
              {t('cancel')}
            </button>
            <button 
              onClick={handleAdd}
              disabled={!newName || !newCommand}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50"
            >
              {t('save')}
            </button>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
        {items.map((item) => {
          let Icon = Command;
          if (item.type === 'url') Icon = Globe;
          if (item.type === 'folder') Icon = FolderOpen;
          if (item.type === 'app') Icon = Rocket;

          return (
            <div 
              key={item.id}
              onClick={() => handleLaunch(item)}
              className="group flex flex-col justify-between p-5 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer h-32"
            >
              <div className="flex justify-between items-start">
                <div className={`p-2 rounded-lg ${item.type === 'app' ? 'bg-blue-500/10 text-blue-500' : 'bg-primary/10 text-primary'}`}>
                  <Icon className="w-6 h-6" />
                </div>
                <button 
                  onClick={(e) => handleDelete(item.id, e)}
                  className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive p-1 rounded-md transition-all"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
              
              <div className="mt-4">
                <h3 className="font-bold truncate">{item.name}</h3>
                <p className="text-xs text-muted-foreground truncate font-mono mt-1 opacity-60">
                  {item.command}
                </p>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
