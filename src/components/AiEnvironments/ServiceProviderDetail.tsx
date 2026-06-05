import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowLeft, KeyRound, Loader2, Save, Settings2, Trash2, Zap } from 'lucide-react';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';
import { ConfigJsonEditor } from './ConfigJsonEditor';
import { ModelMappingTable } from './ModelMappingTable';

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

const ICON_OPTIONS = ['🤖', '🧠', '⚡', '🔧', '🚀', 'AI', 'API', 'LLM'];
const AUTH_ENV_OPTIONS = ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'];

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
  const apiKey = provider?.api_key ? '********' : '';

  if (apiFormat === 'anthropic_messages') {
    env[authKey] = apiKey;
    if (provider?.base_url) {
      env.ANTHROPIC_BASE_URL = provider.base_url;
    }
  } else {
    env.ANTHROPIC_API_KEY = '********';
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

  return JSON.stringify({
    env,
    attribution: provider?.claude_enable_attribution ? undefined : { commit: '', pr: '' },
  }, null, 2);
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
    try {
      const models = await onFetchModels(provider);
      onChange({ fetched_models: models });
      setJsonError(null);
    } catch (e: any) {
      setJsonError(e?.message || (t ? t('fetchModelsFailed', 'Failed to fetch models') : 'Failed to fetch models'));
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
          <ServiceProviderAvatar icon={provider?.icon} name={provider?.name || ''} id={provider?.id || ''} size={40} />
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
                <label>{t ? t('customIcon', 'Icon') : 'Icon'}</label>
                <select value={provider?.icon || ''} onChange={(e) => onChange({ icon: e.target.value || undefined })}>
                  <option value="">{t ? t('iconAuto', 'Auto') : 'Auto'}</option>
                  {ICON_OPTIONS.map((icon) => (
                    <option key={icon} value={icon}>{icon}</option>
                  ))}
                </select>
              </div>
              <div className="field">
                <label>{t ? t('apiKey', 'API Key') : 'API Key'}</label>
                <input type="password" value={provider?.api_key || ''} onChange={(e) => onChange({ api_key: e.target.value })} />
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
                    <option value="open_ai_chat">{t ? t('openAiChatFormat', 'OpenAI Chat') : 'OpenAI Chat'}</option>
                    <option value="open_ai_responses">{t ? t('openAiResponsesFormat', 'OpenAI Responses') : 'OpenAI Responses'}</option>
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
              </div>

              <div className="mt-5 grid gap-3 md:grid-cols-2">
                <label className="checkbox-row info mb-0">
                  <input
                    type="checkbox"
                    checked={provider?.claude_enable_tool_search || false}
                    onChange={(e) => onChange({ claude_enable_tool_search: e.target.checked })}
                  />
                  <span>
                    <span className="label">{t ? t('enableToolSearch', 'Enable Tool Search') : 'Enable Tool Search'}</span>
                  </span>
                </label>
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
