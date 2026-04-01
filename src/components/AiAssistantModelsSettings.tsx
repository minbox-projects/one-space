import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertCircle, Bot, CheckCircle2, Globe, Loader2, Plus, Search, Trash2 } from 'lucide-react';
import type {
  AiAssistantModelProfile,
  AiAssistantProvider,
  AiAssistantSettings,
  WebSearchProvider,
} from '@/lib/aiAssistant';
import { assistantModelTest, assistantSearchProviderTest } from '@/lib/aiAssistant';

type AssistantModelsSection = 'providers' | 'profiles' | 'search';

const PROTOCOL_OPTIONS = [
  { value: 'openai-compatible', label: 'OpenAI Compatible' },
  { value: 'anthropic-messages', label: 'Anthropic Messages' },
  { value: 'google-gemini', label: 'Google Gemini' },
];

const SEARCH_PROVIDER_OPTIONS = [
  { value: 'tavily', label: 'Tavily' },
  { value: 'brave', label: 'Brave Search' },
  { value: 'serpapi', label: 'SerpAPI' },
  { value: 'custom-http', label: 'Custom HTTP' },
];

function createProvider(): AiAssistantProvider {
  return {
    id: `provider-${crypto.randomUUID()}`,
    name: 'New Provider',
    protocol: 'openai-compatible',
    base_url: '',
    auth_scheme: 'bearer',
    api_key: '',
    enabled: true,
    extra_headers: [],
    capabilities: {
      supports_reasoning: true,
      supports_streaming: true,
      supports_web_search: false,
    },
  };
}

function createProfile(providerId?: string): AiAssistantModelProfile {
  return {
    id: `profile-${crypto.randomUUID()}`,
    name: 'new-profile',
    provider_id: providerId || '',
    model_id: '',
    usage: 'chat',
    temperature: 0.3,
    max_tokens: 2048,
    enable_reasoning: true,
  };
}

function createSearchProvider(): WebSearchProvider {
  return {
    id: `search-${crypto.randomUUID()}`,
    name: 'New Search Provider',
    provider_type: 'tavily',
    base_url: '',
    api_key: '',
    enabled: false,
    timeout_secs: 8,
    max_results: 5,
  };
}

export function AiAssistantModelsSettings({
  value,
  onChange,
  section,
  onSectionChange,
}: {
  value: AiAssistantSettings;
  onChange: (next: AiAssistantSettings) => void;
  section: AssistantModelsSection;
  onSectionChange: (next: AssistantModelsSection) => void;
}) {
  const { t } = useTranslation();
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    value.providers[0]?.id || null,
  );
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(
    value.profiles[0]?.id || null,
  );
  const [selectedSearchId, setSelectedSearchId] = useState<string | null>(
    value.search_providers[0]?.id || null,
  );
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [testFeedback, setTestFeedback] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const selectedProvider = useMemo(
    () => value.providers.find((item) => item.id === selectedProviderId) || value.providers[0] || null,
    [selectedProviderId, value.providers],
  );
  const selectedProfile = useMemo(
    () => value.profiles.find((item) => item.id === selectedProfileId) || value.profiles[0] || null,
    [selectedProfileId, value.profiles],
  );
  const selectedSearch = useMemo(
    () =>
      value.search_providers.find((item) => item.id === selectedSearchId) ||
      value.search_providers[0] ||
      null,
    [selectedSearchId, value.search_providers],
  );

  useEffect(() => {
    if (!value.providers.some((provider) => provider.id === selectedProviderId)) {
      setSelectedProviderId(value.providers[0]?.id || null);
    }
  }, [selectedProviderId, value.providers]);

  useEffect(() => {
    if (!value.profiles.some((profile) => profile.id === selectedProfileId)) {
      setSelectedProfileId(value.profiles[0]?.id || null);
    }
  }, [selectedProfileId, value.profiles]);

  useEffect(() => {
    if (!value.search_providers.some((provider) => provider.id === selectedSearchId)) {
      setSelectedSearchId(value.search_providers[0]?.id || null);
    }
  }, [selectedSearchId, value.search_providers]);

  const updateProvider = (providerId: string, patch: Partial<AiAssistantProvider>) => {
    onChange({
      ...value,
      providers: value.providers.map((provider) =>
        provider.id === providerId ? { ...provider, ...patch } : provider,
      ),
    });
  };

  const updateProfile = (profileId: string, patch: Partial<AiAssistantModelProfile>) => {
    onChange({
      ...value,
      profiles: value.profiles.map((profile) =>
        profile.id === profileId ? { ...profile, ...patch } : profile,
      ),
    });
  };

  const updateSearchProvider = (providerId: string, patch: Partial<WebSearchProvider>) => {
    onChange({
      ...value,
      search_providers: value.search_providers.map((provider) =>
        provider.id === providerId ? { ...provider, ...patch } : provider,
      ),
    });
  };

  const removeProvider = (providerId: string) => {
    const nextProviders = value.providers.filter((provider) => provider.id !== providerId);
    const nextProfiles = value.profiles.map((profile) =>
      profile.provider_id === providerId ? { ...profile, provider_id: '' } : profile,
    );
    onChange({
      ...value,
      providers: nextProviders,
      profiles: nextProfiles,
      default_chat_profile_id:
        value.default_chat_profile_id && nextProfiles.some((profile) => profile.id === value.default_chat_profile_id)
          ? value.default_chat_profile_id
          : null,
      default_agent_profile_id:
        value.default_agent_profile_id &&
        nextProfiles.some((profile) => profile.id === value.default_agent_profile_id)
          ? value.default_agent_profile_id
          : null,
      default_summary_profile_id:
        value.default_summary_profile_id &&
        nextProfiles.some((profile) => profile.id === value.default_summary_profile_id)
          ? value.default_summary_profile_id
          : null,
    });
    setSelectedProviderId(nextProviders[0]?.id || null);
  };

  const removeProfile = (profileId: string) => {
    const nextProfiles = value.profiles.filter((profile) => profile.id !== profileId);
    onChange({
      ...value,
      profiles: nextProfiles,
      default_chat_profile_id:
        value.default_chat_profile_id === profileId ? null : value.default_chat_profile_id,
      default_agent_profile_id:
        value.default_agent_profile_id === profileId ? null : value.default_agent_profile_id,
      default_summary_profile_id:
        value.default_summary_profile_id === profileId ? null : value.default_summary_profile_id,
    });
    setSelectedProfileId(nextProfiles[0]?.id || null);
  };

  const removeSearchProvider = (providerId: string) => {
    const nextProviders = value.search_providers.filter((provider) => provider.id !== providerId);
    onChange({
      ...value,
      search_providers: nextProviders,
      active_search_provider_id:
        value.active_search_provider_id === providerId ? null : value.active_search_provider_id,
    });
    setSelectedSearchId(nextProviders[0]?.id || null);
  };

  const runModelConnectionTest = async () => {
    if (!selectedProfile?.id) return;
    setTestingKey(`profile:${selectedProfile.id}`);
    setTestFeedback(null);
    try {
      const result = await assistantModelTest({ profile_id: selectedProfile.id });
      setTestFeedback({
        type: 'success',
        text: `${result.message} (${result.latency_ms}ms)`,
      });
    } catch (error: any) {
      setTestFeedback({
        type: 'error',
        text: error?.toString?.() || String(error),
      });
    } finally {
      setTestingKey(null);
    }
  };

  const runSearchConnectionTest = async () => {
    if (!selectedSearch?.id) return;
    setTestingKey(`search:${selectedSearch.id}`);
    setTestFeedback(null);
    try {
      const result = await assistantSearchProviderTest({ provider_id: selectedSearch.id });
      setTestFeedback({
        type: 'success',
        text: `${result.message} (${result.latency_ms}ms)`,
      });
    } catch (error: any) {
      setTestFeedback({
        type: 'error',
        text: error?.toString?.() || String(error),
      });
    } finally {
      setTestingKey(null);
    }
  };

  return (
    <div className="p-6">
      <div className="grid gap-6 lg:grid-cols-[220px,minmax(0,1fr)]">
        <div className="rounded-2xl border bg-card p-3 space-y-2">
          {[
            { id: 'providers' as const, label: t('assistantModelProviders', '模型供应商'), icon: Globe },
            { id: 'profiles' as const, label: t('assistantModelProfiles', '模型配置'), icon: Bot },
            { id: 'search' as const, label: t('assistantWebSearch', '联网搜索'), icon: Search },
          ].map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onSectionChange(item.id)}
              className={`w-full rounded-xl px-4 py-3 flex items-center gap-3 text-sm transition-colors ${
                section === item.id
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:bg-muted hover:text-foreground'
              }`}
            >
              <item.icon className="w-4 h-4" />
              <span>{item.label}</span>
            </button>
          ))}
        </div>

        <div className="rounded-2xl border bg-card overflow-hidden">
          {section === 'providers' && (
            <div className="grid min-h-[620px] lg:grid-cols-[260px,minmax(0,1fr)]">
              <div className="border-r bg-muted/10 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-semibold">{t('assistantModelProviders', '模型供应商')}</h3>
                  <button
                    type="button"
                    onClick={() => {
                      const created = createProvider();
                      onChange({ ...value, providers: [...value.providers, created] });
                      setSelectedProviderId(created.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs hover:bg-muted"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    {t('add', 'Add')}
                  </button>
                </div>

                <div className="space-y-2">
                  {value.providers.map((provider) => (
                    <button
                      key={provider.id}
                      type="button"
                      onClick={() => setSelectedProviderId(provider.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left transition-colors ${
                        selectedProvider?.id === provider.id
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:bg-muted/40'
                      }`}
                    >
                      <div className="text-sm font-medium">{provider.name || provider.id}</div>
                      <div className="mt-1 text-xs text-muted-foreground">{provider.protocol}</div>
                    </button>
                  ))}
                </div>
              </div>

              <div className="p-6">
                {selectedProvider ? (
                  <div className="space-y-5">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <h2 className="text-lg font-semibold">{selectedProvider.name || 'Provider'}</h2>
                        <p className="text-sm text-muted-foreground">
                          {t(
                            'assistantProviderDesc',
                            'Configure model API protocol, base URL, authentication, and capability hints.',
                          )}
                        </p>
                      </div>
                      <button
                        type="button"
                        onClick={() => removeProvider(selectedProvider.id)}
                        className="inline-flex items-center gap-2 rounded-md border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5"
                      >
                        <Trash2 className="w-4 h-4" />
                        {t('delete', 'Delete')}
                      </button>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('name', 'Name')}</span>
                        <input
                          value={selectedProvider.name}
                          onChange={(e) => updateProvider(selectedProvider.id, { name: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">ID</span>
                        <input
                          value={selectedProvider.id}
                          readOnly
                          className="w-full rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground"
                        />
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('type', 'Type')}</span>
                        <select
                          value={selectedProvider.protocol}
                          onChange={(e) => updateProvider(selectedProvider.id, { protocol: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        >
                          {PROTOCOL_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('apiKey', 'API Key')}</span>
                        <input
                          value={selectedProvider.api_key}
                          onChange={(e) => updateProvider(selectedProvider.id, { api_key: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                          placeholder="sk-..."
                        />
                      </label>
                    </div>

                    <label className="space-y-2 block">
                      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('baseUrl', 'Base URL')}</span>
                      <input
                        value={selectedProvider.base_url}
                        onChange={(e) => updateProvider(selectedProvider.id, { base_url: e.target.value })}
                        className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                      />
                    </label>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('enabled', 'Enabled')}</div>
                          <div className="text-xs text-muted-foreground">{t('assistantProviderEnabledDesc', 'Allow this provider to be used by AI Assistant runtime.')}</div>
                        </div>
                        <input
                          type="checkbox"
                          checked={selectedProvider.enabled}
                          onChange={(e) => updateProvider(selectedProvider.id, { enabled: e.target.checked })}
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('reasoningSummary', 'Reasoning')}</div>
                          <div className="text-xs text-muted-foreground">{t('assistantProviderReasoningDesc', 'Hint that this provider can return reasoning or thinking content.')}</div>
                        </div>
                        <input
                          type="checkbox"
                          checked={selectedProvider.capabilities.supports_reasoning}
                          onChange={(e) =>
                            updateProvider(selectedProvider.id, {
                              capabilities: {
                                ...selectedProvider.capabilities,
                                supports_reasoning: e.target.checked,
                              },
                            })
                          }
                        />
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('streaming', 'Streaming')}</div>
                          <div className="text-xs text-muted-foreground">
                            {t('assistantProviderStreamingDesc', 'Hint that this provider supports incremental streaming responses.')}
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          checked={selectedProvider.capabilities.supports_streaming}
                          onChange={(e) =>
                            updateProvider(selectedProvider.id, {
                              capabilities: {
                                ...selectedProvider.capabilities,
                                supports_streaming: e.target.checked,
                              },
                            })
                          }
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('assistantWebSearch', '联网搜索')}</div>
                          <div className="text-xs text-muted-foreground">
                            {t('assistantProviderSearchDesc', 'Hint that this provider can cooperate with search-enriched prompts.')}
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          checked={selectedProvider.capabilities.supports_web_search}
                          onChange={(e) =>
                            updateProvider(selectedProvider.id, {
                              capabilities: {
                                ...selectedProvider.capabilities,
                                supports_web_search: e.target.checked,
                              },
                            })
                          }
                        />
                      </label>
                    </div>
                  </div>
                ) : (
                  <div className="p-6 text-sm text-muted-foreground">{t('noData', 'No data')}</div>
                )}
              </div>
            </div>
          )}

          {section === 'profiles' && (
            <div className="grid min-h-[620px] lg:grid-cols-[260px,minmax(0,1fr)]">
              <div className="border-r bg-muted/10 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-semibold">{t('assistantModelProfiles', '模型配置')}</h3>
                  <button
                    type="button"
                    onClick={() => {
                      const created = createProfile(value.providers[0]?.id);
                      onChange({ ...value, profiles: [...value.profiles, created] });
                      setSelectedProfileId(created.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs hover:bg-muted"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    {t('add', 'Add')}
                  </button>
                </div>

                <div className="space-y-2">
                  {value.profiles.map((profile) => (
                    <button
                      key={profile.id}
                      type="button"
                      onClick={() => setSelectedProfileId(profile.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left transition-colors ${
                        selectedProfile?.id === profile.id
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:bg-muted/40'
                      }`}
                    >
                      <div className="text-sm font-medium">{profile.name}</div>
                      <div className="mt-1 text-xs text-muted-foreground">{profile.model_id || profile.usage}</div>
                    </button>
                  ))}
                </div>
              </div>

              <div className="p-6">
                {selectedProfile ? (
                  <div className="space-y-5">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <h2 className="text-lg font-semibold">{selectedProfile.name}</h2>
                        <p className="text-sm text-muted-foreground">
                          {t('assistantProfileDesc', 'Bind a model and runtime defaults for chat, agent, or summary use cases.')}
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => void runModelConnectionTest()}
                          disabled={testingKey === `profile:${selectedProfile.id}`}
                          className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                        >
                          {testingKey === `profile:${selectedProfile.id}` ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                          ) : (
                            <CheckCircle2 className="w-4 h-4" />
                          )}
                          {t('testConnection', '测试连接')}
                        </button>
                        <button
                          type="button"
                          onClick={() => removeProfile(selectedProfile.id)}
                          className="inline-flex items-center gap-2 rounded-md border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5"
                        >
                          <Trash2 className="w-4 h-4" />
                          {t('delete', 'Delete')}
                        </button>
                      </div>
                    </div>

                    {testFeedback ? (
                      <div
                        className={`rounded-xl border px-4 py-3 text-sm ${
                          testFeedback.type === 'success'
                            ? 'border-emerald-500/20 bg-emerald-500/5 text-emerald-700 dark:text-emerald-400'
                            : 'border-destructive/20 bg-destructive/5 text-destructive'
                        }`}
                      >
                        <div className="flex items-start gap-2">
                          {testFeedback.type === 'success' ? (
                            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
                          ) : (
                            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                          )}
                          <span>{testFeedback.text}</span>
                        </div>
                      </div>
                    ) : null}

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('name', 'Name')}</span>
                        <input
                          value={selectedProfile.name}
                          onChange={(e) => updateProfile(selectedProfile.id, { name: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('type', 'Type')}</span>
                        <select
                          value={selectedProfile.usage}
                          onChange={(e) => updateProfile(selectedProfile.id, { usage: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        >
                          <option value="chat">chat</option>
                          <option value="agent">agent</option>
                          <option value="summary">summary</option>
                        </select>
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('providerName', 'Provider')}</span>
                        <select
                          value={selectedProfile.provider_id}
                          onChange={(e) => updateProfile(selectedProfile.id, { provider_id: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        >
                          <option value="">{t('selectProvider', 'Select provider')}</option>
                          {value.providers.map((provider) => (
                            <option key={provider.id} value={provider.id}>
                              {provider.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('defaultModel', 'Model')}</span>
                        <input
                          value={selectedProfile.model_id}
                          onChange={(e) => updateProfile(selectedProfile.id, { model_id: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                          placeholder="gpt-4.1"
                        />
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Temperature</span>
                        <input
                          type="number"
                          min="0"
                          max="2"
                          step="0.1"
                          value={selectedProfile.temperature ?? 0}
                          onChange={(e) => updateProfile(selectedProfile.id, { temperature: Number(e.target.value) })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Max Tokens</span>
                        <input
                          type="number"
                          min="1"
                          step="1"
                          value={selectedProfile.max_tokens ?? 1024}
                          onChange={(e) => updateProfile(selectedProfile.id, { max_tokens: Number(e.target.value) })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                    </div>

                    <div className="grid gap-3 md:grid-cols-3">
                      {[
                        {
                          key: 'default_chat_profile_id' as const,
                          label: t('assistantDefaultChatProfile', '默认聊天模型'),
                        },
                        {
                          key: 'default_agent_profile_id' as const,
                          label: t('assistantDefaultAgentProfile', '默认 Agent 模型'),
                        },
                        {
                          key: 'default_summary_profile_id' as const,
                          label: t('assistantDefaultSummaryProfile', '默认总结模型'),
                        },
                      ].map((item) => (
                        <label key={item.key} className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3">
                          <span className="text-sm font-medium">{item.label}</span>
                          <input
                            type="radio"
                            name={item.key}
                            checked={value[item.key] === selectedProfile.id}
                            onChange={() =>
                              onChange({
                                ...value,
                                [item.key]: selectedProfile.id,
                              } as AiAssistantSettings)
                            }
                          />
                        </label>
                      ))}
                    </div>
                  </div>
                ) : (
                  <div className="p-6 text-sm text-muted-foreground">{t('noData', 'No data')}</div>
                )}
              </div>
            </div>
          )}

          {section === 'search' && (
            <div className="grid min-h-[620px] lg:grid-cols-[260px,minmax(0,1fr)]">
              <div className="border-r bg-muted/10 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-semibold">{t('assistantWebSearch', '联网搜索')}</h3>
                  <button
                    type="button"
                    onClick={() => {
                      const created = createSearchProvider();
                      onChange({ ...value, search_providers: [...value.search_providers, created] });
                      setSelectedSearchId(created.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs hover:bg-muted"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    {t('add', 'Add')}
                  </button>
                </div>

                <div className="space-y-2">
                  {value.search_providers.map((provider) => (
                    <button
                      key={provider.id}
                      type="button"
                      onClick={() => setSelectedSearchId(provider.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left transition-colors ${
                        selectedSearch?.id === provider.id
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:bg-muted/40'
                      }`}
                    >
                      <div className="text-sm font-medium">{provider.name}</div>
                      <div className="mt-1 text-xs text-muted-foreground">{provider.provider_type}</div>
                    </button>
                  ))}
                </div>
              </div>

              <div className="p-6">
                {selectedSearch ? (
                  <div className="space-y-5">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <h2 className="text-lg font-semibold">{selectedSearch.name}</h2>
                        <p className="text-sm text-muted-foreground">
                          {t('assistantSearchDesc', 'Configure the search provider used when AI Assistant enables web search.')}
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => void runSearchConnectionTest()}
                          disabled={testingKey === `search:${selectedSearch.id}`}
                          className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                        >
                          {testingKey === `search:${selectedSearch.id}` ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                          ) : (
                            <CheckCircle2 className="w-4 h-4" />
                          )}
                          {t('testConnection', '测试连接')}
                        </button>
                        <button
                          type="button"
                          onClick={() => removeSearchProvider(selectedSearch.id)}
                          className="inline-flex items-center gap-2 rounded-md border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5"
                        >
                          <Trash2 className="w-4 h-4" />
                          {t('delete', 'Delete')}
                        </button>
                      </div>
                    </div>

                    {testFeedback ? (
                      <div
                        className={`rounded-xl border px-4 py-3 text-sm ${
                          testFeedback.type === 'success'
                            ? 'border-emerald-500/20 bg-emerald-500/5 text-emerald-700 dark:text-emerald-400'
                            : 'border-destructive/20 bg-destructive/5 text-destructive'
                        }`}
                      >
                        <div className="flex items-start gap-2">
                          {testFeedback.type === 'success' ? (
                            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
                          ) : (
                            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                          )}
                          <span>{testFeedback.text}</span>
                        </div>
                      </div>
                    ) : null}

                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('name', 'Name')}</span>
                        <input
                          value={selectedSearch.name}
                          onChange={(e) => updateSearchProvider(selectedSearch.id, { name: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('type', 'Type')}</span>
                        <select
                          value={selectedSearch.provider_type}
                          onChange={(e) => updateSearchProvider(selectedSearch.id, { provider_type: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        >
                          {SEARCH_PROVIDER_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>

                    <label className="space-y-2 block">
                      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('baseUrl', 'Base URL')}</span>
                      <input
                        value={selectedSearch.base_url || ''}
                        onChange={(e) => updateSearchProvider(selectedSearch.id, { base_url: e.target.value })}
                        className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                      />
                    </label>

                    <div className="grid gap-4 md:grid-cols-3">
                      <label className="space-y-2 md:col-span-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('apiKey', 'API Key')}</span>
                        <input
                          value={selectedSearch.api_key}
                          onChange={(e) => updateSearchProvider(selectedSearch.id, { api_key: e.target.value })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3 mt-6">
                        <span className="text-sm font-medium">{t('enabled', 'Enabled')}</span>
                        <input
                          type="checkbox"
                          checked={selectedSearch.enabled}
                          onChange={(e) => updateSearchProvider(selectedSearch.id, { enabled: e.target.checked })}
                        />
                      </label>
                    </div>

                    <div className="grid gap-4 md:grid-cols-3">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('timeout', 'Timeout')}</span>
                        <input
                          type="number"
                          min="1"
                          step="1"
                          value={selectedSearch.timeout_secs ?? 8}
                          onChange={(e) => updateSearchProvider(selectedSearch.id, { timeout_secs: Number(e.target.value) })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{t('maxResults', 'Max Results')}</span>
                        <input
                          type="number"
                          min="1"
                          step="1"
                          value={selectedSearch.max_results ?? 5}
                          onChange={(e) => updateSearchProvider(selectedSearch.id, { max_results: Number(e.target.value) })}
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3 mt-6">
                        <span className="text-sm font-medium">{t('assistantSearchActive', '设为默认')}</span>
                        <input
                          type="radio"
                          name="assistant-search-active"
                          checked={value.active_search_provider_id === selectedSearch.id}
                          onChange={() => onChange({ ...value, active_search_provider_id: selectedSearch.id })}
                        />
                      </label>
                    </div>
                  </div>
                ) : (
                  <div className="p-6 text-sm text-muted-foreground">{t('noData', 'No data')}</div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
