import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { confirm as tauriConfirm, open, save } from '@tauri-apps/plugin-dialog';
import {
  Rocket,
  Plus,
  Trash2,
  Command,
  Globe,
  FolderOpen,
  Search,
  Pin,
  PinOff,
  ArrowUp,
  ArrowDown,
  Edit,
  Upload,
  Download,
  ShieldAlert,
  Workflow,
} from 'lucide-react';

interface LauncherItem {
  id: string;
  name: string;
  type: 'app' | 'script' | 'url' | 'folder' | 'internal';
  target: string;
  pinned: boolean;
  pin_order: number;
  launch_count: number;
  last_launched_at?: number;
  trusted: boolean;
  created_at: number;
  updated_at: number;
}

interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

interface LauncherItemInput {
  id?: string;
  name: string;
  type: LauncherItem['type'];
  target: string;
  pinned?: boolean;
  pin_order?: number;
  trusted?: boolean;
}

interface LegacyLauncherItem {
  id?: string;
  name: string;
  command: string;
  type: 'app' | 'script' | 'url' | 'folder';
}

const MIGRATION_MARKER = 'onespace_launcher_migrated_v1';
const SEEDED_MARKER = 'onespace_launcher_seeded_v1';
const LEGACY_STORAGE_KEY = 'onespace_launcher_items';

const DEFAULT_LAUNCHER_ITEMS: LauncherItemInput[] = [
  { name: 'VS Code', type: 'app', target: 'open -a "Visual Studio Code"' },
  { name: 'Google Chrome', type: 'app', target: 'open -a "Google Chrome"' },
  { name: 'System Settings', type: 'app', target: 'open -a "System Settings"' },
];

const INTERNAL_TARGETS: Array<{ id: string; labelKey: string; fallback: string }> = [
  { id: 'launcher', labelKey: 'launcher', fallback: 'Launcher' },
  { id: 'ai-sessions', labelKey: 'aiSessions', fallback: 'AI Terminal Sessions' },
  { id: 'ai-environments', labelKey: 'aiEnvironments', fallback: 'AI Environments' },
  { id: 'skills', labelKey: 'skills', fallback: 'Skills' },
  { id: 'mcp-servers', labelKey: 'mcpServers', fallback: 'MCP Servers' },
  { id: 'ssh', labelKey: 'sshServers', fallback: 'SSH Servers' },
  { id: 'snippets', labelKey: 'snippets', fallback: 'Snippets' },
  { id: 'bookmarks', labelKey: 'bookmarks', fallback: 'Bookmarks' },
  { id: 'notes', labelKey: 'notes', fallback: 'Notes' },
  { id: 'cloud', labelKey: 'cloudDrive', fallback: 'Cloud Drive' },
  { id: 'mail', labelKey: 'mail', fallback: 'Mail' },
  { id: 'settings', labelKey: 'settings', fallback: 'Settings' },
  { id: 'documentation', labelKey: 'usageDocs', fallback: 'Documentation' },
];

function sortLauncherItems(items: LauncherItem[]) {
  return [...items].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    if (a.pinned && b.pinned) return a.pin_order - b.pin_order;
    return (b.last_launched_at || 0) - (a.last_launched_at || 0);
  });
}

function launcherIcon(type: LauncherItem['type']) {
  if (type === 'url') return Globe;
  if (type === 'folder') return FolderOpen;
  if (type === 'script') return Command;
  if (type === 'internal') return Workflow;
  return Rocket;
}

function formatRelativeTime(ts?: number) {
  if (!ts) return '';
  const nowMs = Date.now();
  const diffSec = Math.floor((nowMs - ts * 1000) / 1000);
  if (diffSec < 60) return 'just now';
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
  return `${Math.floor(diffSec / 86400)}d ago`;
}

function formatInvokeError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object') {
    const maybe = err as { message?: unknown; error?: unknown };
    if (typeof maybe.message === 'string' && maybe.message.trim()) return maybe.message;
    if (typeof maybe.error === 'string' && maybe.error.trim()) return maybe.error;
    try {
      return JSON.stringify(err);
    } catch (_e) {
      return String(err);
    }
  }
  return String(err);
}

export function Launcher() {
  const { t } = useTranslation();
  const [items, setItems] = useState<LauncherItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState('');

  const [isEditing, setIsEditing] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [typeInput, setTypeInput] = useState<LauncherItem['type']>('app');
  const [targetInput, setTargetInput] = useState('');
  const [pinnedInput, setPinnedInput] = useState(false);

  const [pendingScriptItem, setPendingScriptItem] = useState<LauncherItem | null>(null);
  const [trustOnConfirm, setTrustOnConfirm] = useState(false);

  const isTauri = '__TAURI_INTERNALS__' in window;

  const sortedItems = useMemo(() => sortLauncherItems(items), [items]);

  const filteredItems = useMemo(() => {
    const term = searchTerm.trim().toLowerCase();
    if (!term) return sortedItems;
    return sortedItems.filter((item) => {
      const title = item.name.toLowerCase();
      const target = item.target.toLowerCase();
      return title.includes(term) || target.includes(term);
    });
  }, [searchTerm, sortedItems]);

  const pinnedOrderIds = useMemo(
    () => sortedItems.filter((item) => item.pinned).map((item) => item.id),
    [sortedItems]
  );

  const listLauncherItems = async (): Promise<LauncherItem[]> => {
    const resp = await invoke<ApiResp<LauncherItem[]>>('launcher_list');
    return resp.data || [];
  };

  const refreshLauncherItems = async () => {
    if (!isTauri) return;
    const loaded = await listLauncherItems();
    setItems(sortLauncherItems(loaded));
  };

  const upsertLauncherItem = async (item: LauncherItemInput) => {
    await invoke('launcher_upsert', { item });
  };

  const migrateLegacyLauncherIfNeeded = async () => {
    if (localStorage.getItem(MIGRATION_MARKER) === '1') return false;
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) {
      localStorage.setItem(MIGRATION_MARKER, '1');
      return false;
    }

    let parsed: LegacyLauncherItem[] = [];
    try {
      parsed = JSON.parse(raw) as LegacyLauncherItem[];
    } catch (_err) {
      localStorage.setItem(MIGRATION_MARKER, '1');
      return false;
    }

    if (!Array.isArray(parsed) || parsed.length === 0) {
      localStorage.setItem(MIGRATION_MARKER, '1');
      return false;
    }

    for (const item of parsed) {
      await upsertLauncherItem({
        id: item.id,
        name: item.name,
        type: item.type,
        target: item.command,
      });
    }

    localStorage.setItem(MIGRATION_MARKER, '1');
    localStorage.removeItem(LEGACY_STORAGE_KEY);
    return true;
  };

  const seedDefaultLauncherIfNeeded = async () => {
    if (localStorage.getItem(SEEDED_MARKER) === '1') return false;
    for (const item of DEFAULT_LAUNCHER_ITEMS) {
      await upsertLauncherItem(item);
    }
    localStorage.setItem(SEEDED_MARKER, '1');
    return true;
  };

  const bootstrap = async () => {
    if (!isTauri) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      let loaded = await listLauncherItems();
      if (loaded.length === 0) {
        const migrated = await migrateLegacyLauncherIfNeeded();
        if (migrated) {
          loaded = await listLauncherItems();
        } else {
          const seeded = await seedDefaultLauncherIfNeeded();
          if (seeded) {
            loaded = await listLauncherItems();
          }
        }
      }
      setItems(sortLauncherItems(loaded));
      emit('refresh-counts').catch(() => {});
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    bootstrap();
  }, []);

  const resetEditor = () => {
    setIsEditing(false);
    setEditingId(null);
    setNameInput('');
    setTypeInput('app');
    setTargetInput('');
    setPinnedInput(false);
  };

  const startCreate = () => {
    setIsEditing(true);
    setEditingId(null);
    setNameInput('');
    setTypeInput('app');
    setTargetInput('');
    setPinnedInput(false);
  };

  const startEdit = (item: LauncherItem) => {
    setIsEditing(true);
    setEditingId(item.id);
    setNameInput(item.name);
    setTypeInput(item.type);
    setTargetInput(item.target);
    setPinnedInput(item.pinned);
  };

  const handleSave = async () => {
    const name = nameInput.trim();
    const target = targetInput.trim();
    if (!name || !target) return;

    try {
      await upsertLauncherItem({
        id: editingId || undefined,
        name,
        type: typeInput,
        target,
        pinned: pinnedInput,
      });
      await refreshLauncherItems();
      emit('refresh-counts').catch(() => {});
      resetEditor();
    } catch (err) {
      console.error(err);
      alert(`${t('failedToSave', 'Failed to save. Check console.')}\n${formatInvokeError(err)}`);
    }
  };

  const handleDelete = async (item: LauncherItem, e: React.MouseEvent) => {
    e.stopPropagation();
    const confirmed = await tauriConfirm(t('confirmDelete', { name: item.name }), {
      okLabel: t('ok'),
      cancelLabel: t('cancel')
    });
    if (!confirmed) return;

    try {
      await invoke('launcher_delete', { payload: { itemId: item.id } });
      await refreshLauncherItems();
      emit('refresh-counts').catch(() => {});
    } catch (err) {
      console.error(err);
      alert(t('deleteFailed', { error: formatInvokeError(err) }));
    }
  };

  const handleTogglePin = async (item: LauncherItem, e: React.MouseEvent) => {
    e.stopPropagation();
    const previousItems = items;
    const nextPinned = !item.pinned;
    setItems(
      sortLauncherItems(
        items.map((it) =>
          it.id === item.id
            ? {
                ...it,
                pinned: nextPinned,
                updated_at: Math.floor(Date.now() / 1000),
              }
            : it
        )
      )
    );
    try {
      await upsertLauncherItem({
        id: item.id,
        name: item.name,
        type: item.type,
        target: item.target,
        pinned: nextPinned,
      });
      await refreshLauncherItems();
    } catch (err) {
      console.error(err);
      setItems(previousItems);
      alert(t('pinFailed', { error: formatInvokeError(err) }));
    }
  };

  const handleMovePinned = async (itemId: string, direction: 'up' | 'down', e: React.MouseEvent) => {
    e.stopPropagation();
    const current = [...pinnedOrderIds];
    const idx = current.findIndex((id) => id === itemId);
    if (idx < 0) return;
    const swapWith = direction === 'up' ? idx - 1 : idx + 1;
    if (swapWith < 0 || swapWith >= current.length) return;

    const next = [...current];
    [next[idx], next[swapWith]] = [next[swapWith], next[idx]];

    try {
      await invoke('launcher_reorder', { ids: next });
      await refreshLauncherItems();
    } catch (err) {
      console.error(err);
    }
  };

  const executeLaunch = async (item: LauncherItem) => {
    if (item.type === 'internal') {
      const setActiveTab = (window as unknown as { setActiveTab?: (tab: string) => void }).setActiveTab;
      setActiveTab?.(item.target);
      await invoke('launcher_mark_launched', { payload: { itemId: item.id } }).catch(() => {});
      await refreshLauncherItems();
      return;
    }

    await invoke('launcher_execute', {
      payload: {
        type: item.type,
        target: item.target,
      },
    });
    await invoke('launcher_mark_launched', { payload: { itemId: item.id } }).catch(() => {});
    await refreshLauncherItems();
    emit('refresh-counts').catch(() => {});
  };

  const handleLaunch = async (item: LauncherItem) => {
    if (!isTauri) return;

    if (item.type === 'script' && !item.trusted) {
      setPendingScriptItem(item);
      setTrustOnConfirm(false);
      return;
    }

    try {
      await executeLaunch(item);
    } catch (err) {
      console.error(err);
      alert(`${t('failedToLaunch', 'Failed to launch. Check console.')}\n${formatInvokeError(err)}`);
    }
  };

  const confirmScriptLaunch = async () => {
    const item = pendingScriptItem;
    if (!item) return;

    setPendingScriptItem(null);
    try {
      if (trustOnConfirm) {
        await invoke('launcher_set_trust', { payload: { itemId: item.id, trusted: true } });
      }
      await executeLaunch(item);
    } catch (err) {
      console.error(err);
      alert(`${t('failedToLaunch', 'Failed to launch. Check console.')}\n${formatInvokeError(err)}`);
    } finally {
      setTrustOnConfirm(false);
    }
  };

  const cancelScriptLaunch = () => {
    setPendingScriptItem(null);
    setTrustOnConfirm(false);
  };

  const handleExport = async () => {
    if (!isTauri) return;
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const outputPath = await save({
        defaultPath: `onespace-launcher-export-${stamp}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;
      await invoke('launcher_export', { outputPath });
      alert(t('exportedTo', { path: outputPath }));
    } catch (err) {
      console.error(err);
      alert(t('exportFailed', { error: formatInvokeError(err) }));
    }
  };

  const handleImport = async () => {
    if (!isTauri) return;
    try {
      const importPath = await open({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!importPath || Array.isArray(importPath)) return;
      const resp = await invoke<ApiResp<{ count: number; total: number }>>('launcher_import', {
        importPath,
        mode: 'merge',
      });
      await refreshLauncherItems();
      emit('refresh-counts').catch(() => {});
      alert(t('launcherImportSuccess', { count: resp.data?.count ?? 0 }));
    } catch (err) {
      console.error(err);
      alert(t('launcherImportFailed', { error: formatInvokeError(err) }));
    }
  };

  const handleSelectApplication = async () => {
    if (!isTauri) return;
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        defaultPath: '/Applications',
        filters: [{ name: 'Applications', extensions: ['app'] }],
      });
      if (selected && typeof selected === 'string') {
        const fileName = selected.split('/').pop() || selected;
        const appName = fileName.endsWith('.app') ? fileName.slice(0, -4) : fileName;
        if (appName) {
          setTargetInput(appName);
          setNameInput(appName);
        }
      }
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <div className="flex flex-col h-full space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('launcher', 'Launcher')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('launcherDesc', 'Quickly launch favorite apps, local directories, and automated workflows')}</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleImport}
            className="bg-secondary text-secondary-foreground hover:bg-secondary/90 px-3 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
          >
            <Upload className="w-4 h-4" />
            {t('import', 'Import')}
          </button>
          <button
            onClick={handleExport}
            className="bg-secondary text-secondary-foreground hover:bg-secondary/90 px-3 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
          >
            <Download className="w-4 h-4" />
            {t('export', 'Export')}
          </button>
          <button
            onClick={startCreate}
            className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm"
          >
            <Plus className="w-4 h-4" />
            {t('addShortcut', 'Add Shortcut')}
          </button>
        </div>
      </div>

      <div className="relative">
        <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
        <input
          type="text"
          placeholder={t('searchLauncher', 'Search launcher items...')}
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="w-full flex h-10 rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm"
        />
      </div>

      {isEditing && (
        <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Rocket className="w-4 h-4 text-primary" />
            {editingId ? t('editShortcut', 'Edit Shortcut') : t('newShortcut', 'New Shortcut')}
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('name')}</label>
              <input
                type="text"
                placeholder={t('appNamePlaceholder', 'e.g. My App')}
                value={nameInput}
                onChange={(e) => setNameInput(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('type', 'Type')}</label>
              <select
                value={typeInput}
                onChange={(e) => setTypeInput(e.target.value as LauncherItem['type'])}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              >
                <option value="app">{t('macApp', 'Mac Application (open -a)')}</option>
                <option value="script">{t('shellCommand', 'Shell Command')}</option>
                <option value="url">{t('websiteUrl', 'Website URL')}</option>
                <option value="folder">{t('localFolder', 'Local Folder')}</option>
                <option value="internal">{t('internalAction', 'Internal Action')}</option>
              </select>
            </div>

            {typeInput === 'internal' ? (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('targetModule', 'Target Module')}</label>
                <select
                  value={targetInput}
                  onChange={(e) => setTargetInput(e.target.value)}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                >
                  <option value="">{t('selectModule', 'Select module...')}</option>
                  {INTERNAL_TARGETS.map((target) => (
                    <option key={target.id} value={target.id}>
                      {t(target.labelKey, target.fallback)}
                    </option>
                  ))}
                </select>
              </div>
            ) : typeInput === 'app' ? (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('launchTarget', 'Command / Path / URL')}</label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    placeholder={t('selectAppFromApplications', 'Choose app from Applications')}
                    value={targetInput}
                    readOnly
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                  <button
                    onClick={handleSelectApplication}
                    className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
                  >
                    <FolderOpen className="w-4 h-4" />
                    {t('browse', 'Browse')}
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-2 md:col-span-2">
                <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('launchTarget', 'Command / Path / URL')}</label>
                <input
                  type="text"
                  placeholder={t('pathOrUrlPlaceholder', 'Path or URL...')}
                  value={targetInput}
                  onChange={(e) => setTargetInput(e.target.value)}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
                />
              </div>
            )}

            <div className="md:col-span-2">
              <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
                <input
                  type="checkbox"
                  checked={pinnedInput}
                  onChange={(e) => setPinnedInput(e.target.checked)}
                  className="w-4 h-4"
                />
                {t('pinShortcut', 'Pin this shortcut')}
              </label>
            </div>
          </div>

          <div className="flex justify-end gap-3 pt-2">
            <button
              onClick={resetEditor}
              className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
            >
              {t('cancel')}
            </button>
            <button
              onClick={handleSave}
              disabled={!nameInput.trim() || !targetInput.trim()}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50"
            >
              {t('save')}
            </button>
          </div>
        </div>
      )}

      {loading ? (
        <div className="text-sm text-muted-foreground">{t('loading', 'Loading...')}</div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
          {filteredItems.map((item) => {
            const Icon = launcherIcon(item.type);
            const pinnedIndex = pinnedOrderIds.findIndex((id) => id === item.id);
            const isPinned = item.pinned;
            const lastUsed = item.last_launched_at ? formatRelativeTime(item.last_launched_at) : t('neverLaunched', 'Never launched');

            return (
              <div
                key={item.id}
                onClick={() => handleLaunch(item)}
                className="group flex flex-col justify-between p-4 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer min-h-40"
              >
                <div className="flex justify-between items-start gap-2">
                  <div className={`p-2 rounded-lg ${item.type === 'app' ? 'bg-blue-500/10 text-blue-500' : item.type === 'internal' ? 'bg-emerald-500/10 text-emerald-500' : 'bg-primary/10 text-primary'}`}>
                    <Icon className="w-5 h-5" />
                  </div>

                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                    <button
                      onClick={(e) => handleTogglePin(item, e)}
                      className="text-muted-foreground hover:text-foreground p-1 rounded-md"
                      title={isPinned ? t('unpin', 'Unpin') : t('pin', 'Pin')}
                    >
                      {isPinned ? <PinOff className="w-4 h-4" /> : <Pin className="w-4 h-4" />}
                    </button>
                    {isPinned && (
                      <>
                        <button
                          onClick={(e) => handleMovePinned(item.id, 'up', e)}
                          disabled={pinnedIndex <= 0}
                          className="text-muted-foreground hover:text-foreground p-1 rounded-md disabled:opacity-40"
                          title={t('moveUp', 'Move Up')}
                        >
                          <ArrowUp className="w-4 h-4" />
                        </button>
                        <button
                          onClick={(e) => handleMovePinned(item.id, 'down', e)}
                          disabled={pinnedIndex < 0 || pinnedIndex >= pinnedOrderIds.length - 1}
                          className="text-muted-foreground hover:text-foreground p-1 rounded-md disabled:opacity-40"
                          title={t('moveDown', 'Move Down')}
                        >
                          <ArrowDown className="w-4 h-4" />
                        </button>
                      </>
                    )}
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        startEdit(item);
                      }}
                      className="text-muted-foreground hover:text-foreground p-1 rounded-md"
                      title={t('edit', 'Edit')}
                    >
                      <Edit className="w-4 h-4" />
                    </button>
                    <button
                      onClick={(e) => handleDelete(item, e)}
                      className="text-muted-foreground hover:text-destructive p-1 rounded-md"
                      title={t('delete', 'Delete')}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>

                <div className="mt-3 space-y-1">
                  <div className="flex items-center gap-2">
                    <h3 className="font-semibold truncate">{item.name}</h3>
                    {isPinned && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-primary/10 text-primary">{t('pinned', 'Pinned')}</span>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground truncate font-mono opacity-80">{item.target}</p>
                  <p className="text-xs text-muted-foreground">
                    {t('launchCount', 'Launches')}: {item.launch_count} · {t('lastUsed', 'Last used')}: {lastUsed}
                  </p>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {pendingScriptItem && (
        <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50 p-4">
          <div className="bg-background border rounded-xl shadow-xl w-full max-w-md p-5 space-y-4">
            <div className="flex items-center gap-2">
              <ShieldAlert className="w-5 h-5 text-amber-500" />
              <h3 className="font-semibold">{t('launcherScriptConfirmTitle', 'Run untrusted command?')}</h3>
            </div>
            <p className="text-sm text-muted-foreground break-all">{pendingScriptItem.target}</p>
            <label className="inline-flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={trustOnConfirm}
                onChange={(e) => setTrustOnConfirm(e.target.checked)}
                className="w-4 h-4"
              />
              {t('launcherTrustThisItem', 'Trust this launcher item for future runs')}
            </label>
            <div className="flex justify-end gap-2">
              <button
                onClick={cancelScriptLaunch}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted"
              >
                {t('cancel')}
              </button>
              <button
                onClick={confirmScriptLaunch}
                className="px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90"
              >
                {t('launch', 'Launch')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
