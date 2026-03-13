import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { useTranslation } from 'react-i18next';
import { ExternalLink, Newspaper, RefreshCw } from 'lucide-react';

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

const isRateLimitError = (message: string) => {
  const normalized = message.toLowerCase();
  return (
    includesAny(normalized, ['http 429', 'http 402']) ||
    includesAny(normalized, ['rate limit', 'too many requests', 'quota', 'limit reached', 'request limit'])
  );
};

const isApiAccessError = (message: string) => {
  const normalized = message.toLowerCase();
  return (
    includesAny(normalized, ['http 4', 'http 5']) ||
    includesAny(normalized, ['connection', 'timeout', 'timed out', 'network', 'dns', 'forbidden', 'unauthorized'])
  );
};

export function AiNews({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const isTauri = '__TAURI_INTERNALS__' in window;

  const [items, setItems] = useState<AiNewsItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sortedItems = useMemo(
    () => [...items].sort((a, b) => b.published_at - a.published_at),
    [items],
  );

  const loadNews = async () => {
    if (!isTauri) return;
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
        const hasRateLimit = providerErrors.some((state) => isRateLimitError(state.last_error || ''));
        const hasApiAccessError = providerErrors.some((state) => isApiAccessError(state.last_error || ''));
        const providerNames = providerErrors
          .map((state) => state.provider)
          .filter(Boolean)
          .join(', ');

        if (hasRateLimit) {
          setError(
            t(
              'aiNewsRefreshRateLimitError',
              'News API free-tier request quota was reached. Please retry later or switch API key/provider.',
              { providers: providerNames || '-' },
            ),
          );
        } else if (hasApiAccessError) {
          setError(
            t(
              'aiNewsRefreshApiAccessError',
              'News API access failed. Please check API key, network, or provider status.',
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
    } catch (e: any) {
      const message = String(e || '');
      if (isRateLimitError(message)) {
        setError(
          t(
            'aiNewsRefreshRateLimitErrorFallback',
            'News API free-tier request quota was reached. Please retry later.',
          ),
        );
      } else if (isApiAccessError(message)) {
        setError(
          t(
            'aiNewsRefreshApiAccessErrorFallback',
            'News API access failed. Please check API key, network, or provider status.',
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
    if (isTauri) {
      await open(url);
      return;
    }
    window.open(url, '_blank', 'noopener,noreferrer');
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

      <div className="flex-1 min-h-0 overflow-y-auto pr-1 space-y-3">
        {loading && sortedItems.length === 0 ? (
          <div className="text-sm text-muted-foreground">{t('loading', 'Loading...')}</div>
        ) : null}

        {!loading && sortedItems.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-center text-muted-foreground">
            <Newspaper className="w-10 h-10 mb-3 opacity-30" />
            <p className="text-sm">{t('noAiNews', 'No news yet.')}</p>
            <p className="text-xs mt-1 opacity-80">
              {t('noAiNewsHint', 'Configure API keys in Settings > News and refresh again.')}
            </p>
          </div>
        ) : null}

        {sortedItems.map((item) => (
          <article
            key={item.id}
            className="rounded-xl border bg-card shadow-sm p-4 transition-colors hover:border-primary/40"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h3 className="text-sm font-semibold leading-relaxed text-foreground line-clamp-2">
                  {item.title || '-'}
                </h3>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <span className="px-1.5 py-0.5 rounded border bg-muted/30">{item.provider || '-'}</span>
                  <span>{item.source || '-'}</span>
                  <span>·</span>
                  <span>{formatTimestamp(item.published_at)}</span>
                  {item.is_new ? (
                    <span className="px-1.5 py-0.5 rounded border border-emerald-500/50 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
                      {t('new', 'New')}
                    </span>
                  ) : null}
                </div>
              </div>
              <button
                onClick={() => handleOpenArticle(item.url)}
                className="shrink-0 inline-flex items-center gap-1 text-xs px-2 py-1 rounded border bg-background hover:bg-muted transition-colors"
              >
                <ExternalLink className="w-3.5 h-3.5" />
                {t('open', 'Open')}
              </button>
            </div>
            {item.description ? (
              <p className="mt-3 text-sm text-muted-foreground leading-relaxed line-clamp-3">
                {item.description}
              </p>
            ) : null}
          </article>
        ))}
      </div>
    </div>
  );
}
