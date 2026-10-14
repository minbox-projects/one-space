import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  ArrowRightLeft,
  Boxes,
  Check,
  ChevronDown,
  Cpu,
  Filter,
  Globe,
  KeyRound,
  Plus,
  RotateCcw,
  Server,
  Sparkles,
  Tag,
  X,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { ProviderQuotaBlock } from "./ProviderQuotaBlock";
import { ProviderGoUsageBlock } from "./ProviderGoUsageBlock";
import {
  ProviderTemplateAvatar,
  resolveEffectiveProviderIcon,
} from "./ProviderTemplateIcon";
import {
  formatDateTime,
  protocolBadgeClass,
  protocolShortLabel,
} from "./gatewayShared";
import {
  AI_GATEWAY_ALERT_AUTO_DISMISS_MS,
  formatGatewayTimestamp,
  isProviderTemplateBindingBroken,
  isMappingDeprecated,
  providerKeyPool,
  providerKeyState,
  providerMarkedKeys,
  PROVIDER_KEY_STATE_TRANSLATION_KEYS,
  resolveEffectiveTemplatePolicy,
  type GatewayModelPool,
  type GatewayModelPoolStatus,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { ModelPoolCard } from "./ModelPoolCard";
import {
  autoDisabledAlertInstanceKey,
  dismissAlertInstanceKeys,
  reconcileDismissedAlertInstanceKeys,
  retiredMappingAlertInstanceKey,
} from "@/lib/aiGatewayAlertBadges";

export type ProviderStatusFilter = "all" | "enabled" | "disabled";
export type ProviderViewMode = "all" | "providers" | "model_pools";

type UpstreamProviderListProps = {
  providers: GatewayUpstreamProvider[];
  /** Loaded template views, used to resolve a provider's bound template icon and retired hint. */
  templates?: GatewayProviderTemplateView[];
  selectedProviderId: string | null;
  busy: boolean;
  onSelect: (providerId: string) => void;
  onToggleEnabled: (provider: GatewayUpstreamProvider, enabled: boolean) => void;
  onAdd: () => void;
  onManageTemplates?: () => void;
  /** Re-enable one runtime-marked key of a provider. */
  onReenableKey?: (providerId: string, keyId: string) => void;
  /** Saved virtual model-pool cards shown beside real providers. */
  modelPools?: GatewayModelPool[];
  poolStatuses?: GatewayModelPoolStatus[];
  onTogglePoolEnabled?: (poolId: string, enabled: boolean) => void;
  onAddPool?: () => void;
  onEditPool?: (pool: GatewayModelPool) => void;
  onDeletePool?: (poolId: string) => void;
  onOpenPoolLogs?: (model: string) => void;
};

type UpstreamProviderCardItemProps = {
  provider: GatewayUpstreamProvider;
  isSelected: boolean;
  busy: boolean;
  templates?: GatewayProviderTemplateView[];
  baseNow?: number | Date;
  quotaRefreshToken: number;
  /** Instance keys the user (or the auto-dismiss timer) has dismissed. */
  dismissedKeys: Set<string>;
  /** Dismiss every provided problem instance of this card's pills. */
  handleDismissInstances: (keys: string[]) => void;
  onSelect: (providerId: string) => void;
  onToggleEnabled: (provider: GatewayUpstreamProvider, enabled: boolean) => void;
  onReenableKey?: (providerId: string, keyId: string) => void;
  handleCardRefreshed: (timestamp: number) => void;
};

function UpstreamProviderCardItem({
  provider,
  isSelected,
  busy,
  templates,
  baseNow,
  quotaRefreshToken,
  dismissedKeys,
  handleDismissInstances,
  onSelect,
  onToggleEnabled,
  onReenableKey,
  handleCardRefreshed,
}: UpstreamProviderCardItemProps) {
  const { t } = useTranslation();

  const isEnabled = provider.enabled;
  const mappingCount = provider.mappings?.length ?? 0;
  const providerKeys = providerKeyPool(provider);
  const markedKeys = providerMarkedKeys(provider);
  const protocolLabel = protocolShortLabel(provider.protocol);
  const templateView = provider.template_id
    ? templates?.find(
        (view) => view.template.id === provider.template_id,
      )
    : undefined;
  const effectivePolicy = resolveEffectiveTemplatePolicy(templates, provider);
  const bindingBroken = isProviderTemplateBindingBroken(templates, provider);

  const autoDisabledInstances = useMemo(
    () =>
      (provider.mappings ?? [])
        .filter((mapping) => mapping.auto_disabled === true)
        .map((mapping) => ({
          key: autoDisabledAlertInstanceKey(
            provider.id,
            mapping.local_model,
            mapping.upstream_model,
          ),
          label: mapping.local_model.trim() || mapping.upstream_model,
        })),
    [provider.id, provider.mappings],
  );

  const retiredInstances = useMemo(() => {
    if (!templateView) return [];
    const templateId = templateView.template.id;
    return (provider.mappings ?? [])
      .filter(
        (mapping) =>
          mapping.enabled === false &&
          isMappingDeprecated(mapping, templateView.template),
      )
      .map((mapping) => ({
        key: retiredMappingAlertInstanceKey(
          provider.id,
          templateId,
          mapping.upstream_model,
        ),
        label: mapping.upstream_model,
      }));
  }, [provider.id, provider.mappings, templateView]);

  const pendingAutoInstances = useMemo(
    () => autoDisabledInstances.filter((instance) => !dismissedKeys.has(instance.key)),
    [autoDisabledInstances, dismissedKeys],
  );
  const pendingRetiredInstances = useMemo(
    () => retiredInstances.filter((instance) => !dismissedKeys.has(instance.key)),
    [retiredInstances, dismissedKeys],
  );

  // Stable key arrays keyed on content so an unrelated re-render does not restart
  // the timer, while a new instance changes the joined string and does restart it.
  const pendingAutoKeyString = useMemo(
    () =>
      pendingAutoInstances
        .map((instance) => instance.key)
        .sort()
        .join("\n"),
    [pendingAutoInstances],
  );
  const pendingAutoKeys = useMemo(
    () => (pendingAutoKeyString ? pendingAutoKeyString.split("\n") : []),
    [pendingAutoKeyString],
  );
  const pendingRetiredKeyString = useMemo(
    () =>
      pendingRetiredInstances
        .map((instance) => instance.key)
        .sort()
        .join("\n"),
    [pendingRetiredInstances],
  );
  const pendingRetiredKeys = useMemo(
    () => (pendingRetiredKeyString ? pendingRetiredKeyString.split("\n") : []),
    [pendingRetiredKeyString],
  );

  useEffect(() => {
    if (!pendingAutoKeyString) return;
    const timer = window.setTimeout(
      () => handleDismissInstances(pendingAutoKeys),
      AI_GATEWAY_ALERT_AUTO_DISMISS_MS,
    );
    return () => window.clearTimeout(timer);
  }, [pendingAutoKeyString, pendingAutoKeys, handleDismissInstances]);

  useEffect(() => {
    if (!pendingRetiredKeyString) return;
    const timer = window.setTimeout(
      () => handleDismissInstances(pendingRetiredKeys),
      AI_GATEWAY_ALERT_AUTO_DISMISS_MS,
    );
    return () => window.clearTimeout(timer);
  }, [pendingRetiredKeyString, pendingRetiredKeys, handleDismissInstances]);

  const dismissAriaLabel = t("aiGatewayAlertBadgeDismissAria", {
    defaultValue: "Dismiss alert",
  });

  return (
    <div
      data-testid={`ai-gateway-provider-${provider.id}`}
      className={`group relative flex flex-col justify-between rounded-xl border bg-card p-3.5 shadow-sm transition-all hover:border-primary/40 hover:shadow-md ${
        isSelected ? "ring-2 ring-primary/20 border-primary" : ""
      } ${!isEnabled ? "opacity-85 hover:opacity-100" : ""}`}
    >
      <div>
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-3 min-w-0 flex-1">
            {(() => {
              const effectiveIcon = resolveEffectiveProviderIcon(
                provider,
                templateView?.template,
              );
              if (effectiveIcon) {
                const isCustom = Boolean(provider.icon);
                return (
                  <span
                    data-testid={`ai-gateway-provider-template-icon-${provider.id}`}
                    title={
                      isCustom
                        ? t("aiGatewayProviderCustomIconTitle", {
                            name: provider.name,
                            defaultValue: `Custom icon for ${provider.name}`,
                          })
                        : templateView
                          ? t("aiGatewayProviderTemplateAvatarTitle", {
                              name: templateView.template.name,
                              defaultValue: `Created from template ${templateView.template.name}`,
                            })
                          : provider.name
                    }
                    className="shrink-0 mt-0.5"
                  >
                    <ProviderTemplateAvatar
                      icon={effectiveIcon}
                      templateId={templateView?.template.id ?? provider.template_id ?? provider.id}
                      templateName={provider.name}
                      baseUrl={provider.base_url}
                      size={36}
                    />
                  </span>
                );
              }
              return (
                <div
                  className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-border/70 bg-muted/40 text-muted-foreground shadow-2xs mt-0.5"
                  aria-hidden="true"
                >
                  <Server className="h-4.5 w-4.5 opacity-70" />
                </div>
              );
            })()}
            <div className="min-w-0 flex-1">
              <button
                type="button"
                onClick={() => onSelect(provider.id)}
                className="truncate text-left text-sm font-semibold text-foreground hover:text-primary transition-colors block w-full leading-5"
                title={provider.name}
              >
                {provider.name}
              </button>
              <div className="mt-1 flex flex-wrap items-center gap-1.5">
                <span
                  className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-4 ${protocolBadgeClass(
                    provider.protocol,
                  )}`}
                >
                  {protocolLabel}
                </span>
                <span
                  data-testid={`ai-gateway-weight-badge-${provider.id}`}
                  className="inline-flex items-center rounded-md border bg-background px-1.5 py-0.5 text-[11px] font-medium leading-4 text-muted-foreground"
                >
                  {t("aiGateway.provider.weight", "Weight")}: {provider.weight ?? 1}
                </span>
              </div>
              {provider.tags && provider.tags.length > 0 ? (
                <div
                  data-testid={`ai-gateway-provider-tags-${provider.id}`}
                  className="mt-1.5 flex flex-wrap items-center gap-1"
                >
                  {provider.tags.map((tag) => (
                    <span
                      key={tag}
                      className="inline-flex items-center rounded-md bg-secondary/80 px-1.5 py-0.5 text-[10px] font-medium text-secondary-foreground"
                    >
                      #{tag}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          </div>

          <div className="shrink-0 pt-0.5">
            <Switch
              aria-label={t("aiGatewayToggleProviderAria", {
                name: provider.name,
                defaultValue: `Enable provider ${provider.name}`,
              })}
              checked={provider.enabled}
              disabled={busy}
              onCheckedChange={(checked) => onToggleEnabled(provider, checked)}
            />
          </div>
        </div>

        {/* 中间信息：Base URL 与映射及密钥数量 */}
        <div className="mt-2.5 space-y-1">
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Globe className="h-3.5 w-3.5 shrink-0 opacity-70" />
            <span className="truncate font-mono" title={provider.base_url}>
              {provider.base_url}
            </span>
          </div>

          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <div className="flex items-center gap-1.5">
              <ArrowRightLeft className="h-3.5 w-3.5 shrink-0 opacity-70" />
              <span>
                {mappingCount > 0
                  ? t("aiGatewayMappingCount", {
                      count: mappingCount,
                      defaultValue: `${mappingCount} mappings configured`,
                    })
                  : t("aiGatewayNoMappingsShort", "No mappings (default routing)")}
              </span>
            </div>

            {providerKeys.length > 0 ? (
              <div
                data-testid={`ai-gateway-provider-keys-${provider.id}`}
                className="flex items-center gap-1.5"
              >
                <KeyRound className="h-3.5 w-3.5 shrink-0 opacity-70" />
                <span>
                  {t("aiGatewayProviderKeysSummary", {
                    count: providerKeys.length,
                    defaultValue: `${providerKeys.length} key(s)`,
                  })}
                </span>
              </div>
            ) : null}
          </div>
        </div>

        {/* 绑定模板（策略严格来自 template_id 解析） */}
        {provider.template_id || bindingBroken ? (
          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            {provider.template_id ? (
              <span
                data-testid={`ai-gateway-provider-template-binding-${provider.id}`}
                className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
              >
                <Sparkles className="h-3 w-3 opacity-70" />
                <span className="truncate">
                  {templateView?.template.name ?? provider.template_id}
                </span>
              </span>
            ) : null}
            {bindingBroken ? (
              <span
                data-testid={`ai-gateway-provider-broken-binding-${provider.id}`}
                className="inline-flex items-center gap-1 rounded-md border border-destructive/40 bg-destructive/10 px-1.5 py-0.5 text-[10px] font-medium text-destructive"
              >
                <AlertTriangle className="h-3 w-3 shrink-0" />
                <span>
                  {t(
                    "aiGatewayProviderTemplateBrokenBinding",
                    "The bound template no longer exists.",
                  )}
                </span>
              </span>
            ) : null}
          </div>
        ) : null}

        {effectivePolicy.quota_strategy === "commandcode" ? (
          <ProviderQuotaBlock
            provider={provider}
            baseNow={baseNow}
            refreshToken={quotaRefreshToken}
            onRefreshed={handleCardRefreshed}
          />
        ) : effectivePolicy.quota_strategy === "opencode_go" ? (
          <ProviderGoUsageBlock
            provider={provider}
            baseNow={baseNow}
            refreshToken={quotaRefreshToken}
            onRefreshed={handleCardRefreshed}
          />
        ) : (
          <div
            data-testid={`ai-gateway-provider-placeholder-${provider.id}`}
            className="mt-2 flex h-8 items-center justify-center gap-1.5 rounded-lg border border-dashed border-border/60 bg-muted/10 text-[11px] text-muted-foreground/60"
          >
            <Cpu className="h-3.5 w-3.5 opacity-70" />
            <span>{t("aiGatewayQuotaNotSupported", "未接入额度监控")}</span>
          </div>
        )}

        {/* 退休映射提示：模板同步移除模型后其派生映射被自动禁用 */}
        {pendingRetiredInstances.length > 0 ? (
          <div
            data-testid={`ai-gateway-provider-retired-mappings-${provider.id}`}
            title={t("aiGatewayTemplateRetiredMappingsTooltip", {
              models: pendingRetiredInstances
                .map((instance) => instance.label)
                .join(", "),
              defaultValue: `Removed from the template and disabled: ${pendingRetiredInstances
                .map((instance) => instance.label)
                .join(", ")}`,
            })}
            className="mt-2 inline-flex items-center gap-1 rounded-full border border-amber-500/40 bg-amber-500/15 pl-2 pr-1 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400"
          >
            <AlertTriangle className="h-3 w-3 shrink-0" />
            <span>
              {t("aiGatewayTemplateRetiredMappings", {
                count: pendingRetiredInstances.length,
                defaultValue: `${pendingRetiredInstances.length} mapping(s) removed from template`,
              })}
            </span>
            <button
              type="button"
              data-testid={`ai-gateway-provider-retired-mappings-dismiss-${provider.id}`}
              aria-label={dismissAriaLabel}
              onClick={(event) => {
                event.stopPropagation();
                handleDismissInstances(pendingRetiredKeys);
              }}
              className="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full text-amber-700/70 transition hover:bg-amber-500/25 hover:text-amber-800 dark:text-amber-400/80 dark:hover:text-amber-200"
            >
              <X className="h-2.5 w-2.5" />
            </button>
          </div>
        ) : null}

        {/* 逐行自动禁用读提示：健康失败累计后由后端按映射行自动禁用 */}
        {pendingAutoInstances.length > 0 ? (
          <div
            data-testid={`ai-gateway-provider-auto-disabled-models-${provider.id}`}
            title={t("aiGatewayProviderAutoDisabledModelsTooltip", {
              models: pendingAutoInstances
                .map((instance) => instance.label)
                .join(", "),
              defaultValue: `Auto-disabled mappings: ${pendingAutoInstances
                .map((instance) => instance.label)
                .join(", ")}`,
            })}
            className="mt-2 inline-flex items-center gap-1 rounded-full border border-amber-500/40 bg-amber-500/15 pl-2 pr-1 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400"
          >
            <AlertTriangle className="h-3 w-3 shrink-0" />
            <span>
              {t("aiGatewayProviderAutoDisabledModelsHint", {
                count: pendingAutoInstances.length,
                defaultValue: `${pendingAutoInstances.length} mapping(s) auto-disabled`,
              })}
            </span>
            <button
              type="button"
              data-testid={`ai-gateway-provider-auto-disabled-models-dismiss-${provider.id}`}
              aria-label={dismissAriaLabel}
              onClick={(event) => {
                event.stopPropagation();
                handleDismissInstances(pendingAutoKeys);
              }}
              className="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full text-amber-700/70 transition hover:bg-amber-500/25 hover:text-amber-800 dark:text-amber-400/80 dark:hover:text-amber-200"
            >
              <X className="h-2.5 w-2.5" />
            </button>
          </div>
        ) : null}

        {/* 已标记密钥区块：展示运行时标记的每把密钥并可单独重新启用 */}
        {markedKeys.length > 0 ? (
          <div
            data-testid={`ai-gateway-provider-marked-keys-${provider.id}`}
            className="mt-2 rounded-lg border border-amber-500/30 bg-amber-500/5 p-2"
          >
            <div className="text-[11px] font-semibold text-amber-700 dark:text-amber-400">
              {t("aiGatewayProviderMarkedKeys", "Marked keys")}
            </div>
            <div className="mt-1.5 space-y-1.5">
              {markedKeys.map((key) => {
                const markedTime = formatGatewayTimestamp(key.marked_at);
                const reason = key.reason?.trim() ?? "";
                return (
                  <div
                    key={key.id}
                    data-testid={`ai-gateway-marked-key-${key.id}`}
                    className="flex flex-col gap-1 rounded-md border border-border/60 bg-background/60 p-1.5 text-[11px] text-muted-foreground"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span
                        className="min-w-0 flex-1 truncate font-medium text-foreground"
                        title={key.name}
                      >
                        {key.name}
                      </span>
                      <span className="inline-flex shrink-0 items-center rounded-md border border-border/70 bg-background px-1.5 py-0.5 text-[10px] font-medium leading-4 text-muted-foreground">
                        {t(PROVIDER_KEY_STATE_TRANSLATION_KEYS[providerKeyState(key)])}
                      </span>
                    </div>
                    {markedTime ? (
                      <span className="opacity-80">
                        {t("aiGatewayProviderKeyMarkedAt", {
                          time: markedTime,
                          defaultValue: `Marked at ${markedTime}`,
                        })}
                      </span>
                    ) : null}
                    {reason ? (
                      <span
                        data-testid={`ai-gateway-marked-key-reason-${key.id}`}
                        className="break-words"
                      >
                        {t("aiGatewayProviderKeyReasonLabel", "Reason")}: {reason}
                      </span>
                    ) : null}
                    <button
                      type="button"
                      data-testid={`ai-gateway-marked-key-reenable-${key.id}`}
                      onClick={() => onReenableKey?.(provider.id, key.id)}
                      disabled={busy}
                      className="inline-flex h-6 w-fit items-center gap-1 rounded-md border border-border bg-background px-2 text-[11px] font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-50"
                    >
                      <RotateCcw className="h-3 w-3" />
                      {t("aiGatewayProviderKeyReenable", "Re-enable")}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function UpstreamProviderList({
  providers,
  templates,
  selectedProviderId,
  busy,
  onSelect,
  onToggleEnabled,
  onAdd,
  onManageTemplates,
  onReenableKey,
  modelPools = [],
  poolStatuses = [],
  onTogglePoolEnabled,
  onAddPool,
  onEditPool,
  onDeletePool,
  onOpenPoolLogs,
}: UpstreamProviderListProps) {
  const { t } = useTranslation();

  // 计算所有服务商之前仅获取一次当前时间（有意读取时钟，providers 变化时重拍）
  // eslint-disable-next-line react-hooks/purity -- baseNow 需要在 providers 变化时快照 Date.now()
  const baseNow = useMemo(() => Date.now(), [providers]);
  const [viewMode, setViewMode] = useState<ProviderViewMode>("all");
  const [statusFilter, setStatusFilter] = useState<ProviderStatusFilter>("all");
  const [poolStatusFilter, setPoolStatusFilter] = useState<ProviderStatusFilter>("all");
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [tagFilterOpen, setTagFilterOpen] = useState(false);
  const [quotaRefreshToken, setQuotaRefreshToken] = useState(0);
  const tagFilterRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!tagFilterOpen) return;
    function handleClickOutside(e: MouseEvent) {
      if (tagFilterRef.current && !tagFilterRef.current.contains(e.target as Node)) {
        setTagFilterOpen(false);
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setTagFilterOpen(false);
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [tagFilterOpen]);

  const allAvailableTags = useMemo(() => {
    const set = new Set<string>();
    for (const p of providers) {
      if (p.tags) {
        for (const tag of p.tags) {
          const trimmed = tag.trim();
          if (trimmed) set.add(trimmed);
        }
      }
    }
    return Array.from(set).sort();
  }, [providers]);

  const tagCounts = useMemo(() => {
    const map = new Map<string, number>();
    for (const tag of allAvailableTags) {
      map.set(tag, providers.filter((p) => p.tags?.includes(tag)).length);
    }
    return map;
  }, [allAvailableTags, providers]);

  const toggleTagFilter = (tag: string) => {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag],
    );
  };

  const clearTagFilter = () => {
    setSelectedTags([]);
  };

  const counts = useMemo(() => {
    let enabled = 0;
    let disabled = 0;
    for (const p of providers) {
      if (p.enabled) {
        enabled++;
      } else {
        disabled++;
      }
    }
    return {
      all: providers.length,
      enabled,
      disabled,
    };
  }, [providers]);

  const filteredProviders = useMemo(() => {
    return providers.filter((p) => {
      if (statusFilter === "enabled" && !p.enabled) return false;
      if (statusFilter === "disabled" && p.enabled) return false;
      if (selectedTags.length > 0) {
        const pTags = p.tags ?? [];
        const matches = selectedTags.some((t) => pTags.includes(t));
        if (!matches) return false;
      }
      return true;
    });
  }, [providers, statusFilter, selectedTags]);

  const poolCounts = useMemo(() => {
    let enabled = 0;
    let disabled = 0;
    for (const pool of modelPools) {
      if (pool.enabled) {
        enabled++;
      } else {
        disabled++;
      }
    }
    return {
      all: modelPools.length,
      enabled,
      disabled,
    };
  }, [modelPools]);

  const filteredModelPools = useMemo(() => {
    return modelPools.filter((pool) => {
      if (poolStatusFilter === "enabled" && !pool.enabled) return false;
      if (poolStatusFilter === "disabled" && pool.enabled) return false;
      return true;
    });
  }, [modelPools, poolStatusFilter]);

  // Every currently actionable warning instance across all providers: auto-disabled
  // rows and template-retired rows. Dismissals are pruned against this set so a
  // resolved instance is forgotten and a later recurrence shows again.
  const currentInstanceKeys = useMemo(() => {
    const keys = new Set<string>();
    for (const provider of providers) {
      const mappings = provider.mappings ?? [];
      for (const mapping of mappings) {
        if (mapping.auto_disabled === true) {
          keys.add(
            autoDisabledAlertInstanceKey(
              provider.id,
              mapping.local_model,
              mapping.upstream_model,
            ),
          );
        }
      }
      const templateView = provider.template_id
        ? templates?.find(
            (view) => view.template.id === provider.template_id,
          )
        : undefined;
      if (templateView) {
        for (const mapping of mappings) {
          if (
            mapping.enabled === false &&
            isMappingDeprecated(mapping, templateView.template)
          ) {
            keys.add(
              retiredMappingAlertInstanceKey(
                provider.id,
                templateView.template.id,
                mapping.upstream_model,
              ),
            );
          }
        }
      }
    }
    return keys;
  }, [providers, templates]);

  const [dismissedKeys, setDismissedKeys] = useState<Set<string>>(() =>
    reconcileDismissedAlertInstanceKeys(currentInstanceKeys),
  );

  useEffect(() => {
    setDismissedKeys((previous) => {
      const reconciled = reconcileDismissedAlertInstanceKeys(currentInstanceKeys);
      const next = new Set<string>();
      for (const key of previous) {
        if (currentInstanceKeys.has(key)) next.add(key);
      }
      for (const key of reconciled) next.add(key);
      return next;
    });
  }, [currentInstanceKeys]);

  const handleDismissInstances = useCallback((keys: string[]) => {
    setDismissedKeys((previous) => {
      const merged = new Set(previous);
      for (const key of keys) merged.add(key);
      return dismissAlertInstanceKeys(merged);
    });
  }, []);

  const refreshableProviderIds = useMemo(() => {
    const ids = new Set<string>();
    for (const p of filteredProviders) {
      const { quota_strategy } = resolveEffectiveTemplatePolicy(templates, p);
      const hasMonitor =
        quota_strategy === "commandcode" || quota_strategy === "opencode_go";
      if (hasMonitor && providerKeyPool(p).length > 0) {
        ids.add(p.id);
      }
    }
    return ids;
  }, [filteredProviders, templates]);

  const canRefreshQuota = refreshableProviderIds.size > 0;
  const [lastRefreshedAt, setLastRefreshedAt] = useState<number | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const handleCardRefreshed = useCallback((ts: number) => {
    setLastRefreshedAt((prev) => (prev ? Math.max(prev, ts) : ts));
    setIsRefreshing(false);
  }, []);

  const handleBulkRefresh = useCallback(() => {
    if (!canRefreshQuota) return;
    setIsRefreshing(true);
    setQuotaRefreshToken((v) => v + 1);
    window.setTimeout(() => {
      setIsRefreshing(false);
    }, 1200);
  }, [canRefreshQuota]);

  return (
    <section className="space-y-5" data-testid="ai-gateway-providers">
      {/* 顶部视图分段切换栏与全局快捷操作 */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between pb-1 border-b border-border/40">
        <div
          role="group"
          aria-label={t("aiGatewayViewModeAria", "Select view mode")}
          data-testid="ai-gateway-view-mode-selector"
          className="inline-flex items-center rounded-lg border border-border/50 bg-muted/60 p-0.5 text-xs shadow-inner"
        >
          <button
            type="button"
            aria-pressed={viewMode === "all"}
            data-testid="ai-gateway-view-mode-all"
            onClick={() => setViewMode("all")}
            className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
              viewMode === "all"
                ? "bg-background text-foreground shadow-xs border border-border/40 font-semibold"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
            }`}
          >
            <span>{t("aiGatewayFilterAll", "All")}</span>
            <span className="rounded-full bg-muted px-1.5 py-0.2 text-[10px] text-muted-foreground">
              {providers.length + modelPools.length}
            </span>
          </button>
          <button
            type="button"
            aria-pressed={viewMode === "providers"}
            data-testid="ai-gateway-view-mode-providers"
            onClick={() => setViewMode("providers")}
            className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
              viewMode === "providers"
                ? "bg-background text-foreground shadow-xs border border-border/40 font-semibold"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
            }`}
          >
            <Server className="h-3.5 w-3.5 text-primary" />
            <span>{t("aiGatewayProvidersSectionTitle", "Providers")}</span>
            <span className="rounded-full bg-muted px-1.5 py-0.2 text-[10px] text-muted-foreground">
              {providers.length}
            </span>
          </button>
          <button
            type="button"
            aria-pressed={viewMode === "model_pools"}
            data-testid="ai-gateway-view-mode-model-pools"
            onClick={() => setViewMode("model_pools")}
            className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
              viewMode === "model_pools"
                ? "bg-background text-foreground shadow-xs border border-border/40 font-semibold"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
            }`}
          >
            <Boxes className="h-3.5 w-3.5 text-violet-500" />
            <span>{t("aiGatewayModelPoolsTab", "Model pools")}</span>
            <span className="rounded-full bg-muted px-1.5 py-0.2 text-[10px] text-muted-foreground">
              {modelPools.length}
            </span>
          </button>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {onAddPool ? (
            <button
              type="button"
              onClick={onAddPool}
              disabled={busy}
              data-testid="ai-gateway-add-model-pool"
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-violet-500/30 bg-violet-500/10 px-3 text-xs font-medium text-violet-700 dark:text-violet-300 shadow-xs transition hover:bg-violet-500/20 active:bg-violet-500/30 disabled:opacity-50"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("aiGatewayModelPoolCreate", "Create model pool")}
            </button>
          ) : null}

          {onManageTemplates ? (
            <button
              type="button"
              onClick={onManageTemplates}
              data-testid="ai-gateway-manage-templates"
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/70 bg-background px-3 text-xs font-medium text-foreground shadow-xs transition hover:bg-muted active:bg-muted/80"
            >
              <Sparkles className="h-3.5 w-3.5 text-primary" />
              <span>{t("aiGatewayProviderTemplatesButton", "Provider templates")}</span>
            </button>
          ) : null}

          <button
            type="button"
            onClick={onAdd}
            disabled={busy}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("aiGatewayAddProvider", "Add provider")}
          </button>
        </div>
      </div>

      {/* 上游服务商分区：在全部模式或服务商专属模式下展示 */}
      {viewMode === "all" || viewMode === "providers" ? (
        <div className="space-y-3.5" data-testid="ai-gateway-providers-section">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="space-y-0.5">
              <div className="flex items-center gap-2">
                <Server className="h-4 w-4 text-primary" />
                <h3 className="text-sm font-semibold text-foreground">
                  {t("aiGatewayProvidersSectionTitle", "Providers")}
                </h3>
                <span className="rounded-full bg-muted px-2 py-0.2 text-[10px] font-semibold text-muted-foreground">
                  {providers.length}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                {t(
                  "aiGatewayProvidersDesc",
                  "Manage remote AI provider endpoints, protocols, and model route mappings.",
                )}
              </p>
            </div>
          </div>

          {providers.length > 0 ? (
            <div
              data-testid="ai-gateway-providers-toolbar"
              className="flex flex-wrap items-center justify-between gap-2.5 pt-0.5"
            >
              <div className="flex flex-wrap items-center gap-2">
                <div
                  role="group"
                  aria-label={t("aiGatewayFilterAria", "Filter providers by status")}
                  data-testid="ai-gateway-provider-status-filter"
                  className="inline-flex items-center rounded-lg border border-border/50 bg-muted/60 p-0.5 text-xs shadow-inner"
                >
                  <button
                    type="button"
                    data-testid="filter-status-all"
                    aria-pressed={statusFilter === "all"}
                    onClick={() => setStatusFilter("all")}
                    className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                      statusFilter === "all"
                        ? "bg-background text-foreground shadow-sm border border-border/40"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    }`}
                  >
                    <span>{t("aiGatewayFilterAll", "All")}</span>
                    <span
                      className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                        statusFilter === "all"
                          ? "bg-muted text-foreground"
                          : "bg-background/60 text-muted-foreground"
                      }`}
                    >
                      {counts.all}
                    </span>
                  </button>

                  <button
                    type="button"
                    data-testid="filter-status-enabled"
                    aria-pressed={statusFilter === "enabled"}
                    onClick={() => setStatusFilter("enabled")}
                    className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                      statusFilter === "enabled"
                        ? "bg-background text-foreground shadow-sm border border-border/40"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    }`}
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                    <span>{t("aiGatewayFilterEnabled", "Enabled")}</span>
                    <span
                      className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                        statusFilter === "enabled"
                          ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 font-semibold"
                          : "bg-background/60 text-muted-foreground"
                      }`}
                    >
                      {counts.enabled}
                    </span>
                  </button>

                  <button
                    type="button"
                    data-testid="filter-status-disabled"
                    aria-pressed={statusFilter === "disabled"}
                    onClick={() => setStatusFilter("disabled")}
                    className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                      statusFilter === "disabled"
                        ? "bg-background text-foreground shadow-sm border border-border/40"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    }`}
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-rose-500" />
                    <span>{t("aiGatewayFilterDisabled", "Disabled")}</span>
                    <span
                      className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                        statusFilter === "disabled"
                          ? "bg-rose-500/15 text-rose-700 dark:text-rose-300 font-semibold"
                          : "bg-background/60 text-muted-foreground"
                      }`}
                    >
                      {counts.disabled}
                    </span>
                  </button>
                </div>

                {allAvailableTags.length > 0 ? (
                  <div className="relative" ref={tagFilterRef}>
                    <button
                      type="button"
                      data-testid="ai-gateway-tag-filter-trigger"
                      aria-expanded={tagFilterOpen}
                      onClick={() => setTagFilterOpen((v) => !v)}
                      className={`inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium shadow-xs transition ${
                        selectedTags.length > 0
                          ? "border-primary/50 bg-primary/10 text-primary"
                          : "border-border/70 bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                      }`}
                    >
                      <Tag className="h-3.5 w-3.5" />
                      <span>{t("aiGatewayFilterByTags", "Tags")}</span>
                      {selectedTags.length > 0 ? (
                        <span
                          data-testid="ai-gateway-tag-filter-badge"
                          className="rounded-full bg-primary px-1.5 py-0.2 text-[10px] font-semibold text-primary-foreground"
                        >
                          {selectedTags.length}
                        </span>
                      ) : null}
                      <ChevronDown
                        className={`h-3 w-3 opacity-60 transition-transform ${
                          tagFilterOpen ? "rotate-180" : ""
                        }`}
                      />
                    </button>

                    {tagFilterOpen && (
                      <div
                        data-testid="ai-gateway-tag-filter-menu"
                        className="absolute left-0 z-50 mt-1.5 min-w-[200px] max-w-[280px] rounded-lg border border-border/80 bg-popover p-1.5 shadow-lg backdrop-blur-md animate-in fade-in-50 zoom-in-95"
                      >
                        <div className="flex items-center justify-between border-b border-border/50 px-2 py-1 pb-1.5 text-xs font-semibold text-foreground">
                          <span>{t("aiGatewayFilterByTags", "Filter by tags")}</span>
                          {selectedTags.length > 0 ? (
                            <button
                              type="button"
                              data-testid="ai-gateway-tag-filter-clear"
                              onClick={clearTagFilter}
                              className="text-[11px] font-normal text-muted-foreground hover:text-foreground"
                            >
                              {t("aiGatewayClearFilter", "Clear")}
                            </button>
                          ) : null}
                        </div>
                        <div className="max-h-56 overflow-y-auto py-1 space-y-0.5">
                          {allAvailableTags.map((tag) => {
                            const isChecked = selectedTags.includes(tag);
                            const count = tagCounts.get(tag) ?? 0;
                            return (
                              <button
                                key={tag}
                                type="button"
                                data-testid={`ai-gateway-tag-option-${tag}`}
                                onClick={() => toggleTagFilter(tag)}
                                className="flex w-full items-center justify-between rounded-md px-2 py-1.5 text-xs text-foreground hover:bg-muted/70 transition-colors"
                              >
                                <div className="flex items-center gap-2 truncate">
                                  <div
                                    className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border ${
                                      isChecked
                                        ? "border-primary bg-primary text-primary-foreground"
                                        : "border-muted-foreground/40 bg-background"
                                    }`}
                                  >
                                    {isChecked && <Check className="h-2.5 w-2.5" />}
                                  </div>
                                  <span className="truncate">{tag}</span>
                                </div>
                                <span className="ml-2 text-[10px] text-muted-foreground">
                                  {count}
                                </span>
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </div>
                ) : null}
              </div>

              <div className="flex items-center gap-2.5 text-xs">
                {lastRefreshedAt ? (
                  <span
                    data-testid="ai-gateway-providers-last-refreshed"
                    className="text-[11px] text-muted-foreground"
                  >
                    {t("aiGatewayProvidersLastRefreshed", "最后刷新：{{time}}", {
                      time: formatDateTime(lastRefreshedAt),
                    })}
                  </span>
                ) : null}

                <button
                  type="button"
                  data-testid="ai-gateway-providers-refresh"
                  disabled={!canRefreshQuota || isRefreshing}
                  aria-label={t("aiGatewayProvidersRefreshQuotaAria")}
                  title={
                    canRefreshQuota
                      ? t("aiGatewayProvidersRefreshQuotaAria")
                      : t("aiGatewayProvidersRefreshQuotaDisabled")
                  }
                  onClick={handleBulkRefresh}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/70 bg-background px-3 text-xs font-medium text-foreground shadow-xs transition hover:bg-muted active:bg-muted/80 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <RotateCcw className={`h-3.5 w-3.5 ${isRefreshing ? "animate-spin" : ""}`} />
                  <span>{t("aiGatewayProvidersRefreshQuota")}</span>
                </button>
              </div>
            </div>
          ) : null}

          {providers.length === 0 ? (
            <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
              <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
                <Server className="h-5 w-5" />
              </div>
              <h4 className="mt-2.5 text-xs font-medium text-foreground">
                {t("aiGatewayNoProviders", "No upstream providers yet.")}
              </h4>
              <p className="mt-1 max-w-sm text-xs text-muted-foreground">
                {t(
                  "aiGatewayNoProvidersGuide",
                  "Add upstream providers like OpenAI, DeepSeek, or any OpenAI-compatible API to start proxying requests.",
                )}
              </p>
              <div className="mt-3.5 flex items-center gap-2">
                <button
                  type="button"
                  onClick={onAdd}
                  disabled={busy}
                  className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
                >
                  <Plus className="h-3 w-3" />
                  {t("aiGatewayAddProvider", "Add provider")}
                </button>
                {onManageTemplates ? (
                  <button
                    type="button"
                    onClick={onManageTemplates}
                    data-testid="ai-gateway-manage-templates-empty"
                    className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border border-border/70 bg-background px-3 text-xs font-medium text-muted-foreground shadow-sm transition hover:bg-muted hover:text-foreground"
                  >
                    <Sparkles className="h-3 w-3 text-primary" />
                    {t("aiGatewayProviderTemplatesButton", "Provider templates")}
                  </button>
                ) : null}
              </div>
            </div>
          ) : filteredProviders.length === 0 ? (
            <div
              data-testid="ai-gateway-providers-filter-empty"
              className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/40 px-6 py-10 text-center"
            >
              <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
                <Filter className="h-5 w-5 opacity-70" />
              </div>
              <h4 className="mt-2.5 text-xs font-medium text-foreground">
                {t("aiGatewayNoMatchingProviders", "No matching upstream providers")}
              </h4>
              <p className="mt-1 max-w-sm text-xs text-muted-foreground">
                {t(
                  "aiGatewayNoMatchingProvidersGuide",
                  "No upstream providers match the selected status filter.",
                )}
              </p>
              <button
                type="button"
                onClick={() => {
                  setStatusFilter("all");
                  setSelectedTags([]);
                }}
                className="mt-3.5 inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
              >
                <RotateCcw className="h-3 w-3" />
                {t("aiGatewayClearFilter", "Show all providers")}
              </button>
            </div>
          ) : (
            <div className="grid gap-3 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3 items-start">
              {filteredProviders.map((provider) => (
                <UpstreamProviderCardItem
                  key={provider.id}
                  provider={provider}
                  isSelected={provider.id === selectedProviderId}
                  busy={busy}
                  templates={templates}
                  baseNow={baseNow}
                  quotaRefreshToken={quotaRefreshToken}
                  dismissedKeys={dismissedKeys}
                  handleDismissInstances={handleDismissInstances}
                  onSelect={onSelect}
                  onToggleEnabled={onToggleEnabled}
                  onReenableKey={onReenableKey}
                  handleCardRefreshed={handleCardRefreshed}
                />
              ))}
            </div>
          )}
        </div>
      ) : null}

      {/* 模型池分区：在全部模式（有模型池时）或模型池专属模式下展示 */}
      {(viewMode === "all" && modelPools.length > 0) || viewMode === "model_pools" ? (
        <div className="space-y-3.5" data-testid="ai-gateway-model-pools-section">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="space-y-0.5">
              <div className="flex items-center gap-2">
                <Boxes className="h-4 w-4 text-violet-500" />
                <h3 className="text-sm font-semibold text-foreground">
                  {t("aiGatewayModelPoolsTab", "Model pools")}
                </h3>
                <span className="rounded-full bg-violet-500/15 px-2 py-0.2 text-[10px] font-semibold text-violet-700 dark:text-violet-300">
                  {modelPools.length}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                {t(
                  "aiGatewayModelPoolsDesc",
                  "Aggregate upstream models into unified public model pools with failover.",
                )}
              </p>
            </div>
          </div>

          {modelPools.length > 0 ? (
            <div
              data-testid="ai-gateway-model-pools-toolbar"
              className="flex flex-wrap items-center justify-between gap-2.5 pt-0.5"
            >
              <div className="flex flex-wrap items-center gap-2">
                <div
                  role="group"
                  aria-label={t("aiGatewayFilterAria", "Filter model pools by status")}
                  data-testid="ai-gateway-pool-status-filter"
                  className="inline-flex items-center rounded-lg border border-border/50 bg-muted/60 p-0.5 text-xs shadow-inner"
                >
                  <button
                    type="button"
                    data-testid="filter-pool-status-all"
                    aria-pressed={poolStatusFilter === "all"}
                    onClick={() => setPoolStatusFilter("all")}
                    className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                      poolStatusFilter === "all"
                        ? "bg-background text-foreground shadow-sm border border-border/40"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    }`}
                  >
                    <span>{t("aiGatewayFilterAll", "All")}</span>
                    <span
                      className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                        poolStatusFilter === "all"
                          ? "bg-muted text-foreground"
                          : "bg-background/60 text-muted-foreground"
                      }`}
                    >
                      {poolCounts.all}
                    </span>
                  </button>
                  <button
                    type="button"
                    data-testid="filter-pool-status-enabled"
                    aria-pressed={poolStatusFilter === "enabled"}
                    onClick={() => setPoolStatusFilter("enabled")}
                    className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                      poolStatusFilter === "enabled"
                        ? "bg-background text-foreground shadow-sm border border-border/40"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    }`}
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                    <span>{t("aiGatewayFilterEnabled", "Enabled")}</span>
                    <span
                      className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                        poolStatusFilter === "enabled"
                          ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 font-semibold"
                          : "bg-background/60 text-muted-foreground"
                      }`}
                    >
                      {poolCounts.enabled}
                    </span>
                  </button>
                  <button
                    type="button"
                    data-testid="filter-pool-status-disabled"
                    aria-pressed={poolStatusFilter === "disabled"}
                    onClick={() => setPoolStatusFilter("disabled")}
                    className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-all ${
                      poolStatusFilter === "disabled"
                        ? "bg-background text-foreground shadow-sm border border-border/40"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    }`}
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-rose-500" />
                    <span>{t("aiGatewayFilterDisabled", "Disabled")}</span>
                    <span
                      className={`rounded-full px-1.5 py-0.2 text-[10px] ${
                        poolStatusFilter === "disabled"
                          ? "bg-rose-500/15 text-rose-700 dark:text-rose-300 font-semibold"
                          : "bg-background/60 text-muted-foreground"
                      }`}
                    >
                      {poolCounts.disabled}
                    </span>
                  </button>
                </div>
              </div>
            </div>
          ) : null}

          {modelPools.length === 0 ? (
            <div
              data-testid="ai-gateway-model-pools-empty"
              className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center"
            >
              <div className="rounded-full bg-violet-500/10 p-2.5 text-violet-600 dark:text-violet-400">
                <Boxes className="h-5 w-5" />
              </div>
              <h4 className="mt-2.5 text-xs font-medium text-foreground">
                {t("aiGatewayNoModelPools", "No model pools yet.")}
              </h4>
              <p className="mt-1 max-w-sm text-xs text-muted-foreground">
                {t(
                  "aiGatewayNoModelPoolsGuide",
                  "Create a model pool to combine models from different providers into a single virtual model.",
                )}
              </p>
              {onAddPool ? (
                <button
                  type="button"
                  onClick={onAddPool}
                  disabled={busy}
                  className="mt-3.5 inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
                >
                  <Plus className="h-3 w-3" />
                  {t("aiGatewayModelPoolCreate", "Create model pool")}
                </button>
              ) : null}
            </div>
          ) : filteredModelPools.length === 0 ? (
            <div
              data-testid="ai-gateway-model-pools-filter-empty"
              className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-card/40 px-6 py-10 text-center"
            >
              <div className="rounded-full bg-muted/60 p-2.5 text-muted-foreground">
                <Filter className="h-5 w-5 opacity-70" />
              </div>
              <h4 className="mt-2.5 text-xs font-medium text-foreground">
                {t("aiGatewayNoMatchingModelPools", "No matching model pools")}
              </h4>
              <p className="mt-1 max-w-sm text-xs text-muted-foreground">
                {t(
                  "aiGatewayNoMatchingModelPoolsGuide",
                  "No model pools match the selected status filter.",
                )}
              </p>
              <button
                type="button"
                onClick={() => setPoolStatusFilter("all")}
                className="mt-3.5 inline-flex h-7.5 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium shadow-sm transition hover:bg-muted"
              >
                <RotateCcw className="h-3 w-3" />
                {t("aiGatewayClearFilter", "Show all")}
              </button>
            </div>
          ) : (
            <div
              data-testid="ai-gateway-model-pools"
              className="grid gap-3 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3 items-start"
            >
              {filteredModelPools.map((pool) => (
                <ModelPoolCard
                  key={pool.id}
                  pool={pool}
                  providers={providers}
                  status={poolStatuses.find((item) => item.id === pool.id) ?? null}
                  busy={busy}
                  onToggleEnabled={(poolId, enabled) =>
                    onTogglePoolEnabled?.(poolId, enabled)
                  }
                  onEdit={(target) => onEditPool?.(target)}
                  onDelete={(poolId) => onDeletePool?.(poolId)}
                  onOpenLogs={(model) => onOpenPoolLogs?.(model)}
                />
              ))}
            </div>
          )}
        </div>
      ) : null}
    </section>
  );
}
