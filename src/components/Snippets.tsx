import { useState, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Code2, Plus, Search, Copy, Check, Trash2, TerminalSquare, Folder, Tag, X, ChevronRight, ChevronDown } from 'lucide-react';
import { v4 as uuidv4 } from 'uuid';
import Editor from 'react-simple-code-editor';
import Prism from 'prismjs';

// Core languages
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-java';
import 'prismjs/components/prism-python';
import 'prismjs/themes/prism-tomorrow.css'; // Dark theme for code

interface Snippet {
  id: string;
  title: string;
  language: string;
  code: string;
  group?: string;
  tags?: string[];
  created_at: number;
  updated_at: number;
}

const LANGUAGES = [
  { id: 'javascript', label: 'JavaScript' },
  { id: 'typescript', label: 'TypeScript' },
  { id: 'bash', label: 'Shell / Bash' },
  { id: 'json', label: 'JSON' },
  { id: 'rust', label: 'Rust' },
  { id: 'java', label: 'Java' },
  { id: 'python', label: 'Python' },
  { id: 'text', label: 'Plain Text' }
];

export function Snippets() {
  const { t } = useTranslation();
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [activeSnippet, setActiveSnippet] = useState<Snippet | null>(null);
  
  // Editor state
  const [isCreating, setIsCreating] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editLanguage, setEditLanguage] = useState('typescript');
  const [editCode, setEditCode] = useState('');
  const [editGroup, setEditGroup] = useState('');
  const [editTags, setEditTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState('');

  // Search & Filter state
  const [searchTerm, setSearchTerm] = useState('');
  const [activeGroup, setActiveGroup] = useState<string | null>(null);
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [activeLangFilter, setActiveLangFilter] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [groupsExpanded, setGroupsExpanded] = useState(true);

  const isTauri = '__TAURI_INTERNALS__' in window;

  const loadSnippets = async () => {
    if (!isTauri) return;
    try {
      const jsonStr: string = await invoke('read_snippets');
      const data = JSON.parse(jsonStr);
      // Sort by updated_at descending
      setSnippets(data.sort((a: Snippet, b: Snippet) => b.updated_at - a.updated_at));
    } catch (err) {
      console.error("Failed to load snippets", err);
    }
  };

  useEffect(() => {
    loadSnippets();
  }, []);

  const saveSnippetsToDisk = async (newSnippets: Snippet[]) => {
    if (!isTauri) return;
    try {
      await invoke('save_snippets', { snippetsJson: JSON.stringify(newSnippets) });
      setSnippets(newSnippets.sort((a, b) => b.updated_at - a.updated_at));
    } catch (err) {
      console.error("Failed to save snippets", err);
      alert(t('failedToSave'));
    }
  };

  const uniqueGroups = useMemo(() => {
    const groups = new Set<string>();
    snippets.forEach(s => {
      if (s.group) groups.add(s.group);
    });
    return Array.from(groups).sort();
  }, [snippets]);

  const uniqueTags = useMemo(() => {
    const tags = new Set<string>();
    snippets.forEach(s => {
      if (s.tags) s.tags.forEach(t => tags.add(t));
    });
    return Array.from(tags).sort();
  }, [snippets]);

  const uniqueLanguages = useMemo(() => {
    const langs = new Set<string>();
    snippets.forEach(s => langs.add(s.language));
    return Array.from(langs).sort();
  }, [snippets]);

  const handleSave = async () => {
    if (!editTitle || !editCode) return;

    let newSnippets = [...snippets];
    const now = Date.now();

    if (activeSnippet && !isCreating) {
      // Update
      newSnippets = newSnippets.map(s => 
        s.id === activeSnippet.id 
          ? { 
              ...s, 
              title: editTitle, 
              language: editLanguage, 
              code: editCode,
              group: editGroup || undefined,
              tags: editTags.length > 0 ? editTags : undefined,
              updated_at: now 
            }
          : s
      );
    } else {
      // Create
      const newSnippet: Snippet = {
        id: uuidv4(),
        title: editTitle,
        language: editLanguage,
        code: editCode,
        group: editGroup || undefined,
        tags: editTags.length > 0 ? editTags : undefined,
        created_at: now,
        updated_at: now
      };
      newSnippets.push(newSnippet);
      setActiveSnippet(newSnippet);
    }

    await saveSnippetsToDisk(newSnippets);
    setIsCreating(false);
  };

  const handleDelete = async (id: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    if (!confirm(t('confirmDelete', { name: t('thisSnippet', 'this snippet') }))) return;
    
    const newSnippets = snippets.filter(s => s.id !== id);
    await saveSnippetsToDisk(newSnippets);
    
    if (activeSnippet?.id === id) {
      setActiveSnippet(null);
      setIsCreating(false);
    }
  };

  const handleCopy = (code: string, id: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    navigator.clipboard.writeText(code);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const startCreate = () => {
    setActiveSnippet(null);
    setEditTitle('');
    setEditCode('');
    setEditLanguage('typescript');
    setEditGroup(activeGroup || '');
    setEditTags([]);
    setTagInput('');
    setIsCreating(true);
  };

  const startEdit = (snippet: Snippet) => {
    setActiveSnippet(snippet);
    setEditTitle(snippet.title);
    setEditCode(snippet.code);
    setEditLanguage(snippet.language);
    setEditGroup(snippet.group || '');
    setEditTags(snippet.tags || []);
    setTagInput('');
    setIsCreating(false);
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

  const filteredSnippets = snippets.filter(s => {
    const matchesSearch = 
      s.title.toLowerCase().includes(searchTerm.toLowerCase()) || 
      s.code.toLowerCase().includes(searchTerm.toLowerCase()) ||
      s.language.toLowerCase().includes(searchTerm.toLowerCase());
      
    const matchesGroup = activeGroup ? s.group === activeGroup : true;
    const matchesTag = activeTag ? s.tags?.includes(activeTag) : true;
    const matchesLang = activeLangFilter ? s.language === activeLangFilter : true;
    
    return matchesSearch && matchesGroup && matchesTag && matchesLang;
  });

  return (
    <div className="flex flex-col h-full space-y-4">
      <div className="flex items-center justify-between shrink-0">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('snippets')}</h2>
          <p className="text-sm text-muted-foreground mt-1">{t('manageSnippets')}</p>
        </div>
        <button
          onClick={startCreate}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm"
        >
          <Plus className="w-4 h-4" />
          {t('newSnippet')}
        </button>
      </div>

      <div className="flex-1 flex gap-6 min-h-0">
        {/* Left Sidebar: List & Filters */}
        <div className="w-1/3 flex flex-col gap-4 border-r pr-6 shrink-0 min-w-[250px]">
          <div className="relative shrink-0">
            <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
            <input 
              type="text" 
              placeholder={t('searchSnippets')}
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full flex h-10 rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
            />
          </div>

          {/* Filters Area */}
          <div className="flex flex-col gap-3 shrink-0 pb-2 border-b">
            {/* Group Filter Tree */}
            <div className="space-y-1">
              <button 
                onClick={() => setGroupsExpanded(!groupsExpanded)}
                className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground uppercase tracking-wider w-full hover:text-foreground transition-colors"
              >
                {groupsExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                {t('allGroups')}
              </button>
              
              {groupsExpanded && (
                <div className="flex flex-col gap-0.5 mt-1 ml-1">
                  <button
                    onClick={() => setActiveGroup(null)}
                    className={`flex items-center gap-2 px-2 py-1.5 rounded-md text-sm transition-colors ${
                      activeGroup === null 
                        ? 'bg-primary/10 text-primary font-medium' 
                        : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'
                    }`}
                  >
                    <Folder className="w-3.5 h-3.5" />
                    <span className="truncate">{t("all", "All")}</span>
                  </button>
                  {uniqueGroups.map(group => (
                    <button
                      key={group}
                      onClick={() => setActiveGroup(group)}
                      className={`flex items-center gap-2 px-2 py-1.5 rounded-md text-sm transition-colors ${
                        activeGroup === group 
                          ? 'bg-primary/10 text-primary font-medium' 
                          : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'
                      }`}
                    >
                      <Folder className="w-3.5 h-3.5" />
                      <span className="truncate">{group}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Tags Filter Pills */}
            {uniqueTags.length > 0 && (
              <div className="space-y-1.5">
                <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
                  <Tag className="w-3 h-3" />
                  {t('allTags')}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <button
                    onClick={() => setActiveTag(null)}
                    className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors ${
                      activeTag === null 
                        ? 'bg-primary text-primary-foreground' 
                        : 'bg-muted text-muted-foreground hover:bg-muted/80'
                    }`}
                  >
                    All
                  </button>
                  {uniqueTags.map(tag => (
                    <button
                      key={tag}
                      onClick={() => setActiveTag(tag)}
                      className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors ${
                        activeTag === tag 
                          ? 'bg-primary text-primary-foreground' 
                          : 'bg-muted text-muted-foreground hover:bg-muted/80'
                      }`}
                    >
                      {tag}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Language Filter Pills */}
            {uniqueLanguages.length > 0 && (
              <div className="space-y-1.5 pt-1">
                <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
                  <Code2 className="w-3 h-3" />
                  {t('language')}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <button
                    onClick={() => setActiveLangFilter(null)}
                    className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors uppercase ${
                      activeLangFilter === null 
                        ? 'bg-primary text-primary-foreground' 
                        : 'bg-muted text-muted-foreground hover:bg-muted/80'
                    }`}
                  >
                    All
                  </button>
                  {uniqueLanguages.map(lang => (
                    <button
                      key={lang}
                      onClick={() => setActiveLangFilter(lang)}
                      className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors uppercase ${
                        activeLangFilter === lang 
                          ? 'bg-primary text-primary-foreground' 
                          : 'bg-muted text-muted-foreground hover:bg-muted/80'
                      }`}
                    >
                      {lang === 'bash' ? 'sh' : lang}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>

          {/* Snippet List */}
          <div className="flex-1 overflow-y-auto space-y-2 pr-1 custom-scrollbar">
            {filteredSnippets.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-10 text-muted-foreground text-center">
                <Code2 className="w-8 h-8 mb-3 opacity-20" />
                <p className="text-sm">{t('noSnippetsFound')}</p>
                {!searchTerm && !activeGroup && !activeTag && !activeLangFilter && (
                  <p className="text-xs mt-1 opacity-70">{t('createFirstSnippet')}</p>
                )}
              </div>
            ) : (
              filteredSnippets.map((snippet) => (
                <div 
                  key={snippet.id}
                  onClick={() => startEdit(snippet)}
                  className={`group p-3 rounded-lg border cursor-pointer transition-all flex flex-col gap-1.5 ${
                    activeSnippet?.id === snippet.id && !isCreating
                      ? 'bg-primary/5 border-primary shadow-sm' 
                      : 'bg-card hover:border-primary/50'
                  }`}
                >
                  <div className="flex justify-between items-start">
                    <h4 className="font-semibold text-sm truncate pr-2 flex-1">{snippet.title}</h4>
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground uppercase tracking-wider shrink-0">
                      {snippet.language === 'bash' ? 'sh' : snippet.language}
                    </span>
                  </div>
                  
                  {/* Badges Preview */}
                  {(snippet.group || (snippet.tags && snippet.tags.length > 0)) && (
                    <div className="flex flex-wrap gap-1">
                      {snippet.group && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-medium bg-secondary text-secondary-foreground">
                          <Folder className="w-2.5 h-2.5 mr-0.5" />
                          {snippet.group}
                        </span>
                      )}
                      {snippet.tags?.slice(0, 2).map(tag => (
                        <span key={tag} className="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-medium bg-muted text-muted-foreground">
                          {tag}
                        </span>
                      ))}
                      {snippet.tags && snippet.tags.length > 2 && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-medium bg-muted text-muted-foreground">
                          +{snippet.tags.length - 2}
                        </span>
                      )}
                    </div>
                  )}

                  <p className="text-xs text-muted-foreground font-mono truncate opacity-60 mt-0.5">
                    {snippet.code.split('\n')[0]}
                  </p>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Right Area: Editor / Viewer */}
        <div className="flex-1 flex flex-col min-w-0 bg-card rounded-xl border shadow-sm overflow-hidden">
          {isCreating || activeSnippet ? (
            <div className="flex flex-col h-full">
              {/* Editor Header */}
              <div className="flex flex-col border-b shrink-0 bg-muted/10">
                <div className="flex items-center gap-3 p-4 pb-2">
                  <input 
                    type="text" 
                    placeholder={t('title')}
                    value={editTitle}
                    onChange={(e) => setEditTitle(e.target.value)}
                    className="flex-1 h-9 rounded-md border border-transparent bg-transparent px-2 py-1 text-base font-semibold focus-visible:outline-none focus-visible:bg-background focus-visible:border-input transition-colors"
                  />
                  
                  <select 
                    value={editLanguage}
                    onChange={(e) => setEditLanguage(e.target.value)}
                    className="h-9 rounded-md border border-input bg-background px-3 py-1 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring shrink-0"
                  >
                    {LANGUAGES.map(l => <option key={l.id} value={l.id}>{l.label}</option>)}
                  </select>

                  <div className="flex gap-2 pl-2 border-l shrink-0">
                    <button 
                      onClick={handleSave}
                      disabled={!editTitle || !editCode}
                      className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-1.5 rounded-md text-sm font-medium transition-colors disabled:opacity-50"
                    >
                      {t('save')}
                    </button>
                    {activeSnippet && !isCreating && (
                      <button 
                        onClick={(e) => handleDelete(activeSnippet.id, e)}
                        className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive px-2 py-1.5 rounded-md transition-colors"
                        title={t('delete')}
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    )}
                  </div>
                </div>

                {/* Metadata Editors */}
                <div className="flex items-center gap-4 px-6 pb-4 text-sm">
                  {/* Group Input */}
                  <div className="flex items-center gap-2 min-w-[150px]">
                    <Folder className="w-4 h-4 text-muted-foreground" />
                    <input
                      type="text"
                      list="snippet-group-options"
                      placeholder={t('group') || "Group"}
                      value={editGroup}
                      onChange={(e) => setEditGroup(e.target.value)}
                      className="bg-transparent border-none focus:ring-0 p-0 text-sm placeholder:text-muted-foreground/50 w-full"
                    />
                    <datalist id="snippet-group-options">
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

              {/* Editor Body */}
              <div className="flex-1 overflow-auto bg-[#1d1f21] relative group">
                <Editor
                  value={editCode}
                  onValueChange={code => setEditCode(code)}
                  highlight={code => Prism.highlight(
                    code, 
                    Prism.languages[editLanguage] || Prism.languages.text, 
                    editLanguage
                  )}
                  padding={20}
                  className="font-mono text-sm min-h-full"
                  style={{
                    fontFamily: '"JetBrains Mono", "Fira Code", monospace',
                    fontSize: 14,
                    color: '#c5c8c6',
                    outline: 'none',
                  }}
                  textareaClassName="focus:outline-none"
                />
                
                {/* Floating Copy Button */}
                {!isCreating && editCode && (
                  <button
                    onClick={() => handleCopy(editCode, activeSnippet!.id)}
                    className="absolute top-4 right-4 p-2 rounded-md bg-white/10 text-white/70 hover:bg-white/20 hover:text-white transition-all opacity-0 group-hover:opacity-100 backdrop-blur-sm"
                    title={t('copyToClipboard')}
                  >
                    {copiedId === activeSnippet?.id ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4" />}
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
              <TerminalSquare className="w-12 h-12 mb-4 opacity-10" />
              <p>{t('selectSnippetSidebar')}</p>
              <p className="text-sm mt-1">{t('createOneToGetStartedSnippet')}</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
