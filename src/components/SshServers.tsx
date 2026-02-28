import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Server, AlertCircle, Loader2, ArrowRight, Plus, History, Key, Lock, FolderOpen, Terminal, Star, EyeOff, Eye } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';
import { formatDistanceToNow } from 'date-fns';

interface SshHost {
  name: string;
  host_name: string;
  user: string;
  port: number;
}

interface SshHistoryEntry {
  id: string;
  type: 'config' | 'custom';
  name: string; // the host alias for config, or custom name
  host_name: string;
  user: string;
  port: number;
  auth_type?: 'key' | 'password';
  auth_val?: string;
  last_connected: number; // timestamp
  connect_count?: number; // times connected
}

export function SshServers() {
  const { t } = useTranslation();
  
  // Views: 'config' | 'history' | 'ignored' | 'custom'
  const [activeView, setActiveView] = useState<'config' | 'history' | 'ignored' | 'custom'>('config');
  
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [history, setHistory] = useState<SshHistoryEntry[]>([]);
  const [favorites, setFavorites] = useState<string[]>([]);
  const [ignored, setIgnored] = useState<string[]>([]);
  
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState('');

  // Custom connection form
  const [customHost, setCustomHost] = useState('');
  const [customUser, setCustomUser] = useState('root');
  const [customPort, setCustomPort] = useState('22');
  const [customAuthType, setCustomAuthType] = useState<'password' | 'key'>('password');
  const [customAuthVal, setCustomAuthVal] = useState('');

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadData = async () => {
    if (!isTauri) {
      setError(t('notInTauri'));
      return;
    }
    
    try {
      setLoading(true);
      setError(null);
      
      // Load config hosts
      const res: SshHost[] = await invoke('get_ssh_hosts');
      setHosts(res);

      // Load history
      const savedHistory = localStorage.getItem('onespace_ssh_history');
      if (savedHistory) {
        setHistory(JSON.parse(savedHistory));
      }

      // Load favorites and ignored
      const savedFavs = localStorage.getItem('onespace_ssh_favorites');
      if (savedFavs) setFavorites(JSON.parse(savedFavs));
      
      const savedIgnored = localStorage.getItem('onespace_ssh_ignored');
      if (savedIgnored) setIgnored(JSON.parse(savedIgnored));

    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const saveHistory = (entry: SshHistoryEntry) => {
    let newHistory = [...history];
    let connectCount = 1;
    
    // Remove if exists to update timestamp and move to top
    newHistory = newHistory.filter(h => {
      const isMatch = (entry.type === 'config') 
        ? h.name === entry.name 
        : h.host_name === entry.host_name && h.user === entry.user;
        
      if (isMatch) {
        connectCount = (h.connect_count || 1) + 1;
        return false;
      }
      return true;
    });

    entry.connect_count = connectCount;
    newHistory.unshift(entry);
    
    // Keep only last 50
    if (newHistory.length > 50) {
      newHistory = newHistory.slice(0, 50);
    }

    setHistory(newHistory);
    localStorage.setItem('onespace_ssh_history', JSON.stringify(newHistory));
  };

  const toggleFavorite = (e: React.MouseEvent, name: string) => {
    e.stopPropagation();
    const newFavs = favorites.includes(name) 
      ? favorites.filter(f => f !== name)
      : [...favorites, name];
    setFavorites(newFavs);
    localStorage.setItem('onespace_ssh_favorites', JSON.stringify(newFavs));
  };

  const toggleIgnore = (e: React.MouseEvent, name: string) => {
    e.stopPropagation();
    const newIgnored = ignored.includes(name) 
      ? ignored.filter(i => i !== name)
      : [...ignored, name];
    setIgnored(newIgnored);
    localStorage.setItem('onespace_ssh_ignored', JSON.stringify(newIgnored));
  };

  const handleConnectConfig = async (host: SshHost) => {
    if (!isTauri) return;
    try {
      await invoke('connect_ssh', { host: host.name });
      saveHistory({
        id: uuidv4(),
        type: 'config',
        name: host.name,
        host_name: host.host_name,
        user: host.user,
        port: host.port,
        last_connected: Date.now()
      });
    } catch (err: any) {
      setError(err.toString());
    }
  };

  const handleConnectCustom = async () => {
    if (!isTauri || !customHost || !customUser || !customPort) return;
    
    try {
      await invoke('connect_ssh_custom', { 
        user: customUser,
        host: customHost,
        port: parseInt(customPort),
        authType: customAuthType,
        authVal: customAuthVal
      });

      saveHistory({
        id: uuidv4(),
        type: 'custom',
        name: `${customUser}@${customHost}`,
        host_name: customHost,
        user: customUser,
        port: parseInt(customPort),
        auth_type: customAuthType,
        auth_val: customAuthVal, // WARNING: Storing password in plain text in localstorage for now. Real app should use keychain.
        last_connected: Date.now()
      });
      
      // Clear sensitive info on success
      if (customAuthType === 'password') {
        setCustomAuthVal('');
      }
    } catch (err: any) {
      setError(err.toString());
    }
  };

  const handleConnectHistory = async (entry: SshHistoryEntry) => {
    if (entry.type === 'config') {
      handleConnectConfig(entry);
    } else {
      try {
        await invoke('connect_ssh_custom', { 
          user: entry.user,
          host: entry.host_name,
          port: entry.port,
          authType: entry.auth_type || 'password',
          authVal: entry.auth_val || ''
        });
        
        // Update timestamp
        saveHistory({ ...entry, last_connected: Date.now() });
      } catch (err: any) {
        setError(err.toString());
      }
    }
  };

  const handleSelectKeyFile = async () => {
    try {
      const selected = await open({
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setCustomAuthVal(selected);
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  const filteredHosts = hosts
    .filter(h => !ignored.includes(h.name))
    .filter(h => 
      h.name.toLowerCase().includes(searchTerm.toLowerCase()) || 
      h.host_name.toLowerCase().includes(searchTerm.toLowerCase())
    )
    .sort((a, b) => {
      // 1. Favorites first
      const aFav = favorites.includes(a.name);
      const bFav = favorites.includes(b.name);
      if (aFav && !bFav) return -1;
      if (!aFav && bFav) return 1;
      
      // 2. Then auto-detect frequently used (from history, if count >= 3)
      const aHistory = history.find(h => h.type === 'config' && h.name === a.name);
      const bHistory = history.find(h => h.type === 'config' && h.name === b.name);
      const aCount = aHistory?.connect_count || 0;
      const bCount = bHistory?.connect_count || 0;
      
      if (aCount >= 3 && bCount < 3) return -1;
      if (aCount < 3 && bCount >= 3) return 1;
      
      // 3. Alphabetical
      return a.name.localeCompare(b.name);
    });

  const filteredHistory = history
    .filter(h => !ignored.includes(h.name))
    .filter(h => 
      h.name.toLowerCase().includes(searchTerm.toLowerCase()) || 
      h.host_name.toLowerCase().includes(searchTerm.toLowerCase())
    );

  const filteredIgnored = hosts
    .filter(h => ignored.includes(h.name))
    .filter(h => 
      h.name.toLowerCase().includes(searchTerm.toLowerCase()) || 
      h.host_name.toLowerCase().includes(searchTerm.toLowerCase())
    )
    .sort((a, b) => a.name.localeCompare(b.name));

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('sshServers')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('manageSshServers')}</p>
        </div>
        <div className="flex bg-muted/50 p-1 rounded-lg">
          <button 
            onClick={() => setActiveView('config')}
            className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${activeView === 'config' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
          >
            <Server className="w-4 h-4 inline-block mr-1.5" />
            {t('config')}
          </button>
          <button 
            onClick={() => setActiveView('history')}
            className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${activeView === 'history' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
          >
            <History className="w-4 h-4 inline-block mr-1.5" />
            {t('history')}
          </button>
          <button 
            onClick={() => setActiveView('ignored')}
            className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${activeView === 'ignored' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
          >
            <EyeOff className="w-4 h-4 inline-block mr-1.5" />
            {t('ignored')}
          </button>
          <button 
            onClick={() => setActiveView('custom')}
            className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${activeView === 'custom' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
          >
            <Plus className="w-4 h-4 inline-block mr-1.5" />
            {t('custom')}
          </button>
        </div>
      </div>

      {error && (
        <div className="bg-destructive/15 text-destructive text-sm p-4 rounded-md flex items-start gap-3">
          <AlertCircle className="w-5 h-5 shrink-0 mt-0.5" />
          <div>{error}</div>
        </div>
      )}

      {/* CUSTOM CONNECTION FORM */}
      {activeView === 'custom' && (
        <div className="bg-card border rounded-xl p-6 shadow-sm max-w-2xl mx-auto w-full space-y-6">
          <h3 className="font-semibold text-lg flex items-center gap-2 border-b pb-4">
            <Terminal className="w-5 h-5 text-primary" />
            {t('newSshConnection')}
          </h3>
          
          <div className="grid grid-cols-2 gap-5">
            <div className="col-span-2 space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('hostOrIp')}</label>
              <input 
                type="text" 
                placeholder={t('hostOrIpPlaceholder')} 
                value={customHost}
                onChange={(e) => setCustomHost(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
            
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('username')}</label>
              <input 
                type="text" 
                placeholder="root" 
                value={customUser}
                onChange={(e) => setCustomUser(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>

            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('port')}</label>
              <input 
                type="number" 
                placeholder="22" 
                value={customPort}
                onChange={(e) => setCustomPort(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>

            <div className="col-span-2 space-y-2 pt-2 border-t mt-2">
              <div className="flex gap-4 mb-3">
                <label className="flex items-center gap-2 text-sm cursor-pointer">
                  <input 
                    type="radio" 
                    name="auth_type" 
                    checked={customAuthType === 'password'} 
                    onChange={() => { setCustomAuthType('password'); setCustomAuthVal(''); }}
                    className="text-primary focus:ring-primary"
                  />
                  {t('password')}
                </label>
                <label className="flex items-center gap-2 text-sm cursor-pointer">
                  <input 
                    type="radio" 
                    name="auth_type" 
                    checked={customAuthType === 'key'} 
                    onChange={() => { setCustomAuthType('key'); setCustomAuthVal(''); }}
                    className="text-primary focus:ring-primary"
                  />
                  {t('identityKeyFile')}
                </label>
              </div>

              {customAuthType === 'password' ? (
                <div className="relative">
                  <Lock className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
                  <input 
                    type="password" 
                    placeholder={t('passwordPlaceholder')} 
                    value={customAuthVal}
                    onChange={(e) => setCustomAuthVal(e.target.value)}
                    className="flex h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                  />
                </div>
              ) : (
                <div className="flex gap-2">
                  <div className="relative flex-1">
                    <Key className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
                    <input 
                      type="text" 
                      readOnly
                      placeholder={t('selectPrivateKey')} 
                      value={customAuthVal}
                      className="flex h-10 w-full rounded-md border border-input bg-muted/50 pl-9 pr-3 py-2 text-sm ring-offset-background cursor-not-allowed"
                    />
                  </div>
                  <button 
                    onClick={handleSelectKeyFile}
                    className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
                  >
                    <FolderOpen className="w-4 h-4" />
                    {t('browse')}
                  </button>
                </div>
              )}
            </div>
          </div>

          <div className="flex justify-end pt-4">
            <button 
              onClick={handleConnectCustom}
              disabled={!customHost || !customUser || !customPort}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-6 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
            >
              {t('connectNow')}
              <ArrowRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}

      {/* CONFIG & HISTORY & IGNORED VIEWS */}
      {(activeView === 'config' || activeView === 'history' || activeView === 'ignored') && (
        <>
          <div className="relative">
            <input 
              type="text" 
              placeholder={t('search') || "Search servers..."}
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full flex h-10 rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
            />
          </div>

          {loading ? (
            <div className="flex justify-center py-10">
              <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              
              {/* CONFIG VIEW */}
              {activeView === 'config' && (
                filteredHosts.length === 0 ? (
                  <div className="col-span-full flex flex-col items-center justify-center h-48 text-muted-foreground bg-card border rounded-xl border-dashed">
                    <Server className="w-10 h-10 mb-3 opacity-20" />
                    <p>{searchTerm ? t('noMatchingServers') : t('noSshConfig')}</p>
                    {!searchTerm && <p className="text-sm mt-1">{t('addThemToConfig')}</p>}
                  </div>
                ) : (
                  filteredHosts.map((host, idx) => {
                    const isFav = favorites.includes(host.name);
                    const isFrequent = (history.find(h => h.type === 'config' && h.name === host.name)?.connect_count || 0) >= 3;
                    
                    return (
                    <div 
                      key={idx} 
                      className="group relative flex flex-col justify-between p-5 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer overflow-hidden"
                      onClick={() => handleConnectConfig(host)}
                    >
                      <div className={`absolute top-0 left-0 w-1 h-full transition-colors ${isFav ? 'bg-amber-500' : 'bg-primary/0 group-hover:bg-primary'}`}></div>
                      
                      <div>
                        <div className="flex items-start justify-between">
                          <h3 className="font-bold text-lg truncate pr-2 flex items-center gap-2">
                            {host.name}
                            {isFrequent && !isFav && <span className="text-[10px] uppercase bg-blue-500/10 text-blue-500 px-1.5 py-0.5 rounded font-bold tracking-wider">{t('frequent')}</span>}
                          </h3>
                          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                            <button 
                              onClick={(e) => toggleIgnore(e, host.name)}
                              className="p-1.5 text-muted-foreground hover:text-destructive hover:bg-destructive/10 rounded-md transition-colors"
                              title={t('ignoreConfig')}
                            >
                              <EyeOff className="w-4 h-4" />
                            </button>
                            <button 
                              onClick={(e) => toggleFavorite(e, host.name)}
                              className={`p-1.5 rounded-md transition-colors ${isFav ? 'text-amber-500 hover:bg-amber-500/10' : 'text-muted-foreground hover:text-amber-500 hover:bg-amber-500/10'}`}
                              title={isFav ? t('removeFromFavorites') : t('addToFavorites')}
                            >
                              <Star className={`w-4 h-4 ${isFav ? 'fill-current' : ''}`} />
                            </button>
                          </div>
                          {!isFav && <Server className="w-5 h-5 text-muted-foreground/50 group-hover:hidden transition-colors shrink-0" />}
                        </div>
                        
                        <div className="mt-3 space-y-1">
                          <p className="text-sm text-muted-foreground flex items-center gap-2">
                            <span className="inline-block w-12 text-xs uppercase tracking-wider opacity-70">{t('host')}</span>
                            <span className="font-mono text-foreground/80 truncate">{host.host_name}</span>
                          </p>
                          <p className="text-sm text-muted-foreground flex items-center gap-2">
                            <span className="inline-block w-12 text-xs uppercase tracking-wider opacity-70">{t('user')}</span>
                            <span className="font-mono text-foreground/80">{host.user}</span>
                          </p>
                        </div>
                      </div>

                      <div className="mt-5 flex items-center justify-between border-t pt-3">
                        <span className="text-xs text-muted-foreground font-mono bg-muted px-2 py-0.5 rounded">
                          {t('port')}: {host.port}
                        </span>
                        
                        <div className="flex items-center text-xs font-medium text-primary opacity-0 group-hover:opacity-100 transition-opacity translate-x-2 group-hover:translate-x-0 transform">
                          {t('connect')}
                          <ArrowRight className="w-3.5 h-3.5 ml-1" />
                        </div>
                      </div>
                    </div>
                  )})
                )
              )}

              {/* HISTORY VIEW */}
              {activeView === 'history' && (
                filteredHistory.length === 0 ? (
                  <div className="col-span-full flex flex-col items-center justify-center h-48 text-muted-foreground bg-card border rounded-xl border-dashed">
                    <History className="w-10 h-10 mb-3 opacity-20" />
                    <p>{searchTerm ? t('noMatchingHistory') : t('noHistoryYet')}</p>
                  </div>
                ) : (
                  filteredHistory.map((entry, idx) => (
                    <div 
                      key={idx} 
                      className="group relative flex flex-col justify-between p-5 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer overflow-hidden"
                      onClick={() => handleConnectHistory(entry)}
                    >
                      <div className={`absolute top-0 left-0 w-1 h-full transition-colors ${entry.type === 'config' ? 'bg-blue-500/0 group-hover:bg-blue-500' : 'bg-emerald-500/0 group-hover:bg-emerald-500'}`}></div>
                      
                      <div>
                        <div className="flex items-start justify-between">
                          <h3 className="font-bold text-lg truncate pr-2 flex items-center gap-2">
                            {entry.name}
                            <span className="text-[10px] uppercase font-normal px-1.5 py-0.5 rounded bg-muted text-muted-foreground">
                              {entry.type}
                            </span>
                          </h3>
                          <History className="w-5 h-5 text-muted-foreground/50 group-hover:text-foreground transition-colors shrink-0" />
                        </div>
                        
                        <div className="mt-3 space-y-1">
                          <p className="text-sm text-muted-foreground flex items-center gap-2">
                            <span className="inline-block w-12 text-xs uppercase tracking-wider opacity-70">{t('host')}</span>
                            <span className="font-mono text-foreground/80 truncate">{entry.host_name}</span>
                          </p>
                          <p className="text-sm text-muted-foreground flex items-center gap-2">
                            <span className="inline-block w-12 text-xs uppercase tracking-wider opacity-70">{t('user')}</span>
                            <span className="font-mono text-foreground/80">{entry.user}</span>
                          </p>
                        </div>
                      </div>

                      <div className="mt-5 flex items-center justify-between border-t pt-3">
                        <span className="text-xs text-muted-foreground font-mono bg-muted px-2 py-0.5 rounded" title={new Date(entry.last_connected).toLocaleString()}>
                          {formatDistanceToNow(entry.last_connected, { addSuffix: true })}
                        </span>
                        
                        <div className="flex items-center text-xs font-medium text-primary opacity-0 group-hover:opacity-100 transition-opacity translate-x-2 group-hover:translate-x-0 transform">
                          {t('reconnect')}
                          <ArrowRight className="w-3.5 h-3.5 ml-1" />
                        </div>
                      </div>
                    </div>
                  ))
                )
              )}

              {/* IGNORED VIEW */}
              {activeView === 'ignored' && (
                filteredIgnored.length === 0 ? (
                  <div className="col-span-full flex flex-col items-center justify-center h-48 text-muted-foreground bg-card border rounded-xl border-dashed">
                    <EyeOff className="w-10 h-10 mb-3 opacity-20" />
                    <p>{searchTerm ? t('noMatchingServers') : t('noIgnoredServers')}</p>
                  </div>
                ) : (
                  filteredIgnored.map((host, idx) => (
                    <div 
                      key={idx} 
                      className="group relative flex flex-col justify-between p-5 rounded-xl border border-dashed bg-card/50 text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 overflow-hidden"
                    >
                      <div>
                        <div className="flex items-start justify-between">
                          <h3 className="font-bold text-lg truncate pr-2 opacity-70">{host.name}</h3>
                          <button 
                            onClick={(e) => toggleIgnore(e, host.name)}
                            className="p-1.5 text-primary hover:bg-primary/10 rounded-md transition-colors opacity-0 group-hover:opacity-100"
                            title={t('restoreConfig')}
                          >
                            <Eye className="w-4 h-4" />
                          </button>
                        </div>
                        
                        <div className="mt-3 space-y-1 opacity-50">
                          <p className="text-sm flex items-center gap-2">
                            <span className="inline-block w-12 text-xs uppercase tracking-wider">{t('host')}</span>
                            <span className="font-mono truncate">{host.host_name}</span>
                          </p>
                          <p className="text-sm flex items-center gap-2">
                            <span className="inline-block w-12 text-xs uppercase tracking-wider">{t('user')}</span>
                            <span className="font-mono">{host.user}</span>
                          </p>
                        </div>
                      </div>
                    </div>
                  ))
                )
              )}

            </div>
          )}
        </>
      )}
    </div>
  );
}