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
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type {
  AiWorkspaceSettings,
  ModelCatalogItem,
  ModelRoleBinding,
  ProviderConnection,
  SearchProviderConnection,
} from '@/lib/aiWorkspace';
import {
  providerConnectionTest,
  providerModelsFetch,
  searchConnectionTest,
} from '@/lib/aiWorkspace';

type ConnectionPanel = 'providers' | 'search' | 'catalog' | 'roles';

const PROVIDER_PROTOCOL_OPTIONS = [
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

const WORKSPACE_ROLE_OPTIONS = [
  { role: 'chat', label: 'Chat', description: '普通对话的默认模型' },
  { role: 'assistant', label: 'Assistant', description: '助手主题和测试运行的主模型' },
  { role: 'summary', label: 'Summary', description: '轻量总结与二段处理模型' },
  { role: 'automation', label: 'Automation', description: '后台自动化任务默认模型' },
  { role: 'quick_assistant', label: 'Quick Assistant', description: '浮窗助手默认模型' },
  { role: 'selection_assistant', label: 'Selection Assistant', description: '划词助手预留绑定' },
  { role: 'translate', label: 'Translate', description: '翻译用途默认模型' },
  { role: 'topic_naming', label: 'Topic Naming', description: '主题命名与摘要标题模型' },
] as const;

function createProvider(): ProviderConnection {
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

function createSearchProvider(): SearchProviderConnection {
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

function ensureRoleBindings(
  bindings: ModelRoleBinding[],
  settings: AiWorkspaceSettings,
): ModelRoleBinding[] {
  return WORKSPACE_ROLE_OPTIONS.map((item) => {
    const existing = bindings.find((binding) => binding.role === item.role);
    return (
      existing || {
        id: `role-${item.role}`,
        role: item.role,
        model_id:
          settings.model_catalog.find((catalogItem) => catalogItem.enabled)?.id ||
          settings.model_catalog[0]?.id ||
          null,
        temperature: item.role === 'summary' ? 0.2 : 0.4,
        max_tokens: item.role === 'summary' ? 2048 : 4096,
        enable_reasoning: item.role !== 'summary' && item.role !== 'topic_naming',
        search_provider_id:
          item.role === 'chat' || item.role === 'assistant' || item.role === 'automation'
            ? settings.active_search_provider_id || null
            : null,
      }
    );
  });
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
  const [selectedSearchId, setSelectedSearchId] = useState<string | null>(value.search_providers[0]?.id || null);
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  useEffect(() => {
    if (!value.providers.some((provider) => provider.id === selectedProviderId)) {
      setSelectedProviderId(value.providers[0]?.id || null);
    }
  }, [selectedProviderId, value.providers]);

  useEffect(() => {
    if (!value.search_providers.some((provider) => provider.id === selectedSearchId)) {
      setSelectedSearchId(value.search_providers[0]?.id || null);
    }
  }, [selectedSearchId, value.search_providers]);

  const selectedProvider = useMemo(
    () => value.providers.find((provider) => provider.id === selectedProviderId) || value.providers[0] || null,
    [selectedProviderId, value.providers],
  );

  const selectedSearchProvider = useMemo(
    () =>
      value.search_providers.find((provider) => provider.id === selectedSearchId) ||
      value.search_providers[0] ||
      null,
    [selectedSearchId, value.search_providers],
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

  const updateSearchProvider = (
    providerId: string,
    patch: Partial<SearchProviderConnection>,
  ) => {
    updateSettings({
      search_providers: value.search_providers.map((provider) =>
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
        text: `${selectedProvider.name} 已刷新 ${catalog.length} 个模型目录项。`,
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

  const runSearchHealthCheck = async () => {
    if (!selectedSearchProvider?.id) return;
    setTestingKey(`search:${selectedSearchProvider.id}`);
    setFeedback(null);
    try {
      const result = await searchConnectionTest({ provider_id: selectedSearchProvider.id });
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

  const panels = [
    { id: 'providers' as const, title: 'Provider Connections', icon: Globe, hint: '模型供应商、密钥与能力开关' },
    { id: 'search' as const, title: 'Search Connections', icon: Search, hint: '联网搜索提供商与默认绑定' },
    { id: 'catalog' as const, title: 'Model Catalog', icon: Layers3, hint: '自动发现模型、标签和能力信息' },
    { id: 'roles' as const, title: 'Role Bindings', icon: Radar, hint: '把角色映射到模型和运行参数' },
  ];

  return (
    <div className="space-y-6 p-6">
      <div className="grid gap-3 md:grid-cols-4">
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">Providers</div>
          <div className="mt-2 text-2xl font-semibold">{value.providers.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">已连接 {value.providers.filter((item) => item.enabled).length} 个可用 Provider</div>
        </div>
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">Search</div>
          <div className="mt-2 text-2xl font-semibold">{value.search_providers.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">默认搜索源：{value.active_search_provider_id ? value.search_providers.find((item) => item.id === value.active_search_provider_id)?.name || value.active_search_provider_id : '未设置'}</div>
        </div>
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">Catalog</div>
          <div className="mt-2 text-2xl font-semibold">{value.model_catalog.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">{enabledCatalog.length} 个模型已启用，可用于角色绑定</div>
        </div>
        <div className="rounded-2xl border bg-card/80 p-4">
          <div className="text-xs uppercase tracking-[0.18em] text-muted-foreground">Roles</div>
          <div className="mt-2 text-2xl font-semibold">{effectiveRoleBindings.length}</div>
          <div className="mt-1 text-xs text-muted-foreground">覆盖聊天、助手、自动化、翻译等默认角色</div>
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
                {panel === 'providers' && 'Provider Connection Center'}
                {panel === 'search' && 'Search Provider Center'}
                {panel === 'catalog' && 'Model Catalog'}
                {panel === 'roles' && 'Role Binding Matrix'}
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {panel === 'providers' &&
                  '这里只处理模型供应商连接、密钥、网络与模型目录拉取，不再承载旧式 profile 配置。'}
                {panel === 'search' &&
                  '搜索提供商单独维护，角色绑定再决定哪些能力默认使用联网搜索。'}
                {panel === 'catalog' &&
                  '模型目录从 Provider 自动拉取，支持标签、能力和启用状态管理。'}
                {panel === 'roles' &&
                  '每个角色都能绑定一个默认模型，并携带温度、最大 token、推理与搜索策略。'}
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
                  <div className="text-sm font-medium">Providers</div>
                  <button
                    type="button"
                    onClick={() => {
                      const created = createProvider();
                      updateSettings({ providers: [created, ...value.providers] });
                      setSelectedProviderId(created.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs hover:bg-muted"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    Add
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
                        <div className="text-xs text-muted-foreground">连接、鉴权、能力标记与目录拉取</div>
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
                        检测连接
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
                        拉取模型目录
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
                        删除
                      </button>
                    </div>
                  </div>

                  <div className="grid gap-4 md:grid-cols-2">
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Name</span>
                      <input
                        value={selectedProvider.name}
                        onChange={(event) => updateProvider(selectedProvider.id, { name: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Protocol</span>
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
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Base URL</span>
                      <input
                        value={selectedProvider.base_url}
                        onChange={(event) => updateProvider(selectedProvider.id, { base_url: event.target.value })}
                        placeholder="https://api.openai.com/v1"
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Auth Scheme</span>
                      <input
                        value={selectedProvider.auth_scheme}
                        onChange={(event) => updateProvider(selectedProvider.id, { auth_scheme: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">API Key</span>
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
                        <div className="text-sm font-medium">启用 Provider</div>
                        <div className="text-xs text-muted-foreground">关闭后不会参与目录拉取与运行</div>
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
                        <div className="text-sm font-medium">支持 Web Search</div>
                        <div className="text-xs text-muted-foreground">如果模型支持原生联网，可在此打标</div>
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
                        <div className="text-sm font-medium">支持 Streaming</div>
                        <div className="text-xs text-muted-foreground">消息流式输出与增量更新</div>
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
                        <div className="text-sm font-medium">支持 Reasoning</div>
                        <div className="text-xs text-muted-foreground">允许在角色层启用推理模式</div>
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
                  先添加一个 Provider 连接，工作台才能拉取模型目录并建立角色绑定。
                </div>
              )}
            </div>
          ) : null}

          {panel === 'search' ? (
            <div className="grid gap-6 xl:grid-cols-[280px,minmax(0,1fr)]">
              <div className="rounded-2xl border bg-muted/10 p-3">
                <div className="mb-3 flex items-center justify-between">
                  <div className="text-sm font-medium">Search Providers</div>
                  <button
                    type="button"
                    onClick={() => {
                      const created = createSearchProvider();
                      updateSettings({ search_providers: [created, ...value.search_providers] });
                      setSelectedSearchId(created.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs hover:bg-muted"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    Add
                  </button>
                </div>
                <div className="space-y-2">
                  {value.search_providers.map((provider) => (
                    <button
                      key={provider.id}
                      type="button"
                      onClick={() => setSelectedSearchId(provider.id)}
                      className={`w-full rounded-xl border px-3 py-3 text-left ${
                        selectedSearchProvider?.id === provider.id
                          ? 'border-primary bg-primary/5'
                          : 'hover:bg-background'
                      }`}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium">{provider.name}</div>
                          <div className="mt-1 text-xs text-muted-foreground">{provider.provider_type}</div>
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

              {selectedSearchProvider ? (
                <div className="space-y-5">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <div className="rounded-xl bg-primary/10 p-2 text-primary">
                        <Search className="h-4 w-4" />
                      </div>
                      <div>
                        <div className="text-base font-semibold">{selectedSearchProvider.name}</div>
                        <div className="text-xs text-muted-foreground">联网搜索诊断、默认源绑定与速率配置</div>
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <button
                        type="button"
                        onClick={() => void runSearchHealthCheck()}
                        disabled={testingKey === `search:${selectedSearchProvider.id}`}
                        className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
                      >
                        {testingKey === `search:${selectedSearchProvider.id}` ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Sparkles className="h-4 w-4" />
                        )}
                        检测连接
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          const nextProviders = value.search_providers.filter((provider) => provider.id !== selectedSearchProvider.id);
                          updateSettings({
                            search_providers: nextProviders,
                            active_search_provider_id:
                              value.active_search_provider_id === selectedSearchProvider.id
                                ? null
                                : value.active_search_provider_id,
                            role_bindings: effectiveRoleBindings.map((binding) =>
                              binding.search_provider_id === selectedSearchProvider.id
                                ? { ...binding, search_provider_id: null }
                                : binding,
                            ),
                          });
                          setSelectedSearchId(nextProviders[0]?.id || null);
                        }}
                        className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5"
                      >
                        <Trash2 className="h-4 w-4" />
                        删除
                      </button>
                    </div>
                  </div>

                  <div className="grid gap-4 md:grid-cols-2">
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Name</span>
                      <input
                        value={selectedSearchProvider.name}
                        onChange={(event) => updateSearchProvider(selectedSearchProvider.id, { name: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Provider Type</span>
                      <select
                        value={selectedSearchProvider.provider_type}
                        onChange={(event) =>
                          updateSearchProvider(selectedSearchProvider.id, { provider_type: event.target.value })
                        }
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      >
                        {SEARCH_PROVIDER_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="space-y-2 md:col-span-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Base URL</span>
                      <input
                        value={selectedSearchProvider.base_url || ''}
                        onChange={(event) => updateSearchProvider(selectedSearchProvider.id, { base_url: event.target.value })}
                        placeholder="https://api.tavily.com"
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">API Key</span>
                      <input
                        value={selectedSearchProvider.api_key}
                        onChange={(event) => updateSearchProvider(selectedSearchProvider.id, { api_key: event.target.value })}
                        className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                      />
                    </label>
                    <label className="space-y-2">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Default Search Provider</span>
                      <div className="flex items-center gap-3 rounded-xl border bg-muted/10 px-4 py-3">
                        <input
                          type="radio"
                          checked={value.active_search_provider_id === selectedSearchProvider.id}
                          onChange={() =>
                            updateSettings({ active_search_provider_id: selectedSearchProvider.id })
                          }
                        />
                        <span className="text-sm">把当前搜索连接设为默认联网搜索源</span>
                      </div>
                    </label>
                  </div>

                  <div className="grid gap-4 md:grid-cols-3">
                    <label className="flex items-center justify-between rounded-2xl border bg-muted/10 px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">启用搜索连接</div>
                        <div className="text-xs text-muted-foreground">关闭后角色绑定无法使用该搜索源</div>
                      </div>
                      <input
                        type="checkbox"
                        checked={selectedSearchProvider.enabled}
                        onChange={(event) => updateSearchProvider(selectedSearchProvider.id, { enabled: event.target.checked })}
                        className="h-4 w-4"
                      />
                    </label>
                    <label className="space-y-2 rounded-2xl border bg-muted/10 px-4 py-3">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Timeout (sec)</span>
                      <input
                        type="number"
                        value={selectedSearchProvider.timeout_secs ?? ''}
                        onChange={(event) =>
                          updateSearchProvider(selectedSearchProvider.id, {
                            timeout_secs: parseNumberInput(event.target.value),
                          })
                        }
                        className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                      />
                    </label>
                    <label className="space-y-2 rounded-2xl border bg-muted/10 px-4 py-3">
                      <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Max Results</span>
                      <input
                        type="number"
                        value={selectedSearchProvider.max_results ?? ''}
                        onChange={(event) =>
                          updateSearchProvider(selectedSearchProvider.id, {
                            max_results: parseNumberInput(event.target.value),
                          })
                        }
                        className="w-full rounded-xl border bg-background px-3 py-2 text-sm"
                      />
                    </label>
                  </div>
                </div>
              ) : (
                <div className="rounded-2xl border border-dashed bg-muted/10 px-6 py-10 text-center text-sm text-muted-foreground">
                  先添加一个搜索提供商，这样聊天、自动化和 Quick Assistant 才能统一使用联网能力。
                </div>
              )}
            </div>
          ) : null}

          {panel === 'catalog' ? (
            <div className="space-y-4">
              <div className="rounded-2xl border bg-muted/10 px-4 py-3 text-sm text-muted-foreground">
                模型目录由 Provider 自动拉取；如果某个角色没有绑定模型，会优先落回到第一个已启用模型目录项。
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
                          Enabled
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
                    当前还没有模型目录。先去 Provider Connections 中检测连接并拉取模型目录。
                  </div>
                )}
              </div>
            </div>
          ) : null}

          {panel === 'roles' ? (
            <div className="space-y-4">
              {effectiveRoleBindings.map((binding) => {
                const roleMeta = WORKSPACE_ROLE_OPTIONS.find((item) => item.role === binding.role);
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
                            <div className="text-sm font-semibold">{roleMeta?.label || binding.role}</div>
                            <div className="text-xs text-muted-foreground">{roleMeta?.description}</div>
                          </div>
                        </div>
                      </div>
                      <div className="rounded-full border px-3 py-1 text-xs text-muted-foreground">
                        {roleModel?.label || '未绑定模型'}
                      </div>
                    </div>

                    <div className="mt-4 grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                      <label className="space-y-2 md:col-span-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Model</span>
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
                          <option value="">选择模型</option>
                          {enabledCatalog.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.label} / {providerLabels.get(item.provider_id) || item.provider_id}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Temperature</span>
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
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Max Tokens</span>
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
                          <div className="text-sm font-medium">Enable Reasoning</div>
                          <div className="text-xs text-muted-foreground">角色默认开启推理能力</div>
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
                      <label className="space-y-2 rounded-2xl border bg-muted/10 px-4 py-3">
                        <span className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">Search Provider</span>
                        <select
                          value={binding.search_provider_id || ''}
                          onChange={(event) =>
                            updateSettings({
                              role_bindings: effectiveRoleBindings.map((item) =>
                                item.role === binding.role
                                  ? { ...item, search_provider_id: event.target.value || null }
                                  : item,
                              ),
                            })
                          }
                          className="w-full rounded-xl border bg-background px-4 py-2.5 text-sm"
                        >
                          <option value="">不默认联网</option>
                          {value.search_providers
                            .filter((provider) => provider.enabled)
                            .map((provider) => (
                              <option key={provider.id} value={provider.id}>
                                {provider.name}
                              </option>
                            ))}
                        </select>
                      </label>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
