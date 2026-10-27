import { Loader2 } from 'lucide-react';
import { ModelMappingTable } from '../ModelMappingTable';
import { AUTH_ENV_OPTIONS, CLAUDE_TOGGLE_FIELDS, type ClaudeModelMapping } from './claudeSettings';

interface ClaudeSettingsSectionProps {
  provider: any;
  onChange: (changes: Partial<any>) => void;
  saving: boolean;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
  connectionMode: string;
  apiFormat: string;
  fetchingModels: boolean;
  fetchModelsError: string | null;
  handleFetchModels: () => void;
  handleMappingChange: (mappings: ClaudeModelMapping[]) => void;
}

export function ClaudeSettingsSection({
  provider,
  onChange,
  saving,
  t,
  connectionMode,
  apiFormat,
  fetchingModels,
  fetchModelsError,
  handleFetchModels,
  handleMappingChange,
}: ClaudeSettingsSectionProps) {
  return (
    <div className="space-y-5">
      <div className="field-grid col-2">
        <div className="field">
          <label>{t ? t('connectionMode', 'Connection Mode') : 'Connection Mode'}</label>
          <select
            disabled={saving}
            value={connectionMode}
            onChange={(e) => onChange({
              claude_connection_mode: e.target.value,
              claude_api_format: e.target.value === 'protocol_router' ? 'open_ai_chat' : 'anthropic_messages',
            })}
          >
            <option value="native_anthropic">{t ? t('nativeAnthropicMode', 'Native Anthropic（原生）') : 'Native Anthropic（原生）'}</option>
            <option value="protocol_router">{t ? t('protocolRouterMode', 'Protocol Router（协议路由）') : 'Protocol Router（协议路由）'}</option>
          </select>
        </div>
        <div className="field">
          <label>{t ? t('apiFormat', 'API Format') : 'API Format'}</label>
          <select
            disabled={saving}
            value={apiFormat}
            onChange={(e) => onChange({
              claude_api_format: e.target.value,
              claude_connection_mode: e.target.value === 'anthropic_messages' ? 'native_anthropic' : 'protocol_router',
            })}
          >
            {connectionMode === 'native_anthropic' ? (
              <option value="anthropic_messages">{t ? t('anthropicMessagesFormat', 'Anthropic Messages') : 'Anthropic Messages'}</option>
            ) : null}
            {connectionMode === 'protocol_router' ? (
              <>
                <option value="open_ai_chat">{t ? t('openAiChatFormat', 'OpenAI Chat (requires protocol router, format: API URL + /chat/completions)') : 'OpenAI Chat (requires protocol router, format: API URL + /chat/completions)'}</option>
                <option value="open_ai_responses">{t ? t('openAiResponsesFormat', 'OpenAI Responses (requires protocol router, format: API URL + /responses)') : 'OpenAI Responses (requires protocol router, format: API URL + /responses)'}</option>
              </>
            ) : null}
          </select>
        </div>
        {connectionMode === 'native_anthropic' ? (
          <div className="field">
            <label>{t ? t('authEnvKey', 'Auth Env Key') : 'Auth Env Key'}</label>
            <select
              disabled={saving}
              value={provider?.claude_auth_env_key || 'ANTHROPIC_API_KEY'}
              onChange={(e) => onChange({ claude_auth_env_key: e.target.value })}
            >
              {AUTH_ENV_OPTIONS.map((key) => <option key={key} value={key}>{key}</option>)}
            </select>
          </div>
        ) : null}
      </div>

      <div>
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="text-sm font-semibold">{t ? t('modelMappings', 'Model Mappings') : 'Model Mappings'}</div>
          <button type="button" className="acc-btn" onClick={handleFetchModels} disabled={saving || fetchingModels}>
            {fetchingModels ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
            {fetchingModels
              ? (t ? t('fetchingModels', 'Fetching...') : 'Fetching...')
              : (t ? t('fetchModels', 'Fetch Models') : 'Fetch Models')}
          </button>
        </div>
        <ModelMappingTable
          mappings={provider?.claude_model_mappings || []}
          onChange={handleMappingChange}
          fetchedModels={provider?.fetched_models}
          t={t ? (key, fallback) => t(key, fallback) : undefined}
        />
        {fetchModelsError ? (
          <p className="mt-2 text-xs text-destructive">
            {t ? t('fetchModelsFailedHint', 'Model list fetch failed. Check API endpoint, API key, or upstream service status.') : 'Model list fetch failed. Check API endpoint, API key, or upstream service status.'}
            {' '}
            {fetchModelsError}
          </p>
        ) : null}
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <div className="field">
          <label>{t ? t('defaultModel', 'Default Model') : 'Default Model'}</label>
          <input
            disabled={saving}
            value={provider?.claude_default_model || ''}
            onChange={(e) => onChange({ claude_default_model: e.target.value || undefined })}
          />
        </div>
        <div className="field">
          <label>{t ? t('reasoningEffort', 'Reasoning Effort') : 'Reasoning Effort'}</label>
          <input
            disabled={saving}
            value={provider?.claude_reasoning_effort || ''}
            onChange={(e) => onChange({ claude_reasoning_effort: e.target.value || undefined })}
            placeholder={t ? t('claudeReasoningEffortPlaceholder', 'high / xhigh / max / auto / custom') : 'high / xhigh / max / auto / custom'}
          />
        </div>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        {CLAUDE_TOGGLE_FIELDS.map((field) => (
          <label key={field.key} className="checkbox-row info mb-0">
            <input
              disabled={saving}
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
            disabled={saving}
            type="checkbox"
            checked={!provider?.claude_enable_attribution}
            onChange={(e) => onChange({ claude_enable_attribution: !e.target.checked })}
          />
          <span>
            <span className="label">{t ? t('hideAttribution', 'Hide AI Attribution') : 'Hide AI Attribution'}</span>
          </span>
        </label>
      </div>
    </div>
  );
}
