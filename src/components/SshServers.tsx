import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Server, AlertCircle, Loader2, ArrowRight } from 'lucide-react';

interface SshHost {
  name: string;
  host_name: string;
  user: string;
  port: number;
}

export function SshServers() {
  const { t } = useTranslation();
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState('');

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadHosts = async () => {
    if (!isTauri) {
      setError(t('notInTauri'));
      return;
    }
    
    try {
      setLoading(true);
      setError(null);
      const res: SshHost[] = await invoke('get_ssh_hosts');
      setHosts(res);
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadHosts();
  }, []);

  const handleConnect = async (hostName: string) => {
    if (!isTauri) return;
    try {
      await invoke('connect_ssh', { host: hostName });
    } catch (err: any) {
      setError(err.toString());
    }
  };

  const filteredHosts = hosts.filter(h => 
    h.name.toLowerCase().includes(searchTerm.toLowerCase()) || 
    h.host_name.toLowerCase().includes(searchTerm.toLowerCase())
  );

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('sshServers')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('manageSshServers') || 'Quick connect to your SSH servers from ~/.ssh/config'}</p>
        </div>
      </div>

      <div className="relative">
        <input 
          type="text" 
          placeholder={t('search') || "Search servers..."}
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="w-full flex h-10 rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        />
      </div>

      {error && (
        <div className="bg-destructive/15 text-destructive text-sm p-4 rounded-md flex items-start gap-3">
          <AlertCircle className="w-5 h-5 shrink-0 mt-0.5" />
          <div>{error}</div>
        </div>
      )}

      {loading ? (
        <div className="flex justify-center py-10">
          <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredHosts.length === 0 ? (
            <div className="col-span-full flex flex-col items-center justify-center h-48 text-muted-foreground bg-card border rounded-xl border-dashed">
              <Server className="w-10 h-10 mb-3 opacity-20" />
              <p>{searchTerm ? 'No matching servers found.' : 'No SSH configurations found.'}</p>
              {!searchTerm && <p className="text-sm mt-1">Add them to your ~/.ssh/config file.</p>}
            </div>
          ) : (
            filteredHosts.map((host, idx) => (
              <div 
                key={idx} 
                className="group relative flex flex-col justify-between p-5 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer overflow-hidden"
                onClick={() => handleConnect(host.name)}
              >
                <div className="absolute top-0 left-0 w-1 h-full bg-primary/0 group-hover:bg-primary transition-colors"></div>
                
                <div>
                  <div className="flex items-start justify-between">
                    <h3 className="font-bold text-lg truncate pr-2">{host.name}</h3>
                    <Server className="w-5 h-5 text-muted-foreground/50 group-hover:text-primary transition-colors shrink-0" />
                  </div>
                  
                  <div className="mt-3 space-y-1">
                    <p className="text-sm text-muted-foreground flex items-center gap-2">
                      <span className="inline-block w-12 text-xs uppercase tracking-wider opacity-70">Host</span>
                      <span className="font-mono text-foreground/80 truncate">{host.host_name}</span>
                    </p>
                    <p className="text-sm text-muted-foreground flex items-center gap-2">
                      <span className="inline-block w-12 text-xs uppercase tracking-wider opacity-70">User</span>
                      <span className="font-mono text-foreground/80">{host.user}</span>
                    </p>
                  </div>
                </div>

                <div className="mt-5 flex items-center justify-between border-t pt-3">
                  <span className="text-xs text-muted-foreground font-mono bg-muted px-2 py-0.5 rounded">
                    Port: {host.port}
                  </span>
                  
                  <div className="flex items-center text-xs font-medium text-primary opacity-0 group-hover:opacity-100 transition-opacity translate-x-2 group-hover:translate-x-0 transform">
                    Connect
                    <ArrowRight className="w-3.5 h-3.5 ml-1" />
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}