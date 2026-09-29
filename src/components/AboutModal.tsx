import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  X,
  RefreshCw,
  Zap,
  ArrowUpCircle,
  CheckCircle2,
  AlertCircle,
  ExternalLink,
  Laptop,
  Github,
  MessageSquare,
  Sparkles,
  Clock,
} from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import * as updater from '../lib/updater';
import type { CurrentReleaseInfo } from '../lib/updater';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { openExternalUrl } from '@/lib/externalActions';

function getPlatformInfo(): string {
  if (typeof navigator === 'undefined') return 'macOS';
  const ua = navigator.userAgent;
  if (/Macintosh|Mac OS X/i.test(ua)) return 'macOS';
  if (/Windows/i.test(ua)) return 'Windows';
  if (/Linux/i.test(ua)) return 'Linux';
  return 'Desktop';
}

function formatDate(dateStr?: string): string {
  if (!dateStr) return '';
  try {
    const d = new Date(dateStr);
    if (isNaN(d.getTime())) return dateStr;
    return d.toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return dateStr;
  }
}

export function AboutModal({
  open: isOpen,
  onClose,
  autoCheckOnOpen = false,
  checkForUpdates = updater.checkForUpdates,
  fetchReleaseNotes = updater.fetchCurrentVersionReleaseNotes,
}: {
  open: boolean;
  onClose: () => void;
  autoCheckOnOpen?: boolean;
  checkForUpdates?: typeof updater.checkForUpdates;
  fetchReleaseNotes?: typeof updater.fetchCurrentVersionReleaseNotes;
}) {
  const { t } = useTranslation();
  const [currentVersion, setCurrentVersion] = useState('');
  const [currentReleaseInfo, setCurrentReleaseInfo] = useState<CurrentReleaseInfo | null>(null);
  const [loadingNotes, setLoadingNotes] = useState(false);
  const [activeTab, setActiveTab] = useState<'current' | 'latest'>('current');
  const [autoUpdateEnabled, setAutoUpdateEnabled] = useState(false);
  const [autoUpdateInterval, setAutoUpdateInterval] = useState(360);

  const {
    status,
    checking,
    updateAvailable,
    installable,
    manifest,
    error: updateError,
    notice,
    downloadProgress,
    lastCheckedAt,
    installUpdate,
    installDownloadedUpdate,
  } = updater.useUpdater();

  // Load config, version, release notes, and actively check for updates when opened
  useEffect(() => {
    if (!isOpen) return;

    let active = true;

    // 1. Get current version and fetch release notes
    getVersion()
      .then((ver) => {
        if (!active) return;
        setCurrentVersion(ver);
        setLoadingNotes(true);
        fetchReleaseNotes(ver)
          .then((info) => {
            if (active) setCurrentReleaseInfo(info);
          })
          .finally(() => {
            if (active) setLoadingNotes(false);
          });
      })
      .catch(() => {
        if (!active) return;
        setCurrentVersion('');
      });

    // 2. Load storage configuration
    invoke<{ auto_update_enabled?: boolean; update_check_interval_minutes?: number }>(
      'get_storage_config',
    )
      .then((cfg) => {
        if (!active) return;
        setAutoUpdateEnabled(!!cfg?.auto_update_enabled);
        setAutoUpdateInterval(Number(cfg?.update_check_interval_minutes ?? 360));
      })
      .catch(() => {
        if (!active) return;
        setAutoUpdateEnabled(false);
        setAutoUpdateInterval(360);
      });

    // 3. Actively trigger update check on open (force check if autoCheckOnOpen is explicitly true)
    void checkForUpdates(false, true, autoCheckOnOpen);

    return () => {
      active = false;
    };
  }, [isOpen, autoCheckOnOpen, checkForUpdates, fetchReleaseNotes]);

  // When a new update becomes available, switch to latest tab by default
  useEffect(() => {
    if (updateAvailable) {
      setActiveTab('latest');
    }
  }, [updateAvailable]);

  const handleManualRecheck = async () => {
    if (checking) return;
    await checkForUpdates(false, true, true);
    if (currentVersion) {
      setLoadingNotes(true);
      const info = await fetchReleaseNotes(currentVersion, true);
      setCurrentReleaseInfo(info);
      setLoadingNotes(false);
    }
  };

  const handleInstallAction = async () => {
    if (status === 'downloading' || status === 'installing') {
      return;
    }
    if (!installable) {
      await openExternalUrl('https://github.com/minbox-projects/one-space/releases');
      return;
    }
    if (status === 'downloaded') {
      await installDownloadedUpdate();
      return;
    }
    await installUpdate();
  };

  if (!isOpen) return null;

  const currentNotesBody = currentReleaseInfo?.body?.trim() || '';
  const latestNotesBody = manifest?.body?.trim() || '';
  const displayedNotes =
    activeTab === 'latest' && updateAvailable
      ? latestNotesBody || t('updateDesc')
      : currentNotesBody;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-background/80 backdrop-blur-sm animate-in fade-in duration-150"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="bg-card w-full max-w-2xl rounded-2xl border shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Header Section */}
        <div className="relative p-6 pb-5 border-b bg-muted/20 flex items-start justify-between">
          <div className="flex items-center gap-4">
            <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 border border-primary/25 shadow-inner flex items-center justify-center flex-shrink-0">
              <img src="/onespace_icon.png" alt="OneSpace Logo" className="w-9 h-9 object-contain" />
            </div>
            <div className="space-y-1">
              <div className="flex items-center gap-2.5">
                <h3 className="font-bold text-2xl tracking-tight text-foreground">OneSpace</h3>
                {currentVersion && (
                  <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-mono font-semibold bg-primary/10 text-primary border border-primary/20">
                    {`v${currentVersion}`}
                  </span>
                )}
              </div>
              <p className="text-xs text-muted-foreground line-clamp-2 max-w-md">
                {t('aboutDescription')}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            title="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Scrollable Center Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-5">
          {/* Status & Update Action Banner */}
          <div className="rounded-xl border bg-muted/30 p-4 transition-all">
            {checking ? (
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3 text-sm font-medium text-foreground">
                  <RefreshCw className="w-4 h-4 text-primary animate-spin" />
                  <span>{t('checking')}</span>
                  <span className="text-xs text-muted-foreground font-normal">
                    {t('contactingGitHub')}
                  </span>
                </div>
              </div>
            ) : updateAvailable ? (
              <div className="space-y-3">
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                  <div className="flex items-center gap-2.5 text-primary">
                    <ArrowUpCircle className="w-5 h-5 flex-shrink-0 animate-bounce" />
                    <div>
                      <p className="font-semibold text-sm">
                        {t('newVersionAvailable', { version: manifest?.version })}
                      </p>
                      <p className="text-xs text-muted-foreground font-normal">
                        {manifest?.date ? formatDate(manifest.date) : ''}
                      </p>
                    </div>
                  </div>
                  <button
                    onClick={handleInstallAction}
                    disabled={status === 'downloading' || status === 'installing'}
                    className="px-4 py-2 bg-primary text-primary-foreground rounded-lg text-xs font-bold shadow-md hover:bg-primary/90 transition-all flex items-center justify-center gap-1.5 self-start sm:self-auto"
                  >
                    <Zap className="w-3.5 h-3.5 fill-current" />
                    {!installable
                      ? t('goToReleases')
                      : status === 'downloading'
                        ? t('downloadingUpdateProgress', { progress: downloadProgress })
                        : status === 'downloaded'
                          ? t('installNowAction')
                          : status === 'installing'
                            ? t('installingUpdate')
                            : t('updateAndRelaunch')}
                  </button>
                </div>

                {installable && (status === 'downloading' || status === 'downloaded' || status === 'installing') && (
                  <div className="space-y-1.5 pt-1">
                    <div className="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                      <div
                        className={`h-full bg-primary transition-all duration-300 ${status === 'installing' ? 'animate-pulse' : ''}`}
                        style={{ width: `${status === 'installing' ? 100 : downloadProgress}%` }}
                      />
                    </div>
                    <div className="flex items-center justify-between text-[11px] text-muted-foreground">
                      <span>
                        {status === 'downloading'
                          ? t('downloadingUpdateProgress', { progress: downloadProgress })
                          : status === 'installing'
                            ? t('installingUpdate')
                            : t('updateDownloadedReady')}
                      </span>
                      {status === 'downloading' && <span>{downloadProgress}%</span>}
                    </div>
                  </div>
                )}

                {!installable && (
                  <p className="text-xs text-amber-600 dark:text-amber-500">{t('fallbackCheckNotice')}</p>
                )}
              </div>
            ) : updateError ? (
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2.5 text-xs text-destructive">
                  <AlertCircle className="w-4 h-4 flex-shrink-0" />
                  <span>
                    {updateError === 'rateLimitNotice' ? t('rateLimitNotice') : t('error', { message: updateError })}
                  </span>
                </div>
                <button
                  onClick={handleManualRecheck}
                  className="px-2.5 py-1 text-xs border rounded-md hover:bg-muted text-muted-foreground hover:text-foreground flex items-center gap-1 transition-colors flex-shrink-0"
                >
                  <RefreshCw className="w-3 h-3" />
                  {t('recheckUpdates')}
                </button>
              </div>
            ) : (
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2.5">
                  <CheckCircle2 className="w-4 h-4 text-emerald-500 flex-shrink-0" />
                  <span className="text-xs text-foreground font-medium">{t('upToDate')}</span>
                  {lastCheckedAt && (
                    <span className="text-[11px] text-muted-foreground hidden sm:inline">
                      ({t('lastCheckedAt')}: {new Date(lastCheckedAt).toLocaleTimeString()})
                    </span>
                  )}
                </div>
                <button
                  onClick={handleManualRecheck}
                  disabled={checking}
                  className="px-2.5 py-1 text-xs border rounded-md hover:bg-muted text-muted-foreground hover:text-foreground flex items-center gap-1 transition-colors flex-shrink-0 disabled:opacity-50"
                >
                  <RefreshCw className="w-3 h-3" />
                  {t('recheckUpdates')}
                </button>
              </div>
            )}

            {notice && (
              <p className="mt-2 text-xs text-amber-600 dark:text-amber-500 bg-amber-50 dark:bg-amber-950/20 p-2 rounded border border-amber-200 dark:border-amber-900">
                {t(notice)}
              </p>
            )}
          </div>

          {/* Release Notes Section */}
          <div className="space-y-3">
            <div className="flex items-center justify-between border-b pb-2">
              <div className="flex items-center gap-2">
                {updateAvailable ? (
                  <div className="flex items-center gap-1.5 p-0.5 bg-muted rounded-lg text-xs font-medium">
                    <button
                      onClick={() => setActiveTab('latest')}
                      className={`px-3 py-1 rounded-md transition-colors flex items-center gap-1.5 ${
                        activeTab === 'latest'
                          ? 'bg-card text-foreground shadow-sm'
                          : 'text-muted-foreground hover:text-foreground'
                      }`}
                    >
                      <Sparkles className="w-3 h-3 text-primary" />
                      {t('latestVersionNotes')} (v{manifest?.version})
                    </button>
                    <button
                      onClick={() => setActiveTab('current')}
                      className={`px-3 py-1 rounded-md transition-colors ${
                        activeTab === 'current'
                          ? 'bg-card text-foreground shadow-sm'
                          : 'text-muted-foreground hover:text-foreground'
                      }`}
                    >
                      {t('currentVersionNotes')} (v{currentVersion})
                    </button>
                  </div>
                ) : (
                  <div className="flex items-center gap-2">
                    <Sparkles className="w-4 h-4 text-primary" />
                    <span className="text-sm font-semibold text-foreground">
                      {t('currentVersionNotes')}
                    </span>
                    {currentVersion && (
                      <span className="text-xs font-mono text-muted-foreground font-medium">
                        v{currentVersion}
                      </span>
                    )}
                  </div>
                )}
              </div>

              {/* Release date or View on GitHub link */}
              <div className="flex items-center gap-3 text-xs text-muted-foreground">
                {activeTab === 'current' && currentReleaseInfo?.publishedAt && (
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {formatDate(currentReleaseInfo.publishedAt)}
                  </span>
                )}
                {activeTab === 'latest' && manifest?.date && (
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {formatDate(manifest.date)}
                  </span>
                )}
                <button
                  onClick={() =>
                    openExternalUrl(
                      activeTab === 'latest'
                        ? `https://github.com/minbox-projects/one-space/releases/tag/v${manifest?.version || ''}`
                        : currentReleaseInfo?.htmlUrl ||
                            `https://github.com/minbox-projects/one-space/releases/tag/v${currentVersion}`
                    )
                  }
                  className="hover:text-primary flex items-center gap-1 transition-colors"
                >
                  <span>{t('viewOnGitHub')}</span>
                  <ExternalLink className="w-3 h-3" />
                </button>
              </div>
            </div>

            {/* Markdown Body Box */}
            <div className="h-60 overflow-y-auto rounded-xl border bg-muted/20 p-4 transition-colors">
              {loadingNotes && !displayedNotes ? (
                <div className="h-full flex flex-col items-center justify-center gap-2 text-muted-foreground text-xs">
                  <RefreshCw className="w-5 h-5 animate-spin text-primary" />
                  <span>{t('loadingReleaseNotes')}</span>
                </div>
              ) : displayedNotes ? (
                <div className="text-xs text-muted-foreground leading-relaxed break-words [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&_a]:text-primary [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:font-mono [&_h1]:text-sm [&_h1]:font-bold [&_h1]:text-foreground [&_h1]:my-2 [&_h2]:text-xs [&_h2]:font-bold [&_h2]:text-foreground [&_h2]:my-2 [&_h3]:text-xs [&_h3]:font-semibold [&_h3]:text-foreground [&_h3]:my-1.5 [&_li]:my-1 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:my-1.5 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted/80 [&_pre]:p-2.5 [&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                    {displayedNotes}
                  </ReactMarkdown>
                </div>
              ) : (
                <div className="h-full flex flex-col items-center justify-center gap-2 text-muted-foreground text-xs">
                  <p>{t('noReleaseNotesFound')}</p>
                  <button
                    onClick={() =>
                      openExternalUrl('https://github.com/minbox-projects/one-space/releases')
                    }
                    className="text-primary hover:underline flex items-center gap-1 mt-1"
                  >
                    <span>{t('allReleases')}</span>
                    <ExternalLink className="w-3 h-3" />
                  </button>
                </div>
              )}
            </div>
          </div>

          {/* System & Policy Metadata Grid */}
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
            <div className="rounded-lg border bg-muted/15 p-2.5 space-y-0.5">
              <span className="text-[11px] text-muted-foreground flex items-center gap-1">
                <Laptop className="w-3 h-3" />
                {t('systemPlatform')}
              </span>
              <p className="text-xs font-medium text-foreground">{getPlatformInfo()}</p>
            </div>
            <div className="rounded-lg border bg-muted/15 p-2.5 space-y-0.5">
              <span className="text-[11px] text-muted-foreground flex items-center gap-1">
                <RefreshCw className="w-3 h-3" />
                {t('autoUpdate')}
              </span>
              <p className="text-xs font-medium text-foreground">
                {autoUpdateEnabled ? `${autoUpdateInterval}m` : t('disabled')}
              </p>
            </div>
            <button
              onClick={() => openExternalUrl('https://github.com/minbox-projects/one-space')}
              className="rounded-lg border bg-muted/15 p-2.5 space-y-0.5 text-left hover:bg-muted/30 transition-colors group"
            >
              <span className="text-[11px] text-muted-foreground flex items-center gap-1">
                <Github className="w-3 h-3" />
                {t('openSourceRepo')}
              </span>
              <p className="text-xs font-medium text-foreground group-hover:text-primary flex items-center justify-between">
                <span>GitHub</span>
                <ExternalLink className="w-3 h-3 opacity-60 group-hover:opacity-100" />
              </p>
            </button>
            <button
              onClick={() => openExternalUrl('https://github.com/minbox-projects/one-space/issues')}
              className="rounded-lg border bg-muted/15 p-2.5 space-y-0.5 text-left hover:bg-muted/30 transition-colors group"
            >
              <span className="text-[11px] text-muted-foreground flex items-center gap-1">
                <MessageSquare className="w-3 h-3" />
                {t('feedback')}
              </span>
              <p className="text-xs font-medium text-foreground group-hover:text-primary flex items-center justify-between">
                <span>Issues</span>
                <ExternalLink className="w-3 h-3 opacity-60 group-hover:opacity-100" />
              </p>
            </button>
          </div>
        </div>

        {/* Footer Area */}
        <div className="py-3 px-6 bg-muted/20 border-t flex flex-col sm:flex-row items-center justify-between gap-1 text-[11px] text-muted-foreground/70">
          <p>{t('copyRight')}</p>
          <p>{t('builtWith')}</p>
        </div>
      </div>
    </div>
  );
}

