import { X, ArrowUpCircle, Loader2, Ban } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

type UpdaterStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'installing' | 'error';

interface UpdateUpgradeModalProps {
  open: boolean;
  currentVersion: string;
  latestVersion: string;
  releaseNotes: string;
  status: UpdaterStatus;
  installable: boolean;
  downloadProgress: number;
  onClose: () => void;
  onUpgradeNow: () => Promise<void> | void;
  onIgnoreVersion: () => Promise<void> | void;
}

export function UpdateUpgradeModal({
  open,
  currentVersion,
  latestVersion,
  releaseNotes,
  status,
  installable,
  downloadProgress,
  onClose,
  onUpgradeNow,
  onIgnoreVersion,
}: UpdateUpgradeModalProps) {
  const { t } = useTranslation();

  if (!open) return null;

  const running = status === 'downloading' || status === 'installing';
  const statusText = !installable
    ? t('fallbackCheckNotice')
    : status === 'downloading'
      ? t('downloadingUpdateProgress', { progress: downloadProgress })
      : status === 'installing'
        ? t('installingUpdate')
        : status === 'downloaded'
          ? t('updateDownloadedReady')
          : '';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-2xl rounded-xl border bg-card shadow-lg overflow-hidden">
        <div className="flex items-center justify-between border-b px-5 py-4">
          <div className="flex items-center gap-2">
            <ArrowUpCircle className="w-5 h-5 text-primary" />
            <h3 className="text-base font-semibold">{t('upgradeDialogTitle', 'Version Update')}</h3>
          </div>
          <button
            onClick={onClose}
            className="rounded-md p-2 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-4 p-5">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
            <div className="rounded-lg border bg-muted/20 p-3">
              <p className="text-xs text-muted-foreground">{t('currentVersionLabel', 'Current Version')}</p>
              <p className="mt-1 font-mono font-semibold">v{currentVersion}</p>
            </div>
            <div className="rounded-lg border bg-primary/5 p-3">
              <p className="text-xs text-muted-foreground">{t('latestVersionLabel', 'Latest Version')}</p>
              <p className="mt-1 font-mono font-semibold text-primary">v{latestVersion}</p>
            </div>
          </div>

          <div className="space-y-2">
            <p className="text-sm font-medium">{t('updateReleaseNotes', 'Release Notes')}</p>
            <div className="max-h-[42vh] overflow-y-auto rounded-lg border bg-background/60 p-3">
              <div className="text-xs text-muted-foreground leading-relaxed break-words [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&_a]:text-primary [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-sm [&_h1]:font-semibold [&_h2]:text-sm [&_h2]:font-semibold [&_h3]:text-sm [&_h3]:font-semibold [&_li]:my-1 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded [&_pre]:bg-muted [&_pre]:p-2 [&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {releaseNotes.trim() || t('updateDesc')}
                </ReactMarkdown>
              </div>
            </div>
            {statusText && (
              <p className="text-xs text-muted-foreground">{statusText}</p>
            )}
          </div>
        </div>

        <div className="flex flex-col-reverse sm:flex-row items-stretch sm:items-center justify-end gap-3 border-t px-5 py-4">
          <button
            onClick={onIgnoreVersion}
            className="inline-flex items-center justify-center gap-2 rounded-md border px-4 py-2.5 text-sm font-medium text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Ban className="w-4 h-4" />
            {t('ignoreThisVersion', 'Ignore This Version')}
          </button>
          <button
            onClick={onUpgradeNow}
            disabled={running}
            className="inline-flex items-center justify-center gap-2 rounded-md bg-primary px-4 py-2.5 text-sm font-semibold text-primary-foreground shadow hover:opacity-95 disabled:opacity-70 transition-opacity"
          >
            {running ? <Loader2 className="w-4 h-4 animate-spin" /> : <ArrowUpCircle className="w-4 h-4" />}
            {installable ? t('upgradeNow', 'Upgrade Now') : t('goToReleases')}
          </button>
        </div>
      </div>
    </div>
  );
}
