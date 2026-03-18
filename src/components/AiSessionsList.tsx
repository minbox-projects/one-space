import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlertCircle,
  Check,
  Copy,
  Edit2,
  FolderOpen,
  Loader2,
  Play,
  Terminal,
  Trash2,
  X,
} from 'lucide-react';
import { ToolIcon } from './AiEnvironments';

export interface AiSessionListItem {
  id: string;
  name: string;
  working_dir: string;
  model_type: string;
  model_name?: string | null;
  tool_session_id: string;
  status?: string;
  created_at: number;
  last_used_at?: number;
}

type AiModelId = 'claude' | 'gemini' | 'codex' | 'opencode';

const AI_MODEL_OPTIONS: Array<{ id: AiModelId; name: string }> = [
  { id: 'claude', name: 'Claude Code' },
  { id: 'gemini', name: 'Gemini' },
  { id: 'codex', name: 'Codex' },
  { id: 'opencode', name: 'OpenCode' },
];

export function AiSessionsList({
  sessions,
  loading = false,
  onLaunch,
  onDelete,
  onRename,
}: {
  sessions: AiSessionListItem[];
  loading?: boolean;
  onLaunch: (session: AiSessionListItem) => void | Promise<void>;
  onDelete: (sessionId: string) => void | Promise<void>;
  onRename: (session: AiSessionListItem, nextName: string) => void | Promise<void>;
}) {
  const { t } = useTranslation();
  const [toolFilter, setToolFilter] = useState<string>('all');
  const [modelFilter, setModelFilter] = useState<string>('all');
  const [nameFilter, setNameFilter] = useState('');
  const [editingSession, setEditingSession] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [sessionToDelete, setSessionToDelete] = useState<string | null>(null);
  const [copiedValueKey, setCopiedValueKey] = useState<string | null>(null);

  const toolFilteredSessions = useMemo(
    () =>
      toolFilter === 'all'
        ? sessions
        : sessions.filter(
            (session) => (session.model_type?.trim().toLowerCase() || '') === toolFilter,
          ),
    [sessions, toolFilter],
  );

  const sessionToolOptions = useMemo(
    () =>
      Array.from(
        new Set(
          sessions
            .map((session) => session.model_type?.trim().toLowerCase() || '')
            .filter((value) => value.length > 0),
        ),
      ).sort((a, b) => a.localeCompare(b)),
    [sessions],
  );

  const sessionModelOptions = useMemo(
    () =>
      Array.from(
        new Set(
          toolFilteredSessions
            .map((session) => session.model_name?.trim())
            .filter((value): value is string => Boolean(value)),
        ),
      ).sort((a, b) => a.localeCompare(b)),
    [toolFilteredSessions],
  );

  const getSessionDisplayName = useCallback(
    (session: AiSessionListItem) => {
      const name = session.name?.trim();
      if (name) return name;
      return session.tool_session_id?.trim()
        ? session.tool_session_id
        : t('syncingTitleFromHistory', 'Syncing title from history');
    },
    [t],
  );

  const filteredSessions = useMemo(
    () =>
      sessions.filter((session) => {
        const normalizedTool = session.model_type?.trim().toLowerCase() || '';
        const normalizedModel = session.model_name?.trim() || '';
        const displayName = getSessionDisplayName(session);
        const normalizedQuery = nameFilter.trim().toLowerCase();

        if (toolFilter !== 'all' && normalizedTool !== toolFilter) {
          return false;
        }
        if (modelFilter !== 'all' && normalizedModel !== modelFilter) {
          return false;
        }
        if (
          normalizedQuery &&
          !displayName.toLowerCase().includes(normalizedQuery) &&
          !normalizedModel.toLowerCase().includes(normalizedQuery) &&
          !(session.working_dir || '').toLowerCase().includes(normalizedQuery)
        ) {
          return false;
        }
        return true;
      }),
    [sessions, toolFilter, modelFilter, nameFilter, getSessionDisplayName],
  );

  const handleCopyValue = async (value: string, key: string, event: React.MouseEvent) => {
    event.stopPropagation();
    await navigator.clipboard.writeText(value);
    setCopiedValueKey(key);
    window.setTimeout(() => setCopiedValueKey(null), 2000);
  };

  const handleStartRename = (session: AiSessionListItem) => {
    setEditingSession(session.id);
    setEditName(session.name);
  };

  const handleSaveRename = async (session: AiSessionListItem) => {
    const nextName = editName.trim();
    if (!nextName || nextName === session.name) {
      setEditingSession(null);
      return;
    }
    await onRename(session, nextName);
    setEditingSession(null);
  };

  const formatTime = (ts: number) =>
    new Date(ts * 1000).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });

  return (
    <>
      <div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
        <div className="grid gap-3 md:grid-cols-3">
          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t('filterByTool', 'Tool')}
            </label>
            <div className="flex h-10 w-full items-center gap-2 rounded-md border border-input bg-background px-3">
              <ToolIcon
                tool={toolFilter === 'all' ? 'terminal' : toolFilter}
                className="w-4 h-4 text-muted-foreground shrink-0"
              />
              <select
                value={toolFilter}
                onChange={(e) => setToolFilter(e.target.value)}
                className="h-full w-full bg-transparent text-sm outline-none"
              >
                <option value="all">{t('allTools', 'All tools')}</option>
                {sessionToolOptions.map((tool) => (
                  <option key={tool} value={tool}>
                    {AI_MODEL_OPTIONS.find((item) => item.id === tool)?.name || tool}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t('filterByModel', 'Model')}
            </label>
            <select
              value={modelFilter}
              onChange={(e) => setModelFilter(e.target.value)}
              className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              <option value="all">{t('allModels', 'All models')}</option>
              {sessionModelOptions.map((model) => (
                <option key={model} value={model}>
                  {model}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t('filterByName', 'Name')}
            </label>
            <input
              type="text"
              value={nameFilter}
              onChange={(e) => setNameFilter(e.target.value)}
              placeholder={t('filterSessionsByName', 'Filter sessions by name...')}
              className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            />
          </div>
        </div>

        <div className="text-xs text-muted-foreground">
          {t('sessionFilterSummary', {
            defaultValue: 'Showing {{visible}} of {{total}} sessions',
            visible: filteredSessions.length,
            total: sessions.length,
          })}
        </div>
      </div>

      <div className="flex-1 overflow-auto rounded-xl border bg-card text-card-foreground shadow-sm">
        {loading && sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
            <Loader2 className="w-8 h-8 mb-3 animate-spin" />
            <p>{t('loading', 'Loading...')}</p>
          </div>
        ) : sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
            <Terminal className="w-10 h-10 mb-3 opacity-20" />
            <p>{t('noActiveSessions')}</p>
            <p className="text-sm mt-1">{t('createOneToGetStarted')}</p>
          </div>
        ) : filteredSessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
            <Terminal className="w-10 h-10 mb-3 opacity-20" />
            <p>{t('noMatchingSessions', 'No matching sessions')}</p>
            <p className="text-sm mt-1">
              {t('adjustSessionFilters', 'Try adjusting the tool, model, or name filters.')}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {filteredSessions.map((session) => {
              const canResume = Boolean(
                session.tool_session_id &&
                  session.status !== 'unbound' &&
                  session.status !== 'pending_bind',
              );
              const isPendingBind = session.status === 'pending_bind';
              const isUnbound = session.status === 'unbound';
              const displayName = getSessionDisplayName(session);
              const displayModelName = session.model_name?.trim() ? session.model_name : null;
              const displaySessionId =
                session.tool_session_id ||
                (isPendingBind
                  ? t('sessionIdPendingBind', 'Syncing from history')
                  : t('sessionIdUnavailable', 'ID unavailable'));
              return (
                <div
                  key={session.id}
                  className="p-4 hover:bg-muted/30 transition-colors group/copy"
                  style={{ contentVisibility: 'auto', containIntrinsicSize: '108px' }}
                >
                  <div className="flex items-center gap-3">
                    <div className="w-1.5 h-1.5 rounded-full shrink-0 bg-muted-foreground/40" />
                    <div className="flex-1 min-w-0">
                      {editingSession === session.id ? (
                        <div className="flex items-center gap-2 mb-3">
                          <input
                            type="text"
                            autoFocus
                            value={editName}
                            onChange={(e) => setEditName(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') void handleSaveRename(session);
                              if (e.key === 'Escape') setEditingSession(null);
                            }}
                            className="flex h-7 rounded-md border border-input bg-background px-2 py-1 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring w-64"
                          />
                          <button
                            onClick={() => void handleSaveRename(session)}
                            className="text-green-500 hover:bg-green-500/10 p-1 rounded transition-colors"
                          >
                            <Check className="w-4 h-4" />
                          </button>
                          <button
                            onClick={() => setEditingSession(null)}
                            className="text-muted-foreground hover:bg-muted p-1 rounded transition-colors"
                          >
                            <X className="w-4 h-4" />
                          </button>
                        </div>
                      ) : (
                        <div className="flex items-center justify-between mb-3">
                          <div className="flex items-center gap-2 group/title">
                            <ToolIcon
                              tool={session.model_type || 'terminal'}
                              className="w-4 h-4 text-muted-foreground shrink-0"
                            />
                            <span className="font-semibold text-base truncate max-w-md">
                              {displayName}
                            </span>
                            {displayModelName ? (
                              <span className="px-2 py-0.5 rounded-full bg-muted text-muted-foreground text-[11px] font-mono shrink-0">
                                {displayModelName}
                              </span>
                            ) : null}
                            <button
                              onClick={() => handleStartRename(session)}
                              className="opacity-0 group-hover/title:opacity-100 text-muted-foreground hover:text-foreground p-0.5 rounded transition-all shrink-0"
                              title={t('edit', 'Rename')}
                            >
                              <Edit2 className="w-3.5 h-3.5" />
                            </button>
                          </div>
                          <div className="flex items-center gap-2">
                            <button
                              onClick={() => canResume && onLaunch(session)}
                              disabled={!canResume}
                              className={`px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors ${
                                canResume
                                  ? 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                                  : 'bg-muted text-muted-foreground cursor-not-allowed'
                              }`}
                            >
                              <Play className="w-3.5 h-3.5" />
                              {canResume
                                ? t('continue', 'Continue')
                                : t('unavailable', 'Unavailable')}
                            </button>
                            <button
                              onClick={() => setSessionToDelete(session.id)}
                              className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                              {t('delete', 'Delete')}
                            </button>
                          </div>
                        </div>
                      )}

                      {editingSession !== session.id && (
                        <div className="flex items-center gap-4 text-sm text-muted-foreground">
                          <div className="flex items-center gap-1.5 font-mono text-xs shrink-0 group/copybtn">
                            <span className="truncate max-w-[320px]">{displaySessionId}</span>
                            {session.tool_session_id &&
                            copiedValueKey === `id:${session.tool_session_id}` ? (
                              <Check className="w-3.5 h-3.5 text-green-500 shrink-0" />
                            ) : session.tool_session_id ? (
                              <button
                                onClick={(e) =>
                                  void handleCopyValue(
                                    session.tool_session_id,
                                    `id:${session.tool_session_id}`,
                                    e,
                                  )
                                }
                                className="opacity-0 group-hover/copy:opacity-100 hover:text-foreground p-0.5 rounded transition-all shrink-0"
                                title={t('copy', 'Copy ID')}
                              >
                                <Copy className="w-3.5 h-3.5" />
                              </button>
                            ) : (
                              <span className="text-[11px] text-muted-foreground/80">
                                {isPendingBind
                                  ? t('sessionBinding', 'syncing from history')
                                  : isUnbound
                                    ? t('sessionUnbound', 'unbound')
                                    : t('sessionIdUnavailable', 'unavailable')}
                              </span>
                            )}
                          </div>
                          <div className="flex items-center gap-1.5 min-w-0 flex-1">
                            <FolderOpen className="w-3 h-3 shrink-0" />
                            <span className="truncate">{session.working_dir}</span>
                            {copiedValueKey === `dir:${session.working_dir}` ? (
                              <Check className="w-3.5 h-3.5 text-green-500 shrink-0" />
                            ) : session.working_dir ? (
                              <button
                                onClick={(e) =>
                                  void handleCopyValue(
                                    session.working_dir,
                                    `dir:${session.working_dir}`,
                                    e,
                                  )
                                }
                                className="opacity-0 group-hover/copy:opacity-100 hover:text-foreground p-0.5 rounded transition-all shrink-0"
                                title={t('copyPath', 'Copy path')}
                              >
                                <Copy className="w-3.5 h-3.5" />
                              </button>
                            ) : null}
                          </div>
                          <span className="text-xs font-normal tabular-nums shrink-0">
                            {formatTime(session.last_used_at || session.created_at)}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {sessionToDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            <div className="p-5">
              <div className="flex items-center gap-3 text-destructive mb-3">
                <div className="bg-destructive/10 p-2 rounded-full">
                  <AlertCircle className="w-5 h-5" />
                </div>
                <h3 className="font-semibold">{t('removeSession', 'Delete Session')}</h3>
              </div>
              <p className="text-sm text-muted-foreground">{t('confirmRemove')}</p>
            </div>
            <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
              <button
                onClick={() => setSessionToDelete(null)}
                disabled={loading}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors disabled:opacity-50"
              >
                {t('cancel', 'Cancel')}
              </button>
              <button
                onClick={async () => {
                  await onDelete(sessionToDelete);
                  setSessionToDelete(null);
                }}
                disabled={loading}
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors disabled:opacity-50"
              >
                {t('delete', 'Delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
