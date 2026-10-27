import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  Copy,
  KeyRound,
  Loader2,
  Pencil,
  RotateCcw,
  Save,
  Settings2,
  Trash2,
  Zap,
} from 'lucide-react';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';
import { ConfigJsonEditor } from './ConfigJsonEditor';
import { cn } from '@/lib/utils';
import { IconPicker } from './IconPicker';
import { OpenCodeModelForm } from './OpenCodeModelForm';
import type {
  OpenCodeModelConfigError,
  OpenCodeModelsFormValue,
} from './opencodeModelConfig';
import { HistoryPanel } from './serviceProviderDetail/HistoryPanel';
import { OpenCodeJsonPanel } from './serviceProviderDetail/OpenCodeJsonPanel';
import { ClaudeSettingsSection } from './serviceProviderDetail/ClaudeSettingsSection';
import {
  buildClaudeSettingsJson,
  fallbackProtocolRouterBaseUrl,
  type ClaudeModelMapping,
} from './serviceProviderDetail/claudeSettings';
import type { HistoryEntry } from './serviceProviderDetail/historyDiff';

type JsonMode = 'claude' | 'generic' | 'opencode';

interface ServiceProviderDetailProps {
  provider: any;
  onChange: (changes: Partial<any>) => void;
  onSave: () => void;
  onActivate: () => void;
  onDelete: () => void;
  onBack: () => void;
  isActive?: boolean;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
  onFetchModels?: (provider: any) => Promise<string[]>;
  jsonMode?: JsonMode;
  jsonValue?: string;
  jsonHistory?: HistoryEntry[];
  jsonError?: string | null;
  isRollbackMode?: boolean;
  onJsonChange?: (value: string) => void;
  onJsonError?: (error: string | null) => void;
  onRollback?: (entry: HistoryEntry) => void;
  onFormatJson?: () => void;
  onCancelRollback?: () => void;
  importedInactiveNotice?: string | null;
  saving?: boolean;
  message?: { type: string; text: string };
  openCodeModelForm?: OpenCodeModelsFormValue;
  openCodeModelErrors?: OpenCodeModelConfigError[];
  openCodeModelFrozen?: boolean;
  onOpenCodeModelFormChange?: (value: OpenCodeModelsFormValue) => void;
}

export function ServiceProviderDetail({
  provider,
  onChange,
  onSave,
  onActivate,
  onDelete,
  onBack,
  isActive,
  t,
  onFetchModels,
  jsonMode,
  jsonValue,
  jsonHistory,
  jsonError,
  isRollbackMode,
  onJsonChange,
  onJsonError,
  onRollback,
  onFormatJson,
  onCancelRollback,
  importedInactiveNotice,
  saving = false,
  message,
  openCodeModelForm,
  openCodeModelErrors = [],
  openCodeModelFrozen = false,
  onOpenCodeModelFormChange,
}: ServiceProviderDetailProps) {
  const [claudeJsonError, setClaudeJsonError] = useState<string | null>(null);
  const [claudeJsonDraft, setClaudeJsonDraft] = useState('');
  const [routerPreviewBaseUrl, setRouterPreviewBaseUrl] = useState<string | undefined>();
  const [fetchingModels, setFetchingModels] = useState(false);
  const [fetchModelsError, setFetchModelsError] = useState<string | null>(null);
  const [apiKeyCopied, setApiKeyCopied] = useState(false);
  const [apiKeyCopyError, setApiKeyCopyError] = useState<string | null>(null);
  const apiKeyCopyRequestIdRef = useRef(0);
  const currentApiKeyCopyTargetRef = useRef({
    providerId: String(provider?.id || ''),
    apiKey: String(provider?.api_key || ''),
  });
  currentApiKeyCopyTargetRef.current = {
    providerId: String(provider?.id || ''),
    apiKey: String(provider?.api_key || ''),
  };

  const tool = provider?.tool;
  const isClaude = tool === 'claude';
  const isCodex = tool === 'codex';
  const isAntigravity = tool === 'antigravity';
  const isOpenCode = tool === 'opencode';
  const connectionMode = provider?.claude_connection_mode || (
    provider?.claude_api_format === 'open_ai_chat' || provider?.claude_api_format === 'open_ai_responses'
      ? 'protocol_router'
      : 'native_anthropic'
  );
  const apiFormat = provider?.claude_api_format || (connectionMode === 'protocol_router' ? 'open_ai_chat' : 'anthropic_messages');

  const effectiveJsonMode = jsonMode || (isClaude ? 'claude' : isOpenCode ? 'opencode' : 'generic');

  const settingsJson = useMemo(() => {
    if (effectiveJsonMode === 'claude') return buildClaudeSettingsJson(provider, routerPreviewBaseUrl);
    if (effectiveJsonMode === 'opencode') return jsonValue || '{}';
    return jsonValue || JSON.stringify(provider || {}, null, 2);
  }, [effectiveJsonMode, jsonValue, provider, routerPreviewBaseUrl]);

  useEffect(() => {
    const isRouterMode = provider?.claude_api_format === 'open_ai_chat'
      || provider?.claude_api_format === 'open_ai_responses'
      || provider?.claude_connection_mode === 'protocol_router';
    if (!provider?.id || !isRouterMode) {
      setRouterPreviewBaseUrl(undefined);
      return;
    }
    let cancelled = false;
    invoke<string>('protocol_router_base_url_for_claude_provider', { providerId: provider.id })
      .then((url) => {
        if (!cancelled) setRouterPreviewBaseUrl(url);
      })
      .catch(() => {
        if (!cancelled) setRouterPreviewBaseUrl(fallbackProtocolRouterBaseUrl(provider.id));
      });
    return () => {
      cancelled = true;
    };
  }, [provider?.id, provider?.claude_api_format, provider?.claude_connection_mode]);

  useEffect(() => {
    if (effectiveJsonMode !== 'claude') return;
    setClaudeJsonDraft(settingsJson);
    setClaudeJsonError(null);
  }, [effectiveJsonMode, settingsJson]);

  useEffect(() => {
    apiKeyCopyRequestIdRef.current += 1;
    setApiKeyCopied(false);
    setApiKeyCopyError(null);
  }, [provider?.id, provider?.api_key]);

  const handleClaudeJsonChange = useCallback((raw: string) => {
    setClaudeJsonDraft(raw);
    try {
      JSON.parse(raw);
      setClaudeJsonError(null);
    } catch (e: any) {
      setClaudeJsonError(e?.message || (t ? t('invalidJson', 'Invalid JSON') : 'Invalid JSON'));
    }
  }, [t]);

  const handleFetchModels = async () => {
    if (!onFetchModels) return;
    setFetchingModels(true);
    setFetchModelsError(null);
    try {
      const models = await onFetchModels(provider);
      onChange({ fetched_models: models });
      setFetchModelsError(null);
    } catch (e: any) {
      setFetchModelsError(
        e?.message || (t ? t('fetchModelsFailed', 'Failed to fetch models') : 'Failed to fetch models'),
      );
    } finally {
      setFetchingModels(false);
    }
  };

  const handleMappingChange = (mappings: ClaudeModelMapping[]) => {
    onChange({ claude_model_mappings: mappings });
  };

  const handleApiKeyCopy = async () => {
    const requestId = ++apiKeyCopyRequestIdRef.current;
    const providerId = String(provider?.id || '');
    const apiKey = String(provider?.api_key || '');
    setApiKeyCopied(false);
    setApiKeyCopyError(null);
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard API unavailable');
      await navigator.clipboard.writeText(apiKey);
      const currentTarget = currentApiKeyCopyTargetRef.current;
      if (
        apiKeyCopyRequestIdRef.current !== requestId
        || currentTarget.providerId !== providerId
        || currentTarget.apiKey !== apiKey
      ) return;
      setApiKeyCopied(true);
    } catch {
      const currentTarget = currentApiKeyCopyTargetRef.current;
      if (
        apiKeyCopyRequestIdRef.current !== requestId
        || currentTarget.providerId !== providerId
        || currentTarget.apiKey !== apiKey
      ) return;
      setApiKeyCopied(false);
      setApiKeyCopyError(t ? t('copyApiKeyFailed', 'Failed to copy API Key') : 'Failed to copy API Key');
    }
  };

  const saveDisabled = saving
    || (effectiveJsonMode === 'claude' ? !!claudeJsonError : !!jsonError)
    || (isOpenCode && openCodeModelErrors.length > 0);
  const providerIdentifierLabel = isOpenCode
    ? (t ? t('providerIdentifier', 'Service Provider Identifier') : 'Service Provider Identifier')
    : (t ? t('providerIdentifier', 'Service Provider Identifier') : 'Service Provider Identifier');

  return (
    <div className="flex h-full flex-col bg-background">
      <div className="shrink-0 border-b bg-card px-5 py-4">
        <div className="flex flex-wrap items-center gap-3">
          <button type="button" className="acc-btn" onClick={onBack} title={t ? t('back', 'Back') : 'Back'}>
            <ArrowLeft className="h-3.5 w-3.5" />
            {t ? t('back', 'Back') : 'Back'}
          </button>
          <IconPicker
            value={provider?.icon}
            name={provider?.name}
            providerId={provider?.id}
            tool={provider?.tool}
            onChange={(icon) => onChange({ icon })}
            t={t}
            triggerClassName="h-auto w-auto border-0 bg-transparent p-0 hover:border-0 hover:bg-transparent"
            trigger={(
              <div className="relative">
                <ServiceProviderAvatar
                  icon={provider?.icon}
                  name={provider?.name || ''}
                  id={provider?.id || ''}
                  tool={provider?.tool}
                  size={40}
                />
                <div className="pointer-events-none absolute -bottom-1 -right-1 rounded-full border border-border bg-background p-1 text-muted-foreground">
                  <Pencil className="h-3 w-3" />
                </div>
              </div>
            )}
          />
          <div className="min-w-[140px] flex-1">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-base font-semibold">
                {provider?.name || t?.('newPreset', 'New Service Provider')}
              </h3>
              {isActive ? (
                <span className="badge-pill bg-green-500/10 text-green-700">
                  {t ? t('active', 'Active') : 'Active'}
                </span>
              ) : null}
            </div>
            <p className="text-xs capitalize text-muted-foreground">{provider?.tool}</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <HistoryPanel
              currentProvider={provider}
              history={jsonHistory || []}
              onRollback={onRollback}
              t={t}
            />
            {!isActive ? (
              <button type="button" className="acc-panel-btn" onClick={onActivate} disabled={saving}>
                <Zap className="h-4 w-4" />
                {t ? t('activateServiceProvider', 'Activate') : 'Activate'}
              </button>
            ) : null}
            <button type="button" className="acc-panel-btn danger" onClick={onDelete} disabled={saving}>
              <Trash2 className="h-4 w-4" />
              {t ? t('delete', 'Delete') : 'Delete'}
            </button>
            <button type="button" className="acc-panel-btn primary" onClick={onSave} disabled={saveDisabled}>
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
              {saving ? (t ? t('saving', 'Saving...') : 'Saving...') : (t ? t('save', 'Save') : 'Save')}
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-5">
        <div className="space-y-6">
          {message?.text ? (
            <div
              className={cn(
                'rounded-md border px-3 py-2 text-sm',
                message.type === 'error'
                  ? 'border-destructive/20 bg-destructive/10 text-destructive'
                  : 'border-green-500/20 bg-green-500/10 text-green-700',
              )}
            >
              {message.text}
            </div>
          ) : null}

          {isRollbackMode ? (
            <div className="flex items-start gap-3 rounded-md border border-amber-200 bg-amber-50 p-3">
              <RotateCcw className="mt-0.5 h-4 w-4 text-amber-600" />
              <div className="space-y-1">
                <p className="text-sm font-semibold text-amber-800">
                  {t ? t('rollbackModeTitle', 'History version loaded.') : 'History version loaded.'}
                </p>
                <p className="text-xs text-amber-700">
                  {t ? t('rollbackModeDesc', 'Review this version before saving to apply the rollback.') : 'Review this version before saving to apply the rollback.'}
                </p>
              </div>
              <button
                type="button"
                onClick={onCancelRollback}
                className="ml-auto text-xs font-medium text-amber-800 hover:underline"
              >
                {t ? t('cancel', 'Cancel') : 'Cancel'}
              </button>
            </div>
          ) : null}

          {importedInactiveNotice ? (
            <div className="rounded-lg border-2 border-amber-500/70 bg-amber-100/80 px-4 py-3 shadow-sm">
              <div className="flex items-start gap-2.5">
                <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-800" />
                <div>
                  <p className="text-sm font-extrabold uppercase tracking-wide text-amber-900">
                    {t ? t('importedButInactiveTitle', 'Imported but inactive') : 'Imported but inactive'}
                  </p>
                  <p className="mt-1 text-sm font-medium text-amber-900/90">{importedInactiveNotice}</p>
                </div>
              </div>
            </div>
          ) : null}

          <section>
            <div className="acc-section-head">
              <Settings2 />
              <h5>{t ? t('basicInfo', 'Basic Info') : 'Basic Info'}</h5>
            </div>
            <div className="field-grid col-2">
              <div className="field">
                <label className="required">{t ? t('providerName', 'Service Provider Name') : 'Service Provider Name'}</label>
                <input disabled={saving} value={provider?.name || ''} onChange={(e) => onChange({ name: e.target.value })} />
              </div>
              <div className="field">
                <label className="required">{providerIdentifierLabel}</label>
                {isOpenCode ? (
                  <input
                    disabled={saving}
                    value={provider?.provider_key || ''}
                    onChange={(e) => onChange({ provider_key: e.target.value.replace(/[^a-zA-Z0-9]/g, '') })}
                  />
                ) : (
                  <input
                    disabled={saving}
                    value={provider?.code || ''}
                    onChange={(e) => onChange({ code: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, '') })}
                  />
                )}
              </div>
              <div className="field full-span">
                <label>{t ? t('providerRemark', 'Service Provider Remark') : 'Service Provider Remark'}</label>
                <textarea
                  disabled={saving}
                  rows={3}
                  value={provider?.remark || ''}
                  onChange={(e) => onChange({ remark: e.target.value })}
                />
              </div>
              <div className="field full-span">
                <label className="required">{t ? t('apiKey', 'API Key') : 'API Key'}</label>
                {(isOpenCode || isCodex || isClaude) ? (
                  <>
                    <div className="relative">
                      <input
                        className="pr-10"
                        disabled={saving}
                        type="text"
                        value={provider?.api_key || ''}
                        onChange={(e) => onChange({ api_key: e.target.value })}
                      />
                      <button
                        type="button"
                        className="absolute right-1 top-1/2 inline-flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                        onClick={() => void handleApiKeyCopy()}
                        disabled={saving || !provider?.api_key}
                        title={apiKeyCopied
                          ? (t ? t('apiKeyCopied', 'API Key copied') : 'API Key copied')
                          : (t ? t('copyApiKey', 'Copy API Key') : 'Copy API Key')}
                        aria-label={apiKeyCopied
                          ? (t ? t('apiKeyCopied', 'API Key copied') : 'API Key copied')
                          : (t ? t('copyApiKey', 'Copy API Key') : 'Copy API Key')}
                      >
                        {apiKeyCopied ? <Check className="h-4 w-4 text-green-600" /> : <Copy className="h-4 w-4" />}
                      </button>
                    </div>
                    {apiKeyCopyError ? <p role="alert" className="text-xs text-destructive">{apiKeyCopyError}</p> : null}
                  </>
                ) : (
                  <input disabled={saving} type="text" value={provider?.api_key || ''} onChange={(e) => onChange({ api_key: e.target.value })} />
                )}
              </div>
              <div className="field full-span">
                <label className="required">{t ? t('baseUrl', 'Base URL') : 'Base URL'}</label>
                <input disabled={saving} value={provider?.base_url || ''} onChange={(e) => onChange({ base_url: e.target.value })} />
              </div>
              {!isClaude && !isOpenCode ? (
                <div className="field full-span">
                  <label>{t ? t('model', 'Primary Model') : 'Primary Model'}</label>
                  <input disabled={saving} value={provider?.model || ''} onChange={(e) => onChange({ model: e.target.value })} />
                </div>
              ) : null}
            </div>
          </section>

          {!isOpenCode ? <section>
            <div className="acc-section-head">
              <KeyRound />
              <h5>{t ? t('toolSpecificConfig', 'Tool Specific Config') : 'Tool Specific Config'}</h5>
            </div>

            {isClaude ? (
              <ClaudeSettingsSection
                provider={provider}
                onChange={onChange}
                saving={saving}
                t={t}
                connectionMode={connectionMode}
                apiFormat={apiFormat}
                fetchingModels={fetchingModels}
                fetchModelsError={fetchModelsError}
                handleFetchModels={handleFetchModels}
                handleMappingChange={handleMappingChange}
              />
            ) : null}

            {isCodex ? (
              <div className="field-grid col-2">
                <label className="checkbox-row info mb-0 full-span">
                  <input
                    disabled={saving}
                    type="checkbox"
                    checked={!!provider?.disable_response_storage}
                    onChange={(e) => onChange({ disable_response_storage: e.target.checked })}
                  />
                  <span>
                    <span className="label">{t ? t('disableResponseStorage', 'Disable Response Storage') : 'Disable Response Storage'}</span>
                    <span className="desc">{t ? t('disableResponseStorageDesc', 'Do not store responses locally for privacy.') : 'Do not store responses locally for privacy.'}</span>
                  </span>
                </label>
                <div className="field">
                  <label>{t ? t('personality', 'Personality') : 'Personality'}</label>
                  <select disabled={saving} value={provider?.personality || ''} onChange={(e) => onChange({ personality: e.target.value || undefined })}>
                    <option value="">{t ? t('personalityDefault', 'Default') : 'Default'}</option>
                    <option value="pragmatic">{t ? t('personalityPragmatic', 'Pragmatic') : 'Pragmatic'}</option>
                    <option value="chatty">{t ? t('personalityChatty', 'Chatty') : 'Chatty'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('wireApi', 'Wire API Format') : 'Wire API Format'}</label>
                  <select disabled={saving} value={provider?.wire_api || ''} onChange={(e) => onChange({ wire_api: e.target.value || undefined })}>
                    <option value="">{t ? t('wireApiDefault', 'Default') : 'Default'}</option>
                    <option value="chat">{t ? t('wireApiChat', 'Chat (Legacy)') : 'Chat (Legacy)'}</option>
                    <option value="responses">{t ? t('wireApiResponses', 'Responses (New)') : 'Responses (New)'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('reasoningEffort', 'Reasoning Effort') : 'Reasoning Effort'}</label>
                  <select disabled={saving} value={provider?.model_reasoning_effort || ''} onChange={(e) => onChange({ model_reasoning_effort: e.target.value || undefined })}>
                    <option value="">{t ? t('reasoningEffortDefault', 'Default') : 'Default'}</option>
                    <option value="minimal">{t ? t('reasoningEffortMinimal', 'Minimal') : 'Minimal'}</option>
                    <option value="low">{t ? t('reasoningEffortLow', 'Low') : 'Low'}</option>
                    <option value="medium">{t ? t('reasoningEffortMedium', 'Medium') : 'Medium'}</option>
                    <option value="high">{t ? t('reasoningEffortHigh', 'High') : 'High'}</option>
                    <option value="xhigh">{t ? t('reasoningEffortXHigh', 'XHigh') : 'XHigh'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('reasoningSummary', 'Reasoning Summary') : 'Reasoning Summary'}</label>
                  <select disabled={saving} value={provider?.model_reasoning_summary || ''} onChange={(e) => onChange({ model_reasoning_summary: e.target.value || undefined })}>
                    <option value="">{t ? t('reasoningSummaryAuto', 'Auto') : 'Auto'}</option>
                    <option value="concise">{t ? t('reasoningSummaryConcise', 'Concise') : 'Concise'}</option>
                    <option value="detailed">{t ? t('reasoningSummaryDetailed', 'Detailed') : 'Detailed'}</option>
                    <option value="none">{t ? t('reasoningSummaryNone', 'None') : 'None'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('approvalPolicy', 'Approval Policy') : 'Approval Policy'}</label>
                  <select disabled={saving} value={provider?.approval_policy || ''} onChange={(e) => onChange({ approval_policy: e.target.value || undefined })}>
                    <option value="">{t ? t('approvalPolicyDefault', 'Default') : 'Default'}</option>
                    <option value="untrusted">{t ? t('approvalPolicyUntrusted', 'Untrusted') : 'Untrusted'}</option>
                    <option value="on-failure">{t ? t('approvalPolicyOnFailure', 'On Failure') : 'On Failure'}</option>
                    <option value="on-request">{t ? t('approvalPolicyOnRequest', 'On Request') : 'On Request'}</option>
                    <option value="never">{t ? t('approvalPolicyNever', 'Never') : 'Never'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('sandboxMode', 'Sandbox Mode') : 'Sandbox Mode'}</label>
                  <select disabled={saving} value={provider?.sandbox_mode || ''} onChange={(e) => onChange({ sandbox_mode: e.target.value || undefined })}>
                    <option value="">{t ? t('sandboxModeDefault', 'Default') : 'Default'}</option>
                    <option value="read-only">{t ? t('sandboxModeReadOnly', 'Read Only') : 'Read Only'}</option>
                    <option value="workspace-write">{t ? t('sandboxModeWorkspaceWrite', 'Workspace Write') : 'Workspace Write'}</option>
                  </select>
                </div>
              </div>
            ) : null}

            {isAntigravity ? (
              <div className="field-grid col-2">
                <div className="field">
                  <label>{t ? t('antigravityAuthType', 'Antigravity Auth Type') : 'Antigravity Auth Type'}</label>
                  <select disabled={saving} value={provider?.antigravity_auth_type || ''} onChange={(e) => onChange({ antigravity_auth_type: e.target.value || undefined })}>
                    <option value="">{t ? t('antigravityAuthDefault', 'Default') : 'Default'}</option>
                    <option value="gemini-api-key">{t ? t('antigravityAuthApiKey', 'API Key') : 'API Key'}</option>
                    <option value="oauth-personal">{t ? t('antigravityAuthOAuth', 'OAuth Personal') : 'OAuth Personal'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('theme', 'Theme') : 'Theme'}</label>
                  <select disabled={saving} value={provider?.theme || ''} onChange={(e) => onChange({ theme: e.target.value || undefined })}>
                    <option value="">{t ? t('themeDefault', 'Default') : 'Default'}</option>
                    <option value="Default">{t ? t('themeDefault', 'Default') : 'Default'}</option>
                    <option value="GitHub Dark">{t ? t('themeGitHubDark', 'GitHub Dark') : 'GitHub Dark'}</option>
                    <option value="Light">{t ? t('themeLight', 'Light') : 'Light'}</option>
                  </select>
                </div>
                <div className="field">
                  <label>{t ? t('defaultApprovalMode', 'Default Approval Mode') : 'Default Approval Mode'}</label>
                  <select disabled={saving} value={provider?.default_approval_mode || ''} onChange={(e) => onChange({ default_approval_mode: e.target.value || undefined })}>
                    <option value="">{t ? t('defaultApprovalModeDefault', 'Default') : 'Default'}</option>
                    <option value="auto_edit">{t ? t('defaultApprovalModeAutoEdit', 'Auto Edit') : 'Auto Edit'}</option>
                    <option value="plan">{t ? t('defaultApprovalModePlan', 'Plan') : 'Plan'}</option>
                  </select>
                </div>
                <label className="checkbox-row info mb-0">
                  <input
                    disabled={saving}
                    type="checkbox"
                    checked={!!provider?.vim_mode}
                    onChange={(e) => onChange({ vim_mode: e.target.checked })}
                  />
                  <span>
                    <span className="label">{t ? t('vimMode', 'Vim Mode') : 'Vim Mode'}</span>
                    <span className="desc">{t ? t('vimModeDesc', 'Enable Vim keybindings.') : 'Enable Vim keybindings.'}</span>
                  </span>
                </label>
              </div>
            ) : null}

          </section> : null}

          {(isCodex || isAntigravity) ? (
            <section>
              <div className="acc-section-head">
                <Settings2 />
                <h5>{t ? t('managedConfig', 'Managed Config') : 'Managed Config'}</h5>
              </div>
              <label className="checkbox-row info mb-0">
                <input
                  disabled={saving}
                  type="checkbox"
                  checked={provider?.env_managed !== false}
                  onChange={(e) => onChange({ env_managed: e.target.checked })}
                />
                <span>
                  <span className="label">{t ? t('envManagedToggle', 'Enable Managed Config') : 'Enable Managed Config'}</span>
                  <span className="desc">
                    {provider?.env_managed !== false
                      ? (t ? t('envManagedEnabledDesc', 'Applying this provider will also update the CLI config automatically.') : 'Applying this provider will also update the CLI config automatically.')
                      : (t ? t('envManagedDisabledDesc', 'Managed config is disabled. CLI config must be maintained manually.') : 'Managed config is disabled. CLI config must be maintained manually.')}
                  </span>
                </span>
              </label>
            </section>
          ) : null}

          {isOpenCode && openCodeModelForm && onOpenCodeModelFormChange ? (
            <section>
              <div className="acc-section-head">
                <Settings2 />
                <h5>{t ? t('openCodeModelList', 'Model list') : 'Model list'}</h5>
              </div>
              <OpenCodeModelForm
                value={openCodeModelForm}
                errors={openCodeModelErrors}
                frozen={openCodeModelFrozen}
                frozenReason={openCodeModelFrozen ? jsonError : null}
                saving={saving}
                onChange={onOpenCodeModelFormChange}
                t={t}
              />
            </section>
          ) : null}

          <section>
            <div className="acc-section-head">
              <Settings2 />
              <h5>{t ? t('configurationJson', 'Configuration JSON') : 'Configuration JSON'}</h5>
            </div>

            {effectiveJsonMode === 'claude' ? (
              <>
                <ConfigJsonEditor
                  value={claudeJsonDraft}
                  onChange={handleClaudeJsonChange}
                  onError={setClaudeJsonError}
                  t={t ? (key, fallback) => t(key, fallback) : undefined}
                />
                {claudeJsonError ? <p className="mt-2 text-xs text-destructive">{claudeJsonError}</p> : null}
              </>
            ) : null}

            {effectiveJsonMode === 'generic' ? (
              <ConfigJsonEditor
                value={settingsJson}
                onChange={(value) => onJsonChange?.(value)}
                onError={(error) => onJsonError?.(error)}
                t={t ? (key, fallback) => t(key, fallback) : undefined}
              />
            ) : null}

            {effectiveJsonMode === 'opencode' ? (
              <OpenCodeJsonPanel
                value={settingsJson}
                jsonError={jsonError}
                isRollbackMode={isRollbackMode}
                onChange={onJsonChange}
                onFormat={onFormatJson}
                t={t}
              />
            ) : null}
          </section>
        </div>
      </div>
    </div>
  );
}
