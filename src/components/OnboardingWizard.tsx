import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { Check, ChevronRight, FolderOpen, HardDrive, KeyRound } from 'lucide-react';

interface StorageConfig {
  storage_type: 'local' | 'git' | 'icloud';
  git_url?: string;
  auth_method?: 'http' | 'ssh';
  http_username?: string;
  http_token?: string;
  ssh_key_path?: string;
  main_shortcut?: string;
  quick_ai_shortcut?: string;
  default_ai_dir?: string;
  default_ai_model?: 'claude' | 'gemini' | 'codex' | 'opencode';
  language?: string;
  local_storage_path?: string;
  icloud_storage_path?: string;
}

export function OnboardingWizard({
  onComplete,
}: {
  onComplete: (storageType: 'local' | 'git' | 'icloud') => void;
}) {
  const { t } = useTranslation();
  const [step, setStep] = useState<1 | 2>(1);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [config, setConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [masterPassword, setMasterPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');

  useEffect(() => {
    invoke<StorageConfig>('get_storage_config')
      .then((cfg) => {
        setConfig({
          ...cfg,
          storage_type: cfg.storage_type || 'local',
          auth_method: cfg.auth_method || 'http',
          main_shortcut: cfg.main_shortcut || 'Alt+Space',
          quick_ai_shortcut: cfg.quick_ai_shortcut || 'Alt+Shift+A',
          default_ai_model: cfg.default_ai_model || 'claude',
        });
      })
      .catch((e) => {
        setError(e.toString());
      });
  }, []);

  const selectICloudPath = async () => {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === 'string') {
        setConfig((prev) => ({ ...prev, icloud_storage_path: selected }));
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const saveAndFinish = async () => {
    setError('');
    if (!masterPassword) {
      setError(t('setMasterPassword', 'Please set a master password.'));
      return;
    }
    if (masterPassword !== confirmPassword) {
      setError(t('passwordNotMatch', 'Passwords do not match.'));
      return;
    }
    if (
      config.storage_type === 'icloud' &&
      config.icloud_storage_path &&
      !config.icloud_storage_path.includes('com~apple~CloudDocs')
    ) {
      setError(
        t(
          'invalidIcloudPath',
          'Selected folder must be inside iCloud Drive (com~apple~CloudDocs).',
        ),
      );
      return;
    }

    setSaving(true);
    try {
      await invoke('save_storage_config', { config });
      const oldPass = await invoke<string>('get_master_password');
      if (oldPass !== masterPassword) {
        await invoke('change_master_password', { oldPass, newPass: masterPassword });
      }
      onComplete(config.storage_type);
    } catch (e: any) {
      setError(e.toString());
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="h-screen w-screen bg-background text-foreground flex items-center justify-center p-6">
      <div className="w-full max-w-2xl rounded-2xl border bg-card shadow-sm">
        <div className="px-8 py-6 border-b">
          <h1 className="text-2xl font-semibold">OneSpace</h1>
          <p className="text-sm text-muted-foreground mt-1">
            {t(
              'onboardingDesc',
              'Complete initial setup before entering the main workspace.',
            )}
          </p>
        </div>

        <div className="px-8 pt-6 flex items-center gap-3 text-sm">
          <div
            className={`h-7 px-3 rounded-full border inline-flex items-center gap-2 ${
              step === 1 ? 'bg-primary text-primary-foreground border-primary' : 'text-muted-foreground'
            }`}
          >
            <HardDrive className="w-3.5 h-3.5" />
            {t('dataStorage', 'Data Storage')}
          </div>
          <ChevronRight className="w-4 h-4 text-muted-foreground" />
          <div
            className={`h-7 px-3 rounded-full border inline-flex items-center gap-2 ${
              step === 2 ? 'bg-primary text-primary-foreground border-primary' : 'text-muted-foreground'
            }`}
          >
            <KeyRound className="w-3.5 h-3.5" />
            {t('security', 'Security')}
          </div>
        </div>

        <div className="px-8 py-6 space-y-5">
          {step === 1 && (
            <>
              <div className="grid grid-cols-3 gap-2 rounded-xl bg-muted/40 p-1">
                {(['local', 'icloud', 'git'] as const).map((type) => (
                  <button
                    key={type}
                    onClick={() => setConfig((prev) => ({ ...prev, storage_type: type }))}
                    className={`rounded-lg px-3 py-2 text-sm font-medium ${
                      config.storage_type === type
                        ? 'bg-background shadow-sm text-foreground'
                        : 'text-muted-foreground hover:text-foreground'
                    }`}
                  >
                    {type === 'local' && t('local', 'Local')}
                    {type === 'icloud' && t('icloud', 'iCloud Drive')}
                    {type === 'git' && 'Git'}
                  </button>
                ))}
              </div>

              {config.storage_type === 'icloud' && (
                <div className="space-y-2">
                  <label className="text-sm font-medium">
                    {t('icloudStoragePath', 'iCloud Storage Path')}
                  </label>
                  <div className="flex gap-2">
                    <input
                      value={config.icloud_storage_path || ''}
                      onChange={(e) =>
                        setConfig((prev) => ({ ...prev, icloud_storage_path: e.target.value }))
                      }
                      placeholder="~/Library/Mobile Documents/com~apple~CloudDocs/onespace"
                      className="flex-1 h-10 px-3 rounded-md border bg-background text-sm"
                    />
                    <button
                      onClick={selectICloudPath}
                      className="h-10 px-3 rounded-md border hover:bg-muted inline-flex items-center gap-1.5 text-sm"
                    >
                      <FolderOpen className="w-4 h-4" />
                      {t('browse', 'Browse')}
                    </button>
                  </div>
                </div>
              )}

              {config.storage_type === 'git' && (
                <p className="text-xs text-muted-foreground">
                  {t(
                    'gitCanSetupLater',
                    'Git storage can be configured later in Settings after onboarding.',
                  )}
                </p>
              )}
            </>
          )}

          {step === 2 && (
            <div className="space-y-3">
              <div>
                <label className="text-sm font-medium">{t('masterPassword', 'Master Password')}</label>
                <input
                  type="password"
                  value={masterPassword}
                  onChange={(e) => setMasterPassword(e.target.value)}
                  className="mt-1 h-10 w-full rounded-md border bg-background px-3 text-sm"
                  placeholder={t('enterMasterPassword', 'Enter master password')}
                />
              </div>
              <div>
                <label className="text-sm font-medium">
                  {t('confirmPassword', 'Confirm Password')}
                </label>
                <input
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  className="mt-1 h-10 w-full rounded-md border bg-background px-3 text-sm"
                  placeholder={t('confirmPassword', 'Confirm Password')}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {t(
                  'onboardingPassNote',
                  'Use the same master password on your other devices to avoid decryption mismatch.',
                )}
              </p>
            </div>
          )}

          {error && (
            <div className="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
              {error}
            </div>
          )}
        </div>

        <div className="px-8 py-5 border-t flex justify-between">
          <button
            disabled={saving || step === 1}
            onClick={() => setStep(1)}
            className="h-10 px-4 rounded-md border text-sm disabled:opacity-50"
          >
            {t('back', 'Back')}
          </button>

          {step === 1 ? (
            <button
              disabled={saving}
              onClick={() => setStep(2)}
              className="h-10 px-4 rounded-md bg-primary text-primary-foreground text-sm inline-flex items-center gap-2 disabled:opacity-60"
            >
              {t('next', 'Next')}
              <ChevronRight className="w-4 h-4" />
            </button>
          ) : (
            <button
              disabled={saving}
              onClick={saveAndFinish}
              className="h-10 px-4 rounded-md bg-primary text-primary-foreground text-sm inline-flex items-center gap-2 disabled:opacity-60"
            >
              <Check className="w-4 h-4" />
              {saving ? t('saving', 'Saving...') : t('finish', 'Finish Setup')}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
