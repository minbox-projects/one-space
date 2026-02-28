import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Cloud, Folder, File as FileIcon, Download, Upload, RefreshCw, HardDrive, Loader2, LogOut } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';
import { filesize } from 'filesize';

// Simple types for mocked implementation
interface CloudFile {
  file_id: string;
  name: string;
  type: 'folder' | 'file';
  size: number;
  updated_at: string;
}

export function CloudDrive() {
  const { t } = useTranslation();
  
  const [refreshToken, setRefreshToken] = useState('');
  const [isConnected, setIsConnected] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const [files, setFiles] = useState<CloudFile[]>([]);
  const [currentFolderId, setCurrentFolderId] = useState('root');
  const [breadcrumbs, setBreadcrumbs] = useState<{id: string, name: string}[]>([{id: 'root', name: 'Root'}]);

  useEffect(() => {
    const savedToken = localStorage.getItem('onespace_aliyun_token');
    if (savedToken) {
      setRefreshToken(savedToken);
      setIsConnected(true);
      fetchFiles('root');
    }
  }, []);

  const handleConnect = async () => {
    if (!refreshToken) return;
    setLoading(true);
    setError(null);
    
    try {
      // MOCK: Verify token and fetch root files
      // In a real implementation, you would call https://auth.aliyundrive.com/v2/oauth/token
      await new Promise(r => setTimeout(r, 1000));
      
      localStorage.setItem('onespace_aliyun_token', refreshToken);
      setIsConnected(true);
      fetchFiles('root');
    } catch (err: any) {
      setError("Failed to connect. Please check your token.");
    } finally {
      setLoading(false);
    }
  };

  const handleDisconnect = () => {
    localStorage.removeItem('onespace_aliyun_token');
    setRefreshToken('');
    setIsConnected(false);
    setFiles([]);
    setBreadcrumbs([{id: 'root', name: 'Root'}]);
  };

  const fetchFiles = async (folderId: string) => {
    setLoading(true);
    setError(null);
    
    try {
      // MOCK: Fetch files from Aliyun API
      // In a real implementation, call https://api.aliyundrive.com/adrive/v3/file/list
      await new Promise(r => setTimeout(r, 800));
      
      let mockFiles: CloudFile[] = [];
      if (folderId === 'root') {
        mockFiles = [
          { file_id: '1', name: 'Documents', type: 'folder', size: 0, updated_at: new Date().toISOString() },
          { file_id: '2', name: 'Photos', type: 'folder', size: 0, updated_at: new Date(Date.now() - 86400000).toISOString() },
          { file_id: '3', name: 'getting-started.pdf', type: 'file', size: 1024 * 1024 * 2.5, updated_at: new Date().toISOString() },
        ];
      } else if (folderId === '1') {
        mockFiles = [
          { file_id: '4', name: 'project-proposal.docx', type: 'file', size: 500000, updated_at: new Date().toISOString() },
          { file_id: '5', name: 'budget-2024.xlsx', type: 'file', size: 120000, updated_at: new Date().toISOString() },
        ];
      } else {
        mockFiles = [];
      }
      
      setFiles(mockFiles);
      setCurrentFolderId(folderId);
    } catch (err) {
      setError("Failed to fetch files.");
    } finally {
      setLoading(false);
    }
  };

  const navigateToFolder = (folderId: string, folderName: string) => {
    fetchFiles(folderId);
    setBreadcrumbs([...breadcrumbs, { id: folderId, name: folderName }]);
  };

  const navigateUp = (index: number) => {
    const target = breadcrumbs[index];
    const newBreadcrumbs = breadcrumbs.slice(0, index + 1);
    fetchFiles(target.id);
    setBreadcrumbs(newBreadcrumbs);
  };

  if (!isConnected) {
    return (
      <div className="flex flex-col items-center justify-center h-full max-w-md mx-auto space-y-6">
        <div className="bg-primary/10 p-4 rounded-full">
          <Cloud className="w-12 h-12 text-primary" />
        </div>
        <div className="text-center space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">{t('connectCloudDrive')}</h2>
          <p className="text-muted-foreground">{t('manageCloudDrive')}</p>
        </div>

        <div className="w-full bg-card border rounded-xl p-6 shadow-sm space-y-4">
          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('refreshToken')}</label>
            <input 
              type="password" 
              placeholder={t('refreshTokenPlaceholder')} 
              value={refreshToken}
              onChange={(e) => setRefreshToken(e.target.value)}
              className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
            />
            <p className="text-xs text-muted-foreground/70 text-right mt-1">
              <a href="https://github.com/tickstep/aliyunpan" target="_blank" rel="noreferrer" className="hover:underline hover:text-primary transition-colors">
                {t('howToGetToken')}
              </a>
            </p>
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}

          <button 
            onClick={handleConnect}
            disabled={!refreshToken || loading}
            className="w-full bg-primary text-primary-foreground hover:bg-primary/90 h-10 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
          >
            {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : <HardDrive className="w-4 h-4" />}
            {t('saveToken')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full space-y-4">
      <div className="flex items-center justify-between shrink-0">
        <div>
          <h2 className="text-xl font-bold tracking-tight flex items-center gap-2">
            <Cloud className="w-5 h-5 text-primary" />
            {t('cloudDrive')}
          </h2>
          <p className="text-sm text-muted-foreground mt-1 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-green-500"></span>
            Connected
          </p>
        </div>
        
        <div className="flex items-center gap-2">
          <button
            onClick={() => fetchFiles(currentFolderId)}
            className="p-2 text-muted-foreground hover:bg-muted rounded-md transition-colors"
            title="Refresh"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
          </button>
          <button
            className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
          >
            <Upload className="w-4 h-4" />
            {t('upload')}
          </button>
          <button
            onClick={handleDisconnect}
            className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 p-2 rounded-md transition-colors"
            title={t('disconnect')}
          >
            <LogOut className="w-4 h-4" />
          </button>
        </div>
      </div>

      <div className="flex-1 bg-card border rounded-xl shadow-sm flex flex-col overflow-hidden">
        {/* Breadcrumbs */}
        <div className="h-12 border-b bg-muted/20 flex items-center px-4 gap-2 text-sm font-medium">
          {breadcrumbs.map((crumb, idx) => (
            <div key={crumb.id} className="flex items-center gap-2">
              {idx > 0 && <span className="text-muted-foreground">/</span>}
              <button 
                onClick={() => navigateUp(idx)}
                className={`hover:text-primary transition-colors ${idx === breadcrumbs.length - 1 ? 'text-foreground' : 'text-muted-foreground'}`}
              >
                {crumb.name}
              </button>
            </div>
          ))}
        </div>

        {/* File List Header */}
        <div className="grid grid-cols-12 gap-4 px-6 py-3 border-b text-xs font-medium text-muted-foreground uppercase tracking-wider bg-muted/5">
          <div className="col-span-6">{t('name')}</div>
          <div className="col-span-3">{t('updatedAt')}</div>
          <div className="col-span-3 text-right">{t('size')}</div>
        </div>

        {/* File List */}
        <div className="flex-1 overflow-y-auto">
          {loading && files.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
              <Loader2 className="w-8 h-8 animate-spin mb-2" />
              <p>{t('loading')}</p>
            </div>
          ) : files.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
              <Folder className="w-12 h-12 mb-3 opacity-20" />
              <p>{t('emptyFolder')}</p>
            </div>
          ) : (
            <div className="divide-y divide-border/50">
              {files.map((file) => (
                <div 
                  key={file.file_id}
                  onClick={() => file.type === 'folder' && navigateToFolder(file.file_id, file.name)}
                  className={`grid grid-cols-12 gap-4 px-6 py-3 items-center group transition-colors ${file.type === 'folder' ? 'cursor-pointer hover:bg-muted/30' : 'hover:bg-muted/10'}`}
                >
                  <div className="col-span-6 flex items-center gap-3">
                    {file.type === 'folder' ? (
                      <Folder className="w-5 h-5 text-blue-400 fill-blue-400/20" />
                    ) : (
                      <FileIcon className="w-5 h-5 text-muted-foreground" />
                    )}
                    <span className="font-medium truncate">{file.name}</span>
                  </div>
                  
                  <div className="col-span-3 text-sm text-muted-foreground truncate" title={new Date(file.updated_at).toLocaleString()}>
                    {formatDistanceToNow(new Date(file.updated_at), { addSuffix: true })}
                  </div>
                  
                  <div className="col-span-3 text-sm text-muted-foreground text-right font-mono flex items-center justify-end gap-3">
                    {file.type === 'file' ? filesize(file.size, { standard: 'jedec' }) : '--'}
                    {file.type === 'file' && (
                      <button className="opacity-0 group-hover:opacity-100 p-1.5 hover:bg-primary/10 hover:text-primary rounded-md transition-all">
                        <Download className="w-4 h-4" />
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}