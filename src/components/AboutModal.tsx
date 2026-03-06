import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { X, RefreshCw, Zap, ArrowUpCircle } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useUpdater } from '../lib/updater';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { useConfirmDialog } from './ConfirmDialogProvider';

export function AboutModal({ open: isOpen, onClose }: { open: boolean, onClose: () => void }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [currentVersion, setCurrentVersion] = useState('');
  const [autoUpdateEnabled, setAutoUpdateEnabled] = useState(false);
  const [autoUpdateInterval, setAutoUpdateInterval] = useState(360);
  const wasOpenRef = useRef(false);
  const {
    status,
    checking,
    updateAvailable,
    installable,
    source,
    manifest,
    error: updateError,
    notice,
    downloadProgress,
    lastCheckedAt,
    checkForUpdates,
    installUpdate,
    installDownloadedUpdate,
  } = useUpdater();

  useEffect(() => {
    if (isOpen) {
      getVersion().then(setCurrentVersion);
      invoke<any>('get_storage_config')
        .then((cfg) => {
          setAutoUpdateEnabled(!!cfg?.auto_update_enabled);
          setAutoUpdateInterval(Number(cfg?.update_check_interval_minutes ?? 360));
        })
        .catch(() => {
          setAutoUpdateEnabled(false);
          setAutoUpdateInterval(360);
        });
    }
  }, [isOpen]);

  useEffect(() => {
    const openingNow = isOpen && !wasOpenRef.current;
    wasOpenRef.current = isOpen;
    if (!openingNow || status !== 'downloaded' || !manifest?.version) {
      return;
    }

    (async () => {
      try {
        const confirmed = await confirmDialog(t('updateReadyInstallPrompt', { version: manifest.version }), {
          title: t('updateReadyTitle'),
          kind: 'info',
          okLabel: t('installNowAction'),
          cancelLabel: t('later'),
        });
        if (confirmed) {
          await installDownloadedUpdate();
        }
      } catch (e) {
        console.error('Failed to prompt downloaded update on About open:', e);
      }
    })();
  }, [isOpen, status, manifest?.version, installDownloadedUpdate, t, confirmDialog]);

  const handleInstallAction = async () => {
    if (!installable || status === 'installing') {
      return;
    }
    if (status === 'downloaded' && manifest?.version) {
      const confirmed = await confirmDialog(t('updateReadyInstallPrompt', { version: manifest.version }), {
        title: t('updateReadyTitle'),
        kind: 'info',
        okLabel: t('installNowAction'),
        cancelLabel: t('later'),
      });
      if (!confirmed) {
        return;
      }
      await installDownloadedUpdate();
      return;
    }
    await installUpdate();
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="bg-card w-full max-w-md rounded-xl border shadow-lg overflow-hidden relative">
        <button 
          onClick={onClose} 
          className="absolute top-4 right-4 p-2 rounded-md hover:bg-muted text-muted-foreground transition-colors z-10"
        >
          <X className="w-5 h-5" />
        </button>

        <div className="flex flex-col items-center py-8 px-6 space-y-8">
          <div className="text-center space-y-2">
            <div className="w-20 h-20 bg-primary/10 rounded-2xl flex items-center justify-center mx-auto mb-4 border border-primary/20 shadow-inner">
              <img src="/onespace_icon.png" alt="Logo" className="w-12 h-12" />
            </div>
            <h3 className="font-bold text-2xl tracking-tight">OneSpace</h3>
            <p className="text-sm text-muted-foreground font-medium">
              {t('version', { version: currentVersion })}
            </p>
          </div>

          <div className="w-full space-y-4">
            <div className="p-4 bg-muted/30 rounded-xl border border-dashed flex flex-col items-center gap-4">
              {updateAvailable ? (
                <div className="w-full space-y-4 text-center">
                  <div className="flex items-center justify-center gap-2 text-primary animate-bounce">
                    <ArrowUpCircle className="w-5 h-5" />
                    <span className="font-semibold text-sm">
                      {t('newVersionAvailable', { version: manifest?.version })}
                    </span>
                  </div>
                  <div className="max-h-32 overflow-y-auto rounded-lg border bg-background/60 p-3 text-left">
                    <div className="text-xs text-muted-foreground leading-relaxed break-words [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&_a]:text-primary [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-sm [&_h1]:font-semibold [&_h2]:text-sm [&_h2]:font-semibold [&_h3]:text-sm [&_h3]:font-semibold [&_li]:my-1 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded [&_pre]:bg-muted [&_pre]:p-2 [&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5">
                      <ReactMarkdown remarkPlugins={[remarkGfm]}>
                        {manifest?.body?.trim() || t('updateDesc')}
                      </ReactMarkdown>
                    </div>
                  </div>
                  {installable && (status === 'downloading' || status === 'downloaded' || status === 'installing') && (
                    <div className="w-full space-y-2">
                      <div className="h-2 w-full rounded-full bg-muted overflow-hidden">
                        <div
                          className={`h-full bg-primary transition-all duration-300 ${status === 'installing' ? 'animate-pulse' : ''}`}
                          style={{ width: `${status === 'installing' ? 100 : downloadProgress}%` }}
                        />
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {status === 'downloading'
                          ? t('downloadingUpdateProgress', { progress: downloadProgress })
                          : status === 'installing'
                            ? t('installingUpdate')
                            : t('updateDownloadedReady')}
                      </p>
                    </div>
                  )}
                  <button
                    onClick={handleInstallAction}
                    disabled={!installable || status === 'downloading' || status === 'installing'}
                    className="w-full py-2.5 bg-primary text-primary-foreground rounded-lg text-sm font-bold shadow-lg hover:shadow-primary/20 transition-all flex items-center justify-center gap-2"
                  >
                    <Zap className="w-4 h-4 fill-current" />
                    {!installable
                      ? t('autoInstallUnavailable')
                      : status === 'downloading'
                        ? t('downloadingUpdateProgress', { progress: downloadProgress })
                        : status === 'downloaded'
                          ? t('installNowAction')
                        : status === 'installing'
                          ? t('installingUpdate')
                          : t('updateAndRelaunch')}
                  </button>
                  {!installable && (
                    <p className="text-xs text-amber-600 dark:text-amber-500">{t('fallbackCheckNotice')}</p>
                  )}
                </div>
              ) : (
                <div className="flex flex-col items-center gap-3 w-full">
                  <button
                    onClick={() => checkForUpdates(false, true)}
                    disabled={checking}
                    className="px-6 py-2 bg-secondary text-secondary-foreground rounded-full text-sm font-semibold hover:bg-secondary/80 disabled:opacity-50 transition-all flex items-center gap-2"
                  >
                    {checking ? <RefreshCw className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
                    {checking ? t('checking') : t('checkForUpdates')}
                  </button>
                  <p className="text-xs text-muted-foreground">
                    {checking ? t('contactingGitHub') : t('upToDate')}
                  </p>
                </div>
              )}
            </div>

            <div className="p-3 bg-card border rounded-lg text-xs space-y-1 text-muted-foreground">
              <p>{t('autoUpdate')}: {autoUpdateEnabled ? t('enabled') : t('disabled')}</p>
              {autoUpdateEnabled && (
                <p>{t('updateCheckFrequency')}: {autoUpdateInterval} {t('minutesUnit')}</p>
              )}
              <p>{t('updateSource')}: {source ? 'GitHub Releases' : '-'}</p>
              <p>{t('lastCheckedAt')}: {lastCheckedAt ? new Date(lastCheckedAt).toLocaleString() : '-'}</p>
              {updateAvailable && (
                <p>
                  <a
                    href="https://github.com/minbox-projects/one-space/releases"
                    onClick={(e) => {
                      e.preventDefault();
                      open('https://github.com/minbox-projects/one-space/releases');
                    }}
                    className="text-primary hover:underline"
                  >
                    {t('goToReleases')}
                  </a>
                </p>
              )}
            </div>
            
            {updateError && (
              <p className="text-xs text-destructive text-center bg-destructive/5 p-2 rounded border border-destructive/10">
                {t('error', { message: updateError })}
              </p>
            )}
            {notice && (
              <p className="text-xs text-amber-600 dark:text-amber-500 text-center bg-amber-50 dark:bg-amber-950/20 p-2 rounded border border-amber-200 dark:border-amber-900">
                {t(notice)}
              </p>
            )}
          </div>

          <div className="flex flex-col items-center gap-1 text-center">
            <p className="text-xs text-muted-foreground/60">{t('copyRight')}</p>
            <p className="text-xs text-muted-foreground/60">{t('builtWith')}</p>
          </div>
        </div>
      </div>
    </div>
  );
}
