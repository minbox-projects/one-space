import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Shield, ShieldAlert, X } from 'lucide-react';
import type { AiModelId, TerminalPermissionMode } from '@/lib/terminalPermissions';
import { getFullAccessFlag } from '@/lib/terminalPermissions';

interface TerminalPermissionConfirmDialogProps {
  open: boolean;
  toolId: AiModelId;
  toolLabel: string;
  configuredMode?: TerminalPermissionMode;
  onConfirm: (mode: TerminalPermissionMode) => void;
  onCancel: () => void;
}

const TOOL_DISPLAY_NAMES: Record<AiModelId, string> = {
  claude: 'Claude Code',
  gemini: 'Gemini',
  codex: 'Codex',
  opencode: 'OpenCode',
};

const FULL_ACCESS_RISK_TEXT_KEY = 'fullAccessRiskDesc';

export function TerminalPermissionConfirmDialog({
  open,
  toolId,
  toolLabel,
  configuredMode = 'full_access',
  onConfirm,
  onCancel,
}: TerminalPermissionConfirmDialogProps) {
  const { t } = useTranslation();
  const [selectedMode, setSelectedMode] = useState<TerminalPermissionMode | null>(configuredMode);

  const handleOpenChange = useCallback(
    (nextOpen: boolean) => {
      if (!nextOpen) {
        setSelectedMode(configuredMode);
        onCancel();
      }
    },
    [onCancel, configuredMode],
  );

  const handleContinue = () => {
    if (selectedMode) {
      onConfirm(selectedMode);
      setSelectedMode(configuredMode);
    }
  };

  if (!open) return null;

  const displayName = toolLabel || TOOL_DISPLAY_NAMES[toolId] || toolId;
  const flagInfo = getFullAccessFlag(toolId);
  const flagText = flagInfo.flag
    ? `\`${flagInfo.flag}\``
    : flagInfo.env
      ? Object.entries(flagInfo.env)
          .map(([k, v]) => `${k}=${v}`)
          .join(' ')
      : '';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
      <div className="bg-card border rounded-xl shadow-lg w-full max-w-md overflow-hidden animate-in fade-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="flex items-center justify-between p-5 pb-0">
          <div className="flex items-center gap-3 text-amber-600">
            <div className="bg-amber-500/10 p-2 rounded-full">
              <ShieldAlert className="w-5 h-5" />
            </div>
            <h3 className="font-semibold text-foreground">
              {t('permissionConfirmTitle', 'Permission Confirmation')}
            </h3>
          </div>
          <button
            onClick={() => handleOpenChange(false)}
            className="text-muted-foreground hover:text-foreground transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="px-5 pt-3 pb-4">
          <p className="text-sm text-muted-foreground mb-4">
            {t(
              'permissionConfirmDesc',
              '{{tool}} is configured with full access for session recovery. Choose the permission mode:',
              { tool: displayName },
            )}
          </p>

          <div className="space-y-3">
            {/* Default option */}
            <label
              className={`flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors ${
                selectedMode === 'default'
                  ? 'border-primary bg-primary/5'
                  : 'border-border hover:bg-muted/50'
              }`}
            >
              <input
                type="radio"
                name="permissionMode"
                value="default"
                checked={selectedMode === 'default'}
                onChange={() => setSelectedMode('default')}
                className="mt-1 accent-primary"
              />
              <div>
                <div className="text-sm font-medium text-foreground">
                  {t('defaultPermissionMode', 'Default')}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t(
                    'defaultModeDesc',
                    'Use standard permission — tool will request approval as needed.',
                  )}
                </div>
              </div>
            </label>

            {/* Full Access option */}
            <label
              className={`flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors ${
                selectedMode === 'full_access'
                  ? 'border-amber-500 bg-amber-500/5'
                  : 'border-border hover:bg-muted/50'
              }`}
            >
              <input
                type="radio"
                name="permissionMode"
                value="full_access"
                checked={selectedMode === 'full_access'}
                onChange={() => setSelectedMode('full_access')}
                className="mt-1 accent-amber-500"
              />
              <div>
                <div className="flex items-center gap-2">
                  <Shield className="w-3.5 h-3.5 text-amber-600" />
                  <span className="text-sm font-medium text-foreground">
                    {t('fullAccessPermissionMode', 'Full Access')}
                  </span>
                </div>
                <div className="text-xs text-muted-foreground mt-0.5">
                  {t(
                    FULL_ACCESS_RISK_TEXT_KEY,
                    'Skip tool permission checks or relax permission control.',
                  )}{' '}
                  {flagText && (
                    <span>
                      {t('fullAccessFlagHint', 'Will use: {{flag}}', { flag: flagText })}
                    </span>
                  )}
                </div>
              </div>
            </label>
          </div>
        </div>

        {/* Footer */}
        <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
          <button
            onClick={() => handleOpenChange(false)}
            className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
          >
            {t('cancel', 'Cancel')}
          </button>
          <button
            onClick={handleContinue}
            disabled={!selectedMode}
            className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${
              selectedMode
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'bg-muted text-muted-foreground cursor-not-allowed'
            }`}
          >
            {t('continue', 'Continue')}
          </button>
        </div>
      </div>
    </div>
  );
}
