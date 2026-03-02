import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { X, RefreshCw, Zap, ArrowUpCircle } from 'lucide-react';
import { useUpdater } from '../lib/updater';
import { getVersion } from '@tauri-apps/api/app';

export function AboutModal({ open: isOpen, onClose }: { open: boolean, onClose: () => void }) {
  const { t } = useTranslation();
  const [currentVersion, setCurrentVersion] = useState('');
  const { checking, updateAvailable, manifest, checkForUpdates, installUpdate, error: updateError } = useUpdater();

  useEffect(() => {
    if (isOpen) {
      getVersion().then(setCurrentVersion);
    }
  }, [isOpen]);

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
                  <p className="text-xs text-muted-foreground line-clamp-3 px-2 italic text-center">
                    {manifest?.body || t('updateDesc')}
                  </p>
                  <button
                    onClick={installUpdate}
                    className="w-full py-2.5 bg-primary text-primary-foreground rounded-lg text-sm font-bold shadow-lg hover:shadow-primary/20 transition-all flex items-center justify-center gap-2"
                  >
                    <Zap className="w-4 h-4 fill-current" />
                    {t('updateAndRelaunch')}
                  </button>
                </div>
              ) : (
                <div className="flex flex-col items-center gap-3 w-full">
                  <button
                    onClick={() => checkForUpdates()}
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
            
            {updateError && (
              <p className="text-xs text-destructive text-center bg-destructive/5 p-2 rounded border border-destructive/10">
                {t('error', { message: updateError })}
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
