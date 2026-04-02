import { useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  Bot,
  CheckCircle2,
  Globe,
  Layers3,
  Loader2,
  Plus,
  Radar,
  ShieldCheck,
  Sparkles,
  Trash2,
} from 'lucide-react';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import type {
  AiWorkspaceSettings,
  ModelCatalogItem,
  ModelRoleBinding,
  ProviderConnection,
  RuntimePreset,
} from '@/lib/aiWorkspace';
import {
  providerConnectionTest,
  providerModelsFetch,
} from '@/lib/aiWorkspace';

type ConnectionPanel = 'providers' | 'catalog' | 'roles' | 'runtime';

const PROVIDER_PROTOCOL_OPTIONS = [
  { value: 'openai-compatible', label: 'OpenAI Compatible' },
  { value: 'anthropic-messages', label: 'Anthropic Messages' },
  { value: 'google-gemini', label: 'Google Gemini' },
];

const WORKSPACE_ROLE_OPTIONS = [
  'chat',
  'assistant',
  'summary',
  'automation',
  'quick_assistant',
  'selection_assistant',
  'translate',
  'topic_naming',
] as const;

function getWorkspaceRoleLabel(role: string, t: TFunction) {
  switch (role) {
    case 'chat':
      return t('chatLabel', 'Chat');
    case 'assistant':
      return t('assistantLabel', 'Assistant');
    case 'summary':
      return t('summaryLabel', 'Summary');
    case 'automation':
      return t('automationLabel', 'Automation');
    case 'quick_assistant':
      return t('quickAssistant', 'Quick Assistant');
    case 'selection_assistant':
      return t('selectionAssistant', 'Selection Assistant');
    case 'translate':
      return t('translateLabel', 'Translate');
    case 'topic_naming':
      return t('topicNamingLabel', 'Topic Naming');
    default:
      return role;
  }
}

function getWorkspaceRoleDescription(role: string, t: TFunction) {
  switch (role) {
    case 'chat':
      return t('aiConnectionRoleChatDesc', 'Default model for regular conversation.');
    case 'assistant':
      return t('aiConnectionRoleAssistantDesc', 'Primary model for assistant topics and test runs.');
    case 'summary':
      return t('aiConnectionRoleSummaryDesc', 'Lightweight model for summaries and second-pass processing.');
    case 'automation':
      return t('aiConnectionRoleAutomationDesc', 'Default model for background automation jobs.');
    case 'quick_assistant':
      return t('aiConnectionRoleQuickAssistantDesc', 'Default model for the Quick Assistant floating window.');
    case 'selection_assistant':
      return t('aiConnectionRoleSelectionAssistantDesc', 'Reserved binding for the Selection Assistant.');
    case 'translate':
      return t('aiConnectionRoleTranslateDesc', 'Default model for translation tasks.');
    case 'topic_naming':
      return t('aiConnectionRoleTopicNamingDesc', 'Model for topic naming and summary titles.');
    default:
      return '';
  }
}

function createDefaultRuntimePresets(t: TFunction): RuntimePreset[] {
  return [
    {
      id: 'balanced',
      name: t('aiConnectionPresetBalanced', 'Balanced'),
      description: t(
        'aiConnectionPresetBalancedDesc',
        'General-purpose preset for chat, quick assistant, and routine work.',
      ),
      temperature: 0.3,
      max_tokens: 2048,
      enable_reasoning: true,
    },
    {
      id: 'deep_reasoning',
      name: t('aiConnectionPresetDeepReasoning', 'Deep Reasoning'),
      description: t(
        'aiConnectionPresetDeepReasoningDesc',
        'Longer answers and stronger reasoning for assistants and automations.',
      ),
      temperature: 0.2,
      max_tokens: 4096,
      enable_reasoning: true,
    },
    {
      id: 'lightweight',
      name: t('aiConnectionPresetLightweight', 'Lightweight'),
      description: t(
        'aiConnectionPresetLightweightDesc',
        'Fast preset for summaries, translation, and topic naming.',
      ),
      temperature: 0.1,
      max_tokens: 1024,
      enable_reasoning: false,
    },
  ];
}

function createProvider(t: TFunction): ProviderConnection {
  return {
    id: `provider-${crypto.randomUUID()}`,
    name: t('aiConnectionNewProvider', 'New Provider'),
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

function createRuntimePreset(t: TFunction): RuntimePreset {
  return {
    id: `preset-${crypto.randomUUID()}`,
    name: t('aiConnectionCustomPreset', 'Custom Preset'),
    description: t(
      'aiConnectionCustomPresetDesc',
      'Reusable runtime profile for a specific assistant workflow.',
    ),
    temperature: 0.3,
    max_tokens: 2048,
    enable_reasoning: true,
  };
}

function ensureRoleBindings(
  bindings: ModelRoleBinding[],
  settings: AiWorkspaceSettings,
): ModelRoleBinding[] {
  return WORKSPACE_ROLE_OPTIONS.map((role) => {
    const existing = bindings.find((binding) => binding.role === role);
    return (
      existing || {
        id: `role-${role}`,
        role,
        model_id:
          settings.model_catalog.find((catalogItem) => catalogItem.enabled)?.id ||
          settings.model_catalog[0]?.id ||
          null,
        runtime_preset_id:
          role === 'assistant' || role === 'automation' || role === 'selection_assistant'
            ? 'deep_reasoning'
            : role === 'summary' || role === 'translate' || role === 'topic_naming'
              ? 'lightweight'
              : 'balanced',
        temperature: role === 'summary' ? 0.2 : 0.4,
        max_tokens: role === 'summary' ? 2048 : 4096,
        enable_reasoning: role !== 'summary' && role !== 'topic_naming',
      }
    );
  });
}

function ensureRuntimePresets(presets: RuntimePreset[] | undefined, t: TFunction): RuntimePreset[] {
  if (Array.isArray(presets) && presets.length > 0) {
    return presets;
  }
  return createDefaultRuntimePresets(t);
}

function parseNumberInput(value: string) {
  if (!value.trim()) return null;
  const next = Number(value);
  return Number.isFinite(next) ? next : null;
}

function capabilityBadge(label: string) {
  return (
    <span className="rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
      {label}
    </span>
  );
}

export function AiConnectionsSettings({
  value,
  onChange,
  onSave,
  saving = false,
}: {
  value: AiWorkspaceSettings;
  onChange: (next: AiWorkspaceSettings) => void;
  onSave?: () => Promise<void> | void;
  saving?: boolean;
}) {
  const { t } = useTranslation();
  const [panel, setPanel] = useState<ConnectionPanel>('providers');
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(value.providers[0]?.id || null);
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  useEffect(() => {
    if (!value.providers.some((provider) => provider.id === selectedProviderId)) {
      setSelectedProviderId(value.providers[0]?.id || null);
    }
  }, [selectedProviderId, value.providers]);

  const selectedProvider = useMemo(
    () => value.providers.find((provider) => provider.id === selectedProviderId) || value.providers[0] || null,
    [selectedProviderId, value.providers],
  );

  const providerLabels = useMemo(() => {
    return new Map(value.providers.map((provider) => [provider.id, provider.name]));
  }, [value.providers]);

  const enabledCatalog = useMemo(
    () => value.model_catalog.filter((item) => item.enabled),
    [value.model_catalog],
  );

  const effectiveRoleBindings = useMemo(
    () => ensureRoleBindings(value.role_bindings || [], value),
    [value],
  );
  const effectiveRuntimePresets = useMemo(
    () => ensureRuntimePresets(value.runtime_presets, t),
    [t, value.runtime_presets],
  );

  const updateSettings = (patch: Partial<AiWorkspaceSettings>) => {
    onChange({
      ...value,
      ...patch,
    });
  };

  const updateProvider = (providerId: string, patch: Partial<ProviderConnection>) => {
    updateSettings({
      providers: value.providers.map((provider) =>
        provider.id === providerId ? { ...provider, ...patch } : provider,
      ),
    });
  };

  const replaceProviderCatalog = (providerId: string, nextItems: ModelCatalogItem[]) => {
    const remaining = value.model_catalog.filter((item) => item.provider_id !== providerId);
    const nextSettings = {
      ...value,
      model_catalog: [...remaining, ...nextItems].sort((a, b) => {
        if (a.provider_id === b.provider_id) {
          return a.label.localeCompare(b.label);
        }
        return a.provider_id.localeCompare(b.provider_id);
      }),
    };
    onChange({
      ...nextSettings,
      role_bindings: ensureRoleBindings(nextSettings.role_bindings || [], nextSettings),
    });
  };

  const runProviderHealthCheck = async () => {
    if (!selectedProvider?.id) return;
    setTestingKey(`provider:${selectedProvider.id}`);
    setFeedback(null);
    try {
      const result = await providerConnectionTest({ provider_id: selectedProvider.id });
      setFeedback({
        type: 'success',
        text: `${result.message} (${result.latency_ms}ms)`,
      });
    } catch (error: any) {
      setFeedback({
        type: 'error',
        text: error?.toString?.() || String(error),
      });
    } finally {
      setTestingKey(null);
    }
  };

  const refreshProviderCatalog = async () => {
    if (!selectedProvider?.id) return;
    setTestingKey(`catalog:${selectedProvider.id}`);
    setFeedback(null);
    try {
      const catalog = await providerModelsFetch({ provider_id: selectedProvider.id });
      replaceProviderCatalog(selectedProvider.id, catalog);
      setFeedback({
        type: 'success',
        text: t('aiConnectionCatalogRefreshSuccess', '{{name}} refreshed {{count}} catalog items.', {
          name: selectedProvider.name,
          count: catalog.length,
        }),
      });
    } catch (error: any) {
      setFeedback({
        type: 'error',
        text: error?.toString?.() || String(error),
      });
    } finally {
      setTestingKey(null);
    }
  };

  const panels = [
    {
      id: 'providers' as const,
      title: t('aiConnectionPanelProviders', 'Provider Connections'),
      icon: Globe,
      hint: t('aiConnectionPanelProvidersHint', 'Provider vendors, keys, and capability switches'),
    },
    {
      id: 'catalog' as const,
      title: t('aiConnectionPanelCatalog', 'Model Catalog'),
      icon: Layers3,
      hint: t('aiConnectionPanelCatalogHint', 'Automatically discover models, tags, and capabilities'),
    },
    {
      id: 'roles' as const,
      title: t('aiConnectionPanelRoles', 'Role Bindings'),
      icon: Radar,
      hint: t('aiConnectionPanelRolesHint', 'Map roles to models and runtime parameters'),
    },
    {
      id: 'runtime' as const,
      title: t('aiConnectionPanelRuntime', 'Runtime Presets'),
      icon: Sparkles,
      hint: t('aiConnectionPanelRuntimeHint', 'Reusable runtime templates for role bindings'),
    },
  ];

  return (
    <div className="space-y-6 p-6">
      <div className="grid gap-3 md:grid-cols-5">
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
            {t('providersLabel', 'Providers')}
          </div>
          <div className="mt-2 text-2xl font-semibold">{value.providers.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t('aiConnectionProvidersSummary', '{{count}} active providers connected', {
              count: value.providers.filter((item) => item.enabled).length,
            })}
          </div>
        </div>
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
            {t('catalogLabel', 'Catalog')}
          </div>
          <div className="mt-2 text-2xl font-semibold">{value.model_catalog.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t('aiConnectionCatalogSummary', '{{count}} models enabled for role bindings', {
              count: enabledCatalog.length,
            })}
          </div>
        </div>
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
            {t('rolesLabel', 'Roles')}
          </div>
          <div className="mt-2 text-2xl font-semibold">{effectiveRoleBindings.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t(
              'aiConnectionRolesSummary',
              'Covers default roles for chat, assistants, automation, translation, and more.',
            )}
          </div>
        </div>
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
            {t('runtimeLabel', 'Runtime')}
          </div>
          <div className="mt-2 text-2xl font-semibold">{effectiveRuntimePresets.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t('aiConnectionRuntimeSummary', 'Reusable runtime templates for role bindings')}
          </div>
        </div>
      </div>

      <div className="grid gap-6 xl:grid-cols-[260px,minmax(0,1fr)]">
        <div className="space-y-3 rounded-3xl border bg-card/90 p-3">
          {panels.map((item) => {
            const Icon = item.icon;
            const active = panel === item.id;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => setPanel(item.id)}
                className={`w-full rounded-2xl border px-4 py-3 text-left transition-colors ${
                  active ? 'border-primary bg-primary/5' : 'hover:bg-muted/40'
                }`}
              >
                <div className="flex items-center gap-3">
                  <div className={`rounded-xl p-2 ${active ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground'}`}>
                    <Icon className="h-4 w-4" />
                  </div>
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium">{item.title}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{item.hint}</div>
                  </div>
                </div>
              </button>
            );
          })}
        </div>

        <div className="space-y-4 rounded-3xl border bg-card p-5">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <div className="text-lg font-semibold">
                {panel === 'providers' && t('aiConnectionProviderCenterTitle', 'Provider Connection Center')}
                {panel === 'catalog' && t('aiConnectionCatalogCenterTitle', 'Model Catalog')}
                {panel === 'roles' && t('aiConnectionRoleBindingMatrixTitle', 'Role Binding Matrix')}
                {panel === 'runtime' && t('aiConnectionPanelRuntime', 'Runtime Presets')}
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {panel === 'providers' &&
                  t(
                    'aiConnectionProviderCenterDesc',
                    'Manage provider vendors, keys, networking, and catalog fetching here without carrying the old profile form.',
                  )}
                {panel === 'catalog' &&
                  t(
                    'aiConnectionCatalogCenterDesc',
                    'The model catalog is fetched automatically from providers, with support for tags, capabilities, and enablement status.',
                  )}
                {panel === 'roles' &&
                  t(
                    'aiConnectionRoleBindingMatrixDesc',
                    'Each role can bind a default model together with temperature, max tokens, reasoning, and search policies.',
                  )}
                {panel === 'runtime' &&
                  t(
                    'aiConnectionRuntimeCenterDesc',
                    'Runtime presets centralize temperature, max tokens, and reasoning switches so role bindings only need to reference or override them.',
                  )}
              </div>
            </div>
            {onSave ? (
              <button
                type="button"
                onClick={() => void onSave()}
                disabled={saving}
                className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              >
                {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <ShieldCheck className="h-4 w-4" />}
                {t('saveCurrentTab', '保存当前分组')}
              </button>
            ) : null}
          </div>

          {feedback ? (
            <div
              className={`flex items-start gap-3 rounded-2xl border px-4 py-3 text-sm ${
                feedback.type === 'success'
                  ? 'border-emerald-500/30 bg-emerald-500/5 text-emerald-700 dark:text-emerald-300'
                  : 'border-destructive/30 bg-destructive/5 text-destructive'
              }`}
            >
              {feedback.type === 'success' ? (
                <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
              ) : (
                <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
              )}
              <span>{feedback.text}</span>
            </div>
          ) : null}

          {panel === 'providers' ? (
            <div className="grid gap-6 xl:grid-cols-[280px,minmax(0,1fr)]">
              <div className="rounded-2xl border bg-muted/10 p-3">
                <div className="mb-3 flex items-center justify-between">
                  <div className="text-sm font-medium">{t('providersLabel', 'Providers')}</div>
                  <button
                    type="button"
                    onClick={() => {
                      const created = createProvider(t);
                      updateSettings({ providers: [created, ...value.providers] });
                      setSelectedProviderId(created.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs hover:bg-muted"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t('add', 'Add')}
                  </button>
                </div>
                <div className="space-y-2">
                  {value.providers.map((provider) => (
                    <button
                      key={provider.id}
                      type="button"
                      onClick={() => setSelectedProviderId(provider.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left ${
                        selectedProvider?.id === provider.id ? 'border-primary bg-primary/5' : 'hover:bg-background'
                      }`}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium">{provider.name}</div>
                          <div className="mt-1 text-xs text-muted-foreground">{provider.protocol}</div>
                        </div>
                        <span
                          className={`rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.14em] ${
                            provider.enabled ? 'border-primary/30 text-primary' : 'text-muted-foreground'
                          }`}
                        >
                          {provider.enabled ? 'ON' : 'OFF'}
                        </span>
                      </div>
                    </button>
                  ))}
                </div>
              </div>

              {selectedProvider ? (
                <div className="space-y-5">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <div className="rounded-xl bg-primary/10 p-2 text-primary">
                        <Globe className="h-4 w-4" />
                      </div>
                      <div>
                        <div className="text-base font-semibold">{selectedProvider.name}</div>
                        <div className="text-xs text-muted-foreground">
                          {t(
                            'aiConnectionProviderStatusDesc',
                            'Connectivity, authentication, capability flags, and catalog fetching.',
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <button
                        type="button"
                        onClick={() => void runProviderHealthCheck()}
                        disabled={testingKey === `provider:${selectedProvider.id}`}
                        className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                      >
                        {testingKey === `provider:${selectedProvider.id}` ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Sparkles className="h-4 w-4" />
                        )}
                        {t('detectConnection', 'Detect Connection')}
                      </button>
                      <button
                        type="button"
                        onClick={() => void refreshProviderCatalog()}
                        disabled={testingKey === `catalog:${selectedProvider.id}`}
                        className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                      >
                        {testingKey === `catalog:${selectedProvider.id}` ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Layers3 className="h-4 w-4" />
                        )}
                        {t('fetchModelCatalog', 'Fetch Model Catalog')}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          const nextProviders = value.providers.filter((provider) => provider.id !== selectedProvider.id);
                          const nextCatalog = value.model_catalog.filter((item) => item.provider_id !== selectedProvider.id);
                          const nextSettings = {
                            ...value,
                            providers: nextProviders,
                            model_catalog: nextCatalog,
                            role_bindings: ensureRoleBindings(
                              value.role_bindings.map((binding) =>
                                binding.model_id && !nextCatalog.some((item) => item.id === binding.model_id)
                                  ? { ...binding, model_id: null }
                                  : binding,
                              ),
                              {
                                ...value,
                                providers: nextProviders,
                                model_catalog: nextCatalog,
                              },
                            ),
                          };
                          onChange(nextSettings);
                          setSelectedProviderId(nextProviders[0]?.id || null);
                        }}
                        className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5"
                      >
                        <Trash2 className="h-4 w-4" />
                        {t('delete', 'Delete')}
                      </button>
                    </div>
                  </div>

                  <div className="grid gap-4 md:grid-cols-2">
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('nameLabel', 'Name')}</span>
                      <input
                        value={selectedProvider.name}
                        onChange={(event) => updateProvider(selectedProvider.id, { name: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('protocolLabel', 'Protocol')}</span>
                      <select
                        value={selectedProvider.protocol}
                        onChange={(event) => updateProvider(selectedProvider.id, { protocol: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      >
                        {PROVIDER_PROTOCOL_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="space-y-2 md:col-span-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('baseUrlLabel', 'Base URL')}</span>
                      <input
                        value={selectedProvider.base_url}
                        onChange={(event) => updateProvider(selectedProvider.id, { base_url: event.target.value })}
                        placeholder="https://api.openai.com/v1"
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('authSchemeLabel', 'Auth Scheme')}</span>
                      <input
                        value={selectedProvider.auth_scheme}
                        onChange={(event) => updateProvider(selectedProvider.id, { auth_scheme: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('apiKey', 'API Key')}</span>
                      <input
                        value={selectedProvider.api_key}
                        onChange={(event) => updateProvider(selectedProvider.id, { api_key: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                  </div>

                  <div className="grid gap-4 md:grid-cols-2">
                    <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('aiConnectionEnableProvider', 'Enable Provider')}</div>
                        <div className="text-xs text-muted-foreground">
                          {t('aiConnectionEnableProviderDesc', 'Disabled providers will not participate in catalog fetching or runtime usage.')}
                        </div>
                      </div>
                      <input
                        type="checkbox"
                        checked={selectedProvider.enabled}
                        onChange={(event) => updateProvider(selectedProvider.id, { enabled: event.target.checked })}
                        className="h-4 w-4"
                      />
                    </label>
                    <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('supportsWebSearchLabel', 'Supports Web Search')}</div>
                        <div className="text-xs text-muted-foreground">
                          {t('supportsWebSearchDesc', 'Mark this if the model supports native web search.')}
                        </div>
                      </div>
                      <input
                        type="checkbox"
                        checked={selectedProvider.capabilities.supports_web_search}
                        onChange={(event) =>
                          updateProvider(selectedProvider.id, {
                            capabilities: {
                              ...selectedProvider.capabilities,
                              supports_web_search: event.target.checked,
                            },
                          })
                        }
                        className="h-4 w-4"
                      />
                    </label>
                    <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('supportsStreamingLabel', 'Supports Streaming')}</div>
                        <div className="text-xs text-muted-foreground">
                          {t('supportsStreamingDesc', 'Enable streaming output and incremental updates.')}
                        </div>
                      </div>
                      <input
                        type="checkbox"
                        checked={selectedProvider.capabilities.supports_streaming}
                        onChange={(event) =>
                          updateProvider(selectedProvider.id, {
                            capabilities: {
                              ...selectedProvider.capabilities,
                              supports_streaming: event.target.checked,
                            },
                          })
                        }
                        className="h-4 w-4"
                      />
                    </label>
                    <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('supportsReasoningLabel', 'Supports Reasoning')}</div>
                        <div className="text-xs text-muted-foreground">
                          {t('supportsReasoningDesc', 'Allow reasoning mode to be enabled at the role level.')}
                        </div>
                      </div>
                      <input
                        type="checkbox"
                        checked={selectedProvider.capabilities.supports_reasoning}
                        onChange={(event) =>
                          updateProvider(selectedProvider.id, {
                            capabilities: {
                              ...selectedProvider.capabilities,
                              supports_reasoning: event.target.checked,
                            },
                          })
                        }
                        className="h-4 w-4"
                      />
                    </label>
                  </div>
                </div>
              ) : (
                <div className="rounded-2xl border border-dashed bg-muted/10 px-6 py-10 text-center text-sm text-muted-foreground">
                  {t(
                    'aiConnectionNoProvidersYet',
                    'Add a provider connection first so the workspace can fetch the model catalog and establish role bindings.',
                  )}
                </div>
              )}
            </div>
          ) : null}

          
          {panel === 'catalog' ? (
            <div className="space-y-4">
              <div className="rounded-2xl border bg-muted/10 px-4 py-3 text-sm text-muted-foreground">
                {t(
                  'aiConnectionCatalogNotice',
                  'The model catalog is fetched automatically from providers. If a role does not have a bound model, it falls back to the first enabled catalog item.',
                )}
              </div>
              <div className="grid gap-3 lg:grid-cols-2 xl:grid-cols-3">
                {value.model_catalog.length > 0 ? (
                  value.model_catalog.map((item) => (
                    <div key={item.id} className="rounded-2xl border bg-card/80 p-4">
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <div className="truncate text-sm font-semibold">{item.label}</div>
                          <div className="mt-1 text-xs text-muted-foreground">
                            {providerLabels.get(item.provider_id) || item.provider_id}
                          </div>
                        </div>
                        <label className="inline-flex items-center gap-2 text-xs text-muted-foreground">
                          <input
                            type="checkbox"
                            checked={item.enabled}
                            onChange={(event) => {
                              const nextCatalog = value.model_catalog.map((catalogItem) =>
                                catalogItem.id === item.id ? { ...catalogItem, enabled: event.target.checked } : catalogItem,
                              );
                              const nextSettings = { ...value, model_catalog: nextCatalog };
                              onChange({
                                ...nextSettings,
                                role_bindings: ensureRoleBindings(nextSettings.role_bindings || [], nextSettings).map((binding) =>
                                  binding.model_id === item.id && !event.target.checked
                                    ? { ...binding, model_id: null }
                                    : binding,
                                ),
                              });
                            }}
                            className="h-4 w-4"
                          />
                          {t('enabledLabel', 'Enabled')}
                        </label>
                      </div>
                      <div className="mt-3 text-xs text-muted-foreground">{item.description || item.model_id}</div>
                      <div className="mt-3 flex flex-wrap gap-2">
                        {capabilityBadge(item.supports_streaming ? 'stream' : 'sync')}
                        {capabilityBadge(item.supports_reasoning ? 'reasoning' : 'standard')}
                        {capabilityBadge(item.supports_web_search ? 'web' : 'no-web')}
                        {item.tags.map((tag) => capabilityBadge(tag))}
                      </div>
                    </div>
                  ))
                ) : (
                  <div className="rounded-2xl border border-dashed bg-muted/10 px-6 py-10 text-center text-sm text-muted-foreground lg:col-span-2 xl:col-span-3">
                    {t(
                      'aiConnectionNoCatalogYet',
                      'No model catalog is available yet. Go to Provider Connections to detect a connection and fetch the catalog.',
                    )}
                  </div>
                )}
              </div>
            </div>
          ) : null}

          {panel === 'roles' ? (
            <div className="space-y-4">
              {effectiveRoleBindings.map((binding) => {
                const roleModel = value.model_catalog.find((item) => item.id === binding.model_id);
                return (
                  <div key={binding.role} className="rounded-2xl border bg-card/80 p-4">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div>
                        <div className="flex items-center gap-2">
                          <div className="rounded-xl bg-primary/10 p-2 text-primary">
                            <Bot className="h-4 w-4" />
                          </div>
                          <div>
                            <div className="text-sm font-semibold">{getWorkspaceRoleLabel(binding.role, t)}</div>
                            <div className="text-xs text-muted-foreground">
                              {getWorkspaceRoleDescription(binding.role, t)}
                            </div>
                          </div>
                        </div>
                      </div>
                      <div className="rounded-full border px-3 py-1 text-xs text-muted-foreground">
                        {roleModel?.label || t('notBoundModel', 'No model bound')}
                      </div>
                    </div>

                    <div className="mt-4 grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                      <label className="space-y-2 md:col-span-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('modelLabel', 'Model')}</span>
                        <select
                          value={binding.model_id || ''}
                          onChange={(event) =>
                            updateSettings({
                              role_bindings: effectiveRoleBindings.map((item) =>
                                item.role === binding.role
                                  ? { ...item, model_id: event.target.value || null }
                                  : item,
                              ),
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">{t('selectModel', 'Select model')}</option>
                          {enabledCatalog.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.label} / {providerLabels.get(item.provider_id) || item.provider_id}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('runtimePresetLabel', 'Runtime Preset')}</span>
                        <select
                          value={binding.runtime_preset_id || ''}
                          onChange={(event) =>
                            updateSettings({
                              role_bindings: effectiveRoleBindings.map((item) =>
                                item.role === binding.role
                                  ? { ...item, runtime_preset_id: event.target.value || null }
                                  : item,
                              ),
                              runtime_presets: effectiveRuntimePresets,
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">{t('noPreset', 'No preset')}</option>
                          {effectiveRuntimePresets.map((preset) => (
                            <option key={preset.id} value={preset.id}>
                              {preset.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('temperatureLabel', 'Temperature')}</span>
                        <input
                          type="number"
                          step="0.1"
                          value={binding.temperature ?? ''}
                          onChange={(event) =>
                            updateSettings({
                              role_bindings: effectiveRoleBindings.map((item) =>
                                item.role === binding.role
                                  ? { ...item, temperature: parseNumberInput(event.target.value) }
                                  : item,
                              ),
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('maxTokensLabel', 'Max Tokens')}</span>
                        <input
                          type="number"
                          value={binding.max_tokens ?? ''}
                          onChange={(event) =>
                            updateSettings({
                              role_bindings: effectiveRoleBindings.map((item) =>
                                item.role === binding.role
                                  ? { ...item, max_tokens: parseNumberInput(event.target.value) }
                                  : item,
                              ),
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        />
                      </label>
                    </div>

                    <div className="mt-4 grid gap-4 md:grid-cols-2">
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('enableReasoning', 'Enable Reasoning')}</div>
                          <div className="text-xs text-muted-foreground">
                            {t('aiConnectionRoleEnableReasoningDesc', 'Reasoning is enabled by default for this role.')}
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          checked={binding.enable_reasoning}
                          onChange={(event) =>
                            updateSettings({
                              role_bindings: effectiveRoleBindings.map((item) =>
                                item.role === binding.role
                                  ? { ...item, enable_reasoning: event.target.checked }
                                  : item,
                              ),
                            })
                          }
                          className="h-4 w-4"
                        />
                      </label>
                                          </div>
                  </div>
                );
              })}
            </div>
          ) : null}

          {panel === 'runtime' ? (
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border bg-muted/10 px-4 py-3">
                <div className="text-sm text-muted-foreground">
                  {t(
                    'aiConnectionRuntimePresetNotice',
                    'Runtime presets are the base templates for role runtime parameters. Role bindings can reuse a preset or override it with more detailed settings.',
                  )}
                </div>
                <button
                  type="button"
                  onClick={() =>
                    updateSettings({
                      runtime_presets: [...effectiveRuntimePresets, createRuntimePreset(t)],
                      role_bindings: effectiveRoleBindings,
                    })
                  }
                  className="inline-flex items-center gap-2 rounded-lg border bg-background px-3 py-2 text-sm hover:bg-muted"
                >
                  <Plus className="h-4 w-4" />
                  {t('addPreset', 'Add Preset')}
                </button>
              </div>
              <div className="grid gap-4 lg:grid-cols-2 xl:grid-cols-3">
                {effectiveRuntimePresets.map((preset) => (
                  <div key={preset.id} className="rounded-2xl border bg-card/80 p-4">
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-semibold">{preset.name}</div>
                        <div className="mt-1 text-xs text-muted-foreground">{preset.id}</div>
                      </div>
                      <span className="rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                        {preset.enable_reasoning ? 'reasoning' : 'standard'}
                      </span>
                    </div>
                    <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border bg-muted/10 px-3 py-2 text-xs text-muted-foreground">
                      <span>
                        {t('boundRoles', 'Bound Roles')}:{' '}
                        {effectiveRoleBindings
                          .filter((binding) => binding.runtime_preset_id === preset.id)
                          .map((binding) => getWorkspaceRoleLabel(binding.role, t))
                          .join(', ') || t('noneLabel', 'None')}
                      </span>
                      <button
                        type="button"
                        disabled={effectiveRuntimePresets.length <= 1}
                        onClick={() => {
                          const nextPresets = effectiveRuntimePresets.filter((item) => item.id !== preset.id);
                          const fallbackPresetId = nextPresets[0]?.id || null;
                          updateSettings({
                            runtime_presets: nextPresets,
                            role_bindings: effectiveRoleBindings.map((binding) =>
                              binding.runtime_preset_id === preset.id
                                ? { ...binding, runtime_preset_id: fallbackPresetId }
                                : binding,
                            ),
                          });
                        }}
                        className="inline-flex items-center gap-1 rounded-lg border border-destructive/30 px-2.5 py-1.5 text-xs text-destructive hover:bg-destructive/5 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        {t('delete', 'Delete')}
                      </button>
                    </div>
                    <div className="mt-3 text-xs text-muted-foreground">{preset.description}</div>
                    <div className="mt-4 space-y-3">
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('nameLabel', 'Name')}</span>
                        <input
                          value={preset.name}
                          onChange={(event) =>
                            updateSettings({
                              runtime_presets: effectiveRuntimePresets.map((item) =>
                                item.id === preset.id ? { ...item, name: event.target.value } : item,
                              ),
                              role_bindings: effectiveRoleBindings,
                            })
                          }
                          className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('descriptionLabel', 'Description')}</span>
                        <textarea
                          value={preset.description}
                          onChange={(event) =>
                            updateSettings({
                              runtime_presets: effectiveRuntimePresets.map((item) =>
                                item.id === preset.id ? { ...item, description: event.target.value } : item,
                              ),
                              role_bindings: effectiveRoleBindings,
                            })
                          }
                          className="min-h-[96px] w-full rounded-xl border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <div className="grid gap-3 md:grid-cols-2">
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('temperatureLabel', 'Temperature')}</span>
                          <input
                            type="number"
                            step="0.1"
                            value={preset.temperature ?? ''}
                            onChange={(event) =>
                              updateSettings({
                                runtime_presets: effectiveRuntimePresets.map((item) =>
                                  item.id === preset.id
                                    ? { ...item, temperature: parseNumberInput(event.target.value) }
                                    : item,
                                ),
                                role_bindings: effectiveRoleBindings,
                              })
                            }
                            className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                          />
                        </label>
                        <label className="space-y-2">
                          <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">{t('maxTokensLabel', 'Max Tokens')}</span>
                          <input
                            type="number"
                            value={preset.max_tokens ?? ''}
                            onChange={(event) =>
                              updateSettings({
                                runtime_presets: effectiveRuntimePresets.map((item) =>
                                  item.id === preset.id
                                    ? { ...item, max_tokens: parseNumberInput(event.target.value) }
                                    : item,
                                ),
                                role_bindings: effectiveRoleBindings,
                              })
                            }
                            className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                          />
                        </label>
                      </div>
                      <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                        <div>
                          <div className="text-sm font-medium">{t('enableReasoning', 'Enable Reasoning')}</div>
                          <div className="text-xs text-muted-foreground">
                            {t(
                              'aiConnectionPresetEnableReasoningDesc',
                              'Acts as the default reasoning switch for roles referencing this preset.',
                            )}
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          checked={preset.enable_reasoning}
                          onChange={(event) =>
                            updateSettings({
                              runtime_presets: effectiveRuntimePresets.map((item) =>
                                item.id === preset.id
                                  ? { ...item, enable_reasoning: event.target.checked }
                                  : item,
                              ),
                              role_bindings: effectiveRoleBindings,
                            })
                          }
                          className="h-4 w-4"
                        />
                      </label>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
