import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Check, ChevronDown, KeyRound, Loader2, Pencil, Save, Settings2, Trash2, Zap } from 'lucide-react';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';
import { ConfigJsonEditor } from './ConfigJsonEditor';
import { ModelMappingTable } from './ModelMappingTable';
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';
import { BuiltinProviderIcon, isBuiltinProviderIcon, type BuiltinProviderIconKey } from './icons';

interface ClaudeModelMapping {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
}

interface ServiceProviderDetailProps {
  provider: any;
  onChange: (changes: Partial<any>) => void;
  onSave: () => void;
  onActivate: () => void;
  onDelete: () => void;
  onBack: () => void;
  isActive?: boolean;
  t?: (key: string, fallback: string) => string;
  onSaveAndMaterialize?: () => void;
  onFetchModels?: (provider: any) => Promise<string[]>;
}

const ICON_OPTIONS = [
  { value: 'builtin:claude', label: 'Claude' },
  { value: 'builtin:chatgpt', label: 'ChatGPT' },
  { value: 'builtin:gemini', label: 'Gemini' },
  { value: 'builtin:opencode', label: 'OpenCode' },
  { value: 'builtin:bailian', label: '阿里百炼' },
  { value: 'builtin:tencent', label: '腾讯' },
  { value: 'builtin:baidu', label: '百度' },
  { value: 'builtin:volcengine', label: '火山引擎' },
  { value: 'builtin:doubao', label: '豆包' },
  { value: 'builtin:deepseek', label: 'DeepSeek' },
  { value: 'builtin:zhipu', label: '智谱' },
  { value: 'builtin:kimi', label: 'Kimi' },
  { value: 'builtin:minimax', label: 'MiniMax' },
  { value: 'builtin:stepfun', label: '阶跃星辰' },
  { value: 'builtin:xfyun', label: '讯飞星火' },
  { value: 'builtin:sensenova', label: '商汤日日新' },
  { value: 'builtin:lingyi', label: '零一万物' },
] as const;
const AUTH_ENV_OPTIONS = ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'];
const CLAUDE_TOGGLE_FIELDS = [
  { key: 'claude_enable_tool_search', labelKey: 'enableToolSearch', fallback: 'Enable Tool Search' },
  { key: 'claude_auto_memory_enabled', labelKey: 'enableAutoMemory', fallback: 'Enable Auto Memory' },
  { key: 'claude_always_thinking_enabled', labelKey: 'alwaysEnableExtendedThinking', fallback: 'Always Enable Extended Thinking' },
  { key: 'claude_away_summary_enabled', labelKey: 'showAwaySummary', fallback: 'Show session summary after returning to terminal' },
  { key: 'claude_include_git_instructions', labelKey: 'includeGitInstructions', fallback: 'Include Git commit / PR instructions' },
] as const;

const modelEnvKeyByFamily: Record<string, string> = {
  haiku: 'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  sonnet: 'ANTHROPIC_DEFAULT_SONNET_MODEL',
  opus: 'ANTHROPIC_DEFAULT_OPUS_MODEL',
};

const modelNameEnvKeyByFamily: Record<string, string> = {
  haiku: 'ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME',
  sonnet: 'ANTHROPIC_DEFAULT_SONNET_MODEL_NAME',
  opus: 'ANTHROPIC_DEFAULT_OPUS_MODEL_NAME',
};

function buildClaudeSettingsJson(provider: any) {
  const env: Record<string, string> = {};
  const apiFormat = provider?.claude_api_format || 'anthropic_messages';
  const authKey = provider?.claude_auth_env_key || 'ANTHROPIC_AUTH_TOKEN';
  const apiKey = provider?.api_key || '';

  if (apiFormat === 'anthropic_messages') {
    env[authKey] = apiKey;
    if (provider?.base_url) {
      env.ANTHROPIC_BASE_URL = provider.base_url;
    }
  } else {
    env.ANTHROPIC_API_KEY = apiKey;
    env.ANTHROPIC_BASE_URL = `http://127.0.0.1:17687/anthropic/service-provider-${provider?.id || '{id}'}/v1`;
  }

  for (const mapping of provider?.claude_model_mappings || []) {
    const modelKey = modelEnvKeyByFamily[mapping.family];
    const nameKey = modelNameEnvKeyByFamily[mapping.family];
    if (!modelKey || !mapping.upstream_model) continue;
    env[modelKey] = mapping.supports_1m && mapping.family !== 'haiku'
      ? `${mapping.upstream_model}${mapping.upstream_model.includes('[1m]') ? '' : '[1m]'}`
      : mapping.upstream_model;
    if (mapping.display_name) {
      env[nameKey] = mapping.display_name;
    }
  }

  if (provider?.claude_enable_tool_search) {
    env.ENABLE_TOOL_SEARCH = 'true';
  }

  const settings: Record<string, any> = {
    env,
    attribution: provider?.claude_enable_attribution ? undefined : { commit: '', pr: '' },
  };

  if (provider?.claude_auto_memory_enabled !== undefined) {
    settings.autoMemoryEnabled = !!provider.claude_auto_memory_enabled;
  }
  if (provider?.claude_always_thinking_enabled !== undefined) {
    settings.alwaysThinkingEnabled = !!provider.claude_always_thinking_enabled;
  }
  if (provider?.claude_away_summary_enabled !== undefined) {
    settings.awaySummaryEnabled = !!provider.claude_away_summary_enabled;
  }
  if (provider?.claude_include_git_instructions !== undefined) {
    settings.includeGitInstructions = !!provider.claude_include_git_instructions;
  }

  return JSON.stringify(settings, null, 2);
}

function IconPicker({
  value,
  onChange,
  t,
  triggerClassName,
  trigger,
}: {
  value?: string;
  onChange: (value?: string) => void;
  t?: (key: string, fallback: string) => string;
  triggerClassName?: string;
  trigger?: React.ReactNode;
}) {
  const selectedLabel = value || (t ? t('iconAuto', 'Auto') : 'Auto');
  const renderPreview = (iconValue?: string, label?: string) => {
    if (iconValue && isBuiltinProviderIcon(iconValue)) {
      return <BuiltinProviderIcon icon={iconValue as BuiltinProviderIconKey} className="h-5 w-5" />;
    }
    return <span className="text-sm font-semibold leading-none">{label || iconValue}</span>;
  };

  return (
    <Dialog>
      <DialogTrigger asChild>
        <button
          type="button"
          className={cn(
            'flex h-10 w-full items-center justify-between rounded-md border border-border bg-background px-3 text-left text-sm text-foreground transition-colors hover:border-foreground/30',
            triggerClassName,
          )}
        >
          {trigger || (
            <>
              <span className="truncate">{selectedLabel}</span>
              <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
            </>
          )}
        </button>
      </DialogTrigger>
      <DialogContent className="max-w-2xl border-slate-200 bg-white p-0 text-slate-900">
        <DialogHeader className="border-b border-slate-200 px-5 py-4 text-left">
          <DialogTitle className="text-base text-slate-900">
            {t ? t('selectIcon', 'Select icon') : 'Select icon'}
          </DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-2 px-5 py-5 sm:grid-cols-3 lg:grid-cols-4">
          <DialogClose asChild>
            <button
              type="button"
              onClick={() => onChange(undefined)}
              className={cn(
                'flex h-16 items-center gap-3 rounded-md border px-3 text-left text-sm transition-colors',
                !value
                  ? 'border-slate-900 bg-slate-50 text-slate-900'
                  : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300 hover:bg-slate-50 hover:text-slate-900',
              )}
            >
              <span className="inline-flex h-9 w-9 items-center justify-center rounded-xl border border-slate-200 bg-gradient-to-b from-white to-slate-50 text-slate-700 shadow-sm">
                A
              </span>
              <span>{t ? t('iconAuto', 'Auto') : 'Auto'}</span>
            </button>
          </DialogClose>
          {ICON_OPTIONS.map((icon) => {
            const selected = value === icon.value;
            return (
              <DialogClose key={icon.value} asChild>
                <button
                  type="button"
                  onClick={() => onChange(icon.value)}
                  className={cn(
                    'flex h-16 items-center gap-3 rounded-md border px-3 text-left text-sm transition-colors',
                    selected
                      ? 'border-slate-900 bg-slate-50 text-slate-900'
                      : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300 hover:bg-slate-50 hover:text-slate-900',
                  )}
                >
                  <span className="inline-flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-slate-200 bg-gradient-to-b from-white to-slate-50 text-slate-700 shadow-sm">
                    {renderPreview(icon.value, icon.label)}
                  </span>
                  <span className="flex min-w-0 flex-1 items-center justify-between gap-2">
                    <span className="truncate">{icon.label}</span>
                    {selected && <Check className="h-3.5 w-3.5 shrink-0 text-slate-900" />}
                  </span>
                </button>
              </DialogClose>
            );
          })}
        </div>
      </DialogContent>
    </Dialog>
  );
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
  onSaveAndMaterialize,
  onFetchModels,
}: ServiceProviderDetailProps) {
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [jsonDraft, setJsonDraft] = useState('');
  const [fetchingModels, setFetchingModels] = useState(false);
  const [fetchModelsError, setFetchModelsError] = useState<string | null>(null);

  const isClaude = provider?.tool === 'claude';
  const apiFormat = provider?.claude_api_format || 'anthropic_messages';

  const settingsJson = useMemo(
    () => (isClaude ? buildClaudeSettingsJson(provider) : JSON.stringify(provider || {}, null, 2)),
    [isClaude, provider],
  );

  useEffect(() => {
    setJsonDraft(settingsJson);
    setJsonError(null);
  }, [settingsJson]);

  const handleJsonChange = useCallback((raw: string) => {
    setJsonDraft(raw);
    try {
      JSON.parse(raw);
      setJsonError(null);
    } catch (e: any) {
      setJsonError(e?.message || (t ? t('invalidJson', 'Invalid JSON') : 'Invalid JSON'));
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

  return (
    <div className="flex h-full flex-col bg-background">
      <div className="shrink-0 border-b bg-card px-5 py-4">
        <div className="flex items-center gap-3">
          <button type="button" className="acc-btn" onClick={onBack} title={t ? t('back', 'Back') : 'Back'}>
            <ArrowLeft className="w-3.5 h-3.5" />
            {t ? t('back', 'Back') : 'Back'}
          </button>
          <IconPicker
            value={provider?.icon}
            onChange={(icon) => onChange({ icon })}
            t={t}
            triggerClassName="h-auto w-auto border-0 bg-transparent p-0 hover:border-0 hover:bg-transparent"
            trigger={(
              <div className="relative">
                <ServiceProviderAvatar icon={provider?.icon} name={provider?.name || ''} id={provider?.id || ''} size={40} />
                <div className="pointer-events-none absolute -bottom-1 -right-1 rounded-full border border-border bg-background p-1 text-muted-foreground">
                  <Pencil className="h-3 w-3" />
                </div>
              </div>
            )}
          />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-base font-semibold">{provider?.name || t?.('newPreset', 'New Service Provider')}</h3>
              {isActive && <span className="badge-pill bg-green-500/10 text-green-700">{t ? t('active', 'Active') : 'Active'}</span>}
            </div>
            <p className="text-xs text-muted-foreground capitalize">{provider?.tool}</p>
          </div>
          <div className="flex items-center gap-2">
            {!isActive && (
              <button type="button" className="acc-panel-btn" onClick={onActivate}>
                <Zap className="w-4 h-4" />
                {t ? t('activateServiceProvider', 'Activate') : 'Activate'}
              </button>
            )}
            {onSaveAndMaterialize && (
              <button type="button" className="acc-panel-btn" onClick={onSaveAndMaterialize}>
                {t ? t('saveAndActivate', 'Save & Activate') : 'Save & Activate'}
              </button>
            )}
            <button type="button" className="acc-panel-btn danger" onClick={onDelete}>
              <Trash2 className="w-4 h-4" />
              {t ? t('delete', 'Delete') : 'Delete'}
            </button>
            <button type="button" className="acc-panel-btn primary" onClick={onSave} disabled={!!jsonError}>
              <Save className="w-4 h-4" />
              {t ? t('save', 'Save') : 'Save'}
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-5">
        <div className="space-y-6">
          <section>
            <div className="acc-section-head">
              <Settings2 />
              <h5>{t ? t('basicInfo', 'Basic Info') : 'Basic Info'}</h5>
            </div>
            <div className="field-grid col-2">
              <div className="field">
                <label className="required">{t ? t('providerName', 'Service Provider Name') : 'Service Provider Name'}</label>
                <input value={provider?.name || ''} onChange={(e) => onChange({ name: e.target.value })} />
              </div>
              <div className="field">
                <label>{t ? t('providerIdentifier', 'Service Provider Identifier') : 'Service Provider Identifier'}</label>
                <input
                  value={provider?.code || ''}
                  onChange={(e) => onChange({ code: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, '') })}
                />
              </div>
              <div className="field full-span">
                <label>{t ? t('providerRemark', 'Service Provider Remark') : 'Service Provider Remark'}</label>
                <textarea
                  rows={3}
                  value={provider?.remark || ''}
                  onChange={(e) => onChange({ remark: e.target.value })}
                />
              </div>
              <div className="field">
                <label>{t ? t('apiKey', 'API Key') : 'API Key'}</label>
                <input type="text" value={provider?.api_key || ''} onChange={(e) => onChange({ api_key: e.target.value })} />
              </div>
              <div className="field">
                <label>{t ? t('baseUrl', 'Base URL') : 'Base URL'}</label>
                <input value={provider?.base_url || ''} onChange={(e) => onChange({ base_url: e.target.value })} />
              </div>
              {!isClaude && (
                <div className="field full-span">
                  <label>{t ? t('model', 'Primary Model') : 'Primary Model'}</label>
                  <input value={provider?.model || ''} onChange={(e) => onChange({ model: e.target.value })} />
                </div>
              )}
            </div>
          </section>

          {isClaude && (
            <section>
              <div className="acc-section-head">
                <KeyRound />
                <h5>{t ? t('authAndEndpoint', 'Authentication & Endpoint') : 'Authentication & Endpoint'}</h5>
              </div>
              <div className="field-grid col-2">
                <div className="field">
                  <label>{t ? t('apiFormat', 'API Format') : 'API Format'}</label>
                  <select value={apiFormat} onChange={(e) => onChange({ claude_api_format: e.target.value })}>
                    <option value="anthropic_messages">{t ? t('anthropicMessagesFormat', 'Anthropic Messages') : 'Anthropic Messages'}</option>
                    <option value="open_ai_chat">{t ? t('openAiChatFormat', 'OpenAI Chat (requires protocol conversion service)') : 'OpenAI Chat (requires protocol conversion service)'}</option>
                    <option value="open_ai_responses">{t ? t('openAiResponsesFormat', 'OpenAI Responses (requires protocol conversion service)') : 'OpenAI Responses (requires protocol conversion service)'}</option>
                  </select>
                </div>
                {apiFormat === 'anthropic_messages' && (
                  <div className="field">
                    <label>{t ? t('authEnvKey', 'Auth Env Key') : 'Auth Env Key'}</label>
                    <select
                      value={provider?.claude_auth_env_key || 'ANTHROPIC_AUTH_TOKEN'}
                      onChange={(e) => onChange({ claude_auth_env_key: e.target.value })}
                    >
                      {AUTH_ENV_OPTIONS.map((key) => <option key={key} value={key}>{key}</option>)}
                    </select>
                  </div>
                )}
              </div>

              <div className="mt-5">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <div className="text-sm font-semibold">{t ? t('modelMappings', 'Model Mappings') : 'Model Mappings'}</div>
                  <button type="button" className="acc-btn" onClick={handleFetchModels} disabled={fetchingModels}>
                    {fetchingModels && <Loader2 className="w-3 h-3 animate-spin" />}
                    {fetchingModels
                      ? (t ? t('fetchingModels', 'Fetching...') : 'Fetching...')
                      : (t ? t('fetchModels', 'Fetch Models') : 'Fetch Models')}
                  </button>
                </div>
                <ModelMappingTable
                  mappings={provider?.claude_model_mappings || []}
                  onChange={handleMappingChange}
                  fetchedModels={provider?.fetched_models}
                  t={t}
                />
                {fetchModelsError && (
                  <p className="mt-2 text-xs text-destructive">
                    {t ? t('fetchModelsFailedHint', 'Model list fetch failed. Check API endpoint, API key, or upstream service status.') : 'Model list fetch failed. Check API endpoint, API key, or upstream service status.'}
                    {' '}
                    {fetchModelsError}
                  </p>
                )}
              </div>

              <div className="mt-5 grid gap-3 md:grid-cols-2">
                {CLAUDE_TOGGLE_FIELDS.map((field) => (
                  <label key={field.key} className="checkbox-row info mb-0">
                    <input
                      type="checkbox"
                      checked={!!provider?.[field.key]}
                      onChange={(e) => onChange({ [field.key]: e.target.checked })}
                    />
                    <span>
                      <span className="label">{t ? t(field.labelKey, field.fallback) : field.fallback}</span>
                    </span>
                  </label>
                ))}
                <label className="checkbox-row info mb-0">
                  <input
                    type="checkbox"
                    checked={!provider?.claude_enable_attribution}
                    onChange={(e) => onChange({ claude_enable_attribution: !e.target.checked })}
                  />
                  <span>
                    <span className="label">{t ? t('hideAttribution', 'Hide AI Attribution') : 'Hide AI Attribution'}</span>
                  </span>
                </label>
              </div>
            </section>
          )}

          {isClaude && (
            <section>
              <div className="acc-section-head">
                <Settings2 />
                <h5>{t ? t('configurationJson', 'Configuration JSON') : 'Configuration JSON'}</h5>
              </div>
              <ConfigJsonEditor value={jsonDraft} onChange={handleJsonChange} onError={setJsonError} t={t} />
              {jsonError && <p className="mt-2 text-xs text-destructive">{jsonError}</p>}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
