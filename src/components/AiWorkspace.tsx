import { useEffect, useMemo, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import {
  Bot,
  FolderOpen,
  Loader2,
  MessageSquare,
  Plus,
  Settings2,
  Sparkles,
  Square,
  ChevronLeft,
  ChevronRight,
  Send,
  X,
  Paperclip,
  Check,
  Terminal,
  Clock,
  Cpu,
  Layers,
  MoreVertical,
  ChevronDown,
  Cloud,
  Command,
  Tag,
} from 'lucide-react';

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from './ui/dialog';

type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: {
    schema_version: number;
    revision: number;
  };
};

type ProjectRecord = {
  id: string;
  name: string;
  root_dir: string;
  default_provider?: string;
  default_model?: string;
  system_template?: string;
  context_budget: number;
  enable_file: boolean;
  enable_image: boolean;
  skills_mode: string;
  advanced_params: Record<string, unknown>;
  created_at: number;
  updated_at: number;
};

type ChatThreadRecord = {
  id: string;
  project_id: string;
  title: string;
  default_provider?: string;
  default_model?: string;
  status: string;
  created_at: number;
  updated_at: number;
};

type MessageModelSnapshot = {
  provider_id: string;
  provider_tool: string;
  model: string;
  params: Record<string, unknown>;
};

type SkillRunRecord = {
  id: string;
  skill_id: string;
  status: string;
};

type ChatMessageRecord = {
  id: string;
  thread_id: string;
  role: 'user' | 'assistant' | 'tool' | string;
  content: string;
  model_snapshot?: MessageModelSnapshot;
  attachment_ids: string[];
  skill_runs: SkillRunRecord[];
  created_at: number;
};

type MessagesPage = {
  messages: ChatMessageRecord[];
  next_cursor?: number;
};

type ProviderOption = {
  id: string;
  name: string;
  tool: string;
  model?: string;
};

type ModelOption = {
  id: string;
  name: string;
  supports_image: boolean;
  supports_reasoning: boolean;
};

type AttachmentRecord = {
  id: string;
  project_id: string;
  file_name: string;
  kind: string;
  mime: string;
  size: number;
};

type SkillPreviewItem = {
  id: string;
  name: string;
  description: string;
  model: string;
};

const toPathArray = (selected: string | string[] | null): string[] => {
  if (!selected) return [];
  if (Array.isArray(selected)) return selected;
  return [selected];
};

/**
 * 美化后的自定义 Select 组件
 */
function CustomSelect<T extends { id: string; name: string }>({
  label,
  icon: Icon,
  options,
  value,
  onChange,
  disabled,
  className = "",
}: {
  label: string;
  icon: any;
  options: T[];
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
  className?: string;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedOption = options.find((o) => o.id === value);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  return (
    <div className={`relative ${className}`} ref={containerRef}>
      <button
        onClick={() => !disabled && setIsOpen(!isOpen)}
        type="button"
        disabled={disabled}
        className={`flex items-center gap-2 px-3 py-2 rounded-xl border border-border/50 bg-background/50 hover:bg-background transition-all text-[11px] font-bold shadow-sm ${
          disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer active:scale-95'
        }`}
      >
        <Icon className="w-3.5 h-3.5 text-primary" />
        <span className="truncate max-w-[100px]">{selectedOption?.name || label}</span>
        <ChevronDown className={`w-3 h-3 transition-transform duration-200 ${isOpen ? 'rotate-180' : ''}`} />
      </button>

      {isOpen && (
        <div className="absolute bottom-full mb-2 left-0 z-[100] min-w-[200px] bg-background border border-border shadow-2xl rounded-2xl p-1.5 animate-in fade-in slide-in-from-bottom-2 duration-200 backdrop-blur-xl">
          <div className="max-h-64 overflow-y-auto custom-scrollbar">
            {options.length === 0 ? (
              <div className="px-3 py-4 text-center text-[10px] text-muted-foreground">No options available</div>
            ) : (
              options.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  onClick={() => {
                    onChange(opt.id);
                    setIsOpen(false);
                  }}
                  className={`w-full text-left px-3 py-2.5 rounded-xl text-[11px] font-bold transition-all flex items-center justify-between group ${
                    value === opt.id ? 'bg-primary text-primary-foreground' : 'hover:bg-muted text-foreground/80 hover:text-foreground'
                  }`}
                >
                  <span className="truncate">{opt.name}</span>
                  {value === opt.id && <Check className="w-3.5 h-3.5" />}
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export function AiWorkspace() {
  const { t } = useTranslation();
  const isTauri = '__TAURI_INTERNALS__' in window;

  const [projects, setProjects] = useState<ProjectRecord[]>([]);
  const [threads, setThreads] = useState<ChatThreadRecord[]>([]);
  const [messages, setMessages] = useState<ChatMessageRecord[]>([]);
  const [providers, setProviders] = useState<ProviderOption[]>([]);
  const [models, setModels] = useState<ModelOption[]>([]);

  const [activeProjectId, setActiveProjectId] = useState<string>('');
  const [activeThreadId, setActiveThreadId] = useState<string>('');
  const [selectedProviderId, setSelectedProviderId] = useState<string>('');
  const [selectedModelId, setSelectedModelId] = useState<string>('');

  const [newProjectName, setNewProjectName] = useState<string>('');
  const [newProjectRoot, setNewProjectRoot] = useState<string>('');
  const [newThreadTitle, setNewThreadTitle] = useState<string>('');

  const [input, setInput] = useState<string>('');
  const [currentStreamId, setCurrentStreamId] = useState<string | null>(null);
  const [streamingText, setStreamingText] = useState<string>('');
  const [error, setError] = useState<string>('');
  const [loading, setLoading] = useState<boolean>(false);

  // Slash Command and Skill Tags
  const [selectedSkillTags, setSelectedSkillTags] = useState<SkillPreviewItem[]>([]);
  const [allSkills, setAllSkills] = useState<SkillPreviewItem[]>([]);
  const [slashMenuOpen, setSlashMenuOpen] = useState(false);
  const [slashHighlightedIndex, setSlashHighlightedIndex] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Settings
  const [contextBudget, setContextBudget] = useState<number>(4000);
  const [enableFile, setEnableFile] = useState<boolean>(true);
  const [enableImage, setEnableImage] = useState<boolean>(false);
  const [skillsMode, setSkillsMode] = useState<string>('confirm');
  const [showAdvanced, setShowAdvanced] = useState<boolean>(false);
  const [temperature, setTemperature] = useState<number>(0.7);
  const [maxTokens, setMaxTokens] = useState<number>(2048);
  const [topP, setTopP] = useState<number>(1);
  const [timeoutMs, setTimeoutMs] = useState<number>(30000);
  const [retry, setRetry] = useState<number>(1);
  const [reasoningEffort, setReasoningEffort] = useState<string>('medium');
  const [reasoningSummary, setReasoningSummary] = useState<string>('auto');

  // UI state
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [switchProjectOpen, setSwitchProjectOpen] = useState(false);
  const [createProjectOpen, setCreateProjectOpen] = useState(false);
  const [selectedAttachments, setSelectedAttachments] = useState<AttachmentRecord[]>([]);
  const [skillPreview, setSkillPreview] = useState<SkillPreviewItem[]>([]);
  const messageEndRef = useRef<HTMLDivElement>(null);

  const activeProject = useMemo(
    () => projects.find((p) => p.id === activeProjectId),
    [projects, activeProjectId],
  );
  const activeThread = useMemo(
    () => threads.find((p) => p.id === activeThreadId),
    [threads, activeThreadId],
  );

  const scrollToBottom = () => {
    messageEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, streamingText]);

  const loadProviders = async () => {
    if (!isTauri) return;
    try {
      const res = await invoke<ApiResp<{ providers: Record<string, unknown>[] }>>('providers_list');
      const list: ProviderOption[] = (res.data.providers || []).map((item) => ({
        id: String(item.id || ''),
        name: String(item.name || item.id || 'Provider'),
        tool: String(item.tool || ''),
        model: typeof item.model === 'string' ? item.model : undefined,
      })).filter((p) => !!p.id);
      setProviders(list);
      if (!selectedProviderId && list.length > 0) {
        setSelectedProviderId(list[0].id);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const loadModels = async (providerId: string) => {
    if (!isTauri || !providerId) return;
    try {
      const res = await invoke<ApiResp<{ family: string; models: ModelOption[] }>>('chat_models_list', {
        provider: providerId,
      });
      const modelList = res.data.models || [];
      setModels(modelList);
      setSelectedModelId((prev) => {
        if (modelList.some((m) => m.id === prev)) return prev;
        return modelList[0]?.id || '';
      });
    } catch (e) {
      console.error(e);
    }
  };

  const loadProjects = async () => {
    if (!isTauri) return;
    try {
      const res = await invoke<ApiResp<ProjectRecord[]>>('projects_list');
      setProjects(res.data);
      if (!activeProjectId && res.data.length > 0) {
        setActiveProjectId(res.data[0].id);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const loadThreads = async (projectId: string) => {
    if (!isTauri || !projectId) return;
    try {
      const res = await invoke<ApiResp<ChatThreadRecord[]>>('chat_threads_list', {
        projectId,
      });
      setThreads(res.data);
      if (res.data.length > 0) {
        setActiveThreadId((prev) => {
          if (res.data.some((t) => t.id === prev)) return prev;
          return res.data[0].id;
        });
      } else {
        setActiveThreadId('');
        setMessages([]);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const loadMessages = async (threadId: string) => {
    if (!isTauri || !threadId) return;
    try {
      const res = await invoke<ApiResp<MessagesPage>>('chat_messages_list', {
        threadId,
        cursor: null,
      });
      setMessages(res.data.messages);
    } catch (e) {
      console.error(e);
    }
  };

  const loadAllSkills = async () => {
    if (!isTauri || !activeProjectId) return;
    try {
      const res = await invoke<ApiResp<{ skills: SkillPreviewItem[] }>>('skills_preview', {
        input: { project_id: activeProjectId, input: '' }
      });
      setAllSkills(res.data.skills || []);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    if (!isTauri) return;
    loadProviders().catch(console.error);
    loadProjects().catch(console.error);
  }, [isTauri]);

  useEffect(() => {
    if (!selectedProviderId) return;
    loadModels(selectedProviderId).catch(console.error);
  }, [selectedProviderId]);

  useEffect(() => {
    if (!activeProjectId) return;
    loadThreads(activeProjectId).catch(console.error);
    loadAllSkills().catch(console.error);
  }, [activeProjectId]);

  useEffect(() => {
    if (!activeThreadId) return;
    loadMessages(activeThreadId).catch(console.error);
  }, [activeThreadId]);

  useEffect(() => {
    if (!activeProject) return;
    if (activeProject.default_provider) {
      setSelectedProviderId(activeProject.default_provider);
    }
    if (activeProject.default_model) {
      setSelectedModelId(activeProject.default_model);
    }
    setContextBudget(activeProject.context_budget);
    setEnableFile(activeProject.enable_file);
    setEnableImage(activeProject.enable_image);
    setSkillsMode(activeProject.skills_mode);
  }, [activeProject]);

  useEffect(() => {
    if (!isTauri) return;
    let unlistenChunk: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    listen<Record<string, unknown>>('chat:chunk', (event) => {
      const payload = event.payload;
      const payloadThreadId = String(payload.thread_id || '');
      if (!payloadThreadId || payloadThreadId !== activeThreadId) return;
      const delta = String(payload.delta || '');
      setStreamingText((prev) => prev + delta);
    }).then((fn) => {
      unlistenChunk = fn;
    }).catch(console.error);

    listen<Record<string, unknown>>('chat:done', (event) => {
      const payload = event.payload;
      const payloadThreadId = String(payload.thread_id || '');
      if (!payloadThreadId || payloadThreadId !== activeThreadId) return;
      setStreamingText('');
      setCurrentStreamId(null);
      setSelectedAttachments([]);
      loadMessages(payloadThreadId).catch(console.error);
    }).then((fn) => {
      unlistenDone = fn;
    }).catch(console.error);

    listen<Record<string, unknown>>('chat:error', (event) => {
      const payload = event.payload;
      setError(String(payload.message || 'Unknown stream error'));
      setCurrentStreamId(null);
      setStreamingText('');
    }).then((fn) => {
      unlistenError = fn;
    }).catch(console.error);

    return () => {
      if (unlistenChunk) unlistenChunk();
      if (unlistenDone) unlistenDone();
      if (unlistenError) unlistenError();
    };
  }, [isTauri, activeThreadId]);

  const pickProjectRoot = async () => {
    if (!isTauri) return;
    const selected = await open({ directory: true, multiple: false });
    const paths = toPathArray(selected);
    if (paths.length > 0) {
      setNewProjectRoot(paths[0]);
    }
  };

  const createProject = async () => {
    if (!isTauri || !newProjectName.trim() || !newProjectRoot.trim()) return;
    setLoading(true);
    setError('');
    try {
      const res = await invoke<ApiResp<ProjectRecord>>('projects_create', {
        project: {
          name: newProjectName.trim(),
          root_dir: newProjectRoot.trim(),
          default_provider: selectedProviderId || null,
          default_model: selectedModelId || null,
          context_budget: contextBudget,
          enable_file: enableFile,
          enable_image: enableImage,
          skills_mode: skillsMode,
          advanced_params: {},
        },
      });
      setActiveProjectId(res.data.id);
      setNewProjectName('');
      setNewProjectRoot('');
      setCreateProjectOpen(false);
      await loadProjects();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const createThread = async (title: string) => {
    if (!isTauri || !activeProjectId || !title.trim()) return;
    setLoading(true);
    setError('');
    try {
      const res = await invoke<ApiResp<ChatThreadRecord>>('chat_thread_create', {
        thread: {
          project_id: activeProjectId,
          title: title.trim(),
          default_provider: selectedProviderId || null,
          default_model: selectedModelId || null,
          status: 'active',
        },
      });
      setNewThreadTitle('');
      setActiveThreadId(res.data.id);
      await loadThreads(activeProjectId);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const uploadAttachments = async () => {
    if (!isTauri || !activeProjectId) return;
    const selected = await open({
      directory: false,
      multiple: true,
      title: t('upload', 'Upload'),
    });
    const paths = toPathArray(selected);
    if (paths.length === 0) return;

    setLoading(true);
    setError('');
    try {
      const res = await invoke<ApiResp<AttachmentRecord[]>>('project_attachments_import', {
        projectId: activeProjectId,
        paths,
      });
      setSelectedAttachments((prev) => {
        const seen = new Set(prev.map((x) => x.id));
        const merged = [...prev];
        for (const item of res.data) {
          if (!seen.has(item.id)) {
            seen.add(item.id);
            merged.push(item);
          }
        }
        return merged;
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const sendMessage = async () => {
    if (!isTauri || !activeThreadId || (!input.trim() && selectedSkillTags.length === 0) || currentStreamId) return;
    if (selectedAttachments.some((a) => a.kind === 'image') && !isModelImageCapable) {
      setError(t('modelNoImageSupport', 'Selected model does not support image input.'));
      return;
    }
    setLoading(true);
    setError('');

    let finalContent = input.trim();
    if (selectedSkillTags.length > 0) {
      const tagContent = selectedSkillTags.map(s => `@${s.id}`).join(' ');
      finalContent = tagContent + (finalContent ? ' ' + finalContent : '');
    }

    try {
      const res = await invoke<ApiResp<{ stream_id: string }>>('chat_stream_start', {
        req: {
          thread_id: activeThreadId,
          content: finalContent,
          provider: selectedProviderId || null,
          model: selectedModelId || null,
          context_budget: contextBudget,
          enable_file: enableFile,
          enable_image: enableImage,
          skills_mode: skillsMode,
          temperature,
          max_tokens: maxTokens,
          top_p: topP,
          timeout_ms: timeoutMs,
          retry,
          reasoning_effort: reasoningEffort,
          reasoning_summary: reasoningSummary,
          attachment_ids: selectedAttachments.map((a) => a.id),
        },
      });
      setCurrentStreamId(res.data.stream_id);
      setInput('');
      setSelectedSkillTags([]);
      await loadMessages(activeThreadId);
    } catch (e) {
      setError(String(e));
      setCurrentStreamId(null);
    } finally {
      setLoading(false);
    }
  };

  const stopStream = async () => {
    if (!isTauri || !currentStreamId) return;
    try {
      await invoke('chat_stream_stop', { streamId: currentStreamId });
    } catch (e) {
      console.error(e);
    }
  };

  const previewSkills = async () => {
    if (!isTauri || !activeProjectId || !input.trim()) return;
    try {
      const res = await invoke<ApiResp<{ skills: SkillPreviewItem[] }>>('skills_preview', {
        input: {
          project_id: activeProjectId,
          input,
        },
      });
      setSkillPreview(res.data.skills || []);
    } catch (e) {
      setError(String(e));
    }
  };

  const executeSkill = async (skill: SkillPreviewItem) => {
    if (!isTauri || !activeProjectId) return;
    try {
      await invoke('skills_execute', {
        input: {
          project_id: activeProjectId,
          thread_id: activeThreadId || null,
          skill_id: skill.id,
          args: null,
          confirm_token: `confirm:${skill.id}`,
        },
      });
      setSkillPreview([]);
      if (activeThreadId) {
        await loadMessages(activeThreadId);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const filteredSkills = useMemo(() => {
    if (!input.startsWith('/')) return [];
    const query = input.slice(1).toLowerCase();
    return allSkills.filter(s => 
      s.name.toLowerCase().includes(query) || 
      s.id.toLowerCase().includes(query)
    );
  }, [input, allSkills]);

  const selectSkillFromMenu = (skill: SkillPreviewItem) => {
    setSelectedSkillTags(prev => {
      if (prev.some(s => s.id === skill.id)) return prev;
      return [...prev, skill];
    });
    setInput('');
    setSlashMenuOpen(false);
    textareaRef.current?.focus();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (slashMenuOpen) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSlashHighlightedIndex(prev => (prev + 1) % Math.max(1, filteredSkills.length));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSlashHighlightedIndex(prev => (prev - 1 + filteredSkills.length) % Math.max(1, filteredSkills.length));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (filteredSkills[slashHighlightedIndex]) {
          selectSkillFromMenu(filteredSkills[slashHighlightedIndex]);
        }
      } else if (e.key === 'Escape') {
        setSlashMenuOpen(false);
      }
    } else {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        sendMessage();
      } else if (e.key === 'Backspace' && input === '' && selectedSkillTags.length > 0) {
        setSelectedSkillTags(prev => prev.slice(0, -1));
      }
    }
  };

  useEffect(() => {
    if (input === '/') {
      setSlashMenuOpen(true);
      setSlashHighlightedIndex(0);
    } else if (!input.startsWith('/')) {
      setSlashMenuOpen(false);
    }
  }, [input]);

  const isModelImageCapable = useMemo(() => {
    const model = models.find((m) => m.id === selectedModelId);
    return model?.supports_image ?? false;
  }, [models, selectedModelId]);

  if (!isTauri) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground bg-muted/20">
        <Bot className="w-10 h-10 mr-3 opacity-20" />
        <span className="text-lg font-medium">{t('notInTauri', 'Only available in Tauri')}</span>
      </div>
    );
  }

  return (
    <div className="flex h-full overflow-hidden bg-background text-foreground">
      {/* Sidebar */}
      <div
        className={`relative flex flex-col border-r bg-muted/10 transition-all duration-300 ease-in-out ${
          sidebarOpen ? 'w-72' : 'w-0'
        }`}
      >
        <div className="p-4 border-b space-y-3 bg-muted/30">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 overflow-hidden">
              <Bot className="w-5 h-5 text-primary flex-shrink-0" />
              <h3 className="text-sm font-bold truncate">
                {activeProject ? activeProject.name : t('noProject', 'No Project')}
              </h3>
            </div>
            <div className="flex items-center gap-1">
              <button
                onClick={() => setSwitchProjectOpen(true)}
                type="button"
                className="p-1.5 hover:bg-background rounded-md transition-colors text-muted-foreground hover:text-primary"
                title={t('switchProject', 'Switch Project')}
              >
                <Layers className="w-4 h-4" />
              </button>
              <button
                onClick={() => setCreateProjectOpen(true)}
                type="button"
                className="p-1.5 hover:bg-background rounded-md transition-colors text-muted-foreground hover:text-primary"
                title={t('newProject', 'New Project')}
              >
                <Plus className="w-4 h-4" />
              </button>
              <button
                onClick={() => setSidebarOpen(false)}
                type="button"
                className="p-1.5 hover:bg-background rounded-md transition-colors"
              >
                <ChevronLeft className="w-4 h-4" />
              </button>
            </div>
          </div>
          {activeProject && (
            <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground bg-background/50 px-2 py-1 rounded-md border border-border/50">
              <FolderOpen className="w-3 h-3 flex-shrink-0" />
              <span className="truncate">{activeProject.root_dir}</span>
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          <div className="flex items-center justify-between px-2 py-2">
            <span className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest">
              {t('conversations', 'Conversations')}
            </span>
            <button
              onClick={() => {
                const title = prompt(t('threadTitle', 'Thread title'), 'New Chat');
                if (title) createThread(title);
              }}
              type="button"
              disabled={!activeProjectId}
              className="p-1 hover:bg-background rounded text-muted-foreground hover:text-primary transition-colors disabled:opacity-30"
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          </div>

          {threads.length === 0 ? (
            <div className="py-8 text-center px-4 space-y-2 opacity-40">
              <MessageSquare className="w-8 h-8 mx-auto mb-1" />
              <p className="text-xs">{t('noThreads', 'No chats yet')}</p>
            </div>
          ) : (
            threads.map((thread) => (
              <button
                key={thread.id}
                type="button"
                onClick={() => setActiveThreadId(thread.id)}
                className={`w-full text-left px-3 py-2.5 rounded-xl text-sm transition-all group relative flex items-center gap-3 ${
                  activeThreadId === thread.id
                    ? 'bg-primary text-primary-foreground shadow-sm font-medium'
                    : 'hover:bg-muted text-foreground/80'
                }`}
              >
                <MessageSquare className={`w-4 h-4 flex-shrink-0 ${activeThreadId === thread.id ? 'opacity-100' : 'opacity-40'}`} />
                <span className="truncate flex-1">{thread.title}</span>
                {activeThreadId !== thread.id && (
                  <MoreVertical className="w-3.5 h-3.5 opacity-0 group-hover:opacity-40 transition-opacity" />
                )}
              </button>
            ))
          )}
        </div>
      </div>

      {/* Main Area */}
      <div className="relative flex flex-col flex-1 min-w-0 bg-background overflow-hidden">
        <header className="h-14 border-b flex items-center justify-between px-6 bg-background/80 backdrop-blur-md sticky top-0 z-10">
          <div className="flex items-center gap-4 min-w-0 flex-1 text-left">
            {!sidebarOpen && (
              <button
                onClick={() => setSidebarOpen(true)}
                type="button"
                className="p-2 hover:bg-muted rounded-md transition-colors"
              >
                <ChevronRight className="w-4 h-4" />
              </button>
            )}
            <div className="min-w-0">
              <h2 className="text-sm font-bold truncate">
                {activeThread ? activeThread.title : activeProject?.name}
              </h2>
            </div>
          </div>

          <div className="flex items-center gap-3">
             <div className="flex items-center gap-2 text-[10px] px-2.5 py-1 bg-muted rounded-full border border-border/50 text-muted-foreground">
               <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
               {activeProject?.name}
             </div>
          </div>
        </header>

        <div className="flex-1 overflow-y-auto px-4 py-8 md:px-12 space-y-10 scroll-smooth">
          {messages.length === 0 && !streamingText && (
            <div className="flex flex-col items-center justify-center h-full max-w-2xl mx-auto text-center space-y-8 animate-in fade-in zoom-in-95 duration-700">
              <div className="w-16 h-16 bg-primary/10 rounded-3xl flex items-center justify-center shadow-inner">
                <Sparkles className="w-8 h-8 text-primary" />
              </div>
              <div className="space-y-3">
                <h1 className="text-3xl font-bold tracking-tight bg-gradient-to-br from-foreground to-foreground/50 bg-clip-text text-transparent">
                  {t('aiWorkspaceHeader', 'AI Workspace')}
                </h1>
                <p className="text-sm text-muted-foreground max-w-md mx-auto leading-relaxed">
                  {t('aiWorkspaceSubHeader', 'A dedicated coding workspace with project context, skills, and advanced model orchestration.')}
                </p>
              </div>
              <div className="grid grid-cols-2 gap-4 w-full">
                {[
                  { icon: Terminal, label: 'Run Git Skill', text: '/git status' },
                  { icon: Clock, label: 'Work Summary', text: 'Summarize today\'s work' },
                ].map((item, i) => (
                  <button
                    key={i}
                    type="button"
                    onClick={() => setInput(item.text)}
                    className="p-4 bg-muted/30 border border-border/50 rounded-2xl hover:border-primary/50 hover:bg-primary/5 transition-all text-left space-y-2 group"
                  >
                    <item.icon className="w-4 h-4 text-primary group-hover:scale-110 transition-transform" />
                    <div className="text-[11px] font-bold text-muted-foreground group-hover:text-primary">{item.label}</div>
                    <div className="text-xs opacity-60 truncate">{item.text}</div>
                  </button>
                ))}
              </div>
            </div>
          )}

          {messages.map((msg) => (
            <div
              key={msg.id}
              className={`flex w-full ${msg.role === 'user' ? 'justify-end' : 'justify-start'} animate-in fade-in slide-in-from-bottom-3 duration-500`}
            >
              <div
                className={`max-w-[85%] md:max-w-[75%] rounded-3xl px-5 py-4 ${
                  msg.role === 'user'
                    ? 'bg-primary text-primary-foreground shadow-lg shadow-primary/10'
                    : msg.role === 'tool'
                      ? 'bg-muted/50 border-2 border-dashed border-primary/20 font-mono text-xs text-muted-foreground'
                      : 'bg-muted/30 border border-border/50 text-foreground backdrop-blur-sm shadow-sm'
                }`}
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[10px] font-black uppercase tracking-widest opacity-40">
                    {msg.role === 'user' ? t('user', 'User') : msg.role}
                  </span>
                </div>
                
                <div className="text-sm leading-relaxed whitespace-pre-wrap break-words">
                  {msg.content}
                </div>

                {msg.attachment_ids.length > 0 && (
                  <div className="mt-4 flex flex-wrap gap-2 pt-3 border-t border-current/10">
                    {msg.attachment_ids.map((id) => (
                      <div key={id} className="flex items-center gap-2 px-2.5 py-1.5 rounded-xl bg-black/5 text-[10px] font-bold">
                        <Paperclip className="w-3 h-3" />
                        {id.slice(0, 12)}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))}

          {streamingText && (
            <div className="flex justify-start animate-in fade-in duration-300">
              <div className="max-w-[85%] md:max-w-[75%] rounded-3xl px-5 py-4 bg-muted/30 border border-border/50 shadow-sm relative overflow-hidden">
                <div className="flex items-center gap-2 mb-2">
                  <span className="text-[10px] font-black uppercase tracking-widest opacity-40">Assistant</span>
                  <div className="flex gap-1">
                    <span className="w-1 h-1 bg-primary rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                    <span className="w-1 h-1 bg-primary rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                    <span className="w-1 h-1 bg-primary rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                  </div>
                </div>
                <div className="text-sm leading-relaxed whitespace-pre-wrap break-words">
                  {streamingText}
                </div>
              </div>
            </div>
          )}

          {error && (
            <div className="flex justify-center py-4">
              <div className="bg-destructive/10 border border-destructive/20 text-destructive text-xs px-5 py-2.5 rounded-full flex items-center gap-2 animate-bounce">
                <X className="w-4 h-4" />
                {error}
                <button onClick={() => setError('')} type="button" className="ml-3 font-bold hover:underline underline-offset-4">OK</button>
              </div>
            </div>
          )}
          
          <div ref={messageEndRef} />
        </div>

        {/* Input Area Wrapper */}
        <div className="px-6 py-8 md:px-12 bg-gradient-to-t from-background via-background to-transparent relative">
          <div className="max-w-4xl mx-auto relative">
             
             {/* Attachment Preview (Moved to Top of Card) */}
             {selectedAttachments.length > 0 && (
                <div className="flex flex-wrap gap-2 mb-3 px-2 animate-in slide-in-from-bottom-2 duration-300">
                  {selectedAttachments.map((item) => (
                    <div key={item.id} className="flex items-center gap-2 bg-primary/5 border border-primary/20 rounded-2xl pl-3 pr-1.5 py-1.5 shadow-sm group">
                      <span className="text-[10px] font-bold truncate max-w-[150px] text-primary">{item.file_name}</span>
                      <button
                        onClick={() => setSelectedAttachments(prev => prev.filter(a => a.id !== item.id))}
                        type="button"
                        className="p-1 hover:bg-destructive hover:text-destructive-foreground rounded-full transition-colors"
                      >
                        <X className="w-3 h-3" />
                      </button>
                    </div>
                  ))}
                </div>
              )}

             {/* Slash Command Menu */}
             {slashMenuOpen && filteredSkills.length > 0 && (
              <div className="absolute bottom-full mb-4 left-0 right-0 z-[110] animate-in slide-in-from-bottom-4 duration-200">
                <div className="bg-background border border-border rounded-3xl shadow-2xl backdrop-blur-xl overflow-hidden">
                  <div className="px-4 py-3 border-b bg-muted/30 flex items-center gap-2">
                    <Command className="w-4 h-4 text-primary" />
                    <span className="text-xs font-bold uppercase tracking-widest text-muted-foreground">{t('availableSkills', 'Skills')}</span>
                  </div>
                  <div className="max-h-64 overflow-y-auto p-2 custom-scrollbar">
                    {filteredSkills.map((skill, idx) => (
                      <button
                        key={skill.id}
                        type="button"
                        onClick={() => selectSkillFromMenu(skill)}
                        onMouseEnter={() => setSlashHighlightedIndex(idx)}
                        className={`w-full text-left px-4 py-3 rounded-2xl transition-all flex items-center justify-between group ${
                          slashHighlightedIndex === idx ? 'bg-primary text-primary-foreground shadow-lg' : 'hover:bg-muted'
                        }`}
                      >
                        <div className="min-w-0 pr-4">
                          <div className="text-sm font-bold truncate flex items-center gap-2">
                            <Sparkles className={`w-3.5 h-3.5 ${slashHighlightedIndex === idx ? 'text-primary-foreground' : 'text-primary'}`} />
                            {skill.name}
                          </div>
                          <div className={`text-[10px] truncate mt-0.5 ${slashHighlightedIndex === idx ? 'text-primary-foreground/70' : 'text-muted-foreground'}`}>
                            {skill.description}
                          </div>
                        </div>
                        {slashHighlightedIndex === idx && <Check className="w-4 h-4 text-primary-foreground" />}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            )}

             {/* Skill Confirmation Overlay */}
             {skillPreview.length > 0 && !slashMenuOpen && (
              <div className="mb-4 animate-in slide-in-from-bottom-4 duration-300 relative z-[110]">
                <div className="bg-background border-2 border-primary/20 rounded-3xl p-5 shadow-2xl backdrop-blur-xl">
                  <div className="flex items-center justify-between mb-4">
                    <div className="flex items-center gap-2.5">
                      <div className="p-1.5 bg-primary/10 rounded-lg">
                        <Sparkles className="w-4 h-4 text-primary" />
                      </div>
                      <h4 className="text-sm font-bold tracking-tight">{t('availableSkills', 'Available Skills')}</h4>
                    </div>
                    <button onClick={() => setSkillPreview([])} type="button" className="p-1.5 hover:bg-muted rounded-full transition-colors">
                      <X className="w-4 h-4" />
                    </button>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    {skillPreview.map((skill) => (
                      <div key={skill.id} className="flex items-center justify-between p-4 rounded-2xl border bg-muted/20 hover:bg-muted/40 transition-all group">
                        <div className="min-w-0 pr-3">
                          <div className="text-[12px] font-bold truncate group-hover:text-primary">{skill.name}</div>
                          <div className="text-[10px] text-muted-foreground truncate mt-0.5">{skill.description}</div>
                        </div>
                        <button
                          onClick={() => executeSkill(skill)}
                          type="button"
                          className="flex-shrink-0 bg-primary text-primary-foreground text-[10px] font-black px-4 py-2 rounded-xl shadow-md hover:scale-105 active:scale-95 transition-all"
                        >
                          {t('execute', 'RUN')}
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}

            {/* Main Unified Input Card */}
            <div className="bg-muted/30 border-2 border-border/60 rounded-[32px] focus-within:border-primary/40 focus-within:bg-background transition-all shadow-xl backdrop-blur-sm flex flex-col relative">
              {/* Tag and Text Input Wrapper */}
              <div className="flex flex-wrap items-start px-6 pt-5 pb-2 gap-2">
                {selectedSkillTags.map((skill) => (
                  <div key={skill.id} className="flex items-center gap-1.5 bg-primary text-primary-foreground rounded-lg px-2.5 py-1 text-[11px] font-black animate-in zoom-in-95 duration-200">
                    <Tag className="w-3 h-3" />
                    {skill.name}
                    <button
                      onClick={() => setSelectedSkillTags(prev => prev.filter(s => s.id !== skill.id))}
                      type="button"
                      className="p-0.5 hover:bg-white/20 rounded-full transition-colors"
                    >
                      <X className="w-2.5 h-2.5" />
                    </button>
                  </div>
                ))}
                <textarea
                  ref={textareaRef}
                  value={input}
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder={selectedSkillTags.length === 0 ? t('chatInputHint', 'Message AI or type / to use skills...') : ''}
                  className="flex-1 bg-transparent border-none focus:ring-0 focus:outline-none p-0 text-sm resize-none min-h-[30px] max-h-60 leading-relaxed shadow-none ring-0"
                  rows={1}
                  style={{ height: 'auto' }}
                  onInput={(e) => {
                    const target = e.target as HTMLTextAreaElement;
                    target.style.height = 'auto';
                    target.style.height = `${target.scrollHeight}px`;
                  }}
                />
              </div>

              {/* Bottom Toolbar */}
              <div className="px-3 py-2 border-t border-border/40 bg-muted/20 flex items-center justify-between gap-2 rounded-b-[32px]">
                <div className="flex items-center gap-2">
                  <CustomSelect
                    label="Provider"
                    icon={Cloud}
                    options={providers}
                    value={selectedProviderId}
                    onChange={setSelectedProviderId}
                  />
                  <CustomSelect
                    label="Model"
                    icon={Cpu}
                    options={models}
                    value={selectedModelId}
                    onChange={setSelectedModelId}
                    className="min-w-[140px]"
                  />
                  
                  <div className="w-[1px] h-4 bg-border/50 mx-1" />
                  <button
                    onClick={uploadAttachments}
                    type="button"
                    disabled={!activeProjectId || !enableFile}
                    className="p-2.5 text-muted-foreground hover:text-primary hover:bg-primary/10 rounded-xl transition-all disabled:opacity-30"
                    title={t('attachFile', 'Attach File')}
                  >
                    <Paperclip className="w-4 h-4" />
                  </button>
                  <button
                    onClick={() => setSettingsOpen(true)}
                    type="button"
                    className="p-2.5 text-muted-foreground hover:text-primary hover:bg-primary/10 rounded-xl transition-all"
                    title={t('advancedParams', 'Advanced Parameters')}
                  >
                    <Settings2 className="w-4 h-4" />
                  </button>
                </div>

                <div className="flex items-center gap-2">
                   {input.trim() && (
                    <button
                      onClick={previewSkills}
                      type="button"
                      className="p-2.5 text-primary hover:bg-primary/10 rounded-xl transition-all group"
                      title={t('predictSkills', 'Predict Skills')}
                    >
                      <Sparkles className="w-4 h-4 group-hover:rotate-12 transition-transform" />
                    </button>
                  )}

                  {currentStreamId ? (
                    <button
                      onClick={stopStream}
                      type="button"
                      className="p-2.5 bg-destructive text-destructive-foreground rounded-2xl shadow-lg hover:brightness-110 active:scale-95 transition-all"
                    >
                      <Square className="w-4 h-4 fill-current" />
                    </button>
                  ) : (
                    <button
                      onClick={sendMessage}
                      type="button"
                      disabled={!activeThreadId || (!input.trim() && selectedSkillTags.length === 0) || loading}
                      className="p-2.5 bg-primary text-primary-foreground rounded-2xl shadow-lg hover:brightness-110 active:scale-95 transition-all disabled:opacity-30 disabled:scale-100 flex items-center justify-center min-w-[44px]"
                    >
                      {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
                    </button>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Switch Project Dialog */}
      <Dialog open={switchProjectOpen} onOpenChange={setSwitchProjectOpen}>
        <DialogContent className="max-w-md rounded-3xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Layers className="w-5 h-5 text-primary" />
              {t('selectProject', 'Switch Project')}
            </DialogTitle>
          </DialogHeader>
          <div className="max-h-80 overflow-y-auto space-y-2 py-4">
            {projects.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => {
                  setActiveProjectId(p.id);
                  setSwitchProjectOpen(false);
                }}
                className={`w-full text-left p-4 rounded-2xl border transition-all flex items-center justify-between group ${
                  activeProjectId === p.id ? 'bg-primary/10 border-primary/40 shadow-inner' : 'hover:bg-muted border-transparent'
                }`}
              >
                <div className="min-w-0 pr-4">
                  <div className="text-sm font-bold truncate group-hover:text-primary transition-colors">{p.name}</div>
                  <div className="text-[10px] text-muted-foreground truncate mt-1">{p.root_dir}</div>
                </div>
                {activeProjectId === p.id && <Check className="w-4 h-4 text-primary flex-shrink-0" />}
              </button>
            ))}
          </div>
        </DialogContent>
      </Dialog>

      {/* Create Project Dialog */}
      <Dialog open={createProjectOpen} onOpenChange={setCreateProjectOpen}>
        <DialogContent className="max-w-md rounded-3xl">
          <DialogHeader>
            <DialogTitle>{t('createNewProject', 'Create Project')}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <label className="text-xs font-bold text-muted-foreground uppercase">{t('name', 'Project Name')}</label>
              <input
                value={newProjectName}
                onChange={(e) => setNewProjectName(e.target.value)}
                placeholder="e.g., My Awesome App"
                className="w-full bg-muted/50 border-2 border-transparent focus:border-primary/20 rounded-2xl px-4 py-3 text-sm focus:ring-0 transition-all outline-none"
              />
            </div>
            <div className="space-y-2">
              <label className="text-xs font-bold text-muted-foreground uppercase">{t('rootDir', 'Root Directory')}</label>
              <div className="flex gap-2">
                <input
                  value={newProjectRoot}
                  readOnly
                  placeholder="/users/path/to/project"
                  className="flex-1 bg-muted/50 border-2 border-transparent rounded-2xl px-4 py-3 text-sm outline-none truncate opacity-70"
                />
                <button
                  onClick={pickProjectRoot}
                  type="button"
                  className="p-3 bg-muted hover:bg-muted/80 rounded-2xl transition-all"
                >
                  <FolderOpen className="w-5 h-5" />
                </button>
              </div>
            </div>
          </div>
          <DialogFooter>
            <button
              onClick={createProject}
              type="button"
              disabled={loading || !newProjectName.trim() || !newProjectRoot.trim()}
              className="w-full py-4 bg-primary text-primary-foreground rounded-2xl font-black text-sm shadow-lg hover:brightness-110 active:scale-[0.98] transition-all disabled:opacity-40"
            >
              {loading ? <Loader2 className="w-5 h-5 animate-spin mx-auto" /> : t('confirmCreate', 'CREATE PROJECT')}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Settings Drawer */}
      {settingsOpen && (
        <div className="fixed inset-0 z-50 flex justify-end bg-black/40 backdrop-blur-sm animate-in fade-in duration-300">
          <div
            className="w-80 h-full bg-background border-l shadow-2xl p-8 flex flex-col gap-8 animate-in slide-in-from-right duration-500"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between">
              <h3 className="text-xl font-black flex items-center gap-2.5">
                <Settings2 className="w-6 h-6 text-primary" />
                {t('config', 'Config')}
              </h3>
              <button onClick={() => setSettingsOpen(false)} type="button" className="p-2 hover:bg-muted rounded-full transition-colors">
                <X className="w-6 h-6" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto space-y-8 pr-2 custom-scrollbar">
              <section className="space-y-5">
                <div className="text-[11px] font-black text-muted-foreground uppercase tracking-widest border-b pb-2">Context & Storage</div>
                <div className="space-y-5">
                  <div className="space-y-2">
                    <label className="text-xs font-bold">Context Budget (Tokens)</label>
                    <input
                      type="number"
                      value={contextBudget}
                      onChange={(e) => setContextBudget(Number(e.target.value))}
                      className="w-full bg-muted/30 border-2 border-transparent focus:border-primary/20 rounded-2xl px-4 py-3 text-sm transition-all outline-none"
                    />
                  </div>
                  <div className="flex items-center justify-between p-4 bg-muted/30 rounded-2xl border border-border/50">
                    <span className="text-xs font-bold">Index Project Files</span>
                    <input
                      type="checkbox"
                      checked={enableFile}
                      onChange={(e) => setEnableFile(e.target.checked)}
                      className="w-5 h-5 rounded-lg text-primary focus:ring-primary/20"
                    />
                  </div>
                </div>
              </section>

              <section className="space-y-5">
                 <div className="flex items-center justify-between border-b pb-2">
                  <div className="text-[11px] font-black text-muted-foreground uppercase tracking-widest">Model Parameters</div>
                  <button
                    onClick={() => setShowAdvanced(!showAdvanced)}
                    type="button"
                    className="text-[10px] font-bold text-primary hover:underline"
                  >
                    {showAdvanced ? 'Hide' : 'Advanced'}
                  </button>
                </div>
                <div className="space-y-6">
                  <div className="space-y-3">
                    <div className="flex justify-between text-xs font-bold">
                      <span>Temperature</span>
                      <span className="text-primary font-mono">{temperature}</span>
                    </div>
                    <input
                      type="range"
                      min="0"
                      max="2"
                      step="0.1"
                      value={temperature}
                      onChange={(e) => setTemperature(Number(e.target.value))}
                      className="w-full h-1.5 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
                    />
                  </div>
                  {showAdvanced && (
                    <div className="space-y-5 animate-in fade-in slide-in-from-top-2">
                       <div className="space-y-2">
                        <label className="text-xs font-bold">Max Tokens</label>
                        <input
                          type="number"
                          value={maxTokens}
                          onChange={(e) => setMaxTokens(Number(e.target.value))}
                          className="w-full bg-muted/30 border rounded-2xl px-4 py-3 text-sm"
                        />
                      </div>
                      <div className="space-y-2">
                        <label className="text-xs font-bold">Reasoning Effort</label>
                        <select
                          value={reasoningEffort}
                          onChange={(e) => setReasoningEffort(e.target.value)}
                          className="w-full bg-muted/30 border rounded-2xl px-4 py-3 text-sm"
                        >
                          <option value="minimal">minimal</option>
                          <option value="low">low</option>
                          <option value="medium">medium</option>
                          <option value="high">high</option>
                        </select>
                      </div>
                    </div>
                  )}
                </div>
              </section>
            </div>

            <button
              onClick={() => setSettingsOpen(false)}
              type="button"
              className="w-full py-4 bg-primary text-primary-foreground rounded-2xl font-black text-sm shadow-lg hover:brightness-110 active:scale-95 transition-all flex items-center justify-center gap-2"
            >
              <Check className="w-5 h-5" />
              APPLY SETTINGS
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
