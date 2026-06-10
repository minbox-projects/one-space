import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { Newspaper, RefreshCw } from 'lucide-react';
import { errorToMessage, recordMessage } from '@/lib/messages';
import { openExternalUrl } from '@/lib/externalActions';

type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };

interface AiNewsItem {
  id: string;
  provider: string;
  title: string;
  description: string;
  url: string;
  source: string;
  language: string;
  published_at: number;
  fetched_at: number;
  rank: number;
  is_new: boolean;
}

interface AiNewsProviderSyncState {
  provider: string;
  status: string;
  fetched_count: number;
  added_count: number;
  last_error?: string | null;
}

interface AiNewsSyncState {
  status: string;
  last_error?: string | null;
  last_sync_at?: number | null;
  added_count: number;
  provider_states: AiNewsProviderSyncState[];
}

const formatTimestamp = (ts: number) => {
  if (!Number.isFinite(ts) || ts <= 0) return '-';
  return new Date(ts * 1000).toLocaleString();
};

const includesAny = (text: string, needles: string[]) => needles.some((needle) => text.includes(needle));

const isRssAccessError = (message: string) => {
  const normalized = message.toLowerCase();
  return (
    includesAny(normalized, ['http 4', 'http 5', 'http 429']) ||
    includesAny(normalized, ['connection', 'timeout', 'timed out', 'network', 'dns', 'forbidden', 'unauthorized'])
  );
};

const SOURCE_PREVIEW_LIMIT = 10;

export function AiNews({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const isTauri = '__TAURI_INTERNALS__' in window;

  const [items, setItems] = useState<AiNewsItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedSources, setExpandedSources] = useState<Record<string, boolean>>({});

  const groupedItems = useMemo(
    () => {
      const groups = new Map<string, { key: string; label: string; items: AiNewsItem[] }>();
      [...items]
        .sort((a, b) => {
          const rankDelta = (b.rank || 0) - (a.rank || 0);
          if (rankDelta !== 0) return rankDelta;
          return b.published_at - a.published_at;
        })
        .forEach((item) => {
          const key = item.provider || item.source || 'rss';
          const label = item.source || item.provider || 'RSS';
          const group = groups.get(key) || { key, label, items: [] };
          group.items.push(item);
          groups.set(key, group);
        });
      return Array.from(groups.values()).sort((a, b) => a.label.localeCompare(b.label));
    },
    [items],
  );

  const loadNews = async () => {
    if (!isTauri) {
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const resp = await invoke<ApiResp<AiNewsItem[]>>('ai_news_read');
      setItems(Array.isArray(resp.data) ? resp.data : []);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadNews();
  }, []);

  useEffect(() => {
    if (!isTauri || !isVisible) return;
    const timer = window.setInterval(() => {
      void loadNews();
    }, 60_000);
    return () => {
      window.clearInterval(timer);
    };
  }, [isTauri, isVisible]);

  const handleRefresh = async () => {
    if (!isTauri) return;
    setRefreshing(true);
    setError(null);
    try {
      const resp = await invoke<ApiResp<AiNewsSyncState>>('ai_news_sync_now');
      const providerErrors = (resp?.data?.provider_states ?? []).filter(
        (state) => state.status === 'error' && !!state.last_error,
      );

      if (providerErrors.length > 0) {
        const hasRssAccessError = providerErrors.some((state) => isRssAccessError(state.last_error || ''));
        const providerNames = providerErrors
          .map((state) => state.provider)
          .filter(Boolean)
          .join(', ');

        if (hasRssAccessError) {
          setError(
            t(
              'aiNewsRefreshRssAccessError',
              'RSS source access failed. Please check network or source availability.',
              { providers: providerNames || '-' },
            ),
          );
        } else {
          const detail = providerErrors
            .map((state) => `${state.provider}: ${state.last_error}`)
            .join(' | ');
          setError(detail);
        }
      }

      await loadNews();
      emit('refresh-counts').catch(() => {});
    } catch (e: any) {
      const message = String(e || '');
      const detail = errorToMessage(e);
      void recordMessage({
        source: 'ai_news',
        category: 'manual_refresh',
        severity: 'error',
        title: t('aiNewsFetchFailedTitle', 'AI News fetch failed'),
        summary: detail.split('\n').find(Boolean) || 'AI News refresh failed',
        detail,
        dedupe_key: 'ai-news:manual-refresh:error',
        target: { tab: 'ai-news' },
      });
      if (isRssAccessError(message)) {
        setError(
          t(
            'aiNewsRefreshRssAccessErrorFallback',
            'RSS source access failed. Please check network or source availability.',
          ),
        );
      } else {
        setError(message);
      }
    } finally {
      setRefreshing(false);
    }
  };

  const handleOpenArticle = async (url: string) => {
    if (!url) return;
    await openExternalUrl(url);
  };

  return (
    <div className="flex flex-col h-full space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">{t('aiNews', 'AI News')}</h2>
          <p className="text-sm text-muted-foreground mt-1">
            {t('aiNewsDesc', 'Latest AI news, sorted by publish time.')}
          </p>
        </div>
        <button
          onClick={handleRefresh}
          disabled={refreshing}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shadow-sm disabled:opacity-60"
        >
          <RefreshCw className={`w-4 h-4 ${refreshing ? 'animate-spin' : ''}`} />
          {t('refresh', 'Refresh')}
        </button>
      </div>

      {error && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="flex-1 min-h-0 overflow-y-auto pr-1 space-y-5">
        {loading && items.length === 0 ? (
          <div className="text-sm text-muted-foreground">{t('loading', 'Loading...')}</div>
        ) : null}

        {!loading && items.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-center text-muted-foreground">
            <Newspaper className="w-10 h-10 mb-3 opacity-30" />
            <p className="text-sm">{t('noAiNews', 'No news yet.')}</p>
            <p className="text-xs mt-1 opacity-80">
              {t('noAiNewsHint', 'Enable RSS news sources in Settings > News and refresh again.')}
            </p>
          </div>
        ) : null}

        {groupedItems.map((group) => {
          const expanded = !!expandedSources[group.key];
          const visibleItems = expanded
            ? group.items
            : group.items.slice(0, SOURCE_PREVIEW_LIMIT);
          const hasMore = group.items.length > SOURCE_PREVIEW_LIMIT;

          return (
            <section key={group.key} className="space-y-2">
              <div className="flex items-center justify-between gap-3 border-b pb-2">
                <div className="min-w-0">
                  <h3 className="truncate text-sm font-semibold text-foreground">
                    {group.label}
                  </h3>
                  <p className="text-xs text-muted-foreground">
                    {t('aiNewsSourceCount', '{{count}} items', { count: group.items.length })}
                  </p>
                </div>
                {hasMore ? (
                  <button
                    type="button"
                    onClick={() =>
                      setExpandedSources((prev) => ({
                        ...prev,
                        [group.key]: !expanded,
                      }))
                    }
                    className="shrink-0 text-xs font-medium text-primary hover:underline"
                  >
                    {expanded ? t('showLess', 'Show less') : t('showMore', 'Show more')}
                  </button>
                ) : null}
              </div>

              <ol className="space-y-1">
                {visibleItems.map((item, index) => (
                  <li
                    key={item.id}
                    className="grid grid-cols-[2rem_minmax(0,1fr)_auto] items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/50"
                  >
                    <span className="text-right text-xs tabular-nums text-muted-foreground">
                      {index + 1}.
                    </span>
                    <button
                      type="button"
                      onClick={() => handleOpenArticle(item.url)}
                      className="min-w-0 truncate text-left font-medium text-foreground hover:text-primary hover:underline"
                      title={item.title || '-'}
                    >
                      {item.title || '-'}
                    </button>
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {formatTimestamp(item.published_at)}
                      {item.is_new ? (
                        <span className="ml-2 rounded border border-emerald-500/50 bg-emerald-500/10 px-1.5 py-0.5 text-emerald-600 dark:text-emerald-400">
                          {t('new', 'New')}
                        </span>
                      ) : null}
                    </span>
                  </li>
                ))}
              </ol>
            </section>
          );
        })}
      </div>
    </div>
  );
}
