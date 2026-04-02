import { useMemo, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Layers3,
  Search,
  Sparkles,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Switch } from '@/components/ui/switch';
import type {
  AiWorkspaceSettings,
  ModelCatalogItem,
  ModelRoleBinding,
  ProviderConnection,
} from '@/lib/aiWorkspace';

const WORKSPACE_ROLES = [
  { role: 'chat', label: 'Chat', description: 'Default model for regular conversation' },
  { role: 'assistant', label: 'Assistant', description: 'Primary model for assistant topics' },
  { role: 'summary', label: 'Summary', description: 'Lightweight model for summaries' },
  { role: 'automation', label: 'Automation', description: 'Default model for automation jobs' },
  { role: 'quick_assistant', label: 'Quick Assistant', description: 'Default model for Quick Assistant' },
  { role: 'selection_assistant', label: 'Selection Assistant', description: 'Reserved for Selection Assistant' },
  { role: 'translate', label: 'Translate', description: 'Default model for translation tasks' },
  { role: 'topic_naming', label: 'Topic Naming', description: 'Model for topic naming' },
] as const;

interface ModelCenterProps {
  settings: AiWorkspaceSettings;
  onChange: (settings: AiWorkspaceSettings) => void;
  onSave: () => void;
}

function capabilityBadge(label: string, enabled: boolean) {
  return (
    <span
      className={`rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${
        enabled ? 'border-primary/30 text-primary' : 'text-muted-foreground'
      }`}
    >
      {label}
    </span>
  );
}

export function ModelCenter({ settings, onChange, onSave }: ModelCenterProps) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [expandedProviders, setExpandedProviders] = useState<Set<string>>(new Set());

  // 按 Provider 分组模型
  const modelsByProvider = useMemo(() => {
    const grouped = new Map<string, ModelCatalogItem[]>();
    for (const model of settings.model_catalog) {
      const existing = grouped.get(model.provider_id) || [];
      existing.push(model);
      grouped.set(model.provider_id, existing);
    }
    return grouped;
  }, [settings.model_catalog]);

  // 获取 Provider 信息
  const providerMap = useMemo(() => {
    const map = new Map<string, ProviderConnection>();
    for (const provider of settings.providers) {
      map.set(provider.id, provider);
    }
    return map;
  }, [settings.providers]);

  // 过滤模型
  const filteredModelsByProvider = useMemo(() => {
    if (!searchQuery.trim()) return modelsByProvider;
    const query = searchQuery.toLowerCase();
    const filtered = new Map<string, ModelCatalogItem[]>();
    for (const [providerId, models] of modelsByProvider) {
      const matching = models.filter(
        (m) =>
          m.label.toLowerCase().includes(query) ||
          m.model_id.toLowerCase().includes(query) ||
          m.tags.some((tag) => tag.toLowerCase().includes(query))
      );
      if (matching.length > 0) {
        filtered.set(providerId, matching);
      }
    }
    return filtered;
  }, [modelsByProvider, searchQuery]);

  // 选中的模型
  const selectedModel = useMemo(() => {
    if (!selectedModelId) return null;
    return settings.model_catalog.find((m) => m.id === selectedModelId) || null;
  }, [settings.model_catalog, selectedModelId]);

  // 该模型的角色绑定
  const modelRoleBindings = useMemo(() => {
    if (!selectedModelId) return [];
    return settings.role_bindings.filter((rb) => rb.model_id === selectedModelId);
  }, [settings.role_bindings, selectedModelId]);

  const toggleProvider = (providerId: string) => {
    setExpandedProviders((current) => {
      const next = new Set(current);
      if (next.has(providerId)) {
        next.delete(providerId);
      } else {
        next.add(providerId);
      }
      return next;
    });
  };

  const handleModelToggle = (modelId: string, enabled: boolean) => {
    const newSettings = {
      ...settings,
      model_catalog: settings.model_catalog.map((m) =>
        m.id === modelId ? { ...m, enabled } : m
      ),
    };
    onChange(newSettings);
    onSave();
  };

  const handleBindRole = (role: string) => {
    const existing = settings.role_bindings.find((rb) => rb.role === role);
    let newSettings: AiWorkspaceSettings;
    if (!existing) {
      const newBinding: ModelRoleBinding = {
        id: `role-binding-${role}`,
        role,
        model_id: selectedModelId,
        enable_reasoning: selectedModel?.supports_reasoning ?? false,
      };
      newSettings = {
        ...settings,
        role_bindings: [...settings.role_bindings, newBinding],
      };
    } else {
      // 更新现有绑定的 model_id
      newSettings = {
        ...settings,
        role_bindings: settings.role_bindings.map((rb) =>
          rb.role === role ? { ...rb, model_id: selectedModelId } : rb
        ),
      };
    }
    onChange(newSettings);
    onSave();
  };

  const handleUnbindRole = (role: string) => {
    const newSettings = {
      ...settings,
      role_bindings: settings.role_bindings.map((rb) =>
        rb.role === role ? { ...rb, model_id: null } : rb
      ),
    };
    onChange(newSettings);
    onSave();
  };

  return (
    <div className="grid h-full gap-6 xl:grid-cols-[280px,minmax(0,1fr)]">
      {/* 左侧模型列表 */}
      <aside className="flex min-h-0 flex-col rounded-3xl border bg-card">
        <div className="border-b px-4 py-4">
          <div className="text-base font-semibold">{t('modelCatalog', 'Model Catalog')}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            按 Provider 分组显示所有模型
          </div>
        </div>

        <div className="space-y-2 p-3">
          <div className="flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
            <Search className="h-4 w-4 text-muted-foreground" />
            <input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search models..."
              className="w-full bg-transparent text-sm outline-none"
            />
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          <div className="space-y-2">
            {Array.from(filteredModelsByProvider.entries()).map(([providerId, models]) => {
              const provider = providerMap.get(providerId);
              const isExpanded = expandedProviders.has(providerId);
              const enabledCount = models.filter((m) => m.enabled).length;

              return (
                <div key={providerId} className="rounded-xl border">
                  <button
                    type="button"
                    onClick={() => toggleProvider(providerId)}
                    className="flex w-full items-center justify-between px-3 py-2.5 text-left hover:bg-muted/30"
                  >
                    <div className="flex items-center gap-2">
                      {isExpanded ? (
                        <ChevronDown className="h-4 w-4 text-muted-foreground" />
                      ) : (
                        <ChevronRight className="h-4 w-4 text-muted-foreground" />
                      )}
                      <div className="rounded-lg bg-primary/10 p-1.5 text-primary">
                        <Layers3 className="h-3.5 w-3.5" />
                      </div>
                      <span className="text-sm font-medium">
                        {provider?.name || providerId}
                      </span>
                    </div>
                    <span className="text-xs text-muted-foreground">
                      {enabledCount}/{models.length}
                    </span>
                  </button>

                  {isExpanded ? (
                    <div className="border-t px-2 py-2">
                      <div className="space-y-1">
                        {models.map((model) => (
                          <button
                            key={model.id}
                            type="button"
                            onClick={() => setSelectedModelId(model.id)}
                            className={`flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                              selectedModelId === model.id
                                ? 'bg-primary/5 text-primary'
                                : 'hover:bg-muted/30'
                            }`}
                          >
                            <div
                              className={`h-2 w-2 rounded-full ${
                                model.enabled ? 'bg-primary' : 'bg-muted-foreground/30'
                              }`}
                            />
                            <span className="truncate">{model.label}</span>
                            {model.supports_reasoning ? (
                              <Sparkles className="ml-auto h-3.5 w-3.5 text-muted-foreground" />
                            ) : null}
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      </aside>

      {/* 右侧模型详情 */}
      <main className="flex min-h-0 flex-col rounded-3xl border bg-card">
        {selectedModel ? (
          <>
            {/* 头部 */}
            <div className="border-b px-6 py-4">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <div className="text-lg font-semibold">{selectedModel.label}</div>
                  <div className="mt-1 text-sm text-muted-foreground">
                    {selectedModel.description || selectedModel.model_id}
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <div className="flex items-center gap-2">
                    <Switch
                      checked={selectedModel.enabled}
                      onCheckedChange={(checked) => handleModelToggle(selectedModel.id, checked)}
                    />
                    <span className="text-sm">{t('enabled', 'Enabled')}</span>
                  </div>
                </div>
              </div>
            </div>

            {/* 内容 */}
            <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
              <div className="space-y-6">
                {/* 模型信息 */}
                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t('modelInfo', 'Model Info')}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    {capabilityBadge('Reasoning', selectedModel.supports_reasoning)}
                    {capabilityBadge('Streaming', selectedModel.supports_streaming)}
                    {capabilityBadge('Web Search', selectedModel.supports_web_search)}
                  </div>
                  {selectedModel.tags.length > 0 ? (
                    <div className="mt-3 flex flex-wrap gap-1">
                      {selectedModel.tags.map((tag) => (
                        <span
                          key={tag}
                          className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  ) : null}
                  <div className="mt-3 text-xs text-muted-foreground">
                    Model ID: {selectedModel.model_id}
                  </div>
                </div>

                {/* 角色绑定 */}
                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t('roleBindings', 'Role Bindings')}
                  </div>
                  <div className="space-y-2">
                    {WORKSPACE_ROLES.map((roleInfo) => {
                      const binding = modelRoleBindings.find((rb) => rb.role === roleInfo.role);
                      const isBound = binding?.model_id === selectedModelId;

                      return (
                        <div
                          key={roleInfo.role}
                          className="flex items-center justify-between rounded-xl border bg-background px-4 py-3"
                        >
                          <div>
                            <div className="text-sm font-medium">{roleInfo.label}</div>
                            <div className="text-xs text-muted-foreground">
                              {roleInfo.description}
                            </div>
                          </div>
                          {isBound ? (
                            <div className="flex items-center gap-2">
                              <span className="text-xs text-primary">Bound</span>
                              <button
                                type="button"
                                onClick={() => handleUnbindRole(roleInfo.role)}
                                className="rounded-lg border px-2 py-1 text-xs hover:bg-muted"
                              >
                                Unbind
                              </button>
                            </div>
                          ) : (
                            <button
                              type="button"
                              onClick={() => handleBindRole(roleInfo.role)}
                              className="rounded-lg border px-3 py-1.5 text-xs hover:bg-muted"
                            >
                              Bind
                            </button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>

                {/* 运行时参数 */}
                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t('runtimeParams', 'Runtime Parameters')}
                  </div>
                  <div className="space-y-4">
                    <div className="grid gap-4 md:grid-cols-2">
                      <label className="space-y-2">
                        <span className="text-xs text-muted-foreground">Temperature</span>
                        <input
                          type="number"
                          step="0.1"
                          min="0"
                          max="2"
                          defaultValue="0.3"
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                      <label className="space-y-2">
                        <span className="text-xs text-muted-foreground">Max Tokens</span>
                        <input
                          type="number"
                          defaultValue="2048"
                          className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                        />
                      </label>
                    </div>
                    <div className="flex items-center justify-between rounded-xl border bg-background px-4 py-3">
                      <div>
                        <div className="text-sm font-medium">{t('enableReasoning', 'Enable Reasoning')}</div>
                        <div className="text-xs text-muted-foreground">
                          {t('enableReasoningDesc', '启用深度推理模式')}
                        </div>
                      </div>
                      <Switch
                        checked={selectedModel.supports_reasoning}
                        onCheckedChange={(checked) => {
                          const newSettings = {
                            ...settings,
                            model_catalog: settings.model_catalog.map((m) =>
                              m.id === selectedModel.id
                                ? { ...m, supports_reasoning: checked }
                                : m
                            ),
                          };
                          onChange(newSettings);
                          onSave();
                        }}
                      />
                    </div>
                  </div>
                </div>

                {/* 联网搜索 */}
                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {t('webSearch', 'Web Search')}
                  </div>
                  <div className="flex items-center justify-between rounded-xl border bg-background px-4 py-3">
                    <div>
                      <div className="text-sm font-medium">{t('enableWebSearch', 'Enable Web Search')}</div>
                      <div className="text-xs text-muted-foreground">
                        {t('enableWebSearchDesc', 'Allow this model to use web search capability')}
                      </div>
                    </div>
                    <Switch
                      checked={selectedModel.supports_web_search}
                      onCheckedChange={(checked) => {
                        const newSettings = {
                          ...settings,
                          model_catalog: settings.model_catalog.map((m) =>
                            m.id === selectedModel.id
                              ? { ...m, supports_web_search: checked }
                              : m
                          ),
                        };
                        onChange(newSettings);
                        onSave();
                      }}
                    />
                  </div>
                </div>
              </div>
            </div>
          </>
        ) : (
          <div className="flex h-full items-center justify-center px-6">
            <div className="max-w-md rounded-3xl border border-dashed bg-muted/10 px-6 py-10 text-center">
              <div className="mx-auto mb-4 inline-flex rounded-full bg-primary/10 p-3 text-primary">
                <Layers3 className="h-6 w-6" />
              </div>
              <div className="text-base font-semibold">{t('selectModelLabel', 'Select a Model')}</div>
              <p className="mt-2 text-sm text-muted-foreground">
                从左侧选择一个模型查看详情和配置角色绑定、运行时参数。
              </p>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}