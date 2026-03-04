import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { History, RotateCcw, Trash2, Plus, AlertTriangle } from 'lucide-react';

interface BackupEntry {
  id: string;
  tool: string;
  file_path: string;
  backup_path: string;
  file_content_hash: string;
  created_at: string;
  file_size: number;
  reason?: string;
}

interface BackupManagerProps {
  activeTool?: string;
}

export function BackupManager({ activeTool }: BackupManagerProps) {
  const [backups, setBackups] = useState<BackupEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    loadBackups();
  }, [activeTool]);

  async function loadBackups() {
    setLoading(true);
    try {
      const result = await invoke('list_backups', { 
        tool: activeTool || undefined 
      });
      setBackups(result as BackupEntry[]);
    } catch (e) {
      console.error('Failed to load backups:', e);
    } finally {
      setLoading(false);
    }
  }

  async function handleCreateBackup() {
    if (!activeTool) return;
    
    setCreating(true);
    try {
      await invoke('create_backup', { 
        tool: activeTool,
        reason: 'Manual backup'
      });
      await loadBackups();
      alert('Backup created successfully!');
    } catch (e) {
      alert(`Failed to create backup: ${e}`);
    } finally {
      setCreating(false);
    }
  }

  async function handleRestore(entryId: string) {
    if (!confirm('Are you sure you want to restore this backup? The current config will be backed up first.')) {
      return;
    }
    
    setRestoring(entryId);
    try {
      await invoke('restore_backup', { entryId });
      await loadBackups();
      alert('Backup restored successfully!');
    } catch (e) {
      alert(`Failed to restore backup: ${e}`);
    } finally {
      setRestoring(null);
    }
  }

  async function handleDelete(entryId: string) {
    if (!confirm('Are you sure you want to delete this backup?')) {
      return;
    }
    
    try {
      await invoke('delete_backup', { entryId });
      await loadBackups();
    } catch (e) {
      alert(`Failed to delete backup: ${e}`);
    }
  }

  async function handleCleanup() {
    if (!confirm('Delete all backups older than 30 days?')) {
      return;
    }
    
    try {
      const deletedCount = await invoke('cleanup_old_backups', { retentionDays: 30 });
      await loadBackups();
      alert(`Deleted ${deletedCount} old backup(s).`);
    } catch (e) {
      alert(`Failed to cleanup backups: ${e}`);
    }
  }

  function formatFileSize(bytes: number): string {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  }

  function formatDate(dateString: string): string {
    try {
      const date = new Date(dateString);
      return date.toLocaleString();
    } catch {
      return dateString;
    }
  }

  return (
    <div className="p-6">
      <div className="flex justify-between items-center mb-6">
        <div>
          <h2 className="text-2xl font-bold flex items-center gap-2">
            <History className="w-6 h-6" />
            Backup Manager
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            Manage configuration backups for {activeTool || 'all tools'}
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={handleCleanup}
            className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80"
          >
            Cleanup Old
          </button>
          <button
            onClick={handleCreateBackup}
            disabled={creating || !activeTool}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm flex items-center gap-2 hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="w-4 h-4" />
            Create Backup
          </button>
        </div>
      </div>

      {!activeTool && (
        <div className="bg-amber-50 border border-amber-200 rounded-md p-4 mb-6 flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-amber-600 mt-0.5" />
          <div>
            <p className="text-sm text-amber-800 font-medium">
              No tool selected
            </p>
            <p className="text-xs text-amber-700 mt-1">
              Select an AI environment tool to view and manage its backups
            </p>
          </div>
        </div>
      )}

      {loading ? (
        <div className="text-center py-12 text-muted-foreground">Loading backups...</div>
      ) : backups.length === 0 ? (
        <div className="text-center py-12">
          <History className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">No Backups Found</h3>
          <p className="text-muted-foreground mb-4">
            Create a backup to save your current configuration
          </p>
          {activeTool && (
            <button
              onClick={handleCreateBackup}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
            >
              Create First Backup
            </button>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          {backups.map(entry => (
            <div 
              key={entry.id}
              className="border rounded-lg p-4 bg-card hover:bg-accent/50 transition-colors"
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <div className="flex items-center gap-2 mb-2">
                    <span className="text-xs px-2 py-1 bg-secondary rounded uppercase font-mono">
                      {entry.tool}
                    </span>
                    {entry.reason && (
                      <span className="text-xs text-muted-foreground">
                        {entry.reason}
                      </span>
                    )}
                  </div>
                  
                  <div className="text-sm font-mono text-muted-foreground mb-2">
                    {entry.file_path}
                  </div>
                  
                  <div className="flex items-center gap-4 text-xs text-muted-foreground">
                    <span>Created: {formatDate(entry.created_at)}</span>
                    <span>Size: {formatFileSize(entry.file_size)}</span>
                    <span className="font-mono">
                      Hash: {entry.file_content_hash.substring(0, 8)}...
                    </span>
                  </div>
                </div>
                
                <div className="flex items-center gap-2 ml-4">
                  <button
                    onClick={() => handleRestore(entry.id)}
                    disabled={restoring === entry.id}
                    className="p-2 bg-primary text-primary-foreground rounded hover:bg-primary/90 disabled:opacity-50"
                    title="Restore"
                  >
                    {restoring === entry.id ? (
                      <RotateCcw className="w-4 h-4 animate-spin" />
                    ) : (
                      <RotateCcw className="w-4 h-4" />
                    )}
                  </button>
                  
                  <button
                    onClick={() => handleDelete(entry.id)}
                    className="p-2 bg-destructive/10 text-destructive rounded hover:bg-destructive/20"
                    title="Delete"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
