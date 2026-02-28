import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { StickyNote, Plus, Search, Trash2 } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface Note {
  id: string;
  title: string;
  content: string;
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
  const [searchTerm, setSearchTerm] = useState('');

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

  // Auto-save effect
  useEffect(() => {
    const timer = setTimeout(() => {
      if (isEditing && activeNote) {
        if (activeNote.title !== editTitle || activeNote.content !== editContent) {
          handleSave();
        }
      }
    }, 1000); // Auto save after 1s of inactivity
    return () => clearTimeout(timer);
  }, [editTitle, editContent]);

  const handleSave = async () => {
    if (!editTitle && !editContent) return;

    let newNotes = [...notes];
    const now = Date.now();

    if (activeNote) {
      // Update
      const updatedNote = { ...activeNote, title: editTitle, content: editContent, updated_at: now };
      newNotes = newNotes.map(n => n.id === activeNote.id ? updatedNote : n);
      setActiveNote(updatedNote); // update current reference
    } else {
      // Create
      const newNote: Note = {
        id: uuidv4(),
        title: editTitle || t('untitledNote'),
        content: editContent,
        created_at: now,
        updated_at: now
      };
      newNotes.unshift(newNote); // Put at top
      setActiveNote(newNote);
    }

    await saveNotesToDisk(newNotes);
  };

  const handleDelete = async (id: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    if (!confirm(t('confirmKill', { name: "this note" }))) return;
    
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
    setIsEditing(true);
  };

  const startEdit = (note: Note) => {
    setActiveNote(note);
    setEditTitle(note.title);
    setEditContent(note.content);
    setIsEditing(true);
  };

  const filteredNotes = notes.filter(n => 
    n.title.toLowerCase().includes(searchTerm.toLowerCase()) || 
    n.content.toLowerCase().includes(searchTerm.toLowerCase())
  );

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
          <div className="relative shrink-0">
            <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
            <input 
              type="text" 
              placeholder={t('searchNotes')}
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full flex h-10 rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
            />
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
              <div className="flex items-center px-6 py-4 border-b shrink-0 bg-muted/5">
                <input 
                  type="text" 
                  placeholder={t('title')}
                  value={editTitle}
                  onChange={(e) => {
                    setEditTitle(e.target.value);
                    if (!activeNote) handleSave(); // Trigger initial save to get an ID
                  }}
                  className="flex-1 h-10 rounded-md border-transparent bg-transparent text-xl font-bold focus-visible:outline-none placeholder:text-muted-foreground/40"
                />
              </div>

              {/* Note Body Split View */}
              <div className="flex-1 flex min-h-0 divide-x">
                {/* Markdown Input */}
                <div className="flex-1 flex flex-col">
                  <textarea
                    value={editContent}
                    onChange={(e) => {
                      setEditContent(e.target.value);
                      if (!activeNote) handleSave(); // Trigger initial save
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