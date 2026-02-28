import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Star, Plus, Search, Trash2, ExternalLink, FolderOpen, Globe, Edit2 } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';

interface Bookmark {
  id: string;
  name: string;
  url: string; // can be a web URL or a local file path
  description: string;
  tags: string[];
  created_at: number;
}

export function Bookmarks() {
  const { t } = useTranslation();
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [searchTerm, setSearchTerm] = useState('');
  
  // Editor state
  const [isCreating, setIsCreating] = useState(false);
  const [activeBookmarkId, setActiveBookmarkId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editUrl, setEditUrl] = useState('');
  const [editDesc, setEditDesc] = useState('');
  const [editTags, setEditTags] = useState('');

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadBookmarks = async () => {
    if (!isTauri) return;
    try {
      const jsonStr: string = await invoke('read_bookmarks');
      const data = JSON.parse(jsonStr);
      setBookmarks(data.sort((a: Bookmark, b: Bookmark) => b.created_at - a.created_at));
    } catch (err) {
      console.error("Failed to load bookmarks", err);
    }
  };

  useEffect(() => {
    loadBookmarks();
  }, []);

  const saveBookmarksToDisk = async (newBookmarks: Bookmark[]) => {
    if (!isTauri) return;
    try {
      await invoke('save_bookmarks', { bookmarksJson: JSON.stringify(newBookmarks) });
      setBookmarks(newBookmarks.sort((a, b) => b.created_at - a.created_at));
    } catch (err) {
      console.error("Failed to save bookmarks", err);
      alert(t('failedToSave'));
    }
  };

  const startCreate = () => {
    setActiveBookmarkId(null);
    setEditName('');
    setEditUrl('');
    setEditDesc('');
    setEditTags('');
    setIsCreating(true);
  };

  const startEdit = (e: React.MouseEvent, bookmark: Bookmark) => {
    e.stopPropagation();
    setActiveBookmarkId(bookmark.id);
    setEditName(bookmark.name);
    setEditUrl(bookmark.url);
    setEditDesc(bookmark.description);
    setEditTags(bookmark.tags.join(', '));
    setIsCreating(true);
  };

  const handleSave = async () => {
    if (!editName || !editUrl) return;

    let newBookmarks = [...bookmarks];
    
    if (activeBookmarkId) {
      // Update existing
      newBookmarks = newBookmarks.map(b => {
        if (b.id === activeBookmarkId) {
          return {
            ...b,
            name: editName,
            url: editUrl,
            description: editDesc,
            tags: editTags.split(',').map(t => t.trim()).filter(t => t.length > 0),
          };
        }
        return b;
      });
    } else {
      // Create new
      const newBookmark: Bookmark = {
        id: uuidv4(),
        name: editName,
        url: editUrl,
        description: editDesc,
        tags: editTags.split(',').map(t => t.trim()).filter(t => t.length > 0),
        created_at: Date.now()
      };
      newBookmarks.push(newBookmark);
    }

    await saveBookmarksToDisk(newBookmarks);
    
    setIsCreating(false);
    setActiveBookmarkId(null);
    setEditName('');
    setEditUrl('');
    setEditDesc('');
    setEditTags('');
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm(t('confirmKill', { name: "this bookmark" }))) return;
    
    const newBookmarks = bookmarks.filter(s => s.id !== id);
    await saveBookmarksToDisk(newBookmarks);
  };

  const handleOpen = async (url: string) => {
    if (!isTauri) return;
    try {
      const isLocal = url.startsWith('/') || url.startsWith('~') || url.startsWith('C:\\');
      if (isLocal) {
        await invoke('open_local_path', { path: url });
      } else {
        await open(url);
      }
    } catch (err) {
      console.error("Failed to open:", err);
      alert(t('failedToOpen'));
    }
  };

  const handleBrowseLocal = async () => {
    if (!isTauri) return;
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setEditUrl(selected);
      }
    } catch (err) {
      console.error(err);
    }
  };

  const [activeTag, setActiveTag] = useState<string | null>(null);

  const allTags = Array.from(new Set(bookmarks.flatMap(b => b.tags))).sort();

  const filteredBookmarks = bookmarks.filter(b => {
    const matchesSearch = 
      b.name.toLowerCase().includes(searchTerm.toLowerCase()) || 
      b.url.toLowerCase().includes(searchTerm.toLowerCase()) ||
      b.tags.some(t => t.toLowerCase().includes(searchTerm.toLowerCase()));
    
    const matchesTag = activeTag ? b.tags.includes(activeTag) : true;
    
    return matchesSearch && matchesTag;
  });

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('bookmarks')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('manageBookmarks')}</p>
        </div>
        <button
          onClick={startCreate}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm"
        >
          <Plus className="w-4 h-4" />
          {t('newBookmark')}
        </button>
      </div>

      <div className="relative">
        <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
        <input 
          type="text" 
          placeholder={t('searchBookmarks')}
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="w-full flex h-10 rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        />
      </div>

      {allTags.length > 0 && (
        <div className="flex flex-wrap gap-2">
          <button
            onClick={() => setActiveTag(null)}
            className={`px-3 py-1 rounded-full text-xs font-medium transition-colors ${
              activeTag === null 
                ? 'bg-primary text-primary-foreground' 
                : 'bg-muted text-muted-foreground hover:bg-muted/80'
            }`}
          >
            All
          </button>
          {allTags.map(tag => (
            <button
              key={tag}
              onClick={() => setActiveTag(tag)}
              className={`px-3 py-1 rounded-full text-xs font-medium transition-colors ${
                activeTag === tag 
                  ? 'bg-primary text-primary-foreground' 
                  : 'bg-muted text-muted-foreground hover:bg-muted/80'
              }`}
            >
              {tag}
            </button>
          ))}
        </div>
      )}

      {isCreating && (
        <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Star className="w-4 h-4 text-primary" />
            {activeBookmarkId ? t('edit') : t('newBookmark')}
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('bookmarkName')}</label>
              <input 
                type="text" 
                placeholder="e.g. Tauri Docs" 
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('tags')}</label>
              <input 
                type="text" 
                placeholder="e.g. rust, docs, frontend" 
                value={editTags}
                onChange={(e) => setEditTags(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background"
              />
            </div>
            <div className="space-y-2 md:col-span-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('bookmarkUrl')}</label>
              <div className="flex gap-2">
                <input 
                  type="text" 
                  placeholder="https://... or /Users/..." 
                  value={editUrl}
                  onChange={(e) => setEditUrl(e.target.value)}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background"
                />
                <button 
                  onClick={handleBrowseLocal}
                  className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
                  title="Browse local directory"
                >
                  <FolderOpen className="w-4 h-4" />
                </button>
              </div>
            </div>
            <div className="space-y-2 md:col-span-2">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('bookmarkDesc')}</label>
              <input 
                type="text" 
                placeholder="Brief description..." 
                value={editDesc}
                onChange={(e) => setEditDesc(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background"
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
              onClick={handleSave}
              disabled={!editName || !editUrl}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50"
            >
              {t('save')}
            </button>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredBookmarks.length === 0 && !isCreating ? (
          <div className="col-span-full flex flex-col items-center justify-center h-48 text-muted-foreground bg-card border rounded-xl border-dashed">
            <Star className="w-10 h-10 mb-3 opacity-20" />
            <p>{searchTerm ? t('noBookmarksFound') : t('noBookmarksFound')}</p>
            {!searchTerm && <p className="text-sm mt-1">{t('createFirstBookmark')}</p>}
          </div>
        ) : (
          filteredBookmarks.map((bookmark) => {
            const isLocal = bookmark.url.startsWith('/') || bookmark.url.startsWith('C:\\');
            
            return (
              <div 
                key={bookmark.id} 
                onClick={() => handleOpen(bookmark.url)}
                className="group flex flex-col justify-between p-5 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer h-40"
              >
                <div>
                  <div className="flex items-start justify-between">
                    <h3 className="font-bold text-lg truncate pr-2 flex items-center gap-2">
                      {isLocal ? <FolderOpen className="w-4 h-4 text-amber-500" /> : <Globe className="w-4 h-4 text-blue-500" />}
                      {bookmark.name}
                    </h3>
                    <div className="opacity-0 group-hover:opacity-100 flex items-center gap-1 transition-all">
                      <button 
                        onClick={(e) => startEdit(e, bookmark)}
                        className="text-muted-foreground hover:text-primary p-1 rounded-md transition-colors"
                        title={t('edit')}
                      >
                        <Edit2 className="w-4 h-4" />
                      </button>
                      <button 
                        onClick={(e) => handleDelete(bookmark.id, e)}
                        className="text-muted-foreground hover:text-destructive p-1 rounded-md transition-colors"
                        title={t('delete')}
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                  
                  <p className="text-xs text-muted-foreground mt-1 truncate">
                    {bookmark.description || bookmark.url}
                  </p>
                </div>

                <div className="mt-auto pt-4 flex items-center justify-between">
                  <div className="flex gap-1.5 overflow-hidden pr-2">
                    {bookmark.tags.slice(0, 3).map((tag, i) => (
                      <span key={i} className="text-[10px] uppercase font-medium px-1.5 py-0.5 rounded bg-muted text-muted-foreground truncate max-w-[60px]">
                        {tag}
                      </span>
                    ))}
                    {bookmark.tags.length > 3 && (
                      <span className="text-[10px] uppercase font-medium px-1.5 py-0.5 rounded bg-muted text-muted-foreground">
                        +{bookmark.tags.length - 3}
                      </span>
                    )}
                  </div>
                  
                  <div className="flex items-center text-xs font-medium text-primary opacity-0 group-hover:opacity-100 transition-opacity transform translate-x-2 group-hover:translate-x-0 shrink-0">
                    {isLocal ? t('openLocal') : t('openInBrowser')}
                    <ExternalLink className="w-3.5 h-3.5 ml-1" />
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}