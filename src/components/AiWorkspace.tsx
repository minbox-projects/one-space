import { useEffect, useEffectEvent, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  Archive,
  ArrowLeft,
  Bot,
  Check,
  Clock3,
  Copy,
  Globe,
  Layers3,
  Loader2,
  MessageSquare,
  PanelRightClose,
  PanelRightOpen,
  Pin,
  PinOff,
  Play,
  Plus,
  Radar,
  Save,
  Search,
  Send,
  Sparkles,
  Trash2,
  Wand2,
  XCircle,
  Zap,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useConfirmDialog } from './ConfirmDialogProvider';
import { AiConnectionsSettings } from './AiConnectionsSettings';
import { ModelCenter } from './ModelCenter';
import { ToolCallsPanel } from './AiWorkspace/ToolCallsPanel';
import {
  aiWorkspaceBootstrap,
  mcpToolPreviewRefresh,
  showQuickAssistantWindow,
  type AiWorkspaceBootstrap,
  type AiWorkspaceSettings,
  type AssistantConversation,
  type AssistantConversationListItem,
  type AssistantMessage,
  type AssistantPreset,
  type AssistantStreamEvent,
  type AutomationJob,
  type AutomationJobView,
  type ManagedMcpCatalogResponse,
  type ManagedMcpServerCatalogItem,
  type McpImpactTag,
  type QuickAssistantPreferences,
  workspaceAssistantMcpCatalog,
  workspaceAssistantDelete,
  workspaceAssistantTestRun,
  workspaceAssistantUpsert,
  workspaceAssistantsList,
  workspaceAutomationDelete,
  workspaceAutomationRunNow,
  workspaceAutomationToggle,
  workspaceAutomationUpsert,
  workspaceAutomationsList,
  workspaceConversationCreate,
  workspaceConversationDelete,
  workspaceConversationGet,
  workspaceConversationResetContext,
  workspaceConversationSend,
  workspaceConversationUpdate,
  workspaceConversationsList,
  workspaceQuickAssistantSave,
  workspaceSettingsSave,
} from '@/lib/aiWorkspace';
import {
  mapMcpServerIdsToLabels,
  upsertToolCall,
} from '@/lib/assistantToolCalls';

const PENDING_CONVERSATION_KEY = 'onespace:pending-assistant-conversation';

type WorkspaceSection = 'conversations' | 'assistants' | 'automations' | 'models' | 'quick';
type AiWorkspaceMode = 'full' | 'automations' | 'models' | 'assistants';

const QUICK_ROLES = [
  { role: 'quick_assistant', label: 'Quick Assistant' },
  { role: 'assistant', label: 'Assistant' },
  { role: 'chat', label: 'Chat' },
  { role: 'summary', label: 'Summary' },
  { role: 'translate', label: 'Translate' },
  { role: 'topic_naming', label: 'Topic Naming' },
] as const;

function formatTimestamp(ts?: number | null) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatTrigger(trigger: AutomationJob['trigger']) {
  if (trigger.kind === 'interval' && trigger.interval_minutes) {
    return `Every ${trigger.interval_minutes} min`;
  }
  if (trigger.kind === 'weekly') {
    const days = trigger.weekdays.length ? trigger.weekdays.join(', ') : '1';
    return `Weekly ${days} ${trigger.time_of_day || '09:00'}`;
  }
  return `Daily ${trigger.time_of_day || '09:00'}`;
}

function parseCommaSeparated(input: string) {
  return input
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function createAssistantDraft(
  settings: AiWorkspaceSettings,
  defaultMcpServerIds: string[] = [],
): AssistantPreset {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: '',
    name: 'New Assistant',
    avatar_emoji: 'AI',
    description: '',
    system_prompt: '',
    primary_model_id:
      settings.role_bindings.find((binding) => binding.role === 'assistant')?.model_id || null,
    light_model_id:
      settings.role_bindings.find((binding) => binding.role === 'summary')?.model_id || null,
    default_model_profile_id: null,
    light_model_profile_id: null,
    tool_policy: {
      web_search: true,
      workspace_read: false,
      notes_search: false,
    },
    knowledge_base_ids: [],
    mcp_server_ids: [...defaultMcpServerIds],
    memory_enabled: false,
    output_contract: '',
    created_at: now,
    updated_at: now,
  };
}

function createAutomationDraft(
  settings: AiWorkspaceSettings,
  assistantId?: string | null,
): AutomationJob {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: '',
    name: 'New Automation',
    assistant_id: assistantId || null,
    agent_id: assistantId || '',
    prompt: '',
    model_profile_id: null,
    model_override_id:
      settings.role_bindings.find((binding) => binding.role === 'automation')?.model_id || null,
    web_search_enabled: false,
    trigger: {
      kind: 'daily',
      time_of_day: '09:00',
      interval_minutes: null,
      weekdays: [1, 2, 3, 4, 5],
    },
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    output_target: 'assistant_conversation',
    conversation_id: null,
    enabled: true,
    next_run_at: null,
    last_run_at: null,
    last_status: null,
    last_error: null,
    created_at: now,
    updated_at: now,
  };
}

function normalizeAssistantDraft(draft: AssistantPreset): AssistantPreset {
  return {
    ...draft,
    name: draft.name.trim() || 'Untitled Assistant',
    description: draft.description.trim(),
    system_prompt: draft.system_prompt.trim(),
    output_contract: draft.output_contract.trim(),
    knowledge_base_ids: draft.knowledge_base_ids.map((item) => item.trim()).filter(Boolean),
    mcp_server_ids: draft.mcp_server_ids.map((item) => item.trim()).filter(Boolean),
  };
}

function capabilityBadge(label: string) {
  return (
    <span className="rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
      {label}
    </span>
  );
}

function formatRuntimeError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function impactBadge(label: string) {
  return (
    <span className="rounded-full border border-dashed px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
      {label}
    </span>
  );
}

function mcpCategoryLabel(
  category: ManagedMcpServerCatalogItem['category'],
  t: (key: string, defaultValue: string) => string,
) {
  switch (category) {
    case 'search':
      return t('mcpCategorySearch', 'Search');
    case 'docs':
      return t('mcpCategoryDocs', 'Docs');
    case 'workspace':
      return t('mcpCategoryWorkspace', 'Workspace');
    case 'automation':
      return t('mcpCategoryAutomation', 'Automation');
    default:
      return t('mcpCategoryIntegration', 'Integration');
  }
}

function mcpImpactLabel(
  tag: McpImpactTag,
  t: (key: string, defaultValue: string) => string,
) {
  switch (tag) {
    case 'network':
      return t('mcpImpactNetwork', 'Network');
    case 'remote_api':
      return t('mcpImpactRemoteApi', 'Remote API');
    case 'credentials':
      return t('mcpImpactCredentials', 'Credentials');
    case 'workspace_read':
      return t('mcpImpactWorkspaceRead', 'Workspace Read');
    case 'workspace_write':
      return t('mcpImpactWorkspaceWrite', 'Workspace Write');
    case 'data_access':
      return t('mcpImpactDataAccess', 'Data Access');
    case 'local_state':
      return t('mcpImpactLocalState', 'Local State');
    case 'browser_automation':
      return t('mcpImpactBrowser', 'Browser');
    case 'trusted':
      return t('mcpImpactTrusted', 'Trusted');
    default:
      return tag;
  }
}

function mcpPreviewSummary(
  item: ManagedMcpServerCatalogItem,
  t: (key: string, defaultValue: string, options?: Record<string, unknown>) => string,
) {
  if (item.tool_preview.status === 'ready') {
    return t('mcpPreviewReady', '{{count}} tools cached', {
      count: item.tool_preview.tool_count,
    });
  }
  if (item.tool_preview.status === 'failed') {
    return item.tool_preview.error || t('mcpPreviewFailed', 'Preview failed');
  }
  return t('mcpPreviewUnchecked', 'Preview not fetched yet');
}

function mergeMcpCatalogItems(
  current: ManagedMcpCatalogResponse | null,
  refreshedItems: ManagedMcpServerCatalogItem[],
): ManagedMcpCatalogResponse | null {
  if (!current) return current;
  const refreshedById = new Map(refreshedItems.map((item) => [item.server_id, item]));
  return {
    ...current,
    items: current.items
      .map((item) => refreshedById.get(item.server_id) || item)
      .sort((a, b) => a.name.localeCompare(b.name)),
  };
}

function McpServerSelector({
  catalog,
  selectedIds,
  onChange,
  onRefresh,
  refreshing,
}: {
  catalog: ManagedMcpCatalogResponse | null;
  selectedIds: string[];
  onChange: (serverIds: string[]) => void;
  onRefresh: (serverIds?: string[]) => void;
  refreshing: boolean;
}) {
  const { t } = useTranslation();
  const items = catalog?.items || [];
  const itemsById = new Map(items.map((item) => [item.server_id, item]));
  const selectedItems = selectedIds.map((serverId, index) => {
    return (
      itemsById.get(serverId) || {
        server_id: serverId,
        config_key: '',
        name: t('mcpCustomServerName', 'Custom MCP {{index}}', {
          index: index + 1,
        }),
        description: '',
        transport: 'unknown',
        category: 'integration' as const,
        capability_summary: t(
          'mcpMissingCatalogDesc',
          'This assistant references a server that is not available in the managed catalog.',
        ),
        capability_tags: [],
        impact_tags: [],
        impact_note: null,
        tool_preview: {
          status: 'unchecked',
          checked_at: null,
          error: null,
          tool_count: 0,
          tools: [],
        },
      }
    );
  });

  const toggleServer = (serverId: string) => {
    if (selectedIds.includes(serverId)) {
      onChange(selectedIds.filter((item) => item !== serverId));
      return;
    }
    onChange([...selectedIds, serverId]);
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
            {t('mcpManagedTitle', 'Managed MCP')}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t(
              'mcpManagedDesc',
              'New assistants bind Exa and Context7 by default. The network retrieval toggle only affects search-class MCP tools.',
            )}
          </div>
        </div>
        <button
          type="button"
          onClick={() => onRefresh(selectedIds.length ? selectedIds : undefined)}
          disabled={refreshing || items.length === 0}
          className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-xs font-medium hover:bg-muted disabled:opacity-60"
        >
          {refreshing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Radar className="h-3.5 w-3.5" />}
          {t('mcpRefreshPreview', 'Refresh Preview')}
        </button>
      </div>

      <div className="rounded-2xl border bg-muted/10 p-3">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          {t('mcpSelectedServers', 'Selected Servers')}
        </div>
        {selectedItems.length > 0 ? (
          <div className="flex flex-wrap gap-2">
            {selectedItems.map((item) => (
              <button
                key={item.server_id}
                type="button"
                onClick={() => toggleServer(item.server_id)}
                className="rounded-2xl border bg-background px-3 py-2 text-left transition-colors hover:bg-muted/40"
              >
                <div className="text-sm font-medium">{item.name}</div>
                <div className="mt-1 max-w-[280px] text-xs leading-5 text-muted-foreground">
                  {item.capability_summary || item.description || t('mcpManagedServerFallback', 'Managed MCP server')}
                </div>
              </button>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted-foreground">
            {t('mcpNoSelection', 'No MCP servers selected yet.')}
          </div>
        )}
      </div>

      {items.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-2">
          {items.map((item) => {
            const selected = selectedIds.includes(item.server_id);
            return (
              <button
                key={item.server_id}
                type="button"
                onClick={() => toggleServer(item.server_id)}
                className={`rounded-2xl border px-4 py-4 text-left transition-colors ${
                  selected ? 'border-primary bg-primary/5' : 'bg-background hover:bg-muted/30'
                }`}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium">{item.name}</div>
                    <div className="mt-1 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                      {mcpCategoryLabel(item.category, t)} · {item.transport}
                    </div>
                  </div>
                  <span
                    className={`rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${
                      selected ? 'border-primary text-primary' : 'text-muted-foreground'
                    }`}
                  >
                    {selected ? t('mcpSelected', 'Selected') : t('mcpAdd', 'Add')}
                  </span>
                </div>

                <div className="mt-3 text-sm leading-6 text-muted-foreground">
                  {item.capability_summary || item.description}
                </div>

                {item.capability_tags.length > 0 ? (
                  <div className="mt-3 flex flex-wrap gap-2">
                    {item.capability_tags.slice(0, 4).map((tag) => (
                      <span key={`${item.server_id}-${tag}`}>{capabilityBadge(tag)}</span>
                    ))}
                  </div>
                ) : null}

                {item.impact_tags.length > 0 ? (
                  <div className="mt-3 flex flex-wrap gap-2">
                    {item.impact_tags.map((tag) => (
                      <span key={`${item.server_id}-${tag}`}>{impactBadge(mcpImpactLabel(tag, t))}</span>
                    ))}
                  </div>
                ) : null}

                <div className="mt-3 rounded-xl border border-dashed bg-muted/10 px-3 py-3">
                  <div className="text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t('mcpToolPreview', 'Tool Preview')}
                  </div>
                  <div className="mt-1 text-sm text-foreground">{mcpPreviewSummary(item, t)}</div>
                  {item.tool_preview.tools.length > 0 ? (
                    <div className="mt-2 flex flex-wrap gap-2">
                      {item.tool_preview.tools.slice(0, 3).map((tool) => (
                        <span
                          key={`${item.server_id}-${tool.name}`}
                          className="rounded-full border bg-background px-2 py-0.5 text-[11px] text-muted-foreground"
                        >
                          {tool.name}
                        </span>
                      ))}
                    </div>
                  ) : null}
                  {item.impact_note ? (
                    <div className="mt-2 text-xs leading-5 text-muted-foreground">{item.impact_note}</div>
                  ) : null}
                </div>
              </button>
            );
          })}
        </div>
      ) : (
        <div className="rounded-2xl border border-dashed bg-muted/10 px-4 py-6 text-sm text-muted-foreground">
          {t('mcpCatalogEmpty', 'Managed MCP catalog is loading or empty.')}
        </div>
      )}
    </div>
  );
}

function MessageCard({ message }: { message: AssistantMessage }) {
  const [isHovered, setIsHovered] = useState(false);
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(message.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback for older browsers
      const textArea = document.createElement('textarea');
      textArea.value = message.content;
      document.body.appendChild(textArea);
      textArea.select();
      document.execCommand('copy');
      document.body.removeChild(textArea);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleCopySelection = async () => {
    const selection = window.getSelection();
    if (selection && selection.toString()) {
      try {
        await navigator.clipboard.writeText(selection.toString());
      } catch {
        // Browser will handle it natively
      }
    }
  };

  if (message.role === 'context_reset') {
    return (
      <div className="flex items-center gap-3 py-2">
        <div className="h-px flex-1 border-t border-dashed border-border" />
        <span className="rounded-full border bg-muted/40 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
          {message.content}
        </span>
        <div className="h-px flex-1 border-t border-dashed border-border" />
      </div>
    );
  }

  const isAssistant = message.role === 'assistant';
  return (
    <div
      className="group relative rounded-3xl border bg-card/90 px-4 py-4 shadow-sm"
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Copy button for user messages - shows on hover */}
      {!isAssistant && isHovered && (
        <button
          onClick={handleCopy}
          className="absolute right-3 top-3 rounded-lg border bg-background p-1.5 text-muted-foreground shadow-sm transition-colors hover:bg-muted hover:text-foreground"
          title="Copy message"
        >
          {copied ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
        </button>
      )}

      {/* Copy button for assistant messages - always visible but subtle */}
      {isAssistant && message.status !== 'streaming' && (
        <button
          onClick={handleCopy}
          className={`absolute right-3 top-3 rounded-lg border bg-background p-1.5 text-muted-foreground shadow-sm transition-colors hover:bg-muted hover:text-foreground ${
            isHovered ? 'opacity-100' : 'opacity-0'
          }`}
          title="Copy message"
        >
          {copied ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
        </button>
      )}

      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <div className={`rounded-full p-2 ${isAssistant ? 'bg-primary/10 text-primary' : 'bg-muted text-foreground'}`}>
            {isAssistant ? <Bot className="h-4 w-4" /> : <MessageSquare className="h-4 w-4" />}
          </div>
          <div>
            <div className="text-sm font-medium">{isAssistant ? 'Assistant' : 'You'}</div>
            <div className="text-[11px] text-muted-foreground">{formatTimestamp(message.created_at)}</div>
          </div>
        </div>
        {message.status === 'streaming' ? (
          <div className="inline-flex items-center gap-2 rounded-full border bg-muted/40 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Streaming
          </div>
        ) : null}
        {message.status === 'failed' ? (
          <div className="inline-flex items-center gap-2 rounded-full border border-destructive/30 bg-destructive/5 px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-destructive">
            <XCircle className="h-3.5 w-3.5" />
            Failed
          </div>
        ) : null}
      </div>

      {message.reasoning ? (
        <div className="mb-4 rounded-2xl border bg-muted/20 p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Thinking
          </div>
          <pre className="whitespace-pre-wrap break-words text-xs leading-6 text-muted-foreground">
            {message.reasoning}
          </pre>
        </div>
      ) : null}

      <div className="prose prose-sm max-w-none dark:prose-invert" onDoubleClick={handleCopySelection}>
        {isAssistant ? (
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content || ' '}</ReactMarkdown>
        ) : (
          <div className="whitespace-pre-wrap break-words text-sm leading-6">{message.content}</div>
        )}
      </div>

      {message.sources.length > 0 ? (
        <div className="mt-4 rounded-2xl border bg-muted/10 p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Sources
          </div>
          <div className="space-y-2">
            {message.sources.map((source, index) => (
              <a
                key={`${source.url}-${index}`}
                href={source.url}
                target="_blank"
                rel="noreferrer"
                className="block rounded-xl border bg-background px-3 py-2 text-sm transition-colors hover:bg-muted/30"
              >
                <div className="font-medium">{source.title || source.url}</div>
                <div className="mt-1 text-xs text-muted-foreground line-clamp-2">{source.snippet}</div>
              </a>
            ))}
          </div>
        </div>
      ) : null}

      <ToolCallsPanel toolCalls={message.tool_calls} />
    </div>
  );
}

export function AiWorkspace({
  isVisible = false,
  mode = 'full',
  onNavigateBack,
}: {
  isVisible?: boolean;
  mode?: AiWorkspaceMode;
  onNavigateBack?: () => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const initialSection: WorkspaceSection = mode === 'automations' ? 'automations' : mode === 'models' ? 'models' : mode === 'assistants' ? 'assistants' : 'conversations';
  const [section, setSection] = useState<WorkspaceSection>(initialSection);
  const [loading, setLoading] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [savingSettings, setSavingSettings] = useState(false);
  const [sending, setSending] = useState(false);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [workspaceMessage, setWorkspaceMessage] = useState<string | null>(null);

  const [settings, setSettings] = useState<AiWorkspaceSettings | null>(null);
  const [assistants, setAssistants] = useState<AssistantPreset[]>([]);
  const [conversations, setConversations] = useState<AssistantConversationListItem[]>([]);
  const [automations, setAutomations] = useState<AutomationJobView[]>([]);
  const [quickPreferences, setQuickPreferences] = useState<QuickAssistantPreferences | null>(null);
  const [mcpCatalog, setMcpCatalog] = useState<ManagedMcpCatalogResponse | null>(null);
  const [refreshingMcpPreview, setRefreshingMcpPreview] = useState(false);

  const [conversationAssistantId, setConversationAssistantId] = useState<string | null>(null);
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const [selectedConversation, setSelectedConversation] = useState<AssistantConversation | null>(null);
  const [selectedAssistantEditorId, setSelectedAssistantEditorId] = useState<string | null>(null);
  const [assistantDraft, setAssistantDraft] = useState<AssistantPreset | null>(null);
  const [selectedAutomationId, setSelectedAutomationId] = useState<string | null>(null);
  const [automationDraft, setAutomationDraft] = useState<AutomationJob | null>(null);
  const [showConversationSidePanel, setShowConversationSidePanel] = useState(true);

  const [conversationSearch, setConversationSearch] = useState('');
  const [assistantSearch, setAssistantSearch] = useState('');
  const [automationSearch, setAutomationSearch] = useState('');
  const [draftMessage, setDraftMessage] = useState('');
  const [assistantTestPrompt, setAssistantTestPrompt] = useState(
    'Summarize the current release risks and next actions.',
  );
  const mcpServerNameById = useMemo(
    () => new Map((mcpCatalog?.items || []).map((item) => [item.server_id, item.name])),
    [mcpCatalog],
  );
  const selectedConversationMcpLabels = useMemo(
    () =>
      mapMcpServerIdsToLabels(
        selectedConversation?.capability_snapshot?.mcp_server_ids || [],
        mcpServerNameById,
      ),
    [mcpServerNameById, selectedConversation?.capability_snapshot?.mcp_server_ids],
  );

  const loadBootstrap = async () => {
    setLoading(true);
    try {
      const [data, catalog]: [AiWorkspaceBootstrap, ManagedMcpCatalogResponse] = await Promise.all([
        aiWorkspaceBootstrap(),
        workspaceAssistantMcpCatalog(),
      ]);
      setSettings(data.settings);
      setAssistants(data.assistants);
      setConversations(data.conversations);
      setAutomations(data.automations);
      setQuickPreferences(data.quick_assistant);
      setMcpCatalog(catalog);

      setConversationAssistantId((current) => {
        if (current && data.assistants.some((assistant) => assistant.id === current)) {
          return current;
        }
        const pendingConversationId = window.localStorage.getItem(PENDING_CONVERSATION_KEY);
        if (pendingConversationId) {
          const pendingConversation = data.conversations.find((item) => item.id === pendingConversationId);
          if (pendingConversation?.assistant_id) {
            window.localStorage.removeItem(PENDING_CONVERSATION_KEY);
            return pendingConversation.assistant_id;
          }
        }
        return data.assistants[0]?.id || null;
      });
      setSelectedAssistantEditorId((current) => current || data.assistants[0]?.id || null);
      setSelectedAutomationId((current) => current || data.automations[0]?.job.id || null);
      setSelectedConversationId((current) => {
        if (current && data.conversations.some((conversation) => conversation.id === current)) {
          return current;
        }
        const pendingConversationId = window.localStorage.getItem(PENDING_CONVERSATION_KEY);
        if (pendingConversationId && data.conversations.some((conversation) => conversation.id === pendingConversationId)) {
          window.localStorage.removeItem(PENDING_CONVERSATION_KEY);
          return pendingConversationId;
        }
        return data.conversations[0]?.id || null;
      });
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    } finally {
      setLoading(false);
    }
  };

  const loadConversationDetail = async (conversationId: string) => {
    setDetailLoading(true);
    try {
      const detail = await workspaceConversationGet(conversationId);
      setSelectedConversation(detail);
      setConversationAssistantId(detail.assistant_id || conversationAssistantId);
      setRuntimeError(null);
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    } finally {
      setDetailLoading(false);
    }
  };

  const refreshConversationList = async () => {
    const items = await workspaceConversationsList();
    setConversations(items);
  };

  const refreshAssistants = async () => {
    const items = await workspaceAssistantsList();
    setAssistants(items);
  };

  const refreshAutomations = async () => {
    const items = await workspaceAutomationsList();
    setAutomations(items);
  };

  const createNewAssistantDraft = () => {
    if (!settings) return;
    const created = createAssistantDraft(settings, mcpCatalog?.default_server_ids || []);
    setAssistants((current) => [created, ...current]);
    setSelectedAssistantEditorId(created.id);
    setAssistantDraft(created);
  };

  const handleRefreshMcpToolPreview = async (serverIds?: string[]) => {
    setRefreshingMcpPreview(true);
    try {
      const refreshed = await mcpToolPreviewRefresh(serverIds);
      setMcpCatalog((current) => mergeMcpCatalogItems(current, refreshed));
      setRuntimeError(null);
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    } finally {
      setRefreshingMcpPreview(false);
    }
  };

  const loadConversationDetailEffect = useEffectEvent((conversationId: string) => {
    void loadConversationDetail(conversationId);
  });

  useEffect(() => {
    if (!isVisible) return;
    void loadBootstrap();
  }, [isVisible]);

  useEffect(() => {
    if (!isVisible || !selectedConversationId) return;
    loadConversationDetailEffect(selectedConversationId);
  }, [isVisible, selectedConversationId]);

  useEffect(() => {
    const selected =
      assistants.find((assistant) => assistant.id === selectedAssistantEditorId) || null;
    setAssistantDraft(
      selected
        ? {
            ...selected,
            tool_policy: { ...selected.tool_policy },
            knowledge_base_ids: [...selected.knowledge_base_ids],
            mcp_server_ids: [...selected.mcp_server_ids],
          }
        : null,
    );
  }, [assistants, selectedAssistantEditorId]);

  useEffect(() => {
    const selected = automations.find((item) => item.job.id === selectedAutomationId)?.job || null;
    setAutomationDraft(
      selected
        ? {
            ...selected,
            trigger: {
              ...selected.trigger,
              weekdays: [...selected.trigger.weekdays],
            },
          }
        : null,
    );
  }, [automations, selectedAutomationId]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<AssistantStreamEvent>('assistant-stream', (event) => {
      if (disposed || !event.payload) return;
      const payload = event.payload;

      setSelectedConversation((current) => {
        if (!current || current.id !== payload.conversation_id) {
          return current;
        }
        const messages = current.messages.map((message) => {
          if (message.id !== payload.message_id) return message;
          if (payload.kind === 'message.delta') {
            return { ...message, content: `${message.content}${payload.text || ''}`, status: 'streaming' };
          }
          if (payload.kind === 'reasoning.delta') {
            return { ...message, reasoning: `${message.reasoning || ''}${payload.text || ''}` };
          }
          if (payload.kind === 'sources') {
            return { ...message, sources: payload.sources || message.sources };
          }
          if (payload.kind === 'tool.started' && payload.tool) {
            return { ...message, tool_calls: upsertToolCall(message.tool_calls, payload.tool) };
          }
          if (payload.kind === 'tool.finished' && payload.tool) {
            return {
              ...message,
              tool_calls: upsertToolCall(message.tool_calls, payload.tool),
            };
          }
          if (payload.kind === 'message.completed') {
            return {
              ...message,
              status: 'done',
              sources: payload.sources || message.sources,
            };
          }
          if (payload.kind === 'message.failed') {
            return { ...message, status: 'failed' };
          }
          return message;
        });
        return {
          ...current,
          updated_at: Math.floor(Date.now() / 1000),
          messages,
        };
      });

      if (payload.kind === 'message.completed' || payload.kind === 'message.failed') {
        setSending(false);
        void refreshConversationList();
        if (payload.error) {
          setRuntimeError(payload.error);
        }
      }
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, []);

  const activeAssistantForConversation = useMemo(
    () => assistants.find((assistant) => assistant.id === conversationAssistantId) || null,
    [assistants, conversationAssistantId],
  );

  const filteredConversations = useMemo(() => {
    const query = conversationSearch.trim().toLowerCase();
    if (!query) return conversations;
    return conversations.filter((conversation) => {
      const haystack = `${conversation.title} ${conversation.preview} ${conversation.search_text}`.toLowerCase();
      return haystack.includes(query);
    });
  }, [conversationSearch, conversations]);

  const filteredAssistants = useMemo(() => {
    const query = assistantSearch.trim().toLowerCase();
    if (!query) return assistants;
    return assistants.filter((assistant) => {
      const haystack = `${assistant.name} ${assistant.description} ${assistant.system_prompt}`.toLowerCase();
      return haystack.includes(query);
    });
  }, [assistantSearch, assistants]);

  const filteredAutomations = useMemo(() => {
    const query = automationSearch.trim().toLowerCase();
    if (!query) return automations;
    return automations.filter((automation) => {
      const haystack = `${automation.job.name} ${automation.job.prompt} ${automation.job.last_status || ''}`.toLowerCase();
      return haystack.includes(query);
    });
  }, [automationSearch, automations]);

  const modelCatalog = settings?.model_catalog || [];
  const enabledCatalog = modelCatalog.filter((item) => item.enabled);
  const roleBindings = settings?.role_bindings || [];
  const quickRoleModelId =
    roleBindings.find((binding) => binding.role === quickPreferences?.preferred_role)?.model_id ||
    null;

  const sections = [
    {
      id: 'conversations' as const,
      title: 'Conversations',
      description: '助手导向的主题会话、消息流和能力侧栏',
      icon: MessageSquare,
    },
    {
      id: 'assistants' as const,
      title: 'Assistant Library',
      description: '统一管理提示词、模型、MCP、知识库与记忆策略',
      icon: Bot,
    },
    {
      id: 'automations' as const,
      title: 'Automations',
      description: '把助手预设绑定到后台任务和触发器',
      icon: Clock3,
    },
    {
      id: 'models' as const,
      title: 'Model Center',
      description: 'Provider 连接、模型目录与角色绑定矩阵',
      icon: Layers3,
    },
    {
      id: 'quick' as const,
      title: 'Quick Assistant',
      description: '浮窗助手、快捷模式与划词助手预留位',
      icon: Zap,
    },
  ];

  const handleCreateConversation = async () => {
    const created = await workspaceConversationCreate({
      title: activeAssistantForConversation ? `${activeAssistantForConversation.name} Topic` : undefined,
      assistant_id: conversationAssistantId || undefined,
      model_override_id: activeAssistantForConversation?.primary_model_id || undefined,
    });
    await refreshConversationList();
    setSelectedConversationId(created.id);
    setSection('conversations');
  };

  const handleSend = async () => {
    const content = draftMessage.trim();
    if (!content || sending || !settings) return;
    setSending(true);
    setRuntimeError(null);
    try {
      let targetId = selectedConversation?.id || selectedConversationId;
      if (!targetId) {
        const created = await workspaceConversationCreate({
          title: activeAssistantForConversation ? `${activeAssistantForConversation.name} Topic` : undefined,
          assistant_id: conversationAssistantId || undefined,
          model_override_id: activeAssistantForConversation?.primary_model_id || undefined,
        });
        targetId = created.id;
        setSelectedConversationId(created.id);
      }
      await workspaceConversationSend({
        conversation_id: targetId,
        content,
        assistant_id: conversationAssistantId || undefined,
        model_override_id:
          selectedConversation?.model_override_id ||
          activeAssistantForConversation?.primary_model_id ||
          roleBindings.find((binding) => binding.role === 'chat')?.model_id ||
          undefined,
        web_search_enabled:
          selectedConversation?.web_search_enabled ?? activeAssistantForConversation?.tool_policy.web_search ?? false,
      });
      setDraftMessage('');
      await loadConversationDetail(targetId);
      await refreshConversationList();
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
      setSending(false);
    }
  };

  const handleTogglePinned = async () => {
    if (!selectedConversation) return;
    const updated = await workspaceConversationUpdate({
      conversation_id: selectedConversation.id,
      pinned: !selectedConversation.pinned,
    });
    setSelectedConversation(updated);
    await refreshConversationList();
  };

  const handleToggleArchived = async () => {
    if (!selectedConversation) return;
    const updated = await workspaceConversationUpdate({
      conversation_id: selectedConversation.id,
      archived: !selectedConversation.archived,
    });
    setSelectedConversation(updated);
    await refreshConversationList();
  };

  const handleToggleWebSearch = async () => {
    if (!selectedConversation) return;
    const updated = await workspaceConversationUpdate({
      conversation_id: selectedConversation.id,
      web_search_enabled: !selectedConversation.web_search_enabled,
    });
    setSelectedConversation(updated);
    await refreshConversationList();
  };

  const handleDeleteConversation = async () => {
    if (!selectedConversation) return;
    const confirmed = await confirmDialog(
      t('assistantDeleteConversationConfirm', '删除当前对话后将无法恢复，是否继续？'),
      {
        title: t('assistantDeleteConversation', '删除对话'),
        okLabel: t('delete', 'Delete'),
      },
    );
    if (!confirmed) return;
    await workspaceConversationDelete(selectedConversation.id);
    setSelectedConversation(null);
    setSelectedConversationId(null);
    await refreshConversationList();
  };

  const handleResetContext = async () => {
    if (!selectedConversation) return;
    const confirmed = await confirmDialog(
      t(
        'assistantClearContextConfirm',
        '不删除已有历史消息，从下一条消息开始不再引用此前上下文。是否继续？',
      ),
      {
        title: t('assistantClearContext', '清空上下文'),
        okLabel: t('assistantClearContext', '清空上下文'),
      },
    );
    if (!confirmed) return;
    const updated = await workspaceConversationResetContext(selectedConversation.id);
    setSelectedConversation(updated);
    await refreshConversationList();
  };

  const handleSaveAssistant = async () => {
    if (!assistantDraft) return;
    const saved = await workspaceAssistantUpsert(normalizeAssistantDraft(assistantDraft));
    await refreshAssistants();
    setSelectedAssistantEditorId(saved.id);
    setWorkspaceMessage('Assistant saved.');
  };

  const handleDeleteAssistant = async () => {
    if (!assistantDraft?.id) return;
    const confirmed = await confirmDialog(
      'Delete this assistant preset? Existing topics and automations will lose the assistant binding.',
      {
        title: 'Delete Assistant',
        okLabel: 'Delete',
      },
    );
    if (!confirmed) return;
    await workspaceAssistantDelete(assistantDraft.id);
    await Promise.all([refreshAssistants(), refreshConversationsList(), refreshAutomations()]);
    setSelectedAssistantEditorId(null);
    setAssistantDraft(null);
  };

  const refreshConversationsList = async () => {
    const items = await workspaceConversationsList();
    setConversations(items);
  };

  const handleAssistantTestRun = async () => {
    if (!assistantDraft?.id) return;
    const result = await workspaceAssistantTestRun({
      agent_id: assistantDraft.id,
      prompt: assistantTestPrompt.trim() || 'Run a quick capability check.',
    });
    window.localStorage.setItem(PENDING_CONVERSATION_KEY, result.conversation_id);
    setSection('conversations');
    setSelectedConversationId(result.conversation_id);
    await refreshConversationsList();
  };

  const handleSaveAutomation = async () => {
    if (!automationDraft) return;
    const saved = await workspaceAutomationUpsert({
      ...automationDraft,
      assistant_id: automationDraft.assistant_id || null,
      agent_id: automationDraft.assistant_id || '',
    });
    await refreshAutomations();
    setSelectedAutomationId(saved.id);
    setWorkspaceMessage('Automation saved.');
  };

  const handleDeleteAutomation = async () => {
    if (!automationDraft?.id) return;
    const confirmed = await confirmDialog('Delete this automation and its recent run history?', {
      title: 'Delete Automation',
      okLabel: 'Delete',
    });
    if (!confirmed) return;
    await workspaceAutomationDelete(automationDraft.id);
    await refreshAutomations();
    setSelectedAutomationId(null);
    setAutomationDraft(null);
  };

  const handleToggleAutomation = async () => {
    if (!automationDraft?.id) return;
    const updated = await workspaceAutomationToggle({
      schedule_id: automationDraft.id,
      enabled: !automationDraft.enabled,
    });
    setAutomationDraft({
      ...updated,
      trigger: {
        ...updated.trigger,
        weekdays: [...updated.trigger.weekdays],
      },
    });
    await refreshAutomations();
  };

  const handleAutomationRunNow = async () => {
    if (!automationDraft?.id) return;
    await workspaceAutomationRunNow({ schedule_id: automationDraft.id });
    await refreshAutomations();
    setWorkspaceMessage('Automation run queued.');
  };

  const handleSaveModelCenter = async () => {
    if (!settings) return;
    setSavingSettings(true);
    try {
      const saved = await workspaceSettingsSave(settings);
      setSettings(saved);
      setWorkspaceMessage('Model center saved.');
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    } finally {
      setSavingSettings(false);
    }
  };

  const handleQuickPreferenceChange = async (next: QuickAssistantPreferences) => {
    setQuickPreferences(next);
    try {
      await workspaceQuickAssistantSave(next);
      setRuntimeError(null);
    } catch (error: unknown) {
      setRuntimeError(formatRuntimeError(error));
    }
  };

  useEffect(() => {
    if (!workspaceMessage) return;
    const timer = window.setTimeout(() => setWorkspaceMessage(null), 2500);
    return () => window.clearTimeout(timer);
  }, [workspaceMessage]);

  return (
    <div className="h-full">
      <div className={`grid h-full gap-6 ${mode === 'full' ? 'xl:grid-cols-[280px,minmax(0,1fr)]' : ''}`}>
        {mode === 'full' ? (
          <aside className="flex min-h-0 flex-col rounded-3xl border bg-card">
            <div className="border-b px-4 py-4">
              <div className="text-base font-semibold">AI Workspace</div>
              <div className="mt-1 text-xs text-muted-foreground">
                OneSpace 的统一 AI 工作台，连接模型中心、助手、主题、自动化和快捷入口。
              </div>
            </div>

            <div className="space-y-2 p-3">
              {sections.map((item) => {
                const Icon = item.icon;
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => setSection(item.id)}
                    className={`w-full rounded-2xl border px-4 py-3 text-left transition-colors ${
                      section === item.id ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'
                    }`}
                  >
                    <div className="flex items-start gap-3">
                      <div className={`rounded-xl p-2 ${section === item.id ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground'}`}>
                        <Icon className="h-4 w-4" />
                      </div>
                      <div className="min-w-0">
                        <div className="text-sm font-medium">{item.title}</div>
                        <div className="mt-1 text-xs text-muted-foreground">{item.description}</div>
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>

          <div className="min-h-0 flex-1 overflow-y-auto border-t p-3">
            {section === 'conversations' ? (
              <div className="space-y-4">
                <div className="rounded-2xl border bg-muted/10 p-3">
                  <div className="mb-3 flex items-center justify-between gap-2">
                    <div>
                      <div className="text-sm font-medium">Assistant Presets</div>
                      <div className="text-xs text-muted-foreground">先选助手，再创建主题</div>
                    </div>
                    <button
                      type="button"
                      onClick={() => void handleCreateConversation()}
                      className="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs hover:bg-muted"
                    >
                      <Plus className="h-3.5 w-3.5" />
                      New
                    </button>
                  </div>
                  <div className="space-y-2">
                    {assistants.map((assistant) => (
                      <button
                        key={assistant.id}
                        type="button"
                        onClick={() => setConversationAssistantId(assistant.id)}
                        className={`w-full rounded-xl border px-3 py-3 text-left ${
                          conversationAssistantId === assistant.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                        }`}
                      >
                        <div className="flex items-center gap-3">
                          <div className="rounded-full bg-primary/10 p-2 text-primary">
                            <Bot className="h-4 w-4" />
                          </div>
                          <div className="min-w-0">
                            <div className="truncate text-sm font-medium">{assistant.name}</div>
                            <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                              {assistant.description || assistant.output_contract || assistant.system_prompt}
                            </div>
                          </div>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>

                <div className="rounded-2xl border bg-muted/10 p-3">
                  <div className="mb-3 flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
                    <Search className="h-4 w-4 text-muted-foreground" />
                    <input
                      value={conversationSearch}
                      onChange={(event) => setConversationSearch(event.target.value)}
                      placeholder="Search topics..."
                      className="w-full bg-transparent text-sm outline-none"
                    />
                  </div>
                  <div className="space-y-2">
                    {filteredConversations.map((conversation) => (
                      <button
                        key={conversation.id}
                        type="button"
                        onClick={() => setSelectedConversationId(conversation.id)}
                        className={`w-full rounded-xl border px-3 py-3 text-left ${
                          selectedConversationId === conversation.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                        }`}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0">
                            <div className="truncate text-sm font-medium">{conversation.title}</div>
                            <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                              {conversation.preview || 'No messages yet'}
                            </div>
                          </div>
                          {conversation.pinned ? <Pin className="h-3.5 w-3.5 shrink-0 text-primary" /> : null}
                        </div>
                        <div className="mt-2 flex items-center justify-between text-[11px] text-muted-foreground">
                          <span>{formatTimestamp(conversation.updated_at)}</span>
                          <span>{conversation.message_count} msgs</span>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            ) : null}

            {section === 'assistants' ? (
              <div className="space-y-4">
                <div className="flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
                  <Search className="h-4 w-4 text-muted-foreground" />
                  <input
                    value={assistantSearch}
                    onChange={(event) => setAssistantSearch(event.target.value)}
                    placeholder="Search assistants..."
                    className="w-full bg-transparent text-sm outline-none"
                  />
                </div>
                <button
                  type="button"
                  onClick={createNewAssistantDraft}
                  className="inline-flex w-full items-center justify-center gap-2 rounded-xl border px-3 py-2.5 text-sm hover:bg-muted"
                >
                  <Plus className="h-4 w-4" />
                  New Assistant
                </button>
                <div className="space-y-2">
                  {filteredAssistants.map((assistant) => (
                    <button
                      key={assistant.id || assistant.name}
                      type="button"
                      onClick={() => setSelectedAssistantEditorId(assistant.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left ${
                        selectedAssistantEditorId === assistant.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                      }`}
                    >
                      <div className="flex items-start gap-3">
                        <div className="rounded-full bg-primary/10 p-2 text-primary">
                          <Bot className="h-4 w-4" />
                        </div>
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium">{assistant.name}</div>
                          <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                            {assistant.description || assistant.output_contract || assistant.system_prompt}
                          </div>
                        </div>
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            ) : null}

            {section === 'automations' ? (
              <div className="space-y-4">
                <div className="flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
                  <Search className="h-4 w-4 text-muted-foreground" />
                  <input
                    value={automationSearch}
                    onChange={(event) => setAutomationSearch(event.target.value)}
                    placeholder="Search automations..."
                    className="w-full bg-transparent text-sm outline-none"
                  />
                </div>
                <button
                  type="button"
                  onClick={() => {
                    if (!settings) return;
                    const created = createAutomationDraft(settings, conversationAssistantId || assistants[0]?.id || null);
                    setAutomations((current) => [{ job: created, recent_runs: [] }, ...current]);
                    setSelectedAutomationId(created.id);
                    setAutomationDraft(created);
                  }}
                  className="inline-flex w-full items-center justify-center gap-2 rounded-xl border px-3 py-2.5 text-sm hover:bg-muted"
                >
                  <Plus className="h-4 w-4" />
                  New Automation
                </button>
                <div className="space-y-2">
                  {filteredAutomations.map((automation) => (
                    <button
                      key={automation.job.id || automation.job.name}
                      type="button"
                      onClick={() => setSelectedAutomationId(automation.job.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left ${
                        selectedAutomationId === automation.job.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                      }`}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium">{automation.job.name}</div>
                          <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                            {automation.job.prompt || 'No prompt yet'}
                          </div>
                        </div>
                        <span
                          className={`rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${
                            automation.job.enabled ? 'border-primary/30 text-primary' : 'text-muted-foreground'
                          }`}
                        >
                          {automation.job.enabled ? 'ON' : 'OFF'}
                        </span>
                      </div>
                      <div className="mt-2 flex items-center justify-between text-[11px] text-muted-foreground">
                        <span>{formatTrigger(automation.job.trigger)}</span>
                        <span>{formatTimestamp(automation.job.next_run_at)}</span>
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            ) : null}

            {section === 'models' ? (
              <div className="rounded-2xl border bg-muted/10 p-4 text-sm text-muted-foreground">
                Provider 连接、模型目录与角色绑定会在右侧展开完整编辑面板。
              </div>
            ) : null}

            {section === 'quick' ? (
              <div className="space-y-4 rounded-2xl border bg-muted/10 p-4 text-sm">
                <div className="font-medium">Quick Assistant Summary</div>
                <div className="text-muted-foreground">
                  当前模式：{quickPreferences?.prefer_assistant_mode ? '助手模式' : '模型模式'}
                </div>
                <div className="text-muted-foreground">
                  当前助手：{assistants.find((assistant) => assistant.id === quickPreferences?.preferred_assistant_id)?.name || '未设置'}
                </div>
                <div className="text-muted-foreground">
                  当前角色模型：{quickRoleModelId || '未绑定'}
                </div>
              </div>
            ) : null}
          </div>
        </aside>
        ) : null}

        <main className={`min-h-0 ${mode !== 'full' ? 'flex h-full flex-col rounded-3xl border bg-card' : ''}`}>
          {/* 返回按钮 - 仅在非 full 模式下显示 */}
          {mode !== 'full' && onNavigateBack ? (
            <div className="border-b px-4 py-3">
              <button
                type="button"
                onClick={onNavigateBack}
                className="inline-flex items-center gap-2 rounded-xl border bg-background px-3 py-2 text-sm hover:bg-muted"
              >
                <ArrowLeft className="h-4 w-4" />
                {t('backToAssistant', '返回助手')}
              </button>
            </div>
          ) : null}

          {loading ? (
            <div className="flex h-full items-center justify-center rounded-3xl border bg-card">
              <div className="inline-flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Loading...
              </div>
            </div>
          ) : null}

          {/* mode='assistants' 时显示助手库列表 + 编辑界面 */}
          {!loading && mode === 'assistants' ? (
            <div className="grid h-full gap-6 xl:grid-cols-[280px,minmax(0,1fr)]">
              {/* 左侧助手列表 */}
              <aside className="flex min-h-0 flex-col rounded-3xl border bg-card">
                <div className="border-b px-4 py-4">
                  <div className="text-base font-semibold">{t('assistantLibrary', 'Assistant Library')}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    统一管理提示词、模型、MCP、知识库与记忆策略
                  </div>
                </div>
                <div className="space-y-2 p-3">
                  <div className="flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
                    <Search className="h-4 w-4 text-muted-foreground" />
                    <input
                      value={assistantSearch}
                      onChange={(event) => setAssistantSearch(event.target.value)}
                      placeholder="Search assistants..."
                      className="w-full bg-transparent text-sm outline-none"
                    />
                  </div>
                  <button
                    type="button"
                    onClick={createNewAssistantDraft}
                    className="inline-flex w-full items-center justify-center gap-2 rounded-xl border px-3 py-2.5 text-sm hover:bg-muted"
                  >
                    <Plus className="h-4 w-4" />
                    New Assistant
                  </button>
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto p-3">
                  <div className="space-y-2">
                    {filteredAssistants.map((assistant) => (
                      <button
                        key={assistant.id || assistant.name}
                        type="button"
                        onClick={() => {
                          setSelectedAssistantEditorId(assistant.id);
                          setAssistantDraft(assistant);
                        }}
                        className={`w-full rounded-xl border px-3 py-3 text-left ${
                          selectedAssistantEditorId === assistant.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                        }`}
                      >
                        <div className="flex items-start gap-3">
                          <div className="rounded-full bg-primary/10 p-2 text-primary">
                            {assistant.avatar_emoji ? (
                              <span className="text-sm">{assistant.avatar_emoji}</span>
                            ) : (
                              <Bot className="h-4 w-4" />
                            )}
                          </div>
                          <div className="min-w-0">
                            <div className="truncate text-sm font-medium">{assistant.name}</div>
                            <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                              {assistant.description || assistant.output_contract || assistant.system_prompt}
                            </div>
                          </div>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              </aside>

              {/* 右侧编辑界面 */}
              <div className="flex min-h-0 flex-col rounded-3xl border bg-card">
                <div className="border-b px-6 py-4">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <div className="text-lg font-semibold">{assistantDraft?.name || 'Select an assistant'}</div>
                      <div className="mt-1 text-sm text-muted-foreground">
                        助手预设统一承载名称、描述、提示词、主模型、轻模型、工具策略和能力绑定。
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      {workspaceMessage ? <span className="text-xs text-muted-foreground">{workspaceMessage}</span> : null}
                      <button
                        type="button"
                        onClick={() => void handleDeleteAssistant()}
                        disabled={!assistantDraft?.id}
                        className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                      >
                        <Trash2 className="h-4 w-4" />
                        Delete
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleAssistantTestRun()}
                        disabled={!assistantDraft?.id}
                        className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                      >
                        <Play className="h-4 w-4" />
                        Test Run
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleSaveAssistant()}
                        disabled={!assistantDraft}
                        className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                      >
                        <Save className="h-4 w-4" />
                        Save
                      </button>
                    </div>
                  </div>
                </div>

                <div className="min-h-0 overflow-y-auto px-6 py-5">
                  {assistantDraft ? (
                    <div className="space-y-6">
                      <div className="grid gap-4 md:grid-cols-3">
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Name</span>
                          <input
                            value={assistantDraft.name}
                            onChange={(event) => setAssistantDraft((current) => (current ? { ...current, name: event.target.value } : current))}
                            className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          />
                        </label>
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Avatar</span>
                          <input
                            value={assistantDraft.avatar_emoji || ''}
                            onChange={(event) => setAssistantDraft((current) => (current ? { ...current, avatar_emoji: event.target.value } : current))}
                            className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          />
                        </label>
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Description</span>
                          <input
                            value={assistantDraft.description}
                            onChange={(event) => setAssistantDraft((current) => (current ? { ...current, description: event.target.value } : current))}
                            className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          />
                        </label>
                      </div>

                      <div className="grid gap-4 md:grid-cols-2">
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Primary Model</span>
                          <select
                            value={assistantDraft.primary_model_id || ''}
                            onChange={(event) => setAssistantDraft((current) => (current ? { ...current, primary_model_id: event.target.value || null } : current))}
                            className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          >
                            <option value="">Follow role binding</option>
                            {enabledCatalog.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.label}
                              </option>
                            ))}
                          </select>
                        </label>
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Light Model</span>
                          <select
                            value={assistantDraft.light_model_id || ''}
                            onChange={(event) => setAssistantDraft((current) => (current ? { ...current, light_model_id: event.target.value || null } : current))}
                            className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          >
                            <option value="">Follow role binding</option>
                            {enabledCatalog.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.label}
                              </option>
                            ))}
                          </select>
                        </label>
                      </div>

                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">System Prompt</span>
                        <textarea
                          value={assistantDraft.system_prompt}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, system_prompt: event.target.value } : current))}
                          className="min-h-[180px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                        />
                      </label>

                      <div className="grid gap-4 md:grid-cols-2">
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Knowledge Bases</span>
                          <input
                            value={assistantDraft.knowledge_base_ids.join(', ')}
                            onChange={(event) =>
                              setAssistantDraft((current) =>
                                current
                                  ? { ...current, knowledge_base_ids: parseCommaSeparated(event.target.value) }
                                  : current,
                              )
                            }
                            placeholder="kb-release, kb-product"
                            className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                          />
                        </label>
                        <div className="space-y-2">
                          <McpServerSelector
                            catalog={mcpCatalog}
                            selectedIds={assistantDraft.mcp_server_ids}
                            onChange={(mcpServerIds) =>
                              setAssistantDraft((current) =>
                                current ? { ...current, mcp_server_ids: mcpServerIds } : current,
                              )
                            }
                            onRefresh={handleRefreshMcpToolPreview}
                            refreshing={refreshingMcpPreview}
                          />
                        </div>
                      </div>

                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Output Contract</span>
                        <textarea
                          value={assistantDraft.output_contract}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, output_contract: event.target.value } : current))}
                          className="min-h-[120px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                        />
                      </label>

                      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                        <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                          <div>
                            <div className="text-sm font-medium">{t('networkRetrieval', 'Network Retrieval')}</div>
                            <div className="text-xs text-muted-foreground">
                              {t(
                                'networkRetrievalMcpDesc',
                                'Allow search-class MCP tools to access current network information.',
                              )}
                            </div>
                          </div>
                          <input
                            type="checkbox"
                            checked={assistantDraft.tool_policy.web_search}
                            onChange={(event) =>
                              setAssistantDraft((current) =>
                                current
                                  ? {
                                      ...current,
                                      tool_policy: { ...current.tool_policy, web_search: event.target.checked },
                                    }
                                  : current,
                              )
                            }
                            className="h-4 w-4"
                          />
                        </label>
                        <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                          <div>
                            <div className="text-sm font-medium">Workspace Read</div>
                            <div className="text-xs text-muted-foreground">允许工作区读取能力</div>
                          </div>
                          <input
                            type="checkbox"
                            checked={assistantDraft.tool_policy.workspace_read}
                            onChange={(event) =>
                              setAssistantDraft((current) =>
                                current
                                  ? {
                                      ...current,
                                      tool_policy: { ...current.tool_policy, workspace_read: event.target.checked },
                                    }
                                  : current,
                              )
                            }
                            className="h-4 w-4"
                          />
                        </label>
                        <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                          <div>
                            <div className="text-sm font-medium">Notes Search</div>
                            <div className="text-xs text-muted-foreground">允许笔记检索能力</div>
                          </div>
                          <input
                            type="checkbox"
                            checked={assistantDraft.tool_policy.notes_search}
                            onChange={(event) =>
                              setAssistantDraft((current) =>
                                current
                                  ? {
                                      ...current,
                                      tool_policy: { ...current.tool_policy, notes_search: event.target.checked },
                                    }
                                  : current,
                              )
                            }
                            className="h-4 w-4"
                          />
                        </label>
                        <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                          <div>
                            <div className="text-sm font-medium">Memory</div>
                            <div className="text-xs text-muted-foreground">启用长期记忆</div>
                          </div>
                          <input
                            type="checkbox"
                            checked={assistantDraft.memory_enabled}
                            onChange={(event) =>
                              setAssistantDraft((current) =>
                                current ? { ...current, memory_enabled: event.target.checked } : current,
                              )
                            }
                            className="h-4 w-4"
                          />
                        </label>
                      </div>

                      <div className="rounded-2xl border bg-muted/10 p-4">
                        <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                          Test Prompt
                        </div>
                        <textarea
                          value={assistantTestPrompt}
                          onChange={(event) => setAssistantTestPrompt(event.target.value)}
                          className="min-h-[110px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                        />
                      </div>
                    </div>
                  ) : (
                    <div className="flex h-full items-center justify-center px-6">
                      <div className="rounded-3xl border border-dashed bg-muted/10 px-8 py-10 text-center text-sm text-muted-foreground">
                        从左侧选择一个 Assistant Preset，或者创建一个新的助手预设。
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
          ) : null}

          {/* mode='automations' 时显示自动化列表 + 编辑界面 */}
          {!loading && mode === 'automations' ? (
            <div className="grid h-full gap-6 xl:grid-cols-[280px,minmax(0,1fr)]">
              {/* 左侧自动化列表 */}
              <aside className="flex min-h-0 flex-col rounded-3xl border bg-card">
                <div className="border-b px-4 py-4">
                  <div className="text-base font-semibold">{t('automations', 'Automations')}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    定时任务与后台自动化编排
                  </div>
                </div>
                <div className="space-y-2 p-3">
                  <div className="flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
                    <Search className="h-4 w-4 text-muted-foreground" />
                    <input
                      value={automationSearch}
                      onChange={(event) => setAutomationSearch(event.target.value)}
                      placeholder="Search automations..."
                      className="w-full bg-transparent text-sm outline-none"
                    />
                  </div>
                  <button
                    type="button"
                    onClick={() => {
                      if (!settings) return;
                      const created = createAutomationDraft(settings, conversationAssistantId || assistants[0]?.id || null);
                      setAutomations((current) => [{ job: created, recent_runs: [] }, ...current]);
                      setSelectedAutomationId(created.id);
                      setAutomationDraft(created);
                    }}
                    className="inline-flex w-full items-center justify-center gap-2 rounded-xl border px-3 py-2.5 text-sm hover:bg-muted"
                  >
                    <Plus className="h-4 w-4" />
                    New Automation
                  </button>
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto p-3">
                  <div className="space-y-2">
                    {filteredAutomations.map((automation) => (
                      <button
                        key={automation.job.id || automation.job.name}
                        type="button"
                        onClick={() => setSelectedAutomationId(automation.job.id)}
                        className={`w-full rounded-xl border px-3 py-3 text-left ${
                          selectedAutomationId === automation.job.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                        }`}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0">
                            <div className="truncate text-sm font-medium">{automation.job.name}</div>
                            <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                              {automation.job.prompt || 'No prompt yet'}
                            </div>
                          </div>
                          <span
                            className={`rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${
                              automation.job.enabled ? 'border-primary/30 text-primary' : 'text-muted-foreground'
                            }`}
                          >
                            {automation.job.enabled ? 'ON' : 'OFF'}
                          </span>
                        </div>
                        <div className="mt-2 flex items-center justify-between text-[11px] text-muted-foreground">
                          <span>{formatTrigger(automation.job.trigger)}</span>
                          <span>{formatTimestamp(automation.job.next_run_at)}</span>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              </aside>

              {/* 右侧编辑界面 */}
              <div className="flex min-h-0 flex-col rounded-3xl border bg-card">
                <div className="border-b px-6 py-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-lg font-semibold">{automationDraft?.name || 'Select an automation'}</div>
                    <div className="mt-1 text-sm text-muted-foreground">
                      自动化继承 Assistant Preset 的默认能力，可在任务层覆盖 Prompt、联网开关和运行模型。
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {workspaceMessage ? <span className="text-xs text-muted-foreground">{workspaceMessage}</span> : null}
                    <button
                      type="button"
                      onClick={() => void handleToggleAutomation()}
                      disabled={!automationDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Radar className="h-4 w-4" />
                      {automationDraft?.enabled ? 'Pause' : 'Enable'}
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleAutomationRunNow()}
                      disabled={!automationDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Play className="h-4 w-4" />
                      Run now
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleDeleteAutomation()}
                      disabled={!automationDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                    >
                      <Trash2 className="h-4 w-4" />
                      Delete
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleSaveAutomation()}
                      disabled={!automationDraft}
                      className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                    >
                      <Save className="h-4 w-4" />
                      Save
                    </button>
                  </div>
                </div>
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                {automationDraft ? (
                  <div className="space-y-6">
                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Name</span>
                        <input
                          value={automationDraft.name}
                          onChange={(event) => setAutomationDraft((current) => (current ? { ...current, name: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Assistant Preset</span>
                        <select
                          value={automationDraft.assistant_id || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    assistant_id: event.target.value || null,
                                    agent_id: event.target.value || '',
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Select assistant</option>
                          {assistants.map((assistant) => (
                            <option key={assistant.id} value={assistant.id}>
                              {assistant.name}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>

                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Prompt</span>
                      <textarea
                        value={automationDraft.prompt}
                        onChange={(event) => setAutomationDraft((current) => (current ? { ...current, prompt: event.target.value } : current))}
                        className="min-h-[140px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                      />
                    </label>

                    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Trigger</span>
                        <select
                          value={automationDraft.trigger.kind}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    trigger: {
                                      ...current.trigger,
                                      kind: event.target.value,
                                    },
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="daily">Daily</option>
                          <option value="weekly">Weekly</option>
                          <option value="interval">Interval</option>
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Time</span>
                        <input
                          value={automationDraft.trigger.time_of_day || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    trigger: { ...current.trigger, time_of_day: event.target.value },
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Interval Minutes</span>
                        <input
                          type="number"
                          value={automationDraft.trigger.interval_minutes || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    trigger: {
                                      ...current.trigger,
                                      interval_minutes: event.target.value ? Number(event.target.value) : null,
                                    },
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Model Override</span>
                        <select
                          value={automationDraft.model_override_id || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current ? { ...current, model_override_id: event.target.value || null } : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Follow automation role</option>
                          {enabledCatalog.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                        <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                          <div>
                            <div className="text-sm font-medium">{t('networkRetrieval', 'Network Retrieval')}</div>
                            <div className="text-xs text-muted-foreground">
                              {t(
                                'networkRetrievalAutomationDesc',
                                'Allow automation jobs to use search-class MCP tools.',
                              )}
                            </div>
                          </div>
                        <input
                          type="checkbox"
                          checked={automationDraft.web_search_enabled}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? { ...current, web_search_enabled: event.target.checked }
                                : current,
                            )
                          }
                          className="h-4 w-4"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Timezone</span>
                        <input
                          value={automationDraft.timezone || ''}
                          onChange={(event) => setAutomationDraft((current) => (current ? { ...current, timezone: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Output</span>
                        <input
                          value={automationDraft.output_target}
                          onChange={(event) => setAutomationDraft((current) => (current ? { ...current, output_target: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <div className="rounded-2xl border bg-muted/10 px-4 py-3">
                        <div className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Next Run</div>
                        <div className="mt-2 text-sm">{formatTimestamp(automationDraft.next_run_at)}</div>
                        <div className="mt-1 text-xs text-muted-foreground">{formatTrigger(automationDraft.trigger)}</div>
                      </div>
                    </div>

                    <div className="rounded-2xl border bg-muted/10 p-4">
                      <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Recent Runs
                      </div>
                      <div className="space-y-2">
                        {(automations.find((item) => item.job.id === automationDraft.id)?.recent_runs || []).map((run) => (
                          <div key={run.id} className="flex items-center justify-between rounded-xl border bg-background px-3 py-2 text-sm">
                            <div>
                              <div className="font-medium">{run.status}</div>
                              <div className="mt-1 text-xs text-muted-foreground">
                                {formatTimestamp(run.started_at)} {run.summary ? `· ${run.summary}` : ''}
                              </div>
                            </div>
                            <span className="text-xs text-muted-foreground">{run.conversation_id || '--'}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="flex h-full items-center justify-center px-6">
                    <div className="rounded-3xl border border-dashed bg-muted/10 px-8 py-10 text-center text-sm text-muted-foreground">
                      没有选择自动化任务，请先创建一个。
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        ) : null}

        {/* mode='models' 时只显示模型中心设置界面 */}
        {!loading && mode === 'models' && settings ? (
          <ModelCenter
            settings={settings}
            onChange={setSettings}
            onSave={handleSaveModelCenter}
          />
        ) : null}

          {!loading && mode === 'models' && !settings ? (
            <div className="flex h-full items-center justify-center rounded-3xl border bg-card">
              <div className="text-center text-sm text-muted-foreground">
                <Layers3 className="mx-auto h-8 w-8 opacity-50" />
                <div className="mt-2">Settings not loaded</div>
                <div className="mt-1 text-xs">Try reloading the AI Workspace</div>
              </div>
            </div>
          ) : null}

          {/* mode='full' 时根据 section 显示对应内容 */}
          {!loading && mode === 'full' && section === 'conversations' ? (
            <div className="grid h-full gap-6 xl:grid-cols-[minmax(0,1fr),320px]">
              <div className="flex min-h-0 flex-col rounded-3xl border bg-card">
                <div className="border-b px-6 py-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-base font-semibold">
                        {selectedConversation?.title || 'Select or create a topic'}
                      </div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        助手主题会默认继承 Assistant Preset 的模型与能力策略，也支持在主题内临时覆盖。
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => setShowConversationSidePanel((current) => !current)}
                      className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
                    >
                      {showConversationSidePanel ? <PanelRightClose className="h-4 w-4" /> : <PanelRightOpen className="h-4 w-4" />}
                      {showConversationSidePanel ? 'Collapse' : 'Expand'}
                    </button>
                  </div>
                </div>

                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                  {selectedConversation ? (
                    detailLoading ? (
                      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                        Loading topic...
                      </div>
                    ) : selectedConversation.messages.length ? (
                      <div className="space-y-4">
                        {selectedConversation.messages.map((message) => (
                          <MessageCard key={message.id} message={message} />
                        ))}
                      </div>
                    ) : (
                      <div className="rounded-3xl border border-dashed bg-muted/10 px-6 py-10 text-center text-sm text-muted-foreground">
                        从一个清晰问题开始，或者先在右侧调整助手、模型和联网能力。
                      </div>
                    )
                  ) : (
                    <div className="flex h-full items-center justify-center">
                      <div className="max-w-md rounded-3xl border border-dashed bg-muted/10 px-6 py-10 text-center">
                        <div className="mx-auto mb-4 inline-flex rounded-full bg-primary/10 p-3 text-primary">
                          <MessageSquare className="h-6 w-6" />
                        </div>
                        <div className="text-base font-semibold">Start from an assistant preset</div>
                        <p className="mt-2 text-sm text-muted-foreground">
                          左侧先选一个助手，再创建主题，就能把提示词、模型和工具策略一起带入会话。
                        </p>
                        <button
                          type="button"
                          onClick={() => void handleCreateConversation()}
                          className="mt-4 inline-flex items-center gap-2 rounded-lg border px-4 py-2 text-sm hover:bg-muted"
                        >
                          <Plus className="h-4 w-4" />
                          New Topic
                        </button>
                      </div>
                    </div>
                  )}
                </div>

                <div className="border-t px-6 py-4">
                  {runtimeError ? (
                    <div className="mb-3 rounded-xl border border-destructive/20 bg-destructive/5 px-4 py-3 text-sm text-destructive">
                      {runtimeError}
                    </div>
                  ) : null}
                  <div className="rounded-3xl border bg-background p-3">
                    <textarea
                      value={draftMessage}
                      onChange={(event) => setDraftMessage(event.target.value)}
                      placeholder="Ask the assistant to plan, summarize, search, translate, or orchestrate tools..."
                      className="min-h-[108px] w-full resize-none bg-transparent text-sm leading-6 outline-none"
                    />
                    <div className="mt-3 flex items-center justify-between gap-3">
                      <div className="text-xs text-muted-foreground">
                        当前助手：{activeAssistantForConversation?.name || '未指定'} · 当前模型：
                        {selectedConversation?.model_override_id ||
                          activeAssistantForConversation?.primary_model_id ||
                          roleBindings.find((binding) => binding.role === 'chat')?.model_id ||
                          '未绑定'}
                      </div>
                      <button
                        type="button"
                        onClick={() => void handleSend()}
                        disabled={!draftMessage.trim() || sending}
                        className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                      >
                        {sending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
                        {t('send', 'Send')}
                      </button>
                    </div>
                  </div>
                </div>
              </div>

              {showConversationSidePanel ? (
                <div className="min-h-0 rounded-3xl border bg-card">
                  <div className="border-b px-5 py-4">
                    <div className="text-sm font-semibold">Capability Panel</div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      模型覆盖、能力快照、联网开关和主题控制统一收敛到这里。
                    </div>
                  </div>
                  <div className="space-y-5 p-5">
                    <div>
                      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Assistant Preset
                      </div>
                      <select
                        value={conversationAssistantId || ''}
                        onChange={async (event) => {
                          const nextAssistantId = event.target.value || null;
                          setConversationAssistantId(nextAssistantId);
                          if (selectedConversation) {
                            const updated = await workspaceConversationUpdate({
                              conversation_id: selectedConversation.id,
                              assistant_id: nextAssistantId || '',
                              model_override_id:
                                assistants.find((assistant) => assistant.id === nextAssistantId)?.primary_model_id || '',
                              web_search_enabled:
                                assistants.find((assistant) => assistant.id === nextAssistantId)?.tool_policy.web_search || false,
                            });
                            setSelectedConversation(updated);
                            await refreshConversationList();
                          }
                        }}
                        className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                      >
                        <option value="">No preset</option>
                        {assistants.map((assistant) => (
                          <option key={assistant.id} value={assistant.id}>
                            {assistant.name}
                          </option>
                        ))}
                      </select>
                    </div>

                    <div>
                      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Model Override
                      </div>
                      <select
                        value={selectedConversation?.model_override_id || ''}
                        onChange={async (event) => {
                          if (!selectedConversation) return;
                          const updated = await workspaceConversationUpdate({
                            conversation_id: selectedConversation.id,
                            model_override_id: event.target.value || '',
                          });
                          setSelectedConversation(updated);
                          await refreshConversationList();
                        }}
                        className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                      >
                        <option value="">Follow preset / role</option>
                        {enabledCatalog.map((item) => (
                          <option key={item.id} value={item.id}>
                            {item.label}
                          </option>
                        ))}
                      </select>
                    </div>

                    <div>
                      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Topic Controls
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          onClick={() => void handleToggleWebSearch()}
                          disabled={!selectedConversation}
                          className={`inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm ${
                            selectedConversation?.web_search_enabled ? 'border-primary bg-primary/5 text-primary' : 'hover:bg-muted'
                          } disabled:opacity-50`}
                        >
                          <Globe className="h-4 w-4" />
                          Web
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleResetContext()}
                          disabled={!selectedConversation}
                          className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                        >
                          <XCircle className="h-4 w-4" />
                          Reset
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleTogglePinned()}
                          disabled={!selectedConversation}
                          className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                        >
                          {selectedConversation?.pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
                          {selectedConversation?.pinned ? 'Unpin' : 'Pin'}
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleToggleArchived()}
                          disabled={!selectedConversation}
                          className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                        >
                          <Archive className="h-4 w-4" />
                          {selectedConversation?.archived ? 'Restore' : 'Archive'}
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleDeleteConversation()}
                          disabled={!selectedConversation}
                          className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                        >
                          <Trash2 className="h-4 w-4" />
                          Delete
                        </button>
                      </div>
                    </div>

                    <div>
                      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Capability Snapshot
                      </div>
                      <div className="space-y-3 rounded-2xl border bg-muted/10 p-4 text-sm">
                        <div className="flex flex-wrap gap-2">
                          {capabilityBadge(
                            selectedConversation?.capability_snapshot?.web_search ? 'web-search' : 'no-web',
                          )}
                          {capabilityBadge(
                            selectedConversation?.capability_snapshot?.workspace_read ? 'workspace-read' : 'no-workspace',
                          )}
                          {capabilityBadge(
                            selectedConversation?.capability_snapshot?.notes_search ? 'notes-search' : 'no-notes',
                          )}
                          {capabilityBadge(
                            selectedConversation?.capability_snapshot?.memory_enabled ? 'memory' : 'no-memory',
                          )}
                        </div>
                        <div>
                          <div className="text-xs font-medium text-muted-foreground">Knowledge Bases</div>
                          <div className="mt-1 text-xs">
                            {selectedConversation?.capability_snapshot?.knowledge_base_ids?.length
                              ? selectedConversation.capability_snapshot.knowledge_base_ids.join(', ')
                              : 'None'}
                          </div>
                        </div>
                        <div>
                          <div className="text-xs font-medium text-muted-foreground">MCP Servers</div>
                          <div className="mt-1 text-xs">
                            {selectedConversationMcpLabels.length
                              ? selectedConversationMcpLabels.join(', ')
                              : 'None'}
                          </div>
                        </div>
                      </div>
                    </div>

                    <div>
                      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Topic Stats
                      </div>
                      <div className="space-y-2 rounded-2xl border bg-muted/10 p-4 text-sm">
                        <div className="flex items-center justify-between">
                          <span className="text-muted-foreground">Messages</span>
                          <span>{selectedConversation?.messages.filter((item) => item.role !== 'context_reset').length || 0}</span>
                        </div>
                        <div className="flex items-center justify-between">
                          <span className="text-muted-foreground">Context resets</span>
                          <span>{selectedConversation?.context_reset_count || 0}</span>
                        </div>
                        <div className="flex items-center justify-between">
                          <span className="text-muted-foreground">Updated</span>
                          <span>{formatTimestamp(selectedConversation?.updated_at)}</span>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}

          {!loading && mode === 'full' && section === 'assistants' ? (
            <div className="flex h-full min-h-0 flex-col rounded-3xl border bg-card">
              <div className="border-b px-6 py-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-lg font-semibold">{assistantDraft?.name || 'Select an assistant'}</div>
                    <div className="mt-1 text-sm text-muted-foreground">
                      助手预设统一承载名称、描述、提示词、主模型、轻模型、工具策略和能力绑定。
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {workspaceMessage ? <span className="text-xs text-muted-foreground">{workspaceMessage}</span> : null}
                    <button
                      type="button"
                      onClick={() => void handleDeleteAssistant()}
                      disabled={!assistantDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                    >
                      <Trash2 className="h-4 w-4" />
                      Delete
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleAssistantTestRun()}
                      disabled={!assistantDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Play className="h-4 w-4" />
                      Test Run
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleSaveAssistant()}
                      disabled={!assistantDraft}
                      className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                    >
                      <Save className="h-4 w-4" />
                      Save
                    </button>
                  </div>
                </div>
              </div>

              <div className="min-h-0 overflow-y-auto px-6 py-5">
                {assistantDraft ? (
                  <div className="space-y-6">
                    <div className="grid gap-4 md:grid-cols-3">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Name</span>
                        <input
                          value={assistantDraft.name}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, name: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Avatar</span>
                        <input
                          value={assistantDraft.avatar_emoji || ''}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, avatar_emoji: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Description</span>
                        <input
                          value={assistantDraft.description}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, description: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Primary Model</span>
                        <select
                          value={assistantDraft.primary_model_id || ''}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, primary_model_id: event.target.value || null } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Follow role binding</option>
                          {enabledCatalog.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Light Model</span>
                        <select
                          value={assistantDraft.light_model_id || ''}
                          onChange={(event) => setAssistantDraft((current) => (current ? { ...current, light_model_id: event.target.value || null } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Follow role binding</option>
                          {enabledCatalog.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>

                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">System Prompt</span>
                      <textarea
                        value={assistantDraft.system_prompt}
                        onChange={(event) => setAssistantDraft((current) => (current ? { ...current, system_prompt: event.target.value } : current))}
                        className="min-h-[180px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                      />
                    </label>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Knowledge Bases</span>
                        <input
                          value={assistantDraft.knowledge_base_ids.join(', ')}
                          onChange={(event) =>
                            setAssistantDraft((current) =>
                              current
                                ? { ...current, knowledge_base_ids: parseCommaSeparated(event.target.value) }
                                : current,
                            )
                          }
                          placeholder="kb-release, kb-product"
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <div className="space-y-2">
                        <McpServerSelector
                          catalog={mcpCatalog}
                          selectedIds={assistantDraft.mcp_server_ids}
                          onChange={(mcpServerIds) =>
                            setAssistantDraft((current) =>
                              current ? { ...current, mcp_server_ids: mcpServerIds } : current,
                            )
                          }
                          onRefresh={handleRefreshMcpToolPreview}
                          refreshing={refreshingMcpPreview}
                        />
                      </div>
                    </div>

                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Output Contract</span>
                      <textarea
                        value={assistantDraft.output_contract}
                        onChange={(event) => setAssistantDraft((current) => (current ? { ...current, output_contract: event.target.value } : current))}
                        className="min-h-[120px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                      />
                    </label>

                    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('networkRetrieval', 'Network Retrieval')}</div>
                          <div className="text-xs text-muted-foreground">
                            {t(
                              'networkRetrievalMcpDesc',
                              'Allow search-class MCP tools to access current network information.',
                            )}
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          checked={assistantDraft.tool_policy.web_search}
                          onChange={(event) =>
                            setAssistantDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    tool_policy: { ...current.tool_policy, web_search: event.target.checked },
                                  }
                                : current,
                            )
                          }
                          className="h-4 w-4"
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">Workspace Read</div>
                          <div className="text-xs text-muted-foreground">允许工作区读取能力</div>
                        </div>
                        <input
                          type="checkbox"
                          checked={assistantDraft.tool_policy.workspace_read}
                          onChange={(event) =>
                            setAssistantDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    tool_policy: { ...current.tool_policy, workspace_read: event.target.checked },
                                  }
                                : current,
                            )
                          }
                          className="h-4 w-4"
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">Notes Search</div>
                          <div className="text-xs text-muted-foreground">允许笔记检索</div>
                        </div>
                        <input
                          type="checkbox"
                          checked={assistantDraft.tool_policy.notes_search}
                          onChange={(event) =>
                            setAssistantDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    tool_policy: { ...current.tool_policy, notes_search: event.target.checked },
                                  }
                                : current,
                            )
                          }
                          className="h-4 w-4"
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">Memory</div>
                          <div className="text-xs text-muted-foreground">启用长期记忆</div>
                        </div>
                        <input
                          type="checkbox"
                          checked={assistantDraft.memory_enabled}
                          onChange={(event) =>
                            setAssistantDraft((current) =>
                              current ? { ...current, memory_enabled: event.target.checked } : current,
                            )
                          }
                          className="h-4 w-4"
                        />
                      </label>
                    </div>

                    <div className="rounded-2xl border bg-muted/10 p-4">
                      <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Test Prompt
                      </div>
                      <textarea
                        value={assistantTestPrompt}
                        onChange={(event) => setAssistantTestPrompt(event.target.value)}
                        className="min-h-[110px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                      />
                    </div>
                  </div>
                ) : (
                  <div className="flex h-full items-center justify-center px-6">
                    <div className="rounded-3xl border border-dashed bg-muted/10 px-8 py-10 text-center text-sm text-muted-foreground">
                      从左侧选择一个 Assistant Preset，或者创建一个新的助手预设。
                    </div>
                  </div>
                )}
              </div>
            </div>
          ) : null}

          {!loading && mode === 'full' && section === 'automations' ? (
            <div className="flex h-full min-h-0 flex-col rounded-3xl border bg-card">
              <div className="border-b px-6 py-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-lg font-semibold">{automationDraft?.name || 'Select an automation'}</div>
                    <div className="mt-1 text-sm text-muted-foreground">
                      自动化继承 Assistant Preset 的默认能力，可在任务层覆盖 Prompt、联网开关和运行模型。
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {workspaceMessage ? <span className="text-xs text-muted-foreground">{workspaceMessage}</span> : null}
                    <button
                      type="button"
                      onClick={() => void handleToggleAutomation()}
                      disabled={!automationDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Radar className="h-4 w-4" />
                      {automationDraft?.enabled ? 'Pause' : 'Enable'}
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleAutomationRunNow()}
                      disabled={!automationDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Play className="h-4 w-4" />
                      Run now
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleDeleteAutomation()}
                      disabled={!automationDraft?.id}
                      className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                    >
                      <Trash2 className="h-4 w-4" />
                      Delete
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleSaveAutomation()}
                      disabled={!automationDraft}
                      className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                    >
                      <Save className="h-4 w-4" />
                      Save
                    </button>
                  </div>
                </div>
              </div>

              <div className="min-h-0 overflow-y-auto px-6 py-5">
                {automationDraft ? (
                  <div className="space-y-6">
                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Name</span>
                        <input
                          value={automationDraft.name}
                          onChange={(event) => setAutomationDraft((current) => (current ? { ...current, name: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Assistant Preset</span>
                        <select
                          value={automationDraft.assistant_id || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    assistant_id: event.target.value || null,
                                    agent_id: event.target.value || '',
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Select assistant</option>
                          {assistants.map((assistant) => (
                            <option key={assistant.id} value={assistant.id}>
                              {assistant.name}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>

                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Prompt</span>
                      <textarea
                        value={automationDraft.prompt}
                        onChange={(event) => setAutomationDraft((current) => (current ? { ...current, prompt: event.target.value } : current))}
                        className="min-h-[140px] w-full rounded-xl border bg-background px-4 py-3 text-sm leading-6"
                      />
                    </label>

                    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Trigger</span>
                        <select
                          value={automationDraft.trigger.kind}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    trigger: {
                                      ...current.trigger,
                                      kind: event.target.value,
                                    },
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="daily">Daily</option>
                          <option value="weekly">Weekly</option>
                          <option value="interval">Interval</option>
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Time</span>
                        <input
                          value={automationDraft.trigger.time_of_day || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    trigger: { ...current.trigger, time_of_day: event.target.value },
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Interval Minutes</span>
                        <input
                          type="number"
                          value={automationDraft.trigger.interval_minutes || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? {
                                    ...current,
                                    trigger: {
                                      ...current.trigger,
                                      interval_minutes: event.target.value ? Number(event.target.value) : null,
                                    },
                                  }
                                : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Model Override</span>
                        <select
                          value={automationDraft.model_override_id || ''}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current ? { ...current, model_override_id: event.target.value || null } : current,
                            )
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Follow automation role</option>
                          {enabledCatalog.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('networkRetrieval', 'Network Retrieval')}</div>
                          <div className="text-xs text-muted-foreground">
                            {t(
                              'networkRetrievalAutomationDesc',
                              'Allow automation jobs to use search-class MCP tools.',
                            )}
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          checked={automationDraft.web_search_enabled}
                          onChange={(event) =>
                            setAutomationDraft((current) =>
                              current
                                ? { ...current, web_search_enabled: event.target.checked }
                                : current,
                            )
                          }
                          className="h-4 w-4"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Timezone</span>
                        <input
                          value={automationDraft.timezone || ''}
                          onChange={(event) => setAutomationDraft((current) => (current ? { ...current, timezone: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Output</span>
                        <input
                          value={automationDraft.output_target}
                          onChange={(event) => setAutomationDraft((current) => (current ? { ...current, output_target: event.target.value } : current))}
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <div className="rounded-2xl border bg-muted/10 px-4 py-3">
                        <div className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Next Run</div>
                        <div className="mt-2 text-sm">{formatTimestamp(automationDraft.next_run_at)}</div>
                        <div className="mt-1 text-xs text-muted-foreground">{formatTrigger(automationDraft.trigger)}</div>
                      </div>
                    </div>

                    <div className="rounded-2xl border bg-muted/10 p-4">
                      <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        Recent Runs
                      </div>
                      <div className="space-y-2">
                        {(automations.find((item) => item.job.id === automationDraft.id)?.recent_runs || []).map((run) => (
                          <div key={run.id} className="flex items-center justify-between rounded-xl border bg-background px-3 py-2 text-sm">
                            <div>
                              <div className="font-medium">{run.status}</div>
                              <div className="mt-1 text-xs text-muted-foreground">
                                {formatTimestamp(run.started_at)} {run.summary ? `· ${run.summary}` : ''}
                              </div>
                            </div>
                            <span className="text-xs text-muted-foreground">{run.conversation_id || '--'}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="flex h-full items-center justify-center px-6">
                    <div className="rounded-3xl border border-dashed bg-muted/10 px-8 py-10 text-center text-sm text-muted-foreground">
                      从左侧选择一个 Automation，或者创建一个新的后台任务。
                    </div>
                  </div>
                )}
              </div>
            </div>
          ) : null}

          {!loading && mode === 'full' && section === 'models' && settings ? (
            <div className="h-full overflow-y-auto rounded-3xl border bg-card">
              <AiConnectionsSettings
                value={settings}
                onChange={setSettings}
                onSave={handleSaveModelCenter}
                saving={savingSettings}
              />
            </div>
          ) : null}

          {!loading && mode === 'full' && section === 'models' && !settings ? (
            <div className="flex h-full items-center justify-center rounded-3xl border bg-card">
              <div className="text-center text-sm text-muted-foreground">
                <Layers3 className="mx-auto h-8 w-8 opacity-50" />
                <div className="mt-2">Settings not loaded</div>
                <div className="mt-1 text-xs">Try reloading the AI Workspace</div>
              </div>
            </div>
          ) : null}

          {!loading && mode === 'full' && section === 'quick' && quickPreferences ? (
            <div className="flex h-full min-h-0 flex-col rounded-3xl border bg-card">
              <div className="border-b px-6 py-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-lg font-semibold">Quick Assistant</div>
                    <div className="mt-1 text-sm text-muted-foreground">
                      保留终端 quick-ai 的同时，新增一个独立的 Quick Assistant 浮窗，支持按助手或按模型角色启动。
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => void showQuickAssistantWindow()}
                    className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                  >
                    <Wand2 className="h-4 w-4" />
                    Open Window
                  </button>
                </div>
              </div>

              <div className="min-h-0 overflow-y-auto px-6 py-5">
                <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr),320px]">
                  <div className="space-y-6">
                    <div className="grid gap-4 md:grid-cols-2">
                      <button
                        type="button"
                        onClick={() =>
                          void handleQuickPreferenceChange({
                            ...quickPreferences,
                            prefer_assistant_mode: true,
                            preferred_assistant_id:
                              quickPreferences.preferred_assistant_id || assistants[0]?.id || null,
                          })
                        }
                        className={`rounded-2xl border px-4 py-4 text-left ${
                          quickPreferences.prefer_assistant_mode ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'
                        }`}
                      >
                        <div className="flex items-center gap-2 text-sm font-medium">
                          <Bot className="h-4 w-4" />
                          Assistant Mode
                        </div>
                        <div className="mt-2 text-xs text-muted-foreground">
                          跟随助手预设的提示词、主模型、联网开关、知识库、MCP 和记忆。
                        </div>
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          void handleQuickPreferenceChange({
                            ...quickPreferences,
                            prefer_assistant_mode: false,
                          })
                        }
                        className={`rounded-2xl border px-4 py-4 text-left ${
                          !quickPreferences.prefer_assistant_mode ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'
                        }`}
                      >
                        <div className="flex items-center gap-2 text-sm font-medium">
                          <Sparkles className="h-4 w-4" />
                          Model Mode
                        </div>
                        <div className="mt-2 text-xs text-muted-foreground">
                          直接使用角色绑定模型，适合轻量改写、翻译、摘要和快速问答。
                        </div>
                      </button>
                    </div>

                    {quickPreferences.prefer_assistant_mode ? (
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Preferred Assistant</span>
                        <select
                          value={quickPreferences.preferred_assistant_id || ''}
                          onChange={(event) =>
                            void handleQuickPreferenceChange({
                              ...quickPreferences,
                              preferred_assistant_id: event.target.value || null,
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">Select assistant</option>
                          {assistants.map((assistant) => (
                            <option key={assistant.id} value={assistant.id}>
                              {assistant.name}
                            </option>
                          ))}
                        </select>
                      </label>
                    ) : (
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Preferred Role</span>
                        <select
                          value={quickPreferences.preferred_role}
                          onChange={(event) =>
                            void handleQuickPreferenceChange({
                              ...quickPreferences,
                              preferred_role: event.target.value,
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          {QUICK_ROLES.map((item) => (
                            <option key={item.role} value={item.role}>
                              {item.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    )}

                    <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">Read Clipboard On Open</div>
                        <div className="text-xs text-muted-foreground">打开浮窗时尝试读取剪贴板文本</div>
                      </div>
                      <input
                        type="checkbox"
                        checked={quickPreferences.read_clipboard_on_open}
                        onChange={(event) =>
                          void handleQuickPreferenceChange({
                            ...quickPreferences,
                            read_clipboard_on_open: event.target.checked,
                          })
                        }
                        className="h-4 w-4"
                      />
                    </label>

                    <div className="rounded-2xl border bg-muted/10 p-4">
                      <div className="mb-2 text-sm font-medium">Selection Assistant</div>
                      <div className="text-sm text-muted-foreground">
                        本轮已经把 Selection Assistant 的角色绑定预留在模型中心，但系统级划词能力还未接入。
                        当前策略是在不支持的平台上明确降级，并保持 Quick Assistant 与终端 quick-ai 双轨并存。
                      </div>
                    </div>
                  </div>

                  <div className="space-y-4 rounded-3xl border bg-muted/10 p-5">
                    <div>
                      <div className="text-sm font-semibold">Current Binding</div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        {quickPreferences.prefer_assistant_mode
                          ? `Assistant: ${assistants.find((assistant) => assistant.id === quickPreferences.preferred_assistant_id)?.name || '未选择'}`
                          : `Role Model: ${quickRoleModelId || '未绑定'}`}
                      </div>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {capabilityBadge('quick-assistant')}
                      {capabilityBadge(quickPreferences.prefer_assistant_mode ? 'assistant-mode' : 'model-mode')}
                      {capabilityBadge(quickPreferences.read_clipboard_on_open ? 'clipboard-on' : 'clipboard-off')}
                    </div>
                    <div className="rounded-2xl border bg-background p-4 text-sm text-muted-foreground">
                      终端 `quick-ai` 仍然保留给 Claude Code / Gemini / Codex / OpenCode。
                      新浮窗 Quick Assistant 与终端入口不会混用会话状态，也不会占用同一份会话编排逻辑。
                    </div>
                  </div>
                </div>
              </div>
            </div>
          ) : null}
        </main>
      </div>
    </div>
  );
}
