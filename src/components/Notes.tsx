import { useState, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { confirm as tauriConfirm } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { StickyNote, Plus, Search, Trash2, Folder, Tag, X } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface Note {
  id: string;
  title: string;
  content: string;
  group?: string;
  tags?: string[];
  created_at: number;
  updated_at: number;
}

export function Notes() {
  const { t } = useTranslation();
  const [notes, setNotes] = useState<Note[]>([]);
  const [activeNote, setActiveNote] = useState<Note | null>(null);
  
  // Editor state
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editContent, setEditContent] = useState('');
  const [editGroup, setEditGroup] = useState('');
  const [editTags, setEditTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState('');

  // Filter state
  const [searchTerm, setSearchTerm] = useState('');
  const [groupFilter, setGroupFilter] = useState('');
  const [tagFilter, setTagFilter] = useState('');

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadNotes = async () => {
    if (!isTauri) return;
    try {
      const jsonStr: string = await invoke('read_notes');
      const data = JSON.parse(jsonStr);
      setNotes(data.sort((a: Note, b: Note) => b.updated_at - a.updated_at));
    } catch (err) {
      console.error("Failed to load notes", err);
    }
  };

  useEffect(() => {
    loadNotes();
  }, []);

  const saveNotesToDisk = async (newNotes: Note[]) => {
    if (!isTauri) return;
    try {
      await invoke('save_notes', { notesJson: JSON.stringify(newNotes) });
      setNotes(newNotes.sort((a, b) => b.updated_at - a.updated_at));
    } catch (err) {
      console.error("Failed to save notes", err);
      alert(t('failedToSave'));
    }
  };

  // Derived lists for filters
  const uniqueGroups = useMemo(() => {
    const groups = new Set<string>();
    notes.forEach(note => {
      if (note.group) groups.add(note.group);
    });
    return Array.from(groups).sort();
  }, [notes]);

  const uniqueTags = useMemo(() => {
    const tags = new Set<string>();
    notes.forEach(note => {
      if (note.tags) note.tags.forEach(t => tags.add(t));
    });
    return Array.from(tags).sort();
  }, [notes]);

  // Auto-save effect
  useEffect(() => {
    const timer = setTimeout(() => {
      if (isEditing && activeNote) {
        const hasChanges = 
          activeNote.title !== editTitle || 
          activeNote.content !== editContent ||
          activeNote.group !== editGroup ||
          JSON.stringify(activeNote.tags || []) !== JSON.stringify(editTags);

        if (hasChanges) {
          handleSave();
        }
      }
    }, 1000); 
    return () => clearTimeout(timer);
  }, [editTitle, editContent, editGroup, editTags]);

  const handleSave = async () => {
    if (!editTitle && !editContent && !editGroup && editTags.length === 0) return;

    let newNotes = [...notes];
    const now = Date.now();

    if (activeNote) {
      // Update
      const updatedNote = { 
        ...activeNote, 
        title: editTitle, 
        content: editContent, 
        group: editGroup || undefined,
        tags: editTags.length > 0 ? editTags : undefined,
        updated_at: now 
      };
      newNotes = newNotes.map(n => n.id === activeNote.id ? updatedNote : n);
      setActiveNote(updatedNote); 
    } else {
      // Create
      const newNote: Note = {
        id: uuidv4(),
        title: editTitle || t('untitledNote'),
        content: editContent,
        group: editGroup || undefined,
        tags: editTags.length > 0 ? editTags : undefined,
        created_at: now,
        updated_at: now
      };
      newNotes.unshift(newNote); 
      setActiveNote(newNote);
    }

    await saveNotesToDisk(newNotes);
  };

  const handleDelete = async (id: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    const confirmed = await tauriConfirm(t('confirmDelete', { name: t('thisNote', 'this note') }), {
      okLabel: t('ok'),
      cancelLabel: t('cancel')
    });
    if (!confirmed) return;
    
    const newNotes = notes.filter(n => n.id !== id);
    await saveNotesToDisk(newNotes);
    
    if (activeNote?.id === id) {
      setActiveNote(null);
      setIsEditing(false);
    }
  };

  const startCreate = () => {
    setActiveNote(null);
    setEditTitle('');
    setEditContent('');
    // Use exact match if group filter matches an existing group, otherwise empty
    setEditGroup(uniqueGroups.includes(groupFilter) ? groupFilter : '');
    setEditTags([]);
    setTagInput('');
    setIsEditing(true);
  };

  const startEdit = (note: Note) => {
    setActiveNote(note);
    setEditTitle(note.title);
    setEditContent(note.content);
    setEditGroup(note.group || '');
    setEditTags(note.tags || []);
    setTagInput('');
    setIsEditing(true);
  };

  const addTag = () => {
    const tag = tagInput.trim();
    if (tag && !editTags.includes(tag)) {
      setEditTags([...editTags, tag]);
    }
    setTagInput('');
  };

  const removeTag = (tagToRemove: string) => {
    setEditTags(editTags.filter(tag => tag !== tagToRemove));
  };

  const filteredNotes = notes.filter(n => {
    const matchesSearch = n.title.toLowerCase().includes(searchTerm.toLowerCase()) || 
                          n.content.toLowerCase().includes(searchTerm.toLowerCase());
    
    const matchesGroup = !groupFilter || (n.group && n.group.toLowerCase().includes(groupFilter.toLowerCase()));
    
    const matchesTag = !tagFilter || (n.tags && n.tags.some(t => t.toLowerCase().includes(tagFilter.toLowerCase())));
    
    return matchesSearch && matchesGroup && matchesTag;
  });

  return (
    <div className="flex flex-col h-full space-y-4">
      <div className="flex items-center justify-between shrink-0">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('notes')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('manageNotes')}</p>
        </div>
        <button
          onClick={startCreate}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm"
        >
          <Plus className="w-4 h-4" />
          {t('newNote')}
        </button>
      </div>

      <div className="flex-1 flex gap-6 min-h-0">
        {/* Left Sidebar: List */}
        <div className="w-1/3 flex flex-col gap-4 border-r pr-6 shrink-0">
          <div className="space-y-3 shrink-0">
            {/* Search */}
            <div className="relative">
              <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="text" 
                placeholder={t('searchNotes')}
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                className="w-full flex h-10 rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
            
            {/* Filters */}
            <div className="flex gap-2">
              <div className="flex-1 min-w-0 relative">
                <Folder className="absolute left-2.5 top-2.5 w-3.5 h-3.5 text-muted-foreground opacity-50 z-10" />
                <input 
                  type="text" 
                  list="filter-groups"
                  placeholder={t('group') || "Group"}
                  value={groupFilter}
                  onChange={(e) => setGroupFilter(e.target.value)}
                  className="w-full h-9 rounded-md border border-input bg-background pl-8 pr-2 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                />
                <datalist id="filter-groups">
                  {uniqueGroups.map(g => <option key={g} value={g} />)}
                </datalist>
              </div>
              <div className="flex-1 min-w-0 relative">
                <Tag className="absolute left-2.5 top-2.5 w-3.5 h-3.5 text-muted-foreground opacity-50 z-10" />
                <input 
                  type="text" 
                  list="filter-tags"
                  placeholder={t('tags') || "Tag"}
                  value={tagFilter}
                  onChange={(e) => setTagFilter(e.target.value)}
                  className="w-full h-9 rounded-md border border-input bg-background pl-8 pr-2 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                />
                <datalist id="filter-tags">
                  {uniqueTags.map(t => <option key={t} value={t} />)}
                </datalist>
              </div>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto space-y-2 pr-1 custom-scrollbar">
            {filteredNotes.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-10 text-muted-foreground text-center">
                <StickyNote className="w-8 h-8 mb-3 opacity-20" />
                <p className="text-sm">{t('noNotesFound')}</p>
                {!searchTerm && <p className="text-xs mt-1 opacity-70">{t('createFirstNote')}</p>}
              </div>
            ) : (
              filteredNotes.map((note) => (
                <div 
                  key={note.id}
                  onClick={() => startEdit(note)}
                  className={`group p-4 rounded-lg border cursor-pointer transition-all flex flex-col gap-2 ${
                    activeNote?.id === note.id 
                      ? 'bg-primary/5 border-primary shadow-sm' 
                      : 'bg-card hover:border-primary/50'
                  }`}
                >
                  <div className="flex justify-between items-start">
                    <h4 className="font-semibold text-sm truncate pr-2 flex-1">
                      {note.title || t('untitledNote')}
                    </h4>
                    <button 
                      onClick={(e) => handleDelete(note.id, e)}
                      className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive shrink-0 transition-opacity"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                  
                  {/* Metadata preview */}
                  <div className="flex flex-wrap gap-1">
                    {note.group && (
                      <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-secondary text-secondary-foreground">
                        <Folder className="w-2.5 h-2.5 mr-1" />
                        {note.group}
                      </span>
                    )}
                    {note.tags?.slice(0, 3).map(tag => (
                      <span key={tag} className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-muted text-muted-foreground">
                        <Tag className="w-2.5 h-2.5 mr-1" />
                        {tag}
                      </span>
                    ))}
                  </div>

                  <p className="text-xs text-muted-foreground truncate opacity-70">
                    {note.content.split('\n')[0] || '...'}
                  </p>
                  <div className="text-[10px] text-muted-foreground/50 font-mono mt-1">
                    {new Date(note.updated_at).toLocaleString()}
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Right Area: Editor / Viewer */}
        <div className="flex-1 flex flex-col min-w-0 bg-card rounded-xl border shadow-sm overflow-hidden">
          {isEditing || activeNote ? (
            <div className="flex flex-col h-full w-full">
              {/* Note Header */}
              <div className="flex flex-col border-b shrink-0 bg-muted/5">
                <div className="flex items-center px-6 py-4">
                  <input 
                    type="text" 
                    placeholder={t('title')}
                    value={editTitle}
                    onChange={(e) => {
                      setEditTitle(e.target.value);
                      if (!activeNote) handleSave();
                    }}
                    className="flex-1 h-10 rounded-md border-transparent bg-transparent text-xl font-bold focus-visible:outline-none placeholder:text-muted-foreground/40"
                  />
                </div>
                
                {/* Metadata Editors */}
                <div className="flex items-center gap-4 px-6 pb-3 text-sm">
                  {/* Group Input */}
                  <div className="flex items-center gap-2 min-w-[150px]">
                    <Folder className="w-4 h-4 text-muted-foreground" />
                    <input
                      type="text"
                      list="group-options"
                      placeholder={t('group') || "Group"}
                      value={editGroup}
                      onChange={(e) => setEditGroup(e.target.value)}
                      className="bg-transparent border-none focus:ring-0 p-0 text-sm placeholder:text-muted-foreground/50 w-full"
                    />
                    <datalist id="group-options">
                      {uniqueGroups.map(g => <option key={g} value={g} />)}
                    </datalist>
                  </div>

                  {/* Tags Input */}
                  <div className="flex items-center gap-2 flex-1">
                    <Tag className="w-4 h-4 text-muted-foreground" />
                    <div className="flex flex-wrap gap-1 items-center flex-1">
                      {editTags.map(tag => (
                        <span key={tag} className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-primary/10 text-primary">
                          {tag}
                          <button onClick={() => removeTag(tag)} className="ml-1 hover:text-destructive">
                            <X className="w-3 h-3" />
                          </button>
                        </span>
                      ))}
                      <input
                        type="text"
                        placeholder={editTags.length === 0 ? (t('addTags') || "Add tags...") : ""}
                        value={tagInput}
                        onChange={(e) => setTagInput(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            addTag();
                          }
                        }}
                        className="bg-transparent border-none focus:ring-0 p-0 text-sm min-w-[60px] flex-1"
                      />
                    </div>
                  </div>
                </div>
              </div>

              {/* Note Body Split View */}
              <div className="flex-1 flex min-h-0 divide-x">
                {/* Markdown Input */}
                <div className="flex-1 flex flex-col">
                  <textarea
                    value={editContent}
                    onChange={(e) => {
                      setEditContent(e.target.value);
                      if (!activeNote) handleSave();
                    }}
                    placeholder={t('writeSomething')}
                    className="flex-1 w-full resize-none bg-transparent p-6 text-sm focus-visible:outline-none placeholder:text-muted-foreground/40 font-mono"
                  />
                </div>
                
                {/* Markdown Preview */}
                <div className="flex-1 overflow-auto p-6 bg-muted/10">
                  <div className="prose prose-sm dark:prose-invert max-w-none">
                    {editContent ? (
                      <ReactMarkdown remarkPlugins={[remarkGfm]}>
                        {editContent}
                      </ReactMarkdown>
                    ) : (
                      <p className="text-muted-foreground/40 italic">{t('preview')}</p>
                    )}
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
              <StickyNote className="w-12 h-12 mb-4 opacity-10" />
              <p>{t('selectNoteSidebar')}</p>
              <p className="text-sm mt-1">{t('createOneToStartWriting')}</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
