import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Code2, Plus, Search, Copy, Check, Trash2, TerminalSquare } from 'lucide-react';
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

  const [searchTerm, setSearchTerm] = useState('');
  const [copiedId, setCopiedId] = useState<string | null>(null);

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
      alert("Failed to save. Check console.");
    }
  };

  const handleSave = async () => {
    if (!editTitle || !editCode) return;

    let newSnippets = [...snippets];
    const now = Date.now();

    if (activeSnippet && !isCreating) {
      // Update
      newSnippets = newSnippets.map(s => 
        s.id === activeSnippet.id 
          ? { ...s, title: editTitle, language: editLanguage, code: editCode, updated_at: now }
          : s
      );
    } else {
      // Create
      const newSnippet: Snippet = {
        id: uuidv4(),
        title: editTitle,
        language: editLanguage,
        code: editCode,
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
    if (!confirm(t('confirmKill', { name: "this snippet" }))) return; // Reuse delete confirm text
    
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
    setIsCreating(true);
  };

  const startEdit = (snippet: Snippet) => {
    setActiveSnippet(snippet);
    setEditTitle(snippet.title);
    setEditCode(snippet.code);
    setEditLanguage(snippet.language);
    setIsCreating(false);
  };

  const filteredSnippets = snippets.filter(s => 
    s.title.toLowerCase().includes(searchTerm.toLowerCase()) || 
    s.code.toLowerCase().includes(searchTerm.toLowerCase()) ||
    s.language.toLowerCase().includes(searchTerm.toLowerCase())
  );

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
        {/* Left Sidebar: List */}
        <div className="w-1/3 flex flex-col gap-4 border-r pr-6 shrink-0">
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

          <div className="flex-1 overflow-y-auto space-y-2 pr-1 custom-scrollbar">
            {filteredSnippets.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-10 text-muted-foreground text-center">
                <Code2 className="w-8 h-8 mb-3 opacity-20" />
                <p className="text-sm">{t('noSnippetsFound')}</p>
                {!searchTerm && <p className="text-xs mt-1 opacity-70">{t('createFirstSnippet')}</p>}
              </div>
            ) : (
              filteredSnippets.map((snippet) => (
                <div 
                  key={snippet.id}
                  onClick={() => startEdit(snippet)}
                  className={`group p-3 rounded-lg border cursor-pointer transition-all ${
                    activeSnippet?.id === snippet.id && !isCreating
                      ? 'bg-primary/5 border-primary shadow-sm' 
                      : 'bg-card hover:border-primary/50'
                  }`}
                >
                  <div className="flex justify-between items-start mb-1">
                    <h4 className="font-semibold text-sm truncate pr-2">{snippet.title}</h4>
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground uppercase tracking-wider shrink-0">
                      {snippet.language === 'bash' ? 'sh' : snippet.language}
                    </span>
                  </div>
                  <p className="text-xs text-muted-foreground font-mono truncate opacity-60">
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
              <div className="flex items-center gap-3 p-4 border-b shrink-0 bg-muted/10">
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